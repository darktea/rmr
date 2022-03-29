use std::os::unix::prelude::AsRawFd;

use log::error;
use log::warn;
use tokio::net::TcpListener;

#[tokio::main]
async fn main() {
    // install global collector configured based on RUST_LOG env var.
    tracing_subscriber::fmt::init();

    // 绑定 TCP listener
    let listener = TcpListener::bind("127.0.0.1:6379").await.unwrap();

    warn!("the server starts to listen on PORT: 6379");

    // 进入主循环
    loop {
        // 进行 accept 操作
        // 如果 accept 到新的 socket，返回这个 socket；
        // TODO: 如果遇到 Err，server 进入 shutdown 流程
        let (socket, _) = listener.accept().await.unwrap();

        // 为每一条连接都生成一个新的任务，
        // `socket` 的所有权将被移动到新的任务中，并在那里进行处理
        tokio::spawn(async move {
            let fd = socket.as_raw_fd();

            if let Err(err) = rmr::process(socket, fd).await {
                error!("this client has an error, disconnect it {}!", err);
            }
        });
    }
}
