use tokio::net::TcpListener;

#[tokio::main]
async fn main() {
    // 绑定 TCP listener
    let listener = TcpListener::bind("127.0.0.1:6379").await.unwrap();

    println!("Listening");

    // 进入主循环
    loop {
        // 进行 accept 操作
        // 如果 accept 到新的 socket，返回这个 socket；
        // TODO: 如果遇到 Err，server 进入 shutdown 流程
        let (socket, _) = listener.accept().await.unwrap();

        println!("Accepted");

        // 为每一条连接都生成一个新的任务，
        // `socket` 的所有权将被移动到新的任务中，并在那里进行处理
        tokio::spawn(async move {
            if let Err(err) = rmr::process(socket).await {
                println!("bad connect {}", err);
            }
        });
    }
}
