use anyhow::Result;
use axum::{
    extract::{Query, State},
    http::{header, StatusCode},
    response::Response,
    Json,
};
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row};
use uuid::Uuid;

use crate::session::{
    generate_code_verifier, pkce_challenge, sign_session_jwt, verify_session_jwt,
};

// ── Config ────────────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct OidcConfig {
    /// External issuer URL — browser-facing, used in the authorization redirect.
    /// e.g. http://localhost:8082
    pub issuer: String,
    /// Internal base URL for server-to-server Zitadel API calls.
    /// e.g. http://zitadel:8080  (must include Host: localhost header — see below)
    pub api_base: String,
    pub client_id: String,
    pub redirect_uri: String,
    pub session_secret: String,
    pub dev_mode: bool,
}

#[derive(Clone)]
pub struct OidcState {
    pub db: PgPool,
    pub config: OidcConfig,
}

// ── DB helpers ────────────────────────────────────────────────────────────────

/// Create or update a user row and return the Observable user UUID.
pub async fn upsert_user(
    pool: &PgPool,
    idp_subject: &str,
    email: &str,
    name: Option<&str>,
) -> Result<Uuid> {
    let row = sqlx::query(
        r#"
        INSERT INTO users (idp_subject, email, name)
        VALUES ($1, $2, $3)
        ON CONFLICT (idp_subject) DO UPDATE
            SET email = EXCLUDED.email,
                name  = EXCLUDED.name,
                updated_at = now()
        RETURNING id
        "#,
    )
    .bind(idp_subject)
    .bind(email)
    .bind(name)
    .fetch_one(pool)
    .await?;
    Ok(row.try_get("id")?)
}

/// Insert or update the user's role for a given tenant.
pub async fn upsert_user_tenant_role(
    pool: &PgPool,
    user_id: Uuid,
    tenant_id: Uuid,
    role: &str,
) -> Result<()> {
    sqlx::query(
        r#"
        INSERT INTO user_tenant_roles (user_id, tenant_id, role)
        VALUES ($1, $2, $3)
        ON CONFLICT (user_id, tenant_id) DO UPDATE SET role = EXCLUDED.role
        "#,
    )
    .bind(user_id)
    .bind(tenant_id)
    .bind(role)
    .execute(pool)
    .await?;
    Ok(())
}

/// Create a session row and return its UUID.
pub async fn create_session(
    pool: &PgPool,
    user_id: Uuid,
    tenant_id: Uuid,
    environment: &str,
) -> Result<Uuid> {
    let expires_at = chrono::Utc::now() + chrono::Duration::hours(1);
    let row = sqlx::query(
        r#"
        INSERT INTO user_sessions (user_id, tenant_id, environment, expires_at)
        VALUES ($1, $2, $3, $4)
        RETURNING id
        "#,
    )
    .bind(user_id)
    .bind(tenant_id)
    .bind(environment)
    .bind(expires_at)
    .fetch_one(pool)
    .await?;
    Ok(row.try_get("id")?)
}

/// Return all tenant memberships for a user as (tenant_id, role) pairs.
pub async fn list_user_tenants(pool: &PgPool, user_id: Uuid) -> Result<Vec<(Uuid, String)>> {
    let rows = sqlx::query("SELECT tenant_id, role FROM user_tenant_roles WHERE user_id = $1")
        .bind(user_id)
        .fetch_all(pool)
        .await?;
    Ok(rows
        .into_iter()
        .map(|r| {
            let tid: Uuid = r.try_get("tenant_id").unwrap();
            let role: String = r.try_get("role").unwrap();
            (tid, role)
        })
        .collect())
}

// ── HTTP handlers ─────────────────────────────────────────────────────────────

/// GET /v1/auth/login
/// Builds PKCE challenge, stores verifier in a short-lived cookie, and
/// redirects to Zitadel's authorization endpoint.
pub async fn login_handler(State(state): State<OidcState>) -> Response {
    let verifier = generate_code_verifier();
    let challenge = pkce_challenge(&verifier);
    let state_param = generate_code_verifier();

    let auth_url = format!(
        "{}/oauth/v2/authorize\
         ?client_id={}\
         &redirect_uri={}\
         &response_type=code\
         &scope=openid+profile+email\
         &code_challenge={}\
         &code_challenge_method=S256\
         &state={}",
        state.config.issuer,
        urlencoding(&state.config.client_id),
        urlencoding(&state.config.redirect_uri),
        urlencoding(&challenge),
        urlencoding(&state_param),
    );

    let set_cv = format!("pkce_cv={verifier}; HttpOnly; SameSite=Lax; Path=/; Max-Age=300");
    let set_state =
        format!("oauth_state={state_param}; HttpOnly; SameSite=Lax; Path=/; Max-Age=300");

    Response::builder()
        .status(StatusCode::FOUND)
        .header(header::LOCATION, auth_url)
        .header(header::SET_COOKIE, set_cv)
        .header(header::SET_COOKIE, set_state)
        .body(axum::body::Body::empty())
        .unwrap()
}

