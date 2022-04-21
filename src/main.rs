use reqwest::header;
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();
    let mut headers = header::HeaderMap::new();
    headers.insert("Accept", header::HeaderValue::from_static("text/plain"));
    headers.insert("User-Agent", header::HeaderValue::from_static("HTTPie/3.1.0"));

    let client = reqwest::Client::builder()
        .default_headers(headers)
        .timeout(Duration::from_secs(3))
        .connection_verbose(true)
        .build()?;

    let doge = client
        .get("http://pie.dev/get")
        .send()
        .await?
        .text()
        .await?;

    println!("Got {:#?}", doge);
    Ok(())
}
