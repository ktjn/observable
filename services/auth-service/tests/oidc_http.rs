use std::sync::Arc;

use auth_service::{
    observability::AuthServiceMetrics,
    oidc::{OidcConfig, OidcState, callback_handler, login_handler, logout_handler},
};
use axum::{
    Router,
    body::Body,
    http::{Method, Request, StatusCode, header},
    routing::{get, post},
};
use sqlx::PgPool;
use tower::ServiceExt;
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{method, path},
};

fn state(api_base: &str, dev_mode: bool) -> OidcState {
    OidcState {
        db: PgPool::connect_lazy("postgres://postgres:postgres@127.0.0.1:1/observable").unwrap(),
        config: OidcConfig {
            issuer: "https://identity.example.com".to_string(),
            api_base: api_base.to_string(),
            client_id: "observable-client".to_string(),
            redirect_uri: "https://observable.example.com/v1/auth/callback".to_string(),
            session_secret: "test-session-secret-with-at-least-32-bytes".to_string(),
            dev_mode,
        },
        http_client: reqwest::Client::new(),
        metrics: Arc::new(AuthServiceMetrics::new()),
    }
}

fn app(state: OidcState) -> Router {
    Router::new()
        .route("/v1/auth/login", get(login_handler))
        .route("/v1/auth/callback", get(callback_handler))
        .with_state(state)
}

fn cookies(response: &axum::response::Response) -> Vec<&str> {
    response
        .headers()
        .get_all(header::SET_COOKIE)
        .iter()
        .map(|value| value.to_str().unwrap())
        .collect()
}

fn has_no_session_cookie(response: &axum::response::Response) -> bool {
    cookies(response)
        .iter()
        .all(|cookie| !cookie.starts_with("session="))
}

#[tokio::test]
async fn login_redirect_contains_pkce_state_and_hardened_transient_cookies() {
    let response = app(state("http://127.0.0.1:1", false))
        .oneshot(
            Request::builder()
                .uri("/v1/auth/login")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::FOUND);

    let location = response
        .headers()
        .get(header::LOCATION)
        .unwrap()
        .to_str()
        .unwrap();
    assert!(location.starts_with("https://identity.example.com/oauth/v2/authorize?"));
    assert!(location.contains("client_id=observable-client"));
    assert!(location.contains("response_type=code"));
    assert!(location.contains("code_challenge_method=S256"));
    assert!(location.contains("code_challenge="));
    assert!(location.contains("state="));

    let cookies = cookies(&response);
    assert_eq!(cookies.len(), 2);
    assert!(cookies.iter().any(|cookie| cookie.starts_with("pkce_cv=")));
    assert!(
        cookies
            .iter()
            .any(|cookie| cookie.starts_with("oauth_state="))
    );
    for cookie in cookies {
        assert!(cookie.contains("HttpOnly"));
        assert!(cookie.contains("SameSite=Lax"));
        assert!(cookie.contains("Path=/"));
        assert!(cookie.contains("Max-Age=300"));
    }
}

#[tokio::test]
async fn callback_without_pkce_cookie_redirects_without_issuing_session() {
    let response = app(state("http://127.0.0.1:1", false))
        .oneshot(
            Request::builder()
                .uri("/v1/auth/callback?code=code&state=expected")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::FOUND);
    assert_eq!(
        response.headers().get(header::LOCATION).unwrap(),
        "/login?error=session_expired"
    );
    assert!(has_no_session_cookie(&response));
}

#[tokio::test]
async fn callback_state_mismatch_redirects_without_issuing_session() {
    let response = app(state("http://127.0.0.1:1", false))
        .oneshot(
            Request::builder()
                .uri("/v1/auth/callback?code=code&state=received")
                .header(header::COOKIE, "pkce_cv=verifier; oauth_state=expected")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::FOUND);
    assert_eq!(
        response.headers().get(header::LOCATION).unwrap(),
        "/login?error=session_expired"
    );
    assert!(has_no_session_cookie(&response));
}

