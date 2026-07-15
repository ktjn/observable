use anyhow::Result;
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey, Header, Validation, decode, encode};
use rand::Rng;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

const SESSION_ISSUER: &str = "observable-auth-service";
const SESSION_AUDIENCE: &str = "observable-services";
const SESSION_TTL_SECS: i64 = 604_800; // 7 days

#[derive(Debug, Serialize, Deserialize)]
pub struct SessionClaims {
    pub iss: String,
    pub aud: String,
    /// User UUID.
    pub sub: String,
    /// Tenant UUID.
    pub tid: String,
    pub role: String,
    pub env: String,
    /// Session UUID (nonce).
    pub nonce: String,
    pub iat: i64,
    pub exp: i64,
}

pub fn sign_session_jwt(
    secret: &str,
    user_id: Uuid,
    tenant_id: Uuid,
    role: &str,
    environment: &str,
    session_id: Uuid,
) -> Result<String> {
    let now = chrono::Utc::now().timestamp();
    let claims = SessionClaims {
        iss: SESSION_ISSUER.to_owned(),
        aud: SESSION_AUDIENCE.to_owned(),
        sub: user_id.to_string(),
        tid: tenant_id.to_string(),
        role: role.to_owned(),
        env: environment.to_owned(),
        nonce: session_id.to_string(),
        iat: now,
        exp: now + SESSION_TTL_SECS,
    };
    let token = encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )?;
    Ok(token)
}

pub fn verify_session_jwt(secret: &str, token: &str) -> Result<SessionClaims> {
    let mut validation = Validation::new(Algorithm::HS256);
    validation.leeway = 0;
    validation.set_issuer(&[SESSION_ISSUER]);
    validation.set_audience(&[SESSION_AUDIENCE]);

    let data = decode::<SessionClaims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &validation,
    )?;
    Ok(data.claims)
}

/// Generate a random 32-byte PKCE code verifier (base64url, no padding).
pub fn generate_code_verifier() -> String {
    let mut bytes = [0u8; 32];
    rand::rng().fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

/// Compute the PKCE S256 code challenge from a verifier.
pub fn pkce_challenge(verifier: &str) -> String {
    let hash = Sha256::digest(verifier.as_bytes());
    URL_SAFE_NO_PAD.encode(hash)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECRET: &str = "test-session-secret-with-at-least-32-bytes";

    fn claims(exp: i64) -> SessionClaims {
        let now = chrono::Utc::now().timestamp();
        SessionClaims {
            iss: SESSION_ISSUER.to_owned(),
            aud: SESSION_AUDIENCE.to_owned(),
            sub: Uuid::new_v4().to_string(),
            tid: Uuid::new_v4().to_string(),
            role: "tenant_admin".to_owned(),
            env: "production".to_owned(),
            nonce: Uuid::new_v4().to_string(),
            iat: now,
            exp,
        }
    }

    fn encode_claims(secret: &str, claims: &SessionClaims) -> String {
        encode(
            &Header::default(),
            claims,
            &EncodingKey::from_secret(secret.as_bytes()),
        )
        .unwrap()
    }

    #[test]
    fn signed_session_round_trips_with_required_claims() {
        let user_id = Uuid::new_v4();
        let tenant_id = Uuid::new_v4();
        let session_id = Uuid::new_v4();

        let token = sign_session_jwt(
            SECRET,
            user_id,
            tenant_id,
            "tenant_admin",
            "production",
            session_id,
        )
        .unwrap();
        let decoded = verify_session_jwt(SECRET, &token).unwrap();

        assert_eq!(decoded.iss, SESSION_ISSUER);
        assert_eq!(decoded.aud, SESSION_AUDIENCE);
        assert_eq!(decoded.sub, user_id.to_string());
        assert_eq!(decoded.tid, tenant_id.to_string());
        assert_eq!(decoded.nonce, session_id.to_string());
    }

    #[test]
    fn forged_signature_is_rejected() {
        let token = encode_claims(
            "different-session-secret-with-at-least-32-bytes",
            &claims(chrono::Utc::now().timestamp() + 60),
        );

        assert!(verify_session_jwt(SECRET, &token).is_err());
    }

    #[test]
    fn expired_session_is_rejected() {
        let token = encode_claims(SECRET, &claims(chrono::Utc::now().timestamp() - 1));

        assert!(verify_session_jwt(SECRET, &token).is_err());
    }

    #[test]
    fn wrong_issuer_is_rejected() {
        let mut invalid = claims(chrono::Utc::now().timestamp() + 60);
        invalid.iss = "another-issuer".to_owned();
        let token = encode_claims(SECRET, &invalid);

        assert!(verify_session_jwt(SECRET, &token).is_err());
    }

    #[test]
    fn wrong_audience_is_rejected() {
        let mut invalid = claims(chrono::Utc::now().timestamp() + 60);
        invalid.aud = "another-audience".to_owned();
        let token = encode_claims(SECRET, &invalid);

        assert!(verify_session_jwt(SECRET, &token).is_err());
    }

    #[test]
    fn malformed_token_is_rejected() {
        assert!(verify_session_jwt(SECRET, "not-a-jwt").is_err());
    }
}
