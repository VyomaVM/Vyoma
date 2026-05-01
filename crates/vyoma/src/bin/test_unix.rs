use reqwest::Client;
#[tokio::main]
async fn main() {
    let _client = Client::builder()
        .unix_socket("/tmp/x.sock").build()
        .unwrap(); // check if reqwest 0.11 builds
}
