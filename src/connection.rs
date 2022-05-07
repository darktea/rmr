use bytes::Buf;

use snafu::prelude::*;

use std::io::{self, Cursor};

use crate::frame::Frame;

use tracing::{error, info};

#[derive(Debug, Snafu)]
pub enum Error {
    #[snafu(display("failed. error is reset"))]
    Reset,
    #[snafu(display("failed for io error {}", source))]
    Io { source: io::Error },
    #[snafu(display("failed for bad frame"))]
    Frame,
}

pub type Result<T, E = Error> = std::result::Result<T, E>;

use bytes::BytesMut;
use tokio::io::{AsyncReadExt, AsyncWriteExt, BufWriter};
use tokio::net::TcpStream;

#[derive(Debug)]
pub struct Connection {
    stream: BufWriter<TcpStream>,

    buffer: BytesMut,
}

impl Connection {
    pub fn new(socket: TcpStream) -> Connection {
        Connection {
            stream: BufWriter::new(socket),
            buffer: BytesMut::with_capacity(4 * 1024),
        }
    }

    pub async fn read_frame(&mut self) -> Result<Option<Frame>> {
        loop {
            if let Some(frame) = self.parse_frame()? {
                return Ok(Some(frame));
            }

            let len = self
                .stream
                .read_buf(&mut self.buffer)
                .await
                .context(IoSnafu)?;

            if 0 == len {
                if self.buffer.is_empty() {
                    return Ok(None);
                } else {
                    ResetSnafu.fail()?;
                }
            }
        }
    }

    fn parse_frame(&mut self) -> Result<Option<Frame>> {
        let mut buf = Cursor::new(&self.buffer[..]);

        // 先快速判断是否可以从 buffer 里面解析出一个完整的 Frame
        // 只有返回 true 的时候，才会真正的做解析 Frame 动作（避免不必要的工作）
        match Frame::check(&mut buf) {
            Ok(_) => {
                // 完整 Frame 的 size 就是：buf.position
                let len = buf.position() as usize;
                // 进行正式 parse 之前，先把 position 还原
                buf.set_position(0);
                let frame = match Frame::parse(&mut buf) {
                    Ok(f) => f,
                    Err(e) => {
                        error!("io error. {:?}", e);
                        FrameSnafu.fail()?
                    }
                };
                self.buffer.advance(len);
                Ok(Some(frame))
            }
            Err(e) => match e {
                crate::frame::Error::IncompleteError => Ok(None),
                other => {
                    error!("io error. {:?}", other);
                    FrameSnafu.fail()?
                }
            },
        }
    }

    /// Write a single `Frame` value to the underlying stream.
    ///
    /// The `Frame` value is written to the socket using the various `write_*`
    /// functions provided by `AsyncWrite`. Calling these functions directly on
    /// a `TcpStream` is **not** advised, as this will result in a large number of
    /// syscalls. However, it is fine to call these functions on a *buffered*
    /// write stream. The data will be written to the buffer. Once the buffer is
    /// full, it is flushed to the underlying socket.
    pub async fn write_frame(&mut self, frame: &Frame) -> Result<()> {
        info!("try to write the frame to client: {:?}", frame);
        // Arrays are encoded by encoding each entry. All other frame types are
        // considered literals. For now, mini-redis is not able to encode
        // recursive frame structures. See below for more details.
        match frame {
            Frame::Array(val) => {
                // Encode the frame type prefix. For an array, it is `*`.
                self.stream.write_u8(b'*').await.context(IoSnafu)?;

                // Encode the length of the array.
                self.write_decimal(val.len() as u64)
                    .await
                    .context(IoSnafu)?;

                // Iterate and encode each entry in the array.
                for entry in &**val {
                    self.write_value(entry).await.context(IoSnafu)?;
                }
            }
            // The frame type is a literal. Encode the value directly.
            _ => self.write_value(frame).await.context(IoSnafu)?,
        }

        // Ensure the encoded frame is written to the socket. The calls above
        // are to the buffered stream and writes. Calling `flush` writes the
        // remaining contents of the buffer to the socket.
        self.stream.flush().await.context(IoSnafu)?;

        Ok(())
    }

    /// Write a decimal frame to the stream
    async fn write_decimal(&mut self, val: u64) -> io::Result<()> {
        use std::io::Write;

        // Convert the value to a string
        let mut buf = [0u8; 20];
        let mut buf = Cursor::new(&mut buf[..]);
        write!(&mut buf, "{}", val)?;

        let pos = buf.position() as usize;
        self.stream.write_all(&buf.get_ref()[..pos]).await?;
        self.stream.write_all(b"\r\n").await?;

        Ok(())
    }

    /// Write a frame literal to the stream
    async fn write_value(&mut self, frame: &Frame) -> io::Result<()> {
        match frame {
            Frame::Simple(val) => {
                self.stream.write_u8(b'+').await?;
                self.stream.write_all(val.as_bytes()).await?;
                self.stream.write_all(b"\r\n").await?;
            }
            Frame::Error(val) => {
                self.stream.write_u8(b'-').await?;
                self.stream.write_all(val.as_bytes()).await?;
                self.stream.write_all(b"\r\n").await?;
            }
            Frame::Integer(val) => {
                self.stream.write_u8(b':').await?;
                self.write_decimal(*val).await?;
            }
            Frame::Null => {
                self.stream.write_all(b"$-1\r\n").await?;
            }
            Frame::Bulk(val) => {
                let len = val.len();

                self.stream.write_u8(b'$').await?;
                self.write_decimal(len as u64).await?;
                self.stream.write_all(val).await?;
                self.stream.write_all(b"\r\n").await?;
            }
            // Encoding an `Array` from within a value cannot be done using a
            // recursive strategy. In general, async fns do not support
            // recursion. Mini-redis has not needed to encode nested arrays yet,
            // so for now it is skipped.
            Frame::Array(_val) => unreachable!(),
        }

        Ok(())
    }
}
