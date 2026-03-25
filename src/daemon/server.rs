use anyhow::Result;
use axum::{
    extract::{Request, State},
    http::{HeaderMap, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Json, Response},
    routing::get,
    Router,
};
use serde::Serialize;
use std::net::SocketAddr;
use tokio::net::TcpListener;

use super::clipboard::{self, ClipboardCache};

#[derive(Clone)]
struct AppState {
    cache: ClipboardCache,
    token: String,
}

#[derive(Serialize)]
struct HealthResponse {
    status: String,
}

#[derive(Serialize)]
struct MetadataResponse {
    format: String,
    width: u32,
    height: u32,
    size_bytes: usize,
    sha256: String,
}

async fn auth_middleware(
    State(state): State<AppState>,
    headers: HeaderMap,
    request: Request,
    next: Next,
) -> Response {
    let authorized = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| {
            v.strip_prefix("Bearer ").is_some_and(|token| {
                use subtle::ConstantTimeEq;
                token.as_bytes().ct_eq(state.token.as_bytes()).into()
            })
        });

    if authorized {
        next.run(request).await
    } else {
        StatusCode::UNAUTHORIZED.into_response()
    }
}

async fn health_handler() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok".into(),
    })
}

async fn clipboard_handler(State(state): State<AppState>) -> Response {
    match clipboard::read_clipboard(&state.cache) {
        Ok(Some(img)) => (StatusCode::OK, [("content-type", "image/png")], img.png).into_response(),
        Ok(None) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => {
            tracing::error!("clipboard read error: {e}");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

async fn metadata_handler(State(state): State<AppState>) -> Response {
    match clipboard::read_clipboard(&state.cache) {
        Ok(Some(img)) => Json(MetadataResponse {
            format: "png".into(),
            width: img.width,
            height: img.height,
            size_bytes: img.png.len(),
            sha256: hex::encode(img.hash),
        })
        .into_response(),
        Ok(None) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => {
            tracing::error!("clipboard metadata error: {e}");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

pub fn create_router(token: &str) -> Router {
    let state = AppState {
        cache: clipboard::new_cache(),
        token: token.to_string(),
    };

    let authed_routes = Router::new()
        .route("/clipboard", get(clipboard_handler))
        .route("/clipboard/metadata", get(metadata_handler))
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            auth_middleware,
        ));

    Router::new()
        .route("/health", get(health_handler))
        .merge(authed_routes)
        .with_state(state)
}

pub async fn run(port: u16, token: &str) -> Result<()> {
    let app = create_router(token);

    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let listener = TcpListener::bind(addr).await?;
    tracing::info!("daemon listening on {addr}");

    axum::serve(listener, app).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    fn make_request(method: &str, uri: &str, auth: Option<&str>) -> axum::http::Request<Body> {
        let mut builder = axum::http::Request::builder().method(method).uri(uri);
        if let Some(token) = auth {
            builder = builder.header("Authorization", format!("Bearer {token}"));
        }
        builder.body(Body::empty()).unwrap()
    }

    #[tokio::test]
    async fn health_endpoint_returns_ok_without_auth() {
        let app = create_router("test-token");
        let req = make_request("GET", "/health", None);
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let body = resp.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["status"], "ok");
    }

    #[tokio::test]
    async fn clipboard_endpoint_requires_auth() {
        let app = create_router("secret-token");
        let req = make_request("GET", "/clipboard", None);
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn clipboard_endpoint_rejects_wrong_token() {
        let app = create_router("correct-token");
        let req = make_request("GET", "/clipboard", Some("wrong-token"));
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn metadata_endpoint_requires_auth() {
        let app = create_router("secret-token");
        let req = make_request("GET", "/clipboard/metadata", None);
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn metadata_endpoint_rejects_wrong_token() {
        let app = create_router("correct-token");
        let req = make_request("GET", "/clipboard/metadata", Some("bad-token"));
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn nonexistent_route_returns_404() {
        let app = create_router("tok");
        let req = make_request("GET", "/nonexistent", Some("tok"));
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn health_endpoint_post_returns_method_not_allowed() {
        let app = create_router("tok");
        let req = make_request("POST", "/health", None);
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::METHOD_NOT_ALLOWED);
    }

    #[tokio::test]
    async fn auth_rejects_non_bearer_scheme() {
        let app = create_router("my-token");
        let req = axum::http::Request::builder()
            .method("GET")
            .uri("/clipboard")
            .header("Authorization", "Basic my-token")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }
}
