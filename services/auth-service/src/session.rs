use anyhow::Result;
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize)]
pub struct SessionClaims {
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

const SESSION_TTL_SECS: i64 = 3600;

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
    let data = decode::<SessionClaims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &Validation::default(),
    )?;
    Ok(data.claims)
}

/// Generate a random 32-byte PKCE code verifier (base64url, no padding).
pub fn generate_code_verifier() -> String {
    let mut bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

/// Compute the PKCE S256 code challenge from a verifier.
pub fn pkce_challenge(verifier: &str) -> String {
    let hash = Sha256::digest(verifier.as_bytes());
    URL_SAFE_NO_PAD.encode(hash)
}
