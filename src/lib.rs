pub mod frame;

pub use frame::Frame;

pub mod connection;
pub use connection::Connection;
pub use connection::Error;

use tokio::net::TcpStream;
use tracing::{info, instrument};

// 针对每个连接，进行无限循环，直到：出错（返回 Err）或者客户端关闭连接（返回一个 Ok）
#[instrument(skip(socket))]
pub async fn process(socket: TcpStream, fd: i32) -> Result<(), Error> {
    info!("the server accepted a new client. fd is: {}", fd);

    let mut connection = Connection::new(socket);

    loop {
        // read_frame 返回 Err 的话，透传 Err 给 process 的调用者
        let maybe_frame = connection.read_frame().await?;

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
        let response = Frame::Simple("OK".to_string());
        // 如果 write_frame 出错，也会结束循环，抛出一个 IoFailed
        connection.write_frame(&response).await?;
        info!("sent response successfully: {:?}", response);
    }
}
