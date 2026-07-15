use std::sync::Arc;

use auth_service::{
    observability::AuthServiceMetrics,
    oidc::{OidcConfig, OidcState, callback_handler, validate_session_handler},
    session::verify_session_jwt,
};
use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode, header},
    routing::{get, post},
};
use sqlx::PgPool;
use std::path::Path;
use testcontainers::{ImageExt, runners::AsyncRunner};
use testcontainers_modules::postgres::Postgres;
use tower::ServiceExt;
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{method, path},
};

async fn apply_migrations(pool: &PgPool) {
    let migrations_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("migrations/postgres");

    let mut entries: Vec<_> = std::fs::read_dir(&migrations_dir)
        .expect("migrations/postgres must exist")
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|x| x == "sql"))
        .collect();
    entries.sort_by_key(|e| e.file_name());

    for entry in entries {
        let sql = std::fs::read_to_string(entry.path()).expect("readable migration");
        sqlx::raw_sql(sqlx::AssertSqlSafe(sql))
            .execute(pool)
            .await
            .expect("migration applied");
    }
}

async fn start_pool() -> (PgPool, testcontainers::ContainerAsync<Postgres>) {
    let container = Postgres::default()
        .with_tag("17")
        .start()
        .await
        .expect("postgres container started");
    let port = container.get_host_port_ipv4(5432).await.unwrap();
    let url = format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres");
    let pool = PgPool::connect(&url).await.expect("pool connected");
    apply_migrations(&pool).await;
    (pool, container)
}

const SESSION_SECRET: &str = "test-session-secret-with-at-least-32-bytes";
const ZITADEL_SUB: &str = "zitadel-user-12345";

#[tokio::test]
async fn callback_happy_path_issues_session_jwt() {
    let (pool, _container) = start_pool().await;
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/oauth/v2/token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "access_token": "mock-access-token",
            "token_type": "Bearer"
        })))
        .mount(&mock_server)
        .await;

    Mock::given(method("GET"))
        .and(path("/oidc/v1/userinfo"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "sub": ZITADEL_SUB,
            "email": "test@example.com",
            "name": "Test User"
        })))
        .mount(&mock_server)
        .await;

    let state = OidcState {
        db: pool.clone(),
        config: OidcConfig {
            issuer: mock_server.uri(),
            api_base: mock_server.uri(),
            client_id: "test-client".to_string(),
            redirect_uri: "http://localhost:5173/auth/callback".to_string(),
            session_secret: SESSION_SECRET.to_string(),
            dev_mode: true,
        },
        http_client: reqwest::Client::new(),
        metrics: Arc::new(AuthServiceMetrics::new()),
    };

    let app = Router::new()
        .route("/v1/auth/callback", get(callback_handler))
        .with_state(state);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/v1/auth/callback?code=auth-code&state=expected")
                .header(header::COOKIE, "pkce_cv=verifier; oauth_state=expected")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::FOUND);
    assert_eq!(
        response.headers().get(header::LOCATION).unwrap(),
        "/",
        "successful callback should redirect to /"
    );

    let cookies: Vec<&str> = response
        .headers()
        .get_all(header::SET_COOKIE)
        .iter()
        .map(|v| v.to_str().unwrap())
        .collect();

    let session_cookie = cookies
        .iter()
        .find(|c| c.starts_with("session=") && !c.starts_with("session=;"))
        .expect("response must contain a session cookie");

    assert!(session_cookie.contains("HttpOnly"));
    assert!(session_cookie.contains("SameSite=Lax"));
    assert!(session_cookie.contains("Max-Age=604800"));

    let jwt = session_cookie
        .split(';')
        .next()
        .unwrap()
        .strip_prefix("session=")
        .unwrap();
    let claims = verify_session_jwt(SESSION_SECRET, jwt).expect("JWT must be valid");
    assert_eq!(claims.role, "tenant_admin");

    let transient_cleared: Vec<&&str> =
        cookies.iter().filter(|c| c.contains("Max-Age=0")).collect();
    assert!(
        transient_cleared.len() >= 2,
        "pkce_cv and oauth_state cookies must be cleared"
    );

    let user_exists: bool =
        sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM users WHERE email = 'test@example.com')")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(user_exists, "user must be upserted into the database");

    let session_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM user_sessions WHERE revoked_at IS NULL")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(session_count >= 1, "an active session must exist");
}

