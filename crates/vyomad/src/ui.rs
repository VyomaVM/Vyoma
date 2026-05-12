use rust_embed::RustEmbed;
use axum::{
    extract::State,
    response::{IntoResponse, Response},
    http::{header, Uri, StatusCode},
    body::Body,
};
use crate::state::AppState;

#[derive(RustEmbed)]
#[folder = "../../ui/dist"]
pub struct Assets;

fn inject_token(html: String, token: &str) -> String {
    let meta_tag = format!(r#"<meta name="vyoma-api-token" content="{}">"#, token);
    html.replace("</head>", &format!("{}</head>", meta_tag))
}

pub async fn ui_handler(
    State(state): State<AppState>,
    uri: Uri,
) -> impl IntoResponse {
    let path = uri.path().trim_start_matches('/');
    let path = if path.is_empty() { "index.html" } else { path };

    let is_html = path.ends_with(".html") || path == "index.html";
    let should_inject = is_html && state.api_token.is_some();

    match Assets::get(path) {
        Some(content) => {
            let mime = mime_guess::from_path(path).first_or_octet_stream();
            
            let body = if should_inject {
                let token = state.api_token.as_ref().unwrap();
                let html = String::from_utf8_lossy(&content.data.to_vec()).into_owned();
                let modified = inject_token(html, token);
                Body::from(modified)
            } else {
                Body::from(content.data.to_vec())
            };
            
            Response::builder()
                .header(header::CONTENT_TYPE, mime.as_ref())
                .body(body)
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
                    let body = if state.api_token.is_some() {
                        let token = state.api_token.as_ref().unwrap();
                        let html = String::from_utf8_lossy(&content.data.to_vec()).into_owned();
                        let modified = inject_token(html, token);
                        Body::from(modified)
                    } else {
                        Body::from(content.data.to_vec())
                    };
                    
                    Response::builder()
                        .header(header::CONTENT_TYPE, "text/html")
                        .body(body)
                        .unwrap()
                },
                None => (StatusCode::NOT_FOUND, "index.html missing").into_response(),
            }
        }
    }
}