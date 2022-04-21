use crate::connection;
use crate::parser;
use crate::Frame;
use snafu::{prelude::*, ResultExt};
use tracing::info;

use reqwest::header;
use std::time::Duration;

#[derive(Debug, Snafu)]
pub enum Error {
    #[snafu(display("failed on network {}", source))]
    ConnectError { source: connection::Error },
    #[snafu(display("failed for parsing error{}", source))]
    CommandError { source: parser::Error },
    #[snafu(display("failed for http error{}", source))]
    HttpError { source: reqwest::Error },
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
        // Redis 的 Get 命令也是一个数组。数组中的第一个元素是字符串 'Get'，
        // 第二个元素也是一个 string：key
        let key = parser.next_string().context(CommandSnafu)?;
        let get = Get::new(key);
        Ok(get)
    }

    // 实现 Get 命令：调用 Http 请求，查询 httpbin.org/ip 服务
    pub async fn apply(self, connection: &mut connection::Connection) -> Result<()> {
        let mut headers = header::HeaderMap::new();
        headers.insert("Accept", header::HeaderValue::from_static("text/plain"));

        let client = reqwest::Client::builder()
            .default_headers(headers)
            .timeout(Duration::from_secs(3))
            .connection_verbose(true)
            .build()
            .context(HttpSnafu)?;

        let doge = client
            .get("https://httpbin.org/ip")
            .send()
            .await
            .context(HttpSnafu)?
            .text()
            .await
            .context(HttpSnafu)?;

        info!("Got {:#?}", doge);

        let response = Frame::Simple(doge.len().to_string());
        // 如果 write_frame 出错，也会结束循环，抛出一个 IoFailed
        connection
            .write_frame(&response)
            .await
            .context(ConnectSnafu)?;
        info!(
            "for get key: {}. the sent response successfully: {:?}",
            self.key, response
        );

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
        // 每个 Redis 命令是一个由 Frames 组成的数组。
        // 而且数组的第一个元素是一个字符串，这个字符串就是命令名字。例如：
        // Get / Set 等命令。
        let s = parser.next_string().context(CommandSnafu)?;

        let cmd = match s.as_str() {
            // 当前我们先只实现 Get 命令
            "get" => {
                let g = Get::parse_frame(&mut parser)?;
                Command::Get(g)
            }
            _ => Command::Unknown(s),
        };

        Ok(cmd)
    }

    pub async fn apply(self, connection: &mut connection::Connection) -> Result<()> {
        // Command 自己是一个 enum，对这个 enum 进行 match
        match self {
            Command::Get(get) => get.apply(connection).await?,
            _ => {
                // 目前先只实现 Get，其他的命令简单回复简单 string：OK
                let response = Frame::Simple("OK".to_string());
                connection
                    .write_frame(&response)
                    .await
                    .context(ConnectSnafu)?;
            }
        };

        Ok(())
    }
}
