use axum::{
    extract::Request,
    http::{header, StatusCode},
    middleware::Next,
    response::Response,
};
use crate::state::AppState;

/// Authentication middleware
/// Checks for valid token in Authorization header or cookie
pub async fn auth_middleware(
    state: axum::extract::State<AppState>,
    request: Request,
    next: Next,
) -> Response {
    // If no token is configured, skip authentication
    let token = match &state.api_token {
        Some(t) => t.clone(),
        None => return next.run(request).await,
    };

    // Check Authorization header first (Bearer token)
    if let Some(auth_header) = request.headers().get(header::AUTHORIZATION) {
        if let Ok(auth_str) = auth_header.to_str() {
            if auth_str.starts_with("Bearer ") {
                let provided_token = &auth_str[7..];
                if provided_token == token {
                    return next.run(request).await;
                }
            }
        }
    }

    // Check cookie as fallback
    if let Some(cookie) = request.headers().get(header::COOKIE) {
        if let Ok(cookie_str) = cookie.to_str() {
            for pair in cookie_str.split(';') {
                let pair = pair.trim();
                if pair.starts_with("vyoma_token=") {
                    let cookie_token = &pair[12..];
                    if cookie_token == token {
                        return next.run(request).await;
                    }
                }
            }
        }
    }

    // No valid token found - return 401
    Response::builder()
        .status(StatusCode::UNAUTHORIZED)
        .header(header::CONTENT_TYPE, "application/json")
        .body(axum::body::Body::from(r#"{"error":"Unauthorized: valid token required"}"#))
        .unwrap()
}