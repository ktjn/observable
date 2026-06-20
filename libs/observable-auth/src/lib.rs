//! Shared auth verification helpers used by services that sit behind
//! `auth-service`'s internal validation endpoints.
//!
//! This crate centralizes the logic that was previously duplicated across
//! `ingest-gateway` and `query-api`: calling `auth-service`'s
//! `/internal/validate` and `/internal/validate-session` endpoints, and
//! extracting auth-related headers from incoming requests.

use axum::http::StatusCode;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Context returned after a successful API-key validation.
#[derive(Clone, Debug)]
pub struct ApiKeyContext {
    pub tenant_id: Uuid,
    pub role: String,
    pub environment: String,
}

/// Context returned after a successful session validation.
#[derive(Clone, Debug)]
pub struct SessionContext {
    pub tenant_id: Uuid,
    pub user_id: Uuid,
    pub role: String,
}

/// Errors that can occur while verifying credentials against auth-service
/// or while extracting auth-related headers from a request.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AuthError {
    Unauthorized,
    Forbidden,
    ServiceUnavailable,
    Internal,
    BadRequest,
}

impl From<AuthError> for StatusCode {
    fn from(err: AuthError) -> Self {
        match err {
            AuthError::Unauthorized => StatusCode::UNAUTHORIZED,
            AuthError::Forbidden => StatusCode::FORBIDDEN,
            AuthError::ServiceUnavailable => StatusCode::SERVICE_UNAVAILABLE,
            AuthError::Internal => StatusCode::INTERNAL_SERVER_ERROR,
            AuthError::BadRequest => StatusCode::BAD_REQUEST,
        }
    }
}

#[derive(Serialize)]
struct ValidateApiKeyRequest<'a> {
    api_key: &'a str,
}

#[derive(Deserialize)]
struct ValidateApiKeyResponse {
    tenant_id: Uuid,
    role: String,
    environment: String,
}

#[derive(Serialize)]
struct ValidateSessionRequest<'a> {
    session_token: &'a str,
}

#[derive(Deserialize)]
struct ValidateSessionResponse {
    user_id: String,
    tenant_id: String,
    role: String,
}

/// Verify an API key against auth-service's `/internal/validate` endpoint.
pub async fn verify_api_key(
    http: &reqwest::Client,
    auth_service_url: &str,
    api_key: &str,
) -> Result<ApiKeyContext, AuthError> {
    let resp = http
        .post(format!("{auth_service_url}/internal/validate"))
        .json(&ValidateApiKeyRequest { api_key })
        .send()
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "auth-service unreachable");
            AuthError::ServiceUnavailable
        })?;

    if !resp.status().is_success() {
        tracing::warn!(status = %resp.status(), reason = "api_key_invalid", "auth rejected");
        return Err(AuthError::Unauthorized);
    }

    let body: ValidateApiKeyResponse = resp.json().await.map_err(|e| {
        tracing::error!(error = %e, "failed to parse validate response");
        AuthError::Internal
    })?;

    Ok(ApiKeyContext {
        tenant_id: body.tenant_id,
        role: body.role,
        environment: body.environment,
    })
}

/// Verify a session token against auth-service's `/internal/validate-session` endpoint.
pub async fn verify_session(
    http: &reqwest::Client,
    auth_service_url: &str,
    session_token: &str,
) -> Result<SessionContext, AuthError> {
    let resp = http
        .post(format!("{auth_service_url}/internal/validate-session"))
        .json(&ValidateSessionRequest { session_token })
        .send()
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "auth-service unreachable");
            AuthError::ServiceUnavailable
        })?;

    if !resp.status().is_success() {
        tracing::warn!(status = %resp.status(), reason = "session_invalid", "auth rejected");
        return Err(AuthError::Unauthorized);
    }

    let body: ValidateSessionResponse = resp.json().await.map_err(|e| {
        tracing::error!(error = %e, "failed to parse validate-session response");
        AuthError::Internal
    })?;

    let tenant_id = body.tenant_id.parse::<Uuid>().map_err(|e| {
        tracing::error!(error = %e, "invalid tenant_id in validate-session response");
        AuthError::Internal
    })?;
    let user_id = body.user_id.parse::<Uuid>().map_err(|e| {
        tracing::error!(error = %e, "invalid user_id in validate-session response");
        AuthError::Internal
    })?;

    Ok(SessionContext {
        tenant_id,
        user_id,
        role: body.role,
    })
}

