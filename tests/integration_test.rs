use mini_redis::client;
use tokio::net::TcpListener;
use tokio::signal;

#[tokio::test]
async fn it_adds_two() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    start_server(listener).await;

    let mut client = client::connect(addr).await.unwrap();
    let value = client.get("hello").await.unwrap().unwrap();
    assert!(value.len() > 0);

    assert_eq!(4, 4);
}

async fn start_server(listener: TcpListener) {
    tokio::spawn(async move {
        rmr::server::run(listener, signal::ctrl_c()).await.unwrap();
    });
}
