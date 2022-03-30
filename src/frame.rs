use bytes::{Buf, Bytes};
use std::string;

use snafu::{prelude::*, ResultExt};

#[derive(Debug, Snafu)]
pub enum Error {
    #[snafu(display("Not enough data is available to parse a message"))]
    IncompleteError,
    #[snafu(display("failed for bad string encode {}", source))]
    EncodeError { source: string::FromUtf8Error },
    #[snafu(display("protocol error; invalid frame type byte: {}", b))]
    ProtocolError { b: u8 },
    #[snafu(display("String to decimal error"))]
    DecimalError,
}

pub type Result<T, E = Error> = std::result::Result<T, E>;

use std::io::Cursor;

#[derive(Clone, Debug)]
pub enum Frame {
    Simple(String),
    Error(String),
    Integer(u64),
    Bulk(Bytes),
    Null,
    Array(Vec<Frame>),
}

impl Frame {
    pub fn check(src: &mut Cursor<&[u8]>) -> Result<()> {
        match get_u8(src)? {
            // 字符串类型
            b'+' => {
                get_line(src)?;
                Ok(())
            }
            // 错误类型
            b'-' => {
                get_line(src)?;
                Ok(())
            }
            // 数字类型
            b':' => {
                let _ = get_decimal(src)?;
                Ok(())
            }
            // Bulk Strings
            b'$' => {
                if b'-' == peek_u8(src)? {
                    // Skip '-1\r\n'
                    skip(src, 4)
                } else {
                    // 先看这个 Bulk String 的长度
                    let len: usize = match get_decimal(src)?.try_into() {
                        Ok(len) => len,
                        Err(_) => IncompleteSnafu.fail()?,
                    };

                    // skip that number of bytes + 2 (\r\n).
                    skip(src, len + 2)
                }
            }
            // 数组
            b'*' => {
                // 先看这个数组有几个元素
                let len = get_decimal(src)?;

                for _ in 0..len {
                    Frame::check(src)?;
                }

                Ok(())
            }
            actual => ProtocolSnafu { b: actual }.fail()?,
        }
    }

    pub fn parse(src: &mut Cursor<&[u8]>) -> Result<Frame> {
        match get_u8(src)? {
            b'+' => {
                // Read the line and convert it to `Vec<u8>`
                let line = get_line(src)?.to_vec();

                // Convert the line to a String
                let string = String::from_utf8(line).context(EncodeSnafu)?;
                Ok(Frame::Simple(string))
            }
            b'-' => {
                // Read the line and convert it to `Vec<u8>`
                let line = get_line(src)?.to_vec();

                // Convert the line to a String
                let string = String::from_utf8(line).context(EncodeSnafu)?;

                Ok(Frame::Error(string))
            }
            b':' => {
                let u = get_decimal(src)?;

                Ok(Frame::Integer(u))
            }
            b'$' => {
                if b'-' == peek_u8(src)? {
                    let line = get_line(src)?;

                    if line != b"-1" {
                        return ProtocolSnafu { b: 1 }.fail();
                    }

                    Ok(Frame::Null)
                } else {
                    // Read the bulk string
                    let len = match get_decimal(src)?.try_into() {
                        Ok(len) => len,
                        Err(_) => IncompleteSnafu.fail()?,
                    };

                    let n = len + 2;

                    if src.remaining() < n {
                        IncompleteSnafu.fail()?;
                    }

                    let data = Bytes::copy_from_slice(&src.chunk()[..len]);

                    // skip that number of bytes + 2 (\r\n).
                    skip(src, n)?;

                    Ok(Frame::Bulk(data))
                }
            }
            b'*' => {
                let len = match get_decimal(src)?.try_into() {
                    Ok(len) => len,
                    Err(_) => IncompleteSnafu.fail()?,
                };

                let mut out = Vec::with_capacity(len);

                for _ in 0..len {
                    out.push(Frame::parse(src)?);
                }

                Ok(Frame::Array(out))
            }
            actual => ProtocolSnafu { b: actual }.fail()?,
        }
    }
}

/// 检测到一个完整的行（\r\n 结尾）
fn get_line<'a>(src: &mut Cursor<&'a [u8]>) -> Result<&'a [u8]> {
    // Scan the bytes directly
    let start = src.position() as usize;
    // Scan to the second to last byte
    let end = src.get_ref().len() - 1;

    for i in start..end {
        if src.get_ref()[i] == b'\r' && src.get_ref()[i + 1] == b'\n' {
            // 成功的获取到一行数据，然后把当前位置推进到 \r\n 之后
            src.set_position((i + 2) as u64);

            // Return the line
            return Ok(&src.get_ref()[start..i]);
        }
    }

    IncompleteSnafu.fail()?
}

// 先从 buff 里面获取第一个字节
fn get_u8(src: &mut Cursor<&[u8]>) -> Result<u8> {
    if !src.has_remaining() {
        IncompleteSnafu.fail()?
    }

    Ok(src.get_u8())
}

fn get_decimal(src: &mut Cursor<&[u8]>) -> Result<u64> {
    use atoi::atoi;

    let line = get_line(src)?;

    atoi::<u64>(line).ok_or_else(|| DecimalSnafu.build())
}

fn peek_u8(src: &mut Cursor<&[u8]>) -> Result<u8> {
    if !src.has_remaining() {
        IncompleteSnafu.fail()?
    }

    Ok(src.chunk()[0])
}

fn skip(src: &mut Cursor<&[u8]>, n: usize) -> Result<()> {
    if src.remaining() < n {
        IncompleteSnafu.fail()?
    }

    src.advance(n);
    Ok(())
}

#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn ts_on_get_u8() {
        let v = vec![b'1', b'2', b'3', b'4', b'5'];
        let mut buff = Cursor::new(&v[..]);

        // 把 position 设置到 buff 的最后，get_u8 出错
        buff.set_position(5);
        assert_eq!(get_u8(&mut buff).is_err(), true);

        //  把 position 设置到 buff 的开头
        buff.set_position(0);

        // 不断的 get 一个 u8，然后推进
        assert_eq!(get_u8(&mut buff).unwrap(), 49);
        assert_eq!(get_u8(&mut buff).unwrap(), 50);
        assert_eq!(get_u8(&mut buff).unwrap(), 51);
        assert_eq!(get_u8(&mut buff).unwrap(), 52);
        assert_eq!(get_u8(&mut buff).unwrap(), 53);
    }
}
