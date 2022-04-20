pub mod cmd;
pub mod frame;
pub mod parser;

pub use frame::Frame;

pub mod connection;
pub use connection::Connection;

pub use parser::Parser;

use tokio::net::TcpStream;
use tracing::{info, instrument};

use snafu::{prelude::*, ResultExt};

#[derive(Debug, Snafu)]
pub enum Error {
    #[snafu(display("failed on network {}", source))]
    ConnectError { source: connection::Error },
    #[snafu(display("failed for command run error{}", source))]
    CommandError { source: cmd::Error },
}

pub type Result<T, E = Error> = std::result::Result<T, E>;

// 针对每个连接，进行无限循环，直到：出错（返回 Err）或者客户端关闭连接（返回一个 Ok）
#[instrument(skip(socket))]
pub async fn process(socket: TcpStream, fd: i32) -> Result<()> {
    info!("the server accepted a new client. fd is: {}", fd);

    let mut connection = Connection::new(socket);

    loop {
        // read_frame 返回 Err 的话，透传 Err 给 process 的调用者
        let maybe_frame = connection.read_frame().await.context(ConnectSnafu)?;

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

        let cmd = cmd::Command::from_frame(frame).context(CommandSnafu)?;
        info!("get first cmd: {:?}", cmd);

        cmd.apply(&mut connection).await.context(CommandSnafu)?;
    }
}
