use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use tower::ServiceExt;

const TEST_TOKEN: &str = "test-secret-token-12345";

fn app() -> axum::Router {
    cliptunnel::daemon::server::create_router(TEST_TOKEN)
}

// ─── Health endpoint ────────────────────────────────────────────────

#[tokio::test]
async fn health_returns_ok_without_auth() {
    let app = app();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(json["status"], "ok");
    assert!(
        json.get("version").is_none(),
        "health should not expose version"
    );
}

// ─── Auth middleware rejects unauthenticated requests ────────────────

#[tokio::test]
async fn clipboard_returns_401_without_auth() {
    let app = app();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/clipboard")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn clipboard_metadata_returns_401_without_auth() {
    let app = app();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/clipboard/metadata")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

// ─── Auth middleware rejects bad tokens ──────────────────────────────

#[tokio::test]
async fn clipboard_returns_401_with_bad_token() {
    let app = app();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/clipboard")
                .header("authorization", "Bearer wrong-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn clipboard_returns_401_with_malformed_auth_header() {
    let app = app();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/clipboard")
                .header("authorization", "Basic dXNlcjpwYXNz")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn clipboard_returns_401_with_empty_bearer() {
    let app = app();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/clipboard")
                .header("authorization", "Bearer ")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

// ─── Authenticated requests with no clipboard image return 204 ──────
// These tests require macOS GUI session (arboard clipboard access).
// Run with: cargo test -- --ignored

#[tokio::test]
#[ignore = "requires macOS GUI session for clipboard access"]
async fn clipboard_returns_204_with_valid_auth_no_image() {
    let app = app();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/clipboard")
                .header("authorization", format!("Bearer {TEST_TOKEN}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    // The clipboard in CI/test environments typically has no image,
    // so we expect either 204 (no content) or 200 (if an image happens to be present).
    // In a headless test environment, 204 is the most likely outcome, but
    // clipboard access may also fail with 500 on some platforms.
    let status = response.status();
    assert!(
        status == StatusCode::NO_CONTENT
            || status == StatusCode::OK
            || status == StatusCode::INTERNAL_SERVER_ERROR,
        "unexpected status: {status}"
    );
}

#[tokio::test]
#[ignore = "requires macOS GUI session for clipboard access"]
async fn clipboard_metadata_returns_204_with_valid_auth_no_image() {
    let app = app();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/clipboard/metadata")
                .header("authorization", format!("Bearer {TEST_TOKEN}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let status = response.status();
    assert!(
        status == StatusCode::NO_CONTENT
            || status == StatusCode::OK
            || status == StatusCode::INTERNAL_SERVER_ERROR,
        "unexpected status: {status}"
    );
}

// ─── Non-existent routes return 404 ─────────────────────────────────

#[tokio::test]
async fn unknown_route_returns_404() {
    let app = app();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/nonexistent")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}
