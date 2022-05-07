use std::future::Future;
use std::os::unix::prelude::AsRawFd;

use reqwest::header;
use std::io;
use std::time::Duration;

use log::error;
use log::warn;
use tokio::net::TcpListener;
use tokio::sync::broadcast;
use tokio::sync::mpsc;

use tracing::{info, instrument};

use snafu::{prelude::*, ResultExt};

use crate::cmd;
use crate::connection;
use crate::connection::Connection;
use crate::shutdown::Shutdown;

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
    shutdown: Shutdown,
    connection: Connection,
    fd: i32,
    cli: reqwest::Client,
    _shutdown_complete: mpsc::Sender<()>,
}

impl Handler {
    pub fn new(
        shutdown: Shutdown,
        connection: Connection,
        fd: i32,
        cli: reqwest::Client,
        _shutdown_complete: mpsc::Sender<()>,
    ) -> Handler {
        Handler {
            shutdown,
            connection,
            fd,
            cli,
            _shutdown_complete,
        }
    }

    // 针对每个连接，进行无限循环，直到：出错（返回 Err）或者客户端关闭连接（返回一个 Ok）
    #[instrument(skip(self), fields(fd = self.fd))]
    pub async fn process(&mut self) -> Result<()> {
        info!("the server accepted a new client. fd is: {}", self.fd);

        while !self.shutdown.is_shutdown() {
            // read_frame 返回 Err 的话，返回 Err 给 process 的调用者
            let maybe_frame = tokio::select! {
                res = self.connection.read_frame() => res,
                _ = self.shutdown.recv() => {
                    info!("the client shutdown grace. fd is: {}", self.fd);
                    return Ok(());
                }
            }
            .context(ConnectSnafu)?;

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

        Ok(())
    }
}

pub async fn loop_on_listener(
    listener: TcpListener,
    notify_shutdown: &broadcast::Sender<()>,
    shutdown_complete_tx: &mpsc::Sender<()>,
) -> Result<()> {
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

        // 给每个连接一个 shutdown 实例，用来通知该连接优雅结束
        let shutdown = Shutdown::new(notify_shutdown.subscribe());

        // server shutdown 时要等所有的异步任务结束才能退出
        // 当异步任务的收尾结束时，利用这个发送者通知 server 该异步任务结束
        let shutdown_complete_tx = shutdown_complete_tx.clone();

        // 为每一条连接都生成一个新的任务，
        // `socket` 的所有权将被移动到新的任务中，并在那里进行处理
        tokio::spawn(async move {
            let fd = socket.as_raw_fd();
            let connection = Connection::new(socket);

            // shutdown_complete_tx 的 ownership 是 handler，当异步任务完成时，
            // handler 被释放，shutdown_complete_tx 也被释放
            // shutdown_complete_tx 是一个 sender，当释放一个 sender 时，会
            // 通知它的「接收者」
            let mut handler = Handler::new(shutdown, connection, fd, cli, shutdown_complete_tx);

            if let Err(err) = handler.process().await {
                error!("this client has an error, disconnect it {}!", err);
            }
        });
    }
}

pub async fn run(listener: TcpListener, shutdown: impl Future) -> Result<()> {
    // 创建一个大小为 1 的 广播型 channel：当要 shutdown 整个 server 时，
    // 对所有的异步 tasks 进行广播现在要 Shutdown
    // 所有的异步任务接收到 shutdown 通知后，从异步任务循环中退出
    let (notify_shutdown, _) = broadcast::channel(1);

    let (shutdown_complete_tx, mut shutdown_complete_rx) = mpsc::channel(1);

    tokio::select! {
        resp = loop_on_listener(listener, &notify_shutdown, &shutdown_complete_tx) => {
            if let Err(e) = resp {
                error!("the server on error: {}", e);
            }
        }
        _ = shutdown => {
            warn!("the server shutdown");
        }
    }

    // 当要 shutdown 时，drop 这个 channel 的发送者，这样所有的接收者会接收到一个 Err 消息作为
    // shutdown 的通知
    drop(notify_shutdown);
    drop(shutdown_complete_tx);

    // 等待所有的异步任务完成收尾工作
    shutdown_complete_rx.recv().await;

    Ok(())
}
