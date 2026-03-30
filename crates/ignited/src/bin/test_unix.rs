use axum::Router;
use hyper_util::rt::{TokioExecutor, TokioIo};
use hyper_util::server::conn::auto;
use tokio::net::UnixListener;
use tower_service::Service;

#[tokio::main]
async fn main() {
    let app = Router::new().route("/", axum::routing::get(|| async { "Hello" }));
    let path = "/tmp/test.sock";
    let _ = std::fs::remove_file(path);
    let listener = UnixListener::bind(path).unwrap();

    loop {
        let (stream, _) = listener.accept().await.unwrap();
        let io = TokioIo::new(stream);
        let tower_service = app.clone();
        
        tokio::spawn(async move {
            let hyper_service = hyper::service::service_fn(move |request: axum::extract::Request<hyper::body::Incoming>| {
                tower_service.clone().call(request)
            });

            if let Err(err) = auto::Builder::new(TokioExecutor::new())
                .serve_connection(io, hyper_service)
                .await
            {
                eprintln!("Error: {:?}", err);
            }
        });
    }
}
