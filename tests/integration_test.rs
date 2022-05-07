use mini_redis::client;
use tokio::net::TcpListener;
use tokio::signal;

#[tokio::test]
async fn test_on_mock_http() {
    let url = start_http_mock().await;

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    start_server(listener).await;

    let mut client = client::connect(addr).await.unwrap();
    let value = client.get(&url).await.unwrap().unwrap();
    assert!(value.len() > 0);
    assert_eq!(b"1.1.1.1", &value[..]);
}

// 启动 redis server
async fn start_server(listener: TcpListener) {
    tokio::spawn(async move {
        rmr::server::run(listener, signal::ctrl_c()).await.unwrap();
    });
}

// 启动 http server，并返回 http url
async fn start_http_mock() -> String {
    use httpmock::prelude::*;
    use serde_json::json;

    // mock a htpp server, and we can get json content from this http mock:
    // {"origin" : "1.1.1.1"}
    let json_key = "origin";
    let json_value = "1.1.1.1";

    // Start a lightweight mock server by async.
    let server = MockServer::start_async().await;

    // Create a mock on the server.
    let _ = server
        .mock_async(|when, then| {
            when.method(GET)
                .path("/translate")
                .query_param("word", "hello");
            then.status(200)
                .header("content-type", "application/json")
                .json_body(json!({ json_key: json_value }));
        })
        .await;

    server.url("/translate?word=hello")
}
