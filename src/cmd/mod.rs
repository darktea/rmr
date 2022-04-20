use crate::connection;
use crate::parser;
use crate::Frame;
use snafu::{prelude::*, ResultExt};
use tracing::info;

#[derive(Debug, Snafu)]
pub enum Error {
    #[snafu(display("failed on network {}", source))]
    ConnectError { source: connection::Error },
    #[snafu(display("failed for parsing error{}", source))]
    CommandError { source: parser::Error },
}

pub type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Debug)]
pub struct Get {
    key: String,
}

impl Get {
    pub fn new(key: impl ToString) -> Get {
        Get {
            key: key.to_string(),
        }
    }

    pub fn parse_frame(parser: &mut parser::Parser) -> Result<Get> {
        let key = parser.next_string().context(CommandSnafu)?;
        let get = Get::new(key);
        Ok(get)
    }

    pub async fn apply(self, connection: &mut connection::Connection) -> Result<()> {
        let response = Frame::Simple(self.key.to_string());
        // 如果 write_frame 出错，也会结束循环，抛出一个 IoFailed
        connection
            .write_frame(&response)
            .await
            .context(ConnectSnafu)?;
        info!("sent response successfully: {:?}", response);

        Ok(())
    }
}

#[derive(Debug)]
pub enum Command {
    Get(Get),
    Publish(String),
    Set(String),
    Subscribe(String),
    Unsubscribe(String),
    Ping(String),
    Unknown(String),
}

impl Command {
    pub fn from_frame(frame: Frame) -> Result<Command> {
        let mut parser = parser::Parser::new(frame).context(CommandSnafu)?;
        let s = parser.next_string().context(CommandSnafu)?;

        let cmd = match &s[..] {
            "get" => {
                let g = Get::parse_frame(&mut parser)?;
                Command::Get(g)
            }
            _ => Command::Unknown(s),
        };

        Ok(cmd)
    }

    pub async fn apply(self, connection: &mut connection::Connection) -> Result<()> {
        match self {
            Command::Get(get) => get.apply(connection).await?,
            _ => {
                let response = Frame::Simple("OK".to_string());
                // 如果 write_frame 出错，也会结束循环，抛出一个 IoFailed
                connection
                    .write_frame(&response)
                    .await
                    .context(ConnectSnafu)?;
            }
        };

        Ok(())
    }
}
