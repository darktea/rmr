use std::future::Future;
use std::os::unix::prelude::AsRawFd;

use reqwest::header;
use std::io;
use std::time::Duration;

use log::error;
use log::warn;
use tokio::net::TcpListener;

use tracing::{info, instrument};

use snafu::{prelude::*, ResultExt};

use crate::cmd;
use crate::connection;
use crate::connection::Connection;

#[derive(Debug, Snafu)]
pub enum Error {
    #[snafu(display("failed on network {}", source))]
    ConnectError { source: connection::Error },
    #[snafu(display("failed for command run error. {}", source))]
    CommandError { source: cmd::Error },
    #[snafu(display("failed on http error. {}", source))]
    HttpError { source: reqwest::Error },
    #[snafu(display("failed for io error {}", source))]
    IoError { source: io::Error },
}

pub type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Debug)]
struct Handler {
    connection: Connection,
    fd: i32,
    cli: reqwest::Client,
}

impl Handler {
    pub fn new(connection: Connection, fd: i32, cli: reqwest::Client) -> Handler {
        Handler {
            connection,
            fd,
            cli,
        }
    }

    // 针对每个连接，进行无限循环，直到：出错（返回 Err）或者客户端关闭连接（返回一个 Ok）
    #[instrument(skip(self), fields(fd = self.fd))]
    pub async fn process(&mut self) -> Result<()> {
        info!("the server accepted a new client. fd is: {}", self.fd);

        loop {
            // read_frame 返回 Err 的话，返回 Err 给 process 的调用者
            let maybe_frame = self.connection.read_frame().await.context(ConnectSnafu)?;

            // 成功读到一个 Fame 的话，又有 2 种可能，match：
            let frame = match maybe_frame {
                Some(frame) => frame,
                // 如果返回 None，代表客户端关闭连接，结束循环，返回 Ok
                None => {
                    info!("peer closed");
                    return Ok(());
                }
            };

            info!("get a new frame: {:?}", frame);

            // 把 Frame 转换为 Command
            let cmd = cmd::Command::from_frame(frame).context(CommandSnafu)?;
            info!("get first cmd: {:?}", cmd);

            // 执行 Command。遇到异常的话，退出循环
            cmd.apply(&mut self.cli, &mut self.connection)
                .await
                .context(CommandSnafu)?;
        }
    }
}

pub async fn loop_on_listener(listener: TcpListener) -> Result<()> {
    let mut headers = header::HeaderMap::new();
    headers.insert("Accept", header::HeaderValue::from_static("text/plain"));
    headers.insert(
        "User-Agent",
        header::HeaderValue::from_static("HTTPie/3.1.0"),
    );

    let client = reqwest::Client::builder()
        .default_headers(headers)
        .timeout(Duration::from_secs(3))
        .connection_verbose(true)
        .pool_max_idle_per_host(20)
        .build()
        .context(HttpSnafu)?;

    // 进入主循环
    loop {
        // 进行 accept 操作
        // 如果 accept 到新的 socket，返回这个 socket；
        // TODO: 如果遇到 Err，server 进入 shutdown 流程
        let (socket, _) = listener.accept().await.context(IoSnafu)?;

        let cli = client.clone();

        // 为每一条连接都生成一个新的任务，
        // `socket` 的所有权将被移动到新的任务中，并在那里进行处理
        tokio::spawn(async move {
            let fd = socket.as_raw_fd();
            let connection = Connection::new(socket);
            let mut handler = Handler::new(connection, fd, cli);

            if let Err(err) = handler.process().await {
                error!("this client has an error, disconnect it {}!", err);
            }
        });
    }
}

pub async fn run(shutdown: impl Future) -> Result<()> {
    let listener = TcpListener::bind("127.0.0.1:6379").await.context(IoSnafu)?;

    warn!("the server starts to listen on PORT: 6379");

    tokio::select! {
        resp = loop_on_listener(listener) => {
            if let Err(e) = resp {
                error!("the server on error: {}", e);
            }
            Ok(())
        }
        _ = shutdown => {
            warn!("the server shutdown");
            Ok(())
        }
    }
}
