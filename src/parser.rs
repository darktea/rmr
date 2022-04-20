use crate::Frame;

use std::{str, vec};

use snafu::{prelude::*, ResultExt};

#[derive(Debug, Snafu)]
pub enum Error {
    #[snafu(display("failed for bad string encode {}", source))]
    EncodeError { source: str::Utf8Error },
    #[snafu(display("failed to parse the frame"))]
    ParseError,
}

pub type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Debug)]
pub struct Parser {
    /// Array frame iterator.
    parts: vec::IntoIter<Frame>,
}

impl Parser {
    pub fn new(frame: Frame) -> Result<Parser> {
        let arr = match frame {
            Frame::Array(arr) => arr,
            _ => ParseSnafu.fail()?,
        };

        Ok(Parser {
            parts: arr.into_iter(),
        })
    }

    fn next(&mut self) -> Result<Frame> {
        self.parts.next().ok_or_else(|| ParseSnafu.build())
    }

    pub fn next_string(&mut self) -> Result<String> {
        match self.next()? {
            // Both `Simple` and `Bulk` representation may be strings. Strings
            // are parsed to UTF-8.
            //
            // While errors are stored as strings, they are considered separate
            // types.
            Frame::Simple(s) => Ok(s),
            Frame::Bulk(data) => {
                let s = str::from_utf8(&data[..]).context(EncodeSnafu)?.to_string();
                Ok(s)
            }
            _ => ParseSnafu.fail()?,
        }
    }
}
