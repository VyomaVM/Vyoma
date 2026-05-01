use rust_embed::RustEmbed;
use axum::{
    response::{IntoResponse, Response},
    http::{header, Uri, StatusCode},
    body::Body,
};

#[derive(RustEmbed)]
#[folder = "../../ui/dist"]
pub struct Assets;

pub async fn ui_handler(uri: Uri) -> impl IntoResponse {
    let path = uri.path().trim_start_matches('/');
    let path = if path.is_empty() { "index.html" } else { path };

    match Assets::get(path) {
        Some(content) => {
            let mime = mime_guess::from_path(path).first_or_octet_stream();
            Response::builder()
                .header(header::CONTENT_TYPE, mime.as_ref())
                .body(Body::from(content.data))
                .unwrap()
        }
        None => {
            // If explicit file extension request failed, 404.
            if path.contains('.') {
                return (StatusCode::NOT_FOUND, "404 Not Found").into_response();
            }
            // Otherwise fallback to index.html (SPA)
             match Assets::get("index.html") {
                Some(content) => {
                    Response::builder()
                        .header(header::CONTENT_TYPE, "text/html")
                        .body(Body::from(content.data))
                        .unwrap()
                },
                None => (StatusCode::NOT_FOUND, "index.html missing").into_response(),
            }
        }
    }
}