/// Extract the raw bearer token value from the Authorization header.
///
/// Returns `Ok(None)` if the header is absent. Returns
/// `Err(AuthError::Unauthorized)` if the header is present but malformed
/// (missing the `Bearer ` prefix, or not valid UTF-8).
pub fn extract_bearer_token(headers: &axum::http::HeaderMap) -> Result<Option<String>, AuthError> {
    let Some(value) = headers.get("Authorization") else {
        return Ok(None);
    };

    value
        .to_str()
        .ok()
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(|s| Some(s.to_owned()))
        .ok_or_else(|| {
            tracing::warn!(reason = "malformed_authorization_header", "auth rejected");
            AuthError::Unauthorized
        })
}

/// Extract the `session` cookie value from the Cookie header.
pub fn extract_session_cookie(headers: &axum::http::HeaderMap) -> Option<String> {
    let cookie = headers.get("cookie")?.to_str().ok()?;
    cookie.split(';').map(str::trim).find_map(|part| {
        let (k, v) = part.split_once('=')?;
        if k.trim() == "session" {
            Some(v.trim().to_owned())
        } else {
            None
        }
    })
}

/// Extract and parse the X-Tenant-ID header value as a UUID.
///
/// Returns `Err(AuthError::Unauthorized)` if the header is missing, and
/// `Err(AuthError::BadRequest)` if it is present but not a valid UUID.
pub fn extract_tenant_id_header(headers: &axum::http::HeaderMap) -> Result<Uuid, AuthError> {
    headers
        .get("X-Tenant-ID")
        .ok_or_else(|| {
            tracing::warn!(reason = "missing_tenant_id_header", "auth rejected");
            AuthError::Unauthorized
        })?
        .to_str()
        .ok()
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| {
            tracing::warn!(reason = "malformed_tenant_id_header", "auth rejected");
            AuthError::BadRequest
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderMap;

    fn headers_from(pairs: &[(&str, &str)]) -> HeaderMap {
        let mut map = HeaderMap::new();
        for (k, v) in pairs {
            map.insert(
                axum::http::HeaderName::from_bytes(k.as_bytes()).unwrap(),
                axum::http::HeaderValue::from_str(v).unwrap(),
            );
        }
        map
    }

    #[test]
    fn missing_authorization_header_is_rejected() {
        let headers = headers_from(&[("X-Tenant-ID", "00000000-0000-0000-0000-000000000001")]);
        let result = extract_bearer_token(&headers);
        assert_eq!(result.unwrap(), None);
    }

    #[test]
    fn malformed_authorization_header_is_unauthorized() {
        let headers = headers_from(&[("Authorization", "NotBearer some-token")]);
        let result = extract_bearer_token(&headers);
        assert_eq!(result.unwrap_err(), AuthError::Unauthorized);
    }

    #[test]
    fn bearer_prefix_is_stripped_correctly() {
        let headers = headers_from(&[("Authorization", "Bearer dev-api-key-0000")]);
        let result = extract_bearer_token(&headers).unwrap();
        assert_eq!(result, Some("dev-api-key-0000".into()));
    }

    #[test]
    fn missing_tenant_id_is_rejected() {
        let headers = headers_from(&[("Authorization", "Bearer some-token")]);
        let result = extract_tenant_id_header(&headers);
        assert_eq!(result.unwrap_err(), AuthError::Unauthorized);
    }

    #[test]
    fn malformed_tenant_id_is_bad_request() {
        let headers = headers_from(&[
            ("Authorization", "Bearer some-token"),
            ("X-Tenant-ID", "not-a-uuid"),
        ]);
        let result = extract_tenant_id_header(&headers);
        assert_eq!(result.unwrap_err(), AuthError::BadRequest);
    }

    #[test]
    fn valid_tenant_header_parses() {
        let tenant_id = Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap();
        let headers = headers_from(&[
            ("Authorization", "Bearer dev-api-key-0000"),
            ("X-Tenant-ID", "00000000-0000-0000-0000-000000000001"),
        ]);
        let result = extract_tenant_id_header(&headers).unwrap();
        assert_eq!(result, tenant_id);
    }

    #[test]
    fn no_session_cookie_returns_none() {
        let headers = headers_from(&[("cookie", "other=value")]);
        assert_eq!(extract_session_cookie(&headers), None);
    }

    #[test]
    fn session_cookie_is_extracted() {
        let headers = headers_from(&[("cookie", "foo=bar; session=abc123; baz=qux")]);
        assert_eq!(extract_session_cookie(&headers), Some("abc123".to_string()));
    }

    #[test]
    fn missing_cookie_header_returns_none() {
        let headers = HeaderMap::new();
        assert_eq!(extract_session_cookie(&headers), None);
    }

    #[tokio::test]
    async fn verify_api_key_success() {
        use wiremock::{
            Mock, MockServer, ResponseTemplate,
            matchers::{method, path},
        };

        let mock_server = MockServer::start().await;
        let tenant_id = Uuid::new_v4();

        Mock::given(method("POST"))
            .and(path("/internal/validate"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "tenant_id": tenant_id.to_string(),
                "role": "member",
                "environment": "production",
            })))
            .mount(&mock_server)
            .await;

        let http = reqwest::Client::new();
        let ctx = verify_api_key(&http, &mock_server.uri(), "some-api-key")
            .await
            .unwrap();

        assert_eq!(ctx.tenant_id, tenant_id);
        assert_eq!(ctx.role, "member");
        assert_eq!(ctx.environment, "production");
    }

    #[tokio::test]
    async fn verify_api_key_unauthorized_on_non_2xx() {
        use wiremock::{
            Mock, MockServer, ResponseTemplate,
            matchers::{method, path},
        };

        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/internal/validate"))
            .respond_with(ResponseTemplate::new(401))
            .mount(&mock_server)
            .await;

        let http = reqwest::Client::new();
        let err = verify_api_key(&http, &mock_server.uri(), "bad-key")
            .await
            .unwrap_err();

        assert_eq!(err, AuthError::Unauthorized);
    }

    #[tokio::test]
    async fn verify_session_success() {
        use wiremock::{
            Mock, MockServer, ResponseTemplate,
            matchers::{method, path},
        };

        let mock_server = MockServer::start().await;
        let tenant_id = Uuid::new_v4();
        let user_id = Uuid::new_v4();

        Mock::given(method("POST"))
            .and(path("/internal/validate-session"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "user_id": user_id.to_string(),
                "tenant_id": tenant_id.to_string(),
                "role": "admin",
            })))
            .mount(&mock_server)
            .await;

        let http = reqwest::Client::new();
        let ctx = verify_session(&http, &mock_server.uri(), "some-session-token")
            .await
            .unwrap();

        assert_eq!(ctx.tenant_id, tenant_id);
        assert_eq!(ctx.user_id, user_id);
        assert_eq!(ctx.role, "admin");
    }

    #[tokio::test]
    async fn verify_session_unauthorized_on_non_2xx() {
        use wiremock::{
            Mock, MockServer, ResponseTemplate,
            matchers::{method, path},
        };

        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/internal/validate-session"))
            .respond_with(ResponseTemplate::new(401))
            .mount(&mock_server)
            .await;

        let http = reqwest::Client::new();
        let err = verify_session(&http, &mock_server.uri(), "bad-token")
            .await
            .unwrap_err();

        assert_eq!(err, AuthError::Unauthorized);
    }

    #[tokio::test]
    async fn verify_session_internal_on_bad_uuid() {
        use wiremock::{
            Mock, MockServer, ResponseTemplate,
            matchers::{method, path},
        };

        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/internal/validate-session"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "user_id": "not-a-uuid",
                "tenant_id": "00000000-0000-0000-0000-000000000001",
                "role": "admin",
            })))
            .mount(&mock_server)
            .await;

        let http = reqwest::Client::new();
        let err = verify_session(&http, &mock_server.uri(), "token")
            .await
            .unwrap_err();

        assert_eq!(err, AuthError::Internal);
    }
}
