use log::warn;
use tokio::net::TcpListener;
use tokio::signal;

#[tokio::main]
async fn main() {
    // install global collector configured based on RUST_LOG env var.
    tracing_subscriber::fmt::init();

    let listener = TcpListener::bind("127.0.0.1:6379").await.unwrap();

    warn!("the server starts to listen on PORT: 6379");

    rmr::server::run(listener, signal::ctrl_c()).await.unwrap();
}