#[tokio::test]
async fn validate_session_handler_accepts_valid_jwt() {
    let (pool, _container) = start_pool().await;
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/oauth/v2/token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "access_token": "mock-access-token",
            "token_type": "Bearer"
        })))
        .mount(&mock_server)
        .await;

    Mock::given(method("GET"))
        .and(path("/oidc/v1/userinfo"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "sub": "validate-session-user",
            "email": "validate@example.com",
            "name": "Validate User"
        })))
        .mount(&mock_server)
        .await;

    let state = OidcState {
        db: pool.clone(),
        config: OidcConfig {
            issuer: mock_server.uri(),
            api_base: mock_server.uri(),
            client_id: "test-client".to_string(),
            redirect_uri: "http://localhost:5173/auth/callback".to_string(),
            session_secret: SESSION_SECRET.to_string(),
            dev_mode: true,
        },
        http_client: reqwest::Client::new(),
        metrics: Arc::new(AuthServiceMetrics::new()),
    };

    let callback_app = Router::new()
        .route("/v1/auth/callback", get(callback_handler))
        .with_state(state.clone());

    let callback_response = callback_app
        .oneshot(
            Request::builder()
                .uri("/v1/auth/callback?code=auth-code&state=expected")
                .header(header::COOKIE, "pkce_cv=verifier; oauth_state=expected")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let jwt = callback_response
        .headers()
        .get_all(header::SET_COOKIE)
        .iter()
        .find_map(|v| {
            let s = v.to_str().unwrap();
            if s.starts_with("session=") && !s.starts_with("session=;") {
                Some(
                    s.split(';')
                        .next()
                        .unwrap()
                        .strip_prefix("session=")
                        .unwrap()
                        .to_string(),
                )
            } else {
                None
            }
        })
        .expect("callback must issue a session JWT");

    let validate_app = Router::new()
        .route("/internal/validate-session", post(validate_session_handler))
        .with_state(state);

    let validate_response = validate_app
        .oneshot(
            Request::builder()
                .method(axum::http::Method::POST)
                .uri("/internal/validate-session")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    serde_json::to_string(&serde_json::json!({
                        "session_token": jwt
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(validate_response.status(), StatusCode::OK);
}

#[tokio::test]
async fn validate_session_rejects_forged_jwt() {
    let (pool, _container) = start_pool().await;

    let state = OidcState {
        db: pool,
        config: OidcConfig {
            issuer: "http://localhost:1".to_string(),
            api_base: "http://localhost:1".to_string(),
            client_id: "test-client".to_string(),
            redirect_uri: "http://localhost:5173/auth/callback".to_string(),
            session_secret: SESSION_SECRET.to_string(),
            dev_mode: true,
        },
        http_client: reqwest::Client::new(),
        metrics: Arc::new(AuthServiceMetrics::new()),
    };

    let forged_jwt = auth_service::session::sign_session_jwt(
        "wrong-secret-that-does-not-match-at-all",
        uuid::Uuid::new_v4(),
        uuid::Uuid::new_v4(),
        "tenant_admin",
        "production",
        uuid::Uuid::new_v4(),
    )
    .unwrap();

    let app = Router::new()
        .route("/internal/validate-session", post(validate_session_handler))
        .with_state(state);

    let response = app
        .oneshot(
            Request::builder()
                .method(axum::http::Method::POST)
                .uri("/internal/validate-session")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    serde_json::to_string(&serde_json::json!({
                        "session_token": forged_jwt
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}
