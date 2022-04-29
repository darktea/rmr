use tokio::signal;

#[tokio::main]
async fn main() {
    // install global collector configured based on RUST_LOG env var.
    tracing_subscriber::fmt::init();

    rmr::server::run(signal::ctrl_c()).await.unwrap();
}