#[derive(Deserialize)]
pub struct CallbackParams {
    pub code: String,
    pub state: String,
}

/// GET /v1/auth/callback
/// Exchanges the authorization code, upserts the user, issues a session JWT.
pub async fn callback_handler(
    State(state): State<OidcState>,
    Query(params): Query<CallbackParams>,
    headers: axum::http::HeaderMap,
) -> Result<Response, StatusCode> {
    let verifier = extract_cookie(&headers, "pkce_cv").ok_or(StatusCode::BAD_REQUEST)?;

    let cookie_state = extract_cookie(&headers, "oauth_state").ok_or(StatusCode::BAD_REQUEST)?;

    if cookie_state != params.state {
        return Err(StatusCode::BAD_REQUEST);
    }

    // Exchange code for tokens at Zitadel (server-to-server, internal URL).
    // Host header identifies the Zitadel instance (ExternalDomain).
    let zitadel_host = state
        .config
        .issuer
        .trim_start_matches("http://")
        .trim_start_matches("https://")
        .split(':')
        .next()
        .unwrap_or("localhost")
        .to_owned();
    let token_resp = reqwest::Client::new()
        .post(format!("{}/oauth/v2/token", state.config.api_base))
        .header("Host", &zitadel_host)
        .form(&[
            ("grant_type", "authorization_code"),
            ("code", &params.code),
            ("redirect_uri", &state.config.redirect_uri),
            ("client_id", &state.config.client_id),
            ("code_verifier", &verifier),
        ])
        .send()
        .await
        .map_err(|_| StatusCode::BAD_GATEWAY)?
        .json::<serde_json::Value>()
        .await
        .map_err(|_| StatusCode::BAD_GATEWAY)?;

    let access_token = token_resp["access_token"]
        .as_str()
        .ok_or(StatusCode::UNAUTHORIZED)?
        .to_owned();

    // Fetch user info from Zitadel (server-to-server, internal URL).
    let userinfo = reqwest::Client::new()
        .get(format!("{}/oidc/v1/userinfo", state.config.api_base))
        .header("Host", &zitadel_host)
        .bearer_auth(&access_token)
        .send()
        .await
        .map_err(|_| StatusCode::BAD_GATEWAY)?
        .json::<serde_json::Value>()
        .await
        .map_err(|_| StatusCode::BAD_GATEWAY)?;

    let sub = userinfo["sub"]
        .as_str()
        .ok_or(StatusCode::UNAUTHORIZED)?
        .to_owned();
    let email = userinfo["email"].as_str().unwrap_or("").to_owned();
    let name = userinfo["name"].as_str().map(ToOwned::to_owned);

    let user_id = upsert_user(&state.db, &sub, &email, name.as_deref())
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let mut tenants = list_user_tenants(&state.db, user_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Dev mode: first login has no role yet — seed tenant_admin on dev-tenant automatically.
    if tenants.is_empty() && state.config.dev_mode {
        let dev_tenant = uuid::Uuid::parse_str("00000000-0000-0000-0000-000000000002")
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        upsert_user_tenant_role(&state.db, user_id, dev_tenant, "tenant_admin")
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        tenants = vec![(dev_tenant, "tenant_admin".to_string())];
    }

    let (tenant_id, role) = tenants.into_iter().next().ok_or(StatusCode::FORBIDDEN)?;

    let env_row = sqlx::query_scalar::<_, String>(
        "SELECT DISTINCT environment FROM api_keys WHERE tenant_id = $1 AND environment != '' LIMIT 1",
    )
    .bind(tenant_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let environment = env_row.unwrap_or_else(|| "default".to_string());

    let session_id = create_session(&state.db, user_id, tenant_id, &environment)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    crate::audit::write(
        &state.db,
        &crate::audit::AuditEntry::login(sub.clone(), tenant_id),
    )
    .await;

    let jwt = sign_session_jwt(
        &state.config.session_secret,
        user_id,
        tenant_id,
        &role,
        &environment,
        session_id,
    )
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let secure_attr = if state.config.dev_mode {
        String::new()
    } else {
        "; Secure".to_string()
    };
    let set_session = format!(
        "session={}; HttpOnly{}; SameSite=Strict; Path=/; Max-Age=3600",
        jwt, secure_attr
    );

    Ok(Response::builder()
        .status(StatusCode::FOUND)
        .header(header::LOCATION, "/")
        .header(header::SET_COOKIE, set_session)
        .header(header::SET_COOKIE, "pkce_cv=; Max-Age=0; Path=/")
        .header(header::SET_COOKIE, "oauth_state=; Max-Age=0; Path=/")
        .body(axum::body::Body::empty())
        .unwrap())
}

/// POST /v1/auth/logout
/// Revokes active sessions for the current user+tenant and clears the cookie.
pub async fn logout_handler(
    State(state): State<OidcState>,
    headers: axum::http::HeaderMap,
) -> Response {
    if let Some(token) = extract_cookie(&headers, "session") {
        if let Ok(claims) = verify_session_jwt(&state.config.session_secret, &token) {
            if let (Ok(user_id), Ok(tenant_id)) =
                (Uuid::parse_str(&claims.sub), Uuid::parse_str(&claims.tid))
            {
                let _ = sqlx::query(
                    "UPDATE user_sessions SET revoked_at = now() \
                     WHERE user_id = $1 AND tenant_id = $2 AND revoked_at IS NULL",
                )
                .bind(user_id)
                .bind(tenant_id)
                .execute(&state.db)
                .await;

                crate::audit::write(
                    &state.db,
                    &crate::audit::AuditEntry::logout(claims.sub.clone(), tenant_id),
                )
                .await;
            }
        }
    }

    Response::builder()
        .status(StatusCode::FOUND)
        .header(header::LOCATION, "/login")
        .header(header::SET_COOKIE, "session=; Max-Age=0; Path=/")
        .body(axum::body::Body::empty())
        .unwrap()
}

#[derive(Serialize)]
pub struct MeResponse {
    pub user_id: String,
    pub email: String,
    pub tenants: Vec<TenantMembership>,
}

#[derive(Serialize)]
pub struct TenantMembership {
    pub tenant_id: String,
    pub role: String,
}

/// GET /v1/auth/me
/// Returns the current user's identity and tenant memberships.
pub async fn me_handler(
    State(state): State<OidcState>,
    headers: axum::http::HeaderMap,
) -> Result<Json<MeResponse>, StatusCode> {
    let token = extract_cookie(&headers, "session").ok_or(StatusCode::UNAUTHORIZED)?;
    let claims = verify_session_jwt(&state.config.session_secret, &token)
        .map_err(|_| StatusCode::UNAUTHORIZED)?;

    let user_id = Uuid::parse_str(&claims.sub).map_err(|_| StatusCode::UNAUTHORIZED)?;

    let email: String = sqlx::query_scalar("SELECT email FROM users WHERE id = $1")
        .bind(user_id)
        .fetch_optional(&state.db)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::UNAUTHORIZED)?;

    let tenants = list_user_tenants(&state.db, user_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(MeResponse {
        user_id: claims.sub,
        email,
        tenants: tenants
            .into_iter()
            .map(|(tid, role)| TenantMembership {
                tenant_id: tid.to_string(),
                role,
            })
            .collect(),
    }))
}

// ── Internal endpoint ─────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct ValidateSessionRequest {
    pub session_token: String,
}

