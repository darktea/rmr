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
    if src.get_ref().is_empty() {
        IncompleteSnafu.fail()?
    }

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

// 切掉当前 position 之前的内容，然后返回剩余内容的第一个 u8
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
    fn ts_err_get_line() {
        // should end of \r\n
        let v = vec![b'1', b'2'];
        let mut buff = Cursor::new(&v[..]);

        buff.set_position(0);
        assert!(get_line(&mut buff).is_err());

        // should not be an empty buff
        let v_empty: Vec<u8> = Vec::new();
        let mut buff_empty = Cursor::new(&v_empty[..]);

        buff_empty.set_position(0);
        assert!(get_line(&mut buff_empty).is_err());
    }

    #[test]
    fn ts_on_get_line() {
        let v = vec![b'1', b'2', b'\r', b'\n', b'5', b'\r', b'\n'];
        let mut buff = Cursor::new(&v[..]);

        // 把 position 设置到 buff 的最后，get_u8 出错
        buff.set_position(v.len().try_into().unwrap());
        assert!(get_line(&mut buff).is_err());

        //  把 position 设置到 buff 的开头
        buff.set_position(0);
        assert_eq!(buff.position(), 0);

        // get the first line
        let line = get_line(&mut buff).unwrap().to_vec();
        // Convert the first line to a String
        let line_str = String::from_utf8(line).unwrap();

        assert_eq!(line_str, "12");
        assert_eq!(buff.position(), 4);

        // get the 2nd line
        let line_2 = get_line(&mut buff).unwrap().to_vec();
        // Convert the 2nd line to a String
        let line_str_2 = String::from_utf8(line_2).unwrap();

        assert_eq!(line_str_2, "5");
        assert_eq!(buff.position(), 7);
    }

    #[test]
    fn ts_on_get_u8() {
        let v = vec![b'1', b'2', b'3', b'4', b'5'];
        let mut buff = Cursor::new(&v[..]);

        // 把 position 设置到 buff 的最后，get_u8 出错
        buff.set_position(v.len().try_into().unwrap());
        assert!(get_u8(&mut buff).is_err());

        //  把 position 设置到 buff 的开头
        buff.set_position(0);

        // 不断的 get 一个 u8，然后推进
        assert_eq!(get_u8(&mut buff).unwrap(), b'1');
        assert_eq!(get_u8(&mut buff).unwrap(), b'2');
        assert_eq!(get_u8(&mut buff).unwrap(), b'3');
        assert_eq!(get_u8(&mut buff).unwrap(), b'4');
        assert_eq!(get_u8(&mut buff).unwrap(), b'5');
    }

    #[test]
    fn ts_on_peek_u8() {
        let v = vec![b'1', b'2', b'3', b'4', b'5'];
        let mut buff = Cursor::new(&v[..]);

        // 把 position 设置到 buff 的最后，get_u8 出错
        buff.set_position(v.len().try_into().unwrap());
        assert!(peek_u8(&mut buff).is_err());

        //  把 position 设置为 3（从 0 开始）
        buff.set_position(3);

        // peek 位置是 3 的数据
        assert_eq!(peek_u8(&mut buff).unwrap(), b'4');
    }

    #[tokio::test]
    async fn ts_on_http_mock() {
        use httpmock::prelude::*;
        use serde_json::{json, Value};

        // mock a htpp server, and we can get json content from this http mock:
        // {"origin" : "1.1.1.1"}
        let json_key = "origin";
        let json_value = "1.1.1.1";

        // Start a lightweight mock server by async.
        let server = MockServer::start_async().await;

        // Create a mock on the server.
        let hello_mock = server
            .mock_async(|when, then| {
                when.method(GET)
                    .path("/translate")
                    .query_param("word", "hello");
                then.status(200)
                    .header("content-type", "application/json")
                    .json_body(json!({ json_key: json_value }));
            })
            .await;

        // Send an HTTP request to the mock server
        // the body is a json string
        let body = reqwest::get(server.url("/translate?word=hello"))
            .await
            .unwrap()
            .text()
            .await
            .unwrap();

        // Ensure the specified mock was called exactly one time (or fail with a detailed error description).
        hello_mock.assert_async().await;

        // try to parse the json value
        let v: Value = serde_json::from_str(body.as_str()).unwrap();

        let origin = v[json_key].as_str().unwrap();

        assert_eq!(origin, json_value);
    }
}
