use bytes::Buf;

use snafu::prelude::*;

use std::io::{self, Cursor};

use crate::frame::Frame;

use tracing::{error, info};

#[derive(Debug, Snafu)]
pub enum Error {
    #[snafu(display("failed. error is reset"))]
    ResetError,
    #[snafu(display("failed for io error {}", source))]
    IoError { source: io::Error },
    #[snafu(display("failed for bad frame"))]
    FrameError,
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

    pub async fn write_frame(&mut self, frame: &Frame) -> Result<()> {
        info!("try to write the frame to client: {:?}", frame);

        self.stream.write_u8(b'+').await.context(IoSnafu)?;
        self.stream
            .write_all("OK".as_bytes())
            .await
            .context(IoSnafu)?;
        self.stream.write_all(b"\r\n").await.context(IoSnafu)?;

        self.stream.flush().await.context(IoSnafu)?;

        Ok(())
    }
}