#[tokio::test]
async fn token_endpoint_outage_redirects_without_issuing_session() {
    let response = app(state("http://127.0.0.1:1", false))
        .oneshot(
            Request::builder()
                .uri("/v1/auth/callback?code=code&state=expected")
                .header(header::COOKIE, "pkce_cv=verifier; oauth_state=expected")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::FOUND);
    assert_eq!(
        response.headers().get(header::LOCATION).unwrap(),
        "/login?error=provider_error"
    );
    assert!(has_no_session_cookie(&response));
}

#[tokio::test]
async fn callback_without_oauth_state_cookie_redirects_without_issuing_session() {
    let response = app(state("http://127.0.0.1:1", false))
        .oneshot(
            Request::builder()
                .uri("/v1/auth/callback?code=code&state=expected")
                .header(header::COOKIE, "pkce_cv=verifier")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::FOUND);
    assert_eq!(
        response.headers().get(header::LOCATION).unwrap(),
        "/login?error=session_expired"
    );
    assert!(has_no_session_cookie(&response));
}

#[tokio::test]
async fn token_exchange_without_access_token_redirects_auth_failed() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/oauth/v2/token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "token_type": "Bearer"
        })))
        .mount(&mock_server)
        .await;

    let response = app(state(&mock_server.uri(), false))
        .oneshot(
            Request::builder()
                .uri("/v1/auth/callback?code=code&state=expected")
                .header(header::COOKIE, "pkce_cv=verifier; oauth_state=expected")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::FOUND);
    assert_eq!(
        response.headers().get(header::LOCATION).unwrap(),
        "/login?error=auth_failed"
    );
    assert!(has_no_session_cookie(&response));
}

#[tokio::test]
async fn userinfo_without_sub_redirects_auth_failed() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/oauth/v2/token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "access_token": "test-access-token",
            "token_type": "Bearer"
        })))
        .mount(&mock_server)
        .await;

    Mock::given(method("GET"))
        .and(path("/oidc/v1/userinfo"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "email": "user@example.com",
            "name": "Test User"
        })))
        .mount(&mock_server)
        .await;

    let response = app(state(&mock_server.uri(), false))
        .oneshot(
            Request::builder()
                .uri("/v1/auth/callback?code=code&state=expected")
                .header(header::COOKIE, "pkce_cv=verifier; oauth_state=expected")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::FOUND);
    assert_eq!(
        response.headers().get(header::LOCATION).unwrap(),
        "/login?error=auth_failed"
    );
    assert!(has_no_session_cookie(&response));
}

#[tokio::test]
async fn userinfo_endpoint_outage_redirects_provider_error() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/oauth/v2/token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "access_token": "test-access-token",
            "token_type": "Bearer"
        })))
        .mount(&mock_server)
        .await;

    // No userinfo mock — requests will return 404 which fails JSON parse

    let response = app(state(&mock_server.uri(), false))
        .oneshot(
            Request::builder()
                .uri("/v1/auth/callback?code=code&state=expected")
                .header(header::COOKIE, "pkce_cv=verifier; oauth_state=expected")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::FOUND);
    let location = response
        .headers()
        .get(header::LOCATION)
        .unwrap()
        .to_str()
        .unwrap();
    assert!(
        location.contains("error="),
        "should redirect with error param"
    );
    assert!(has_no_session_cookie(&response));
}

#[tokio::test]
async fn logout_clears_session_cookie_and_redirects_to_login() {
    let app = Router::new()
        .route("/v1/auth/logout", post(logout_handler))
        .with_state(state("http://127.0.0.1:1", false));

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/v1/auth/logout")
                .header(header::COOKIE, "session=invalid-jwt")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::FOUND);
    assert_eq!(response.headers().get(header::LOCATION).unwrap(), "/login");
    let cookies = cookies(&response);
    assert!(
        cookies
            .iter()
            .any(|c| c.starts_with("session=;") && c.contains("Max-Age=0")),
        "session cookie must be cleared"
    );
}

#[tokio::test]
async fn login_redirect_in_dev_mode_uses_correct_issuer() {
    let response = app(state("http://127.0.0.1:1", true))
        .oneshot(
            Request::builder()
                .uri("/v1/auth/login")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::FOUND);
    let location = response
        .headers()
        .get(header::LOCATION)
        .unwrap()
        .to_str()
        .unwrap();
    assert!(location.starts_with("https://identity.example.com/oauth/v2/authorize?"));
}