#[derive(Serialize)]
pub struct ValidateSessionResponse {
    pub user_id: String,
    pub tenant_id: String,
    pub role: String,
    pub environment: String,
}

/// POST /internal/validate-session
/// Called by query-api and other internal services to validate a session JWT.
pub async fn validate_session_handler(
    State(state): State<OidcState>,
    Json(req): Json<ValidateSessionRequest>,
) -> Result<Json<ValidateSessionResponse>, StatusCode> {
    let claims = verify_session_jwt(&state.config.session_secret, &req.session_token)
        .map_err(|_| StatusCode::UNAUTHORIZED)?;

    let nonce_uuid = uuid::Uuid::parse_str(&claims.nonce).map_err(|_| StatusCode::UNAUTHORIZED)?;

    let user_id = Uuid::parse_str(&claims.sub).map_err(|_| StatusCode::UNAUTHORIZED)?;
    let _tenant_id = Uuid::parse_str(&claims.tid).map_err(|_| StatusCode::UNAUTHORIZED)?;

    let active: bool = sqlx::query_scalar(
        r#"
        SELECT EXISTS (
            SELECT 1 FROM user_sessions 
            WHERE id = $1 AND user_id = $2 AND revoked_at IS NULL AND expires_at > now()
        )
        "#,
    )
    .bind(nonce_uuid)
    .bind(user_id)
    .fetch_one(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if !active {
        return Err(StatusCode::UNAUTHORIZED);
    }

    Ok(Json(ValidateSessionResponse {
        user_id: claims.sub,
        tenant_id: claims.tid,
        role: claims.role,
        environment: claims.env,
    }))
}

// ── Utilities ─────────────────────────────────────────────────────────────────

pub fn extract_cookie(headers: &axum::http::HeaderMap, name: &str) -> Option<String> {
    let cookie_header = headers.get(header::COOKIE)?.to_str().ok()?;
    cookie_header.split(';').map(str::trim).find_map(|part| {
        let (k, v) = part.split_once('=')?;
        if k.trim() == name {
            Some(v.trim().to_owned())
        } else {
            None
        }
    })
}

fn urlencoding(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' => c.to_string(),
            _ => format!("%{:02X}", c as u32),
        })
        .collect()
}
