# Identity Provider Integration (Zitadel 2.x) — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Integrate Zitadel 2.71.x as Observable's identity provider, adding OIDC PKCE login/logout, a user principal model stored in PostgreSQL, short-lived session JWTs, per-tenant role mapping, and a dev-mode seeded admin user.

**Architecture:** Zitadel runs alongside existing services, sharing the PostgreSQL instance on a separate `zitadel` database. `auth-service` grows four OIDC endpoints (login, callback, logout, me); it exchanges auth codes with Zitadel, upserts users into PostgreSQL, and issues HS256 session JWTs. The frontend gains a login page, OIDC callback handler, and a user menu. All existing API-key ingest paths remain unchanged — both credential types resolve to the same `(tenant_id, role, environment)` tuple.

**Tech Stack:** Rust/Axum (`auth-service`), PostgreSQL 16 (users/sessions), Zitadel 2.71.x (IdP), `jsonwebtoken 9` (HS256 JWTs), `rand 0.8` + `base64 0.22` (PKCE), `reqwest` workspace (HTTP to Zitadel), React 19 / TypeScript (`@tanstack/react-router`, `@tanstack/react-query`), Docker Compose, Helm (Kubernetes).

---

## File Map

**Create:**
- `migrations/postgres/018_create_user_tables.sql`
- `migrations/postgres/019_extend_audit_log_auth_method.sql`
- `config/zitadel/dev-first-instance.yaml`
- `services/auth-service/src/session.rs`
- `services/auth-service/src/oidc.rs`
- `services/auth-service/src/dev_bootstrap.rs`
- `services/auth-service/tests/session_integration.rs`
- `apps/frontend/src/api/auth.ts`
- `apps/frontend/src/pages/LoginPage.tsx`
- `apps/frontend/src/pages/AuthCallbackPage.tsx`
- `apps/frontend/src/components/UserMenu.tsx`
- `apps/frontend/src/pages/IdentitySettingsPage.tsx`
- `charts/observable/charts/zitadel/values.yaml`

**Modify:**
- `docker-compose.yml`
- `services/auth-service/Cargo.toml`
- `services/auth-service/src/lib.rs`
- `services/auth-service/src/main.rs`
- `services/auth-service/src/audit.rs`
- `apps/frontend/src/router.ts`
- `apps/frontend/src/components/AppShell.tsx`

---

## Task 1: Database migrations — user tables

**Files:**
- Create: `migrations/postgres/018_create_user_tables.sql`

- [ ] **Step 1: Write the migration**

```sql
-- Human user principals linked to Zitadel via idp_subject (Zitadel's user ID / sub claim).
CREATE TABLE IF NOT EXISTS users (
    id          UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    idp_subject TEXT        UNIQUE NOT NULL,
    email       TEXT        NOT NULL,
    name        TEXT,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS users_email_idx ON users (email);

-- Maps a user to a tenant with a coarse role.  One row per user+tenant pair.
-- Fine-grained ReBAC (OpenFGA) is P4-S4; this is the P4-S3 coarse model.
CREATE TABLE IF NOT EXISTS user_tenant_roles (
    user_id    UUID NOT NULL REFERENCES users(id)   ON DELETE CASCADE,
    tenant_id  UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    role       TEXT NOT NULL CHECK (role IN ('tenant_admin', 'member', 'viewer')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (user_id, tenant_id)
);

-- Server-side session records.  Used for revocation; the JWT itself is short-lived (1h).
CREATE TABLE IF NOT EXISTS user_sessions (
    id          UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id     UUID        NOT NULL REFERENCES users(id)   ON DELETE CASCADE,
    tenant_id   UUID        NOT NULL REFERENCES tenants(id),
    environment TEXT        NOT NULL,
    issued_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at  TIMESTAMPTZ NOT NULL,
    revoked_at  TIMESTAMPTZ
);

CREATE INDEX IF NOT EXISTS user_sessions_active_idx
    ON user_sessions (user_id, expires_at)
    WHERE revoked_at IS NULL;
```

- [ ] **Step 2: Verify migration applies to testcontainer**

```
cargo test --test postgres_integration -- --nocapture
```

Expected: all existing tests pass. The migration file is picked up automatically by `apply_migrations()` in `tests/postgres_integration.rs`.

- [ ] **Step 3: Commit**

```
git add migrations/postgres/018_create_user_tables.sql
git commit -m "feat(db): add users, user_tenant_roles, and user_sessions tables"
```

---

## Task 2: Database migration — extend audit log

**Files:**
- Create: `migrations/postgres/019_extend_audit_log_auth_method.sql`

- [ ] **Step 1: Write the migration**

```sql
-- Track whether a credential event came from an API key or an OIDC session.
-- NULL means the row predates this migration (legacy api_key row).
ALTER TABLE credential_audit_log
    ADD COLUMN IF NOT EXISTS auth_method TEXT;   -- 'api_key' | 'oidc_session' | NULL (legacy)

-- The action column previously only held 'credential_validate'.
-- New values: 'login', 'logout', 'tenant_select'.
-- No enum type change needed — action is TEXT.

COMMENT ON COLUMN credential_audit_log.auth_method IS
    'api_key = ingest token; oidc_session = human login; NULL = legacy row';
```

- [ ] **Step 2: Run integration tests to confirm migrations still pass**

```
cargo test --test postgres_integration -- --nocapture
```

Expected: all tests pass.

- [ ] **Step 3: Commit**

```
git add migrations/postgres/019_extend_audit_log_auth_method.sql
git commit -m "feat(db): add auth_method column to credential_audit_log"
```

---

## Task 3: Add Zitadel to docker-compose

**Files:**
- Create: `config/zitadel/dev-first-instance.yaml`
- Modify: `docker-compose.yml`

OpenFGA already uses host port 8080. Zitadel's container port is also 8080, so map it to host port **8082**.

- [ ] **Step 1: Create the Zitadel first-instance steps file**

```yaml
# config/zitadel/dev-first-instance.yaml
#
# Processed once by Zitadel on first start-from-init.
# Creates the default IAM owner org and human admin user.
# Run scripts/setup-dev-idp.sh after first start to create dev-tenant org and OIDC app.
FirstInstance:
  InstanceName: Observable Dev
  Org:
    Name: Observable
    Human:
      UserName: admin
      FirstName: Dev
      LastName: Admin
      Email:
        Address: admin@dev.observable
        IsEmailVerified: true
      Password:
        Value: ${OBSERVABLE_DEV_ADMIN_PASSWORD:-Dev@Admin1234!}
        ChangeRequired: false
```

- [ ] **Step 2: Add the Zitadel service to docker-compose.yml**

Insert after the `openfga` service block (before the `# --- SETUP & MIGRATIONS ---` comment):

```yaml
  zitadel:
    image: ghcr.io/zitadel/zitadel:v2.71.0
    command: start-from-init --masterkeyFromEnv --tlsMode disabled --steps /zitadel-steps.yaml
    environment:
      ZITADEL_DATABASE_POSTGRES_HOST: postgres
      ZITADEL_DATABASE_POSTGRES_PORT: 5432
      ZITADEL_DATABASE_POSTGRES_DATABASE: zitadel
      ZITADEL_DATABASE_POSTGRES_USER_USERNAME: ${PG_USER:-observable}
      ZITADEL_DATABASE_POSTGRES_USER_PASSWORD: ${PG_PASSWORD:-observable}
      ZITADEL_DATABASE_POSTGRES_USER_SSL_MODE: disable
      ZITADEL_DATABASE_POSTGRES_ADMIN_USERNAME: ${PG_USER:-observable}
      ZITADEL_DATABASE_POSTGRES_ADMIN_PASSWORD: ${PG_PASSWORD:-observable}
      ZITADEL_DATABASE_POSTGRES_ADMIN_SSL_MODE: disable
      ZITADEL_MASTERKEY: ${ZITADEL_MASTERKEY:-observabledevelopermasterkey0000}
      ZITADEL_EXTERNALDOMAIN: localhost
      ZITADEL_EXTERNALPORT: 8082
      ZITADEL_EXTERNALINSECURE: "true"
      OBSERVABLE_DEV_ADMIN_PASSWORD: ${OBSERVABLE_DEV_ADMIN_PASSWORD:-Dev@Admin1234!}
    volumes:
      - ./config/zitadel/dev-first-instance.yaml:/zitadel-steps.yaml:ro
    ports:
      - "8082:8080"
    depends_on:
      postgres:
        condition: service_healthy
    healthcheck:
      test: ["CMD", "wget", "--spider", "-q", "http://localhost:8080/debug/healthz"]
      interval: 5s
      timeout: 2s
      retries: 30
      start_period: 15s
```

- [ ] **Step 3: Start Zitadel and verify it becomes healthy**

```
docker compose up zitadel --wait
```

Expected: `zitadel` service reaches health `healthy`. Open `http://localhost:8082` and confirm the Zitadel login page loads.

- [ ] **Step 4: Commit**

```
git add config/zitadel/dev-first-instance.yaml docker-compose.yml
git commit -m "feat(infra): add Zitadel 2.71.0 to docker-compose on port 8082"
```

---

## Task 4: Create OIDC app in Zitadel and record the client credentials

This task configures Zitadel via its Management REST API and stores the resulting OIDC client ID in `platform_config`. Run this once after Task 3 has Zitadel healthy.

**Files:**
- Create: `scripts/setup-dev-idp.sh`

- [ ] **Step 1: Write the setup script**

```bash
#!/usr/bin/env bash
# scripts/setup-dev-idp.sh
# One-time Zitadel configuration for local dev.
# Run after: docker compose up zitadel --wait
#
# Outputs: ZITADEL_CLIENT_ID written to .env.local
set -euo pipefail

ZITADEL_BASE="http://localhost:8082"
ADMIN_USER="${OBSERVABLE_DEV_ADMIN_PASSWORD:-Dev@Admin1234!}"

# 1. Obtain a service token by logging in as the instance admin.
# The default machine user PAT is written to stdout during start-from-init;
# for a human login, use the password grant (only available when explicitly enabled).
# Here we use the password grant against the management API.
TOKEN=$(curl -sf -X POST "${ZITADEL_BASE}/oauth/v2/token" \
  -H "Content-Type: application/x-www-form-urlencoded" \
  -d "grant_type=password&username=admin%40dev.observable&password=$(python3 -c "import urllib.parse,sys; print(urllib.parse.quote(sys.argv[1]))" "${ADMIN_USER}")&scope=openid+profile+email+urn:zitadel:iam:org:project:id:zitadel:aud" \
  | python3 -c "import json,sys; print(json.load(sys.stdin)['access_token'])")

echo "Obtained admin token."

# 2. Create the Observable OIDC web application in the default project.
# The default project ID in Zitadel's first instance is known after the first start.
# Retrieve the default org's project list first.
DEFAULT_ORG_ID=$(curl -sf "${ZITADEL_BASE}/management/v1/orgs/me" \
  -H "Authorization: Bearer ${TOKEN}" \
  | python3 -c "import json,sys; print(json.load(sys.stdin)['org']['id'])")

echo "Default org ID: ${DEFAULT_ORG_ID}"

PROJECT_ID=$(curl -sf -X POST "${ZITADEL_BASE}/management/v1/projects" \
  -H "Authorization: Bearer ${TOKEN}" \
  -H "Content-Type: application/json" \
  -d '{"name":"Observable"}' \
  | python3 -c "import json,sys; print(json.load(sys.stdin)['id'])")

echo "Project ID: ${PROJECT_ID}"

# 3. Create a web OIDC app (public client, PKCE, no client secret).
APP_RESPONSE=$(curl -sf -X POST "${ZITADEL_BASE}/management/v1/projects/${PROJECT_ID}/apps/oidc" \
  -H "Authorization: Bearer ${TOKEN}" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "Observable Frontend",
    "redirectUris": ["http://localhost:5173/auth/callback"],
    "responseTypes": ["RESPONSE_TYPE_CODE"],
    "grantTypes": ["GRANT_TYPE_AUTHORIZATION_CODE"],
    "appType": "OIDC_APP_TYPE_WEB",
    "authMethodType": "OIDC_AUTH_METHOD_TYPE_NONE",
    "postLogoutRedirectUris": ["http://localhost:5173/login"],
    "devMode": true
  }')

CLIENT_ID=$(echo "${APP_RESPONSE}" | python3 -c "import json,sys; print(json.load(sys.stdin)['clientId'])")
echo "OIDC Client ID: ${CLIENT_ID}"

# 4. Write to .env.local for auth-service to pick up.
cat > .env.local <<EOF
ZITADEL_ISSUER=http://localhost:8082
ZITADEL_CLIENT_ID=${CLIENT_ID}
ZITADEL_REDIRECT_URI=http://localhost:5173/auth/callback
EOF

echo "Written to .env.local. Restart auth-service to pick up the new config."
```

- [ ] **Step 2: Make it executable and run it**

```
chmod +x scripts/setup-dev-idp.sh
docker compose up zitadel --wait && bash scripts/setup-dev-idp.sh
```

Expected: `.env.local` is created with `ZITADEL_ISSUER`, `ZITADEL_CLIENT_ID`, `ZITADEL_REDIRECT_URI`.

- [ ] **Step 3: Commit**

```
git add scripts/setup-dev-idp.sh
git commit -m "feat(infra): add dev IdP setup script for Zitadel OIDC app creation"
```

---

## Task 5: auth-service — add JWT and PKCE dependencies

**Files:**
- Modify: `services/auth-service/Cargo.toml`

- [ ] **Step 1: Add new dependencies**

In `services/auth-service/Cargo.toml`, add to `[dependencies]`:

```toml
jsonwebtoken = "9"
rand         = "0.8"
base64       = "0.22"
reqwest      = { workspace = true }
```

- [ ] **Step 2: Verify the crate compiles**

```
cargo build -p auth-service
```

Expected: compiles without errors.

- [ ] **Step 3: Commit**

```
git add services/auth-service/Cargo.toml
git commit -m "feat(auth-service): add jsonwebtoken, rand, base64, reqwest deps"
```

---

## Task 6: session.rs — JWT signing and verification

**Files:**
- Create: `services/auth-service/src/session.rs`
- Create: `services/auth-service/tests/session_integration.rs`

- [ ] **Step 1: Write the failing test**

Create `services/auth-service/tests/session_integration.rs`:

```rust
use auth_service::session::{sign_session_jwt, verify_session_jwt};
use uuid::Uuid;

#[test]
fn round_trip_session_jwt() {
    let secret = "testsecretfortests1234567890abc";
    let user_id = Uuid::new_v4();
    let tenant_id = Uuid::new_v4();

    let token = sign_session_jwt(secret, user_id, tenant_id, "member", "production")
        .expect("sign must succeed");
    let claims = verify_session_jwt(secret, &token).expect("verify must succeed");

    assert_eq!(claims.sub, user_id.to_string());
    assert_eq!(claims.tid, tenant_id.to_string());
    assert_eq!(claims.role, "member");
    assert_eq!(claims.env, "production");
}

#[test]
fn wrong_secret_is_rejected() {
    let user_id = Uuid::new_v4();
    let tenant_id = Uuid::new_v4();

    let token = sign_session_jwt("correctsecret1234567890abcdefgh", user_id, tenant_id, "member", "prod")
        .expect("sign");
    let result = verify_session_jwt("wrongsecretXXXXXXXXXXXXXXXXXXXX", &token);

    assert!(result.is_err(), "wrong secret must be rejected");
}

#[test]
fn pkce_challenge_is_deterministic() {
    let verifier = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
    let challenge = auth_service::session::pkce_challenge(verifier);
    // RFC 7636 test vector S256("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk")
    assert_eq!(challenge, "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM");
}
```

- [ ] **Step 2: Run to confirm failure**

```
cargo test --test session_integration 2>&1 | head -20
```

Expected: compile error — `auth_service::session` does not exist yet.

- [ ] **Step 3: Write session.rs**

Create `services/auth-service/src/session.rs`:

```rust
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
) -> Result<String> {
    let now = chrono::Utc::now().timestamp();
    let claims = SessionClaims {
        sub: user_id.to_string(),
        tid: tenant_id.to_string(),
        role: role.to_owned(),
        env: environment.to_owned(),
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
```

- [ ] **Step 4: Export session from lib.rs**

Add to `services/auth-service/src/lib.rs`:

```rust
pub mod session;
pub mod validate;
```

(Replace the existing `pub mod validate;` line with both lines.)

- [ ] **Step 5: Run tests to confirm passing**

```
cargo test --test session_integration
```

Expected: all 3 tests pass.

- [ ] **Step 6: Commit**

```
git add services/auth-service/src/session.rs services/auth-service/src/lib.rs services/auth-service/tests/session_integration.rs
git commit -m "feat(auth-service): add session JWT signing/verification and PKCE helpers"
```

---

## Task 7: oidc.rs — OIDC login/callback/logout/me handlers

**Files:**
- Create: `services/auth-service/src/oidc.rs`

These handlers implement the OIDC authorization-code-with-PKCE flow against Zitadel.

- [ ] **Step 1: Write failing integration tests (in postgres_integration.rs)**

Add to the end of `services/auth-service/tests/postgres_integration.rs`:

```rust
#[tokio::test]
async fn upsert_user_creates_and_deduplicates() {
    let (pool, _container) = start_pool().await;

    let tenant_id = uuid::Uuid::parse_str("00000000-0000-0000-0000-000000000002").unwrap();

    // First upsert — creates
    let user_id_1 = auth_service::oidc::upsert_user(
        &pool,
        "zitadel|user-1",
        "alice@example.com",
        Some("Alice"),
    )
    .await
    .expect("upsert 1");

    // Second upsert — updates, returns same ID
    let user_id_2 = auth_service::oidc::upsert_user(
        &pool,
        "zitadel|user-1",
        "alice-updated@example.com",
        Some("Alice Updated"),
    )
    .await
    .expect("upsert 2");

    assert_eq!(user_id_1, user_id_2, "same subject must return same UUID");

    // Assign role
    auth_service::oidc::upsert_user_tenant_role(&pool, user_id_1, tenant_id, "member")
        .await
        .expect("role assignment");

    // Verify role is stored
    let role: String = sqlx::query_scalar(
        "SELECT role FROM user_tenant_roles WHERE user_id = $1 AND tenant_id = $2",
    )
    .bind(user_id_1)
    .bind(tenant_id)
    .fetch_one(&pool)
    .await
    .expect("role row");

    assert_eq!(role, "member");
}
```

- [ ] **Step 2: Run to confirm failure**

```
cargo test --test postgres_integration upsert_user 2>&1 | head -20
```

Expected: compile error — `auth_service::oidc` not found.

- [ ] **Step 3: Write oidc.rs**

Create `services/auth-service/src/oidc.rs`:

```rust
use anyhow::{anyhow, Result};
use axum::{
    extract::{Query, State},
    http::{header, StatusCode},
    response::{IntoResponse, Redirect, Response},
    Json,
};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

use crate::session::{generate_code_verifier, pkce_challenge, sign_session_jwt, verify_session_jwt};

// ── DB helpers ────────────────────────────────────────────────────────────────

/// Create or update a user row and return the Observable user UUID.
pub async fn upsert_user(
    pool: &PgPool,
    idp_subject: &str,
    email: &str,
    name: Option<&str>,
) -> Result<Uuid> {
    let row = sqlx::query!(
        r#"
        INSERT INTO users (idp_subject, email, name)
        VALUES ($1, $2, $3)
        ON CONFLICT (idp_subject) DO UPDATE
            SET email = EXCLUDED.email,
                name  = EXCLUDED.name,
                updated_at = now()
        RETURNING id
        "#,
        idp_subject,
        email,
        name,
    )
    .fetch_one(pool)
    .await?;
    Ok(row.id)
}

/// Insert or update the user's role for a given tenant.
pub async fn upsert_user_tenant_role(
    pool: &PgPool,
    user_id: Uuid,
    tenant_id: Uuid,
    role: &str,
) -> Result<()> {
    sqlx::query!(
        r#"
        INSERT INTO user_tenant_roles (user_id, tenant_id, role)
        VALUES ($1, $2, $3)
        ON CONFLICT (user_id, tenant_id) DO UPDATE SET role = EXCLUDED.role
        "#,
        user_id,
        tenant_id,
        role,
    )
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
    let row = sqlx::query!(
        r#"
        INSERT INTO user_sessions (user_id, tenant_id, environment, expires_at)
        VALUES ($1, $2, $3, $4)
        RETURNING id
        "#,
        user_id,
        tenant_id,
        environment,
        expires_at,
    )
    .fetch_one(pool)
    .await?;
    Ok(row.id)
}

/// Revoke a session by ID; silently succeeds if already revoked or not found.
pub async fn revoke_session(pool: &PgPool, session_id: Uuid) -> Result<()> {
    sqlx::query!(
        "UPDATE user_sessions SET revoked_at = now() WHERE id = $1 AND revoked_at IS NULL",
        session_id,
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// Return all tenant memberships for a user as (tenant_id, role) pairs.
pub async fn list_user_tenants(pool: &PgPool, user_id: Uuid) -> Result<Vec<(Uuid, String)>> {
    let rows = sqlx::query!(
        "SELECT tenant_id, role FROM user_tenant_roles WHERE user_id = $1",
        user_id
    )
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(|r| (r.tenant_id, r.role)).collect())
}

// ── HTTP handlers ─────────────────────────────────────────────────────────────

/// Shared state passed to each OIDC handler.
#[derive(Clone)]
pub struct OidcConfig {
    pub issuer: String,
    pub client_id: String,
    pub redirect_uri: String,
    pub session_secret: String,
}

/// GET /v1/auth/login
/// Builds PKCE challenge, stores verifier in a short-lived cookie, and
/// redirects to Zitadel's authorization endpoint.
pub async fn login_handler(State(cfg): State<OidcConfig>) -> Response {
    let verifier = generate_code_verifier();
    let challenge = pkce_challenge(&verifier);
    let state_param = generate_code_verifier(); // csrf nonce

    let auth_url = format!(
        "{}/oauth/v2/authorize\
         ?client_id={}\
         &redirect_uri={}\
         &response_type=code\
         &scope=openid+profile+email\
         &code_challenge={}\
         &code_challenge_method=S256\
         &state={}",
        cfg.issuer,
        urlencoding(&cfg.client_id),
        urlencoding(&cfg.redirect_uri),
        urlencoding(&challenge),
        urlencoding(&state_param),
    );

    let set_cv = format!(
        "pkce_cv={verifier}; HttpOnly; SameSite=Lax; Path=/; Max-Age=300"
    );
    let set_state = format!(
        "oauth_state={state_param}; HttpOnly; SameSite=Lax; Path=/; Max-Age=300"
    );

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
    State((pool, cfg)): State<(PgPool, OidcConfig)>,
    Query(params): Query<CallbackParams>,
    cookies: axum::http::HeaderMap,
) -> Result<Response, StatusCode> {
    // Extract PKCE verifier from cookie.
    let verifier = extract_cookie(&cookies, "pkce_cv")
        .ok_or(StatusCode::BAD_REQUEST)?;

    // Exchange code for tokens at Zitadel.
    let token_resp = reqwest::Client::new()
        .post(format!("{}/oauth/v2/token", cfg.issuer))
        .form(&[
            ("grant_type", "authorization_code"),
            ("code", &params.code),
            ("redirect_uri", &cfg.redirect_uri),
            ("client_id", &cfg.client_id),
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

    // Fetch user info from Zitadel.
    let userinfo = reqwest::Client::new()
        .get(format!("{}/oidc/v1/userinfo", cfg.issuer))
        .bearer_auth(&access_token)
        .send()
        .await
        .map_err(|_| StatusCode::BAD_GATEWAY)?
        .json::<serde_json::Value>()
        .await
        .map_err(|_| StatusCode::BAD_GATEWAY)?;

    let sub = userinfo["sub"].as_str().ok_or(StatusCode::UNAUTHORIZED)?.to_owned();
    let email = userinfo["email"].as_str().unwrap_or("").to_owned();
    let name = userinfo["name"].as_str().map(ToOwned::to_owned);

    // Upsert user record.
    let user_id = upsert_user(&pool, &sub, &email, name.as_deref())
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Find this user's tenants.
    let tenants = list_user_tenants(&pool, user_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Use the first available tenant (user can switch in the UI).
    let (tenant_id, role) = tenants
        .into_iter()
        .next()
        .ok_or(StatusCode::FORBIDDEN)?;

    // Fetch first environment for this tenant.
    let env_row = sqlx::query_scalar::<_, String>(
        "SELECT DISTINCT environment FROM api_keys WHERE tenant_id = $1 LIMIT 1",
    )
    .bind(tenant_id)
    .fetch_optional(&pool)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let environment = env_row.unwrap_or_else(|| "default".to_string());

    // Create session record.
    let _session_id = create_session(&pool, user_id, tenant_id, &environment)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Sign session JWT.
    let jwt = sign_session_jwt(&cfg.session_secret, user_id, tenant_id, &role, &environment)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let set_session = format!(
        "session={jwt}; HttpOnly; SameSite=Lax; Path=/; Max-Age=3600"
    );

    Ok(Response::builder()
        .status(StatusCode::FOUND)
        .header(header::LOCATION, "/")
        .header(header::SET_COOKIE, set_session)
        // Clear PKCE cookies.
        .header(header::SET_COOKIE, "pkce_cv=; Max-Age=0; Path=/")
        .header(header::SET_COOKIE, "oauth_state=; Max-Age=0; Path=/")
        .body(axum::body::Body::empty())
        .unwrap())
}

/// POST /v1/auth/logout
/// Revokes the current session and clears the session cookie.
pub async fn logout_handler(
    State((pool, cfg)): State<(PgPool, OidcConfig)>,
    cookies: axum::http::HeaderMap,
) -> Response {
    if let Some(token) = extract_cookie(&cookies, "session") {
        if let Ok(claims) = verify_session_jwt(&cfg.session_secret, &token) {
            if let Ok(session_id) = Uuid::parse_str(&claims.sub) {
                // sub holds user_id; we need to revoke via a different mechanism.
                // Here we revoke all active sessions for this user+tenant.
                let _ = sqlx::query!(
                    "UPDATE user_sessions SET revoked_at = now() \
                     WHERE user_id = $1::uuid AND tenant_id = $2::uuid AND revoked_at IS NULL",
                    claims.sub,
                    claims.tid,
                )
                .execute(&pool)
                .await;
                let _ = session_id; // suppress lint
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
    State((pool, cfg)): State<(PgPool, OidcConfig)>,
    cookies: axum::http::HeaderMap,
) -> Result<Json<MeResponse>, StatusCode> {
    let token = extract_cookie(&cookies, "session").ok_or(StatusCode::UNAUTHORIZED)?;
    let claims = verify_session_jwt(&cfg.session_secret, &token)
        .map_err(|_| StatusCode::UNAUTHORIZED)?;

    let user_id = Uuid::parse_str(&claims.sub).map_err(|_| StatusCode::UNAUTHORIZED)?;

    let email: String =
        sqlx::query_scalar("SELECT email FROM users WHERE id = $1")
            .bind(user_id)
            .fetch_optional(&pool)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
            .ok_or(StatusCode::UNAUTHORIZED)?;

    let tenants = list_user_tenants(&pool, user_id)
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

// ── Utilities ─────────────────────────────────────────────────────────────────

fn extract_cookie(headers: &axum::http::HeaderMap, name: &str) -> Option<String> {
    let cookie_header = headers.get(header::COOKIE)?.to_str().ok()?;
    cookie_header
        .split(';')
        .map(str::trim)
        .find_map(|part| {
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
```

- [ ] **Step 4: Export oidc from lib.rs**

In `services/auth-service/src/lib.rs`:

```rust
pub mod oidc;
pub mod session;
pub mod validate;
```

- [ ] **Step 5: Run tests**

```
cargo test --test postgres_integration upsert_user
```

Expected: `upsert_user_creates_and_deduplicates` passes.

- [ ] **Step 6: Commit**

```
git add services/auth-service/src/oidc.rs services/auth-service/src/lib.rs
git commit -m "feat(auth-service): add OIDC handlers (login, callback, logout, me)"
```

---

## Task 8: audit.rs — add login/logout event support

**Files:**
- Modify: `services/auth-service/src/audit.rs`

- [ ] **Step 1: Write the failing test**

Add to the inline tests in `services/auth-service/src/audit.rs`:

```rust
    #[test]
    fn login_entry_has_oidc_session_auth_method() {
        let tenant = Uuid::parse_str("00000000-0000-0000-0000-000000000002").unwrap();
        let entry = AuditEntry::login("user-jwt-hash".to_string(), tenant);
        assert_eq!(entry.action, "login");
        assert_eq!(entry.auth_method, Some("oidc_session"));
        assert_eq!(entry.outcome, "allow");
    }
```

- [ ] **Step 2: Run to confirm failure**

```
cargo test -p auth-service -- audit 2>&1 | head -20
```

Expected: compile error — `AuditEntry::login` not found, `action` and `auth_method` fields missing.

- [ ] **Step 3: Extend AuditEntry and the write function**

Replace the full content of `services/auth-service/src/audit.rs`:

```rust
use sqlx::PgPool;
use uuid::Uuid;

pub struct AuditEntry {
    pub action: &'static str,
    pub credential_hash: String,
    pub tenant_id: Option<Uuid>,
    pub outcome: &'static str,
    pub denial_reason: Option<&'static str>,
    pub auth_method: Option<&'static str>,
}

impl AuditEntry {
    pub fn allow(credential_hash: String, tenant_id: Uuid) -> Self {
        Self {
            action: "credential_validate",
            credential_hash,
            tenant_id: Some(tenant_id),
            outcome: "allow",
            denial_reason: None,
            auth_method: Some("api_key"),
        }
    }

    pub fn deny_not_found(credential_hash: String) -> Self {
        Self {
            action: "credential_validate",
            credential_hash,
            tenant_id: None,
            outcome: "deny",
            denial_reason: Some("not_found"),
            auth_method: Some("api_key"),
        }
    }

    pub fn deny(credential_hash: String, tenant_id: Uuid, reason: &'static str) -> Self {
        Self {
            action: "credential_validate",
            credential_hash,
            tenant_id: Some(tenant_id),
            outcome: "deny",
            denial_reason: Some(reason),
            auth_method: Some("api_key"),
        }
    }

    pub fn login(credential_hash: String, tenant_id: Uuid) -> Self {
        Self {
            action: "login",
            credential_hash,
            tenant_id: Some(tenant_id),
            outcome: "allow",
            denial_reason: None,
            auth_method: Some("oidc_session"),
        }
    }

    pub fn logout(credential_hash: String, tenant_id: Uuid) -> Self {
        Self {
            action: "logout",
            credential_hash,
            tenant_id: Some(tenant_id),
            outcome: "allow",
            denial_reason: None,
            auth_method: Some("oidc_session"),
        }
    }
}

pub async fn write(db: &PgPool, entry: &AuditEntry) {
    let result = sqlx::query(
        "INSERT INTO credential_audit_log \
         (action, outcome, credential_hash, tenant_id, denial_reason, auth_method) \
         VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind(entry.action)
    .bind(entry.outcome)
    .bind(&entry.credential_hash)
    .bind(entry.tenant_id)
    .bind(entry.denial_reason)
    .bind(entry.auth_method)
    .execute(db)
    .await;

    if let Err(e) = result {
        tracing::warn!(error = %e, "failed to write audit log");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allow_entry_fields() {
        let tenant = Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap();
        let entry = AuditEntry::allow("abc".to_string(), tenant);
        assert_eq!(entry.outcome, "allow");
        assert_eq!(entry.tenant_id, Some(tenant));
        assert!(entry.denial_reason.is_none());
        assert_eq!(entry.auth_method, Some("api_key"));
    }

    #[test]
    fn deny_not_found_entry_fields() {
        let entry = AuditEntry::deny_not_found("abc".to_string());
        assert_eq!(entry.outcome, "deny");
        assert_eq!(entry.denial_reason, Some("not_found"));
        assert!(entry.tenant_id.is_none());
    }

    #[test]
    fn deny_with_reason_entry_fields() {
        let tenant = Uuid::new_v4();
        let entry = AuditEntry::deny("abc".to_string(), tenant, "revoked");
        assert_eq!(entry.outcome, "deny");
        assert_eq!(entry.denial_reason, Some("revoked"));
        assert_eq!(entry.tenant_id, Some(tenant));
    }

    #[test]
    fn login_entry_has_oidc_session_auth_method() {
        let tenant = Uuid::parse_str("00000000-0000-0000-0000-000000000002").unwrap();
        let entry = AuditEntry::login("user-jwt-hash".to_string(), tenant);
        assert_eq!(entry.action, "login");
        assert_eq!(entry.auth_method, Some("oidc_session"));
        assert_eq!(entry.outcome, "allow");
    }
}
```

- [ ] **Step 4: Run all auth-service unit tests**

```
cargo test -p auth-service -- --include-ignored
```

Expected: all tests pass.

- [ ] **Step 5: Commit**

```
git add services/auth-service/src/audit.rs
git commit -m "feat(auth-service): extend audit log with auth_method and login/logout events"
```

---

## Task 9: main.rs — wire OIDC routes and state

**Files:**
- Modify: `services/auth-service/src/main.rs`

- [ ] **Step 1: Replace main.rs with the extended version**

```rust
mod audit;
mod oidc;
mod session;

use auth_service::{lookup_api_key, validate};
use axum::{
    extract::State,
    http::{header, StatusCode},
    routing::{get, post},
    Json, Router,
};
use oidc::OidcConfig;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use tower_http::trace::TraceLayer;
use tracing::{Instrument as _, Level};
use uuid::Uuid;

#[derive(Clone)]
struct AppState {
    db: PgPool,
    oidc: OidcConfig,
}

#[derive(Deserialize)]
struct ValidateRequest {
    api_key: String,
}

#[derive(Serialize)]
struct ValidateResponse {
    tenant_id: Uuid,
    role: String,
    environment: String,
}

async fn validate_handler(
    State(state): State<AppState>,
    Json(req): Json<ValidateRequest>,
) -> Result<Json<ValidateResponse>, StatusCode> {
    let hash = validate::sha256_hex(&req.api_key);
    async move {
        match lookup_api_key(&state.db, &req.api_key).await {
            Ok((tenant_id, role, environment)) => {
                audit::write(&state.db, &audit::AuditEntry::allow(hash, tenant_id)).await;
                Ok(Json(ValidateResponse { tenant_id, role, environment }))
            }
            Err(e) => {
                let reason = if e.to_string().contains("revoked") {
                    "revoked"
                } else if e.to_string().contains("not found") {
                    audit::write(&state.db, &audit::AuditEntry::deny_not_found(hash)).await;
                    return Err(StatusCode::UNAUTHORIZED);
                } else {
                    "hash_mismatch"
                };
                audit::write(
                    &state.db,
                    &audit::AuditEntry::deny(hash, Uuid::nil(), reason),
                )
                .await;
                Err(StatusCode::UNAUTHORIZED)
            }
        }
    }
    .instrument(tracing::info_span!("auth.validate"))
    .await
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _telemetry = domain::telemetry::init_self_observability_telemetry("auth-service")?;

    let db_url = std::env::var("DATABASE_URL")?;
    let db = PgPool::connect(&db_url).await?;

    let port: u16 = std::env::var("AUTH_SERVICE_PORT")
        .unwrap_or_else(|_| "4319".into())
        .parse()?;

    let oidc = OidcConfig {
        issuer: std::env::var("ZITADEL_ISSUER")
            .unwrap_or_else(|_| "http://localhost:8082".into()),
        client_id: std::env::var("ZITADEL_CLIENT_ID")
            .unwrap_or_else(|_| "dev-client-id".into()),
        redirect_uri: std::env::var("ZITADEL_REDIRECT_URI")
            .unwrap_or_else(|_| "http://localhost:5173/auth/callback".into()),
        session_secret: std::env::var("SESSION_SECRET")
            .unwrap_or_else(|_| "dev-session-secret-change-in-prod!!".into()),
    };

    let state = AppState { db: db.clone(), oidc: oidc.clone() };
    let oidc_state = (db.clone(), oidc);

    let app = Router::new()
        .route("/health", get(|| async { StatusCode::OK }))
        .route("/internal/validate", post(validate_handler))
        // OIDC endpoints
        .route("/v1/auth/login",    get(oidc::login_handler).with_state(oidc_state.clone()))
        .route("/v1/auth/callback", get(oidc::callback_handler).with_state(oidc_state.clone()))
        .route("/v1/auth/logout",   post(oidc::logout_handler).with_state(oidc_state.clone()))
        .route("/v1/auth/me",       get(oidc::me_handler).with_state(oidc_state.clone()))
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(domain::telemetry::OtelMakeSpan::new(Level::INFO)),
        )
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(("0.0.0.0", port)).await?;
    tracing::info!(port, "auth-service listening");
    axum::serve(listener, app).await?;
    Ok(())
}
```

> **Note:** The OIDC routes use a separate `(PgPool, OidcConfig)` state tuple because Axum's `.with_state()` per-route is needed to provide the composite type. If the compiler rejects this, move the state management into a single `AppState` and add `OidcConfig` fields there. Axum 0.7 supports per-route `.with_state()`.

- [ ] **Step 2: Build to confirm it compiles**

```
cargo build -p auth-service
```

Expected: builds without errors.

- [ ] **Step 3: Run the integration tests**

```
cargo test --test postgres_integration
```

Expected: all existing tests pass.

- [ ] **Step 4: Commit**

```
git add services/auth-service/src/main.rs
git commit -m "feat(auth-service): wire OIDC endpoints with Zitadel config from env"
```

---

## Task 10: dev_bootstrap.rs — dev seed user and tenant role

**Files:**
- Create: `services/auth-service/src/dev_bootstrap.rs`

This module runs at auth-service startup when `OBSERVABLE_ENV=dev`. It creates a `member` role for the dev admin user on `dev-tenant` (UUID `00000000-0000-0000-0000-000000000002`), so the admin can log in and immediately see the dev-tenant.

- [ ] **Step 1: Write dev_bootstrap.rs**

```rust
// dev_bootstrap.rs — only called when OBSERVABLE_ENV=dev.
// Ensures admin@dev.observable has a member role on dev-tenant after Zitadel login.
// The user row itself is created by the OIDC callback on first login; this pre-seeds
// the role so that login → tenant assignment succeeds immediately.
//
// This module is a no-op if the user has not yet logged in (no user row yet).

use anyhow::Result;
use sqlx::PgPool;
use uuid::Uuid;

const DEV_TENANT_ID: &str = "00000000-0000-0000-0000-000000000002";

pub async fn seed_dev_admin_role(pool: &PgPool, dev_admin_email: &str) -> Result<()> {
    let tenant_id = Uuid::parse_str(DEV_TENANT_ID)?;

    // If the user has already logged in once via OIDC, assign them tenant_admin.
    let user_id: Option<Uuid> =
        sqlx::query_scalar("SELECT id FROM users WHERE email = $1")
            .bind(dev_admin_email)
            .fetch_optional(pool)
            .await?;

    if let Some(uid) = user_id {
        sqlx::query!(
            r#"
            INSERT INTO user_tenant_roles (user_id, tenant_id, role)
            VALUES ($1, $2, 'tenant_admin')
            ON CONFLICT (user_id, tenant_id) DO NOTHING
            "#,
            uid,
            tenant_id,
        )
        .execute(pool)
        .await?;
        tracing::info!(email = dev_admin_email, "dev admin role ensured on dev-tenant");
    } else {
        tracing::info!(
            email = dev_admin_email,
            "dev admin has not logged in yet; role will be seeded on first callback"
        );
    }

    Ok(())
}
```

- [ ] **Step 2: Call from main.rs when OBSERVABLE_ENV=dev**

Add to `main.rs` just before `axum::serve(...)`:

```rust
    if std::env::var("OBSERVABLE_ENV").as_deref() == Ok("dev") {
        let dev_email = std::env::var("DEV_ADMIN_EMAIL")
            .unwrap_or_else(|_| "admin@dev.observable".into());
        if let Err(e) = dev_bootstrap::seed_dev_admin_role(&db, &dev_email).await {
            tracing::warn!(error = %e, "dev bootstrap role seed failed (non-fatal)");
        }
    }
```

Also add `mod dev_bootstrap;` at the top of `main.rs`.

- [ ] **Step 3: Build to confirm**

```
cargo build -p auth-service
```

Expected: compiles.

- [ ] **Step 4: Commit**

```
git add services/auth-service/src/dev_bootstrap.rs services/auth-service/src/main.rs
git commit -m "feat(auth-service): add dev bootstrap to seed admin role on dev-tenant"
```

---

## Task 11: Run the full auth-service test suite

- [ ] **Step 1: Run all tests**

```
cargo test -p auth-service
cargo test --test postgres_integration
cargo test --test session_integration
```

Expected: all tests pass.

- [ ] **Step 2: Smoke test with docker compose**

```
docker compose up --build auth-service --wait
curl -sf http://localhost:4319/health
curl -s http://localhost:4319/v1/auth/login
```

Expected: `/health` returns 200; `/v1/auth/login` returns a 302 redirect to `http://localhost:8082/oauth/v2/authorize?...`.

- [ ] **Step 3: Commit**

Nothing to commit if all tests pass. Otherwise fix and commit.

---

## Task 12: Frontend — auth API client

**Files:**
- Create: `apps/frontend/src/api/auth.ts`

- [ ] **Step 1: Write the failing test**

Create `apps/frontend/src/api/auth.test.ts`:

```typescript
import { describe, it, expect, vi, beforeEach } from "vitest";

describe("auth api", () => {
  beforeEach(() => {
    vi.stubGlobal("fetch", vi.fn());
  });

  it("me() returns user when authenticated", async () => {
    (fetch as ReturnType<typeof vi.fn>).mockResolvedValueOnce({
      ok: true,
      json: async () => ({
        user_id: "abc",
        email: "alice@example.com",
        tenants: [{ tenant_id: "t1", role: "member" }],
      }),
    });

    const { me } = await import("./auth");
    const user = await me();
    expect(user.email).toBe("alice@example.com");
    expect(user.tenants).toHaveLength(1);
  });

  it("me() throws when unauthenticated", async () => {
    (fetch as ReturnType<typeof vi.fn>).mockResolvedValueOnce({ ok: false, status: 401 });

    const { me } = await import("./auth");
    await expect(me()).rejects.toThrow("401");
  });
});
```

- [ ] **Step 2: Run to confirm failure**

```
cd apps/frontend && npx vitest run src/api/auth.test.ts 2>&1 | head -20
```

Expected: module not found.

- [ ] **Step 3: Write auth.ts**

```typescript
// apps/frontend/src/api/auth.ts
export interface TenantMembership {
  tenant_id: string;
  role: string;
}

export interface MeResponse {
  user_id: string;
  email: string;
  tenants: TenantMembership[];
}

export async function me(): Promise<MeResponse> {
  const res = await fetch("/v1/auth/me", { credentials: "include" });
  if (!res.ok) throw new Error(`me() failed: ${res.status}`);
  return res.json() as Promise<MeResponse>;
}

export function initiateLogin(): void {
  window.location.href = "/v1/auth/login";
}

export async function logout(): Promise<void> {
  const res = await fetch("/v1/auth/logout", {
    method: "POST",
    credentials: "include",
  });
  if (!res.ok) throw new Error(`logout failed: ${res.status}`);
}
```

- [ ] **Step 4: Run tests**

```
cd apps/frontend && npx vitest run src/api/auth.test.ts
```

Expected: both tests pass.

- [ ] **Step 5: Commit**

```
git add apps/frontend/src/api/auth.ts apps/frontend/src/api/auth.test.ts
git commit -m "feat(frontend): add auth API client (me, login, logout)"
```

---

## Task 13: Frontend — LoginPage and AuthCallbackPage

**Files:**
- Create: `apps/frontend/src/pages/LoginPage.tsx`
- Create: `apps/frontend/src/pages/AuthCallbackPage.tsx`
- Modify: `apps/frontend/src/router.ts`

- [ ] **Step 1: Write LoginPage.tsx**

```tsx
// apps/frontend/src/pages/LoginPage.tsx
import { initiateLogin } from "../api/auth";

export default function LoginPage() {
  return (
    <div
      style={{
        display: "flex",
        flexDirection: "column",
        alignItems: "center",
        justifyContent: "center",
        height: "100vh",
        gap: "1.5rem",
        background: "var(--background)",
        color: "var(--text)",
      }}
    >
      <div style={{ fontSize: "1.5rem", fontWeight: 700, letterSpacing: "0.1em" }}>
        OBSERVABLE
      </div>
      <button
        onClick={initiateLogin}
        style={{
          padding: "0.6rem 1.6rem",
          background: "var(--accent, #3b82f6)",
          color: "#fff",
          border: "none",
          borderRadius: "var(--radius, 4px)",
          cursor: "pointer",
          fontSize: "1rem",
        }}
      >
        Sign in
      </button>
    </div>
  );
}
```

- [ ] **Step 2: Write AuthCallbackPage.tsx**

The browser lands here after Zitadel redirects back. The real exchange happens server-side at `GET /v1/auth/callback`; this page just passes the `?code=&state=` parameters through.

```tsx
// apps/frontend/src/pages/AuthCallbackPage.tsx
import { useEffect } from "react";
import { useNavigate } from "@tanstack/react-router";

export default function AuthCallbackPage() {
  const navigate = useNavigate();

  useEffect(() => {
    // The backend callback endpoint is /v1/auth/callback.
    // Zitadel redirects to /auth/callback (frontend route).
    // Forward the query string to the backend, then follow the backend's redirect.
    const qs = window.location.search;
    window.location.href = `/v1/auth/callback${qs}`;
  }, [navigate]);

  return (
    <div style={{ display: "flex", alignItems: "center", justifyContent: "center", height: "100vh" }}>
      Completing sign-in…
    </div>
  );
}
```

- [ ] **Step 3: Add routes to router.ts**

Add to `apps/frontend/src/router.ts` — import the two new pages:

```typescript
import LoginPage from "./pages/LoginPage";
import AuthCallbackPage from "./pages/AuthCallbackPage";
```

Add two new route constants after the existing `nlqRoute` definition:

```typescript
const loginRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/login",
  component: LoginPage,
});

const authCallbackRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/auth/callback",
  component: AuthCallbackPage,
});
```

Add both to `routeTree`:

```typescript
export const router = createRouter({
  routeTree: rootRoute.addChildren([
    homeRoute,
    setupRoute,
    loginRoute,
    authCallbackRoute,
    // ... rest of existing routes unchanged ...
    servicesRoute,
    serviceDetailRoute,
    serviceLogsRoute,
    serviceMetricsRoute,
    serviceTracesRoute,
    infrastructureRoute,
    infrastructureDetailRoute,
    serviceOverviewRoute,
    dashboardsRoute,
    alertsRoute,
    adminRoute,
    traceSearchRoute,
    traceDetailRoute,
    logSearchRoute,
    metricsSearchRoute,
    nlqRoute,
  ]),
});
```

- [ ] **Step 4: Build the frontend**

```
cd apps/frontend && npm run build
```

Expected: builds without TypeScript errors.

- [ ] **Step 5: Commit**

```
git add apps/frontend/src/pages/LoginPage.tsx apps/frontend/src/pages/AuthCallbackPage.tsx apps/frontend/src/router.ts
git commit -m "feat(frontend): add LoginPage and AuthCallbackPage routes"
```

---

## Task 14: Frontend — UserMenu component and AppShell integration

**Files:**
- Create: `apps/frontend/src/components/UserMenu.tsx`
- Modify: `apps/frontend/src/components/AppShell.tsx`

- [ ] **Step 1: Write UserMenu.tsx**

```tsx
// apps/frontend/src/components/UserMenu.tsx
import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import { me, logout } from "../api/auth";

export function UserMenu() {
  const queryClient = useQueryClient();
  const { data, isLoading } = useQuery({
    queryKey: ["me"],
    queryFn: me,
    retry: false,
  });

  const logoutMutation = useMutation({
    mutationFn: logout,
    onSuccess: () => {
      queryClient.clear();
      window.location.href = "/login";
    },
  });

  if (isLoading) return null;
  if (!data) {
    return (
      <a href="/login" style={{ color: "var(--text-muted, #888)", fontSize: "0.85rem" }}>
        Sign in
      </a>
    );
  }

  return (
    <div style={{ display: "flex", alignItems: "center", gap: "0.75rem", fontSize: "0.85rem" }}>
      <span style={{ color: "var(--text-muted, #888)" }}>{data.email}</span>
      <button
        onClick={() => logoutMutation.mutate()}
        disabled={logoutMutation.isPending}
        style={{
          background: "none",
          border: "1px solid var(--border)",
          borderRadius: "var(--radius, 4px)",
          padding: "2px 8px",
          cursor: "pointer",
          color: "var(--text)",
          fontSize: "inherit",
        }}
      >
        Sign out
      </button>
    </div>
  );
}
```

- [ ] **Step 2: Add UserMenu to AppShell.tsx**

In `apps/frontend/src/components/AppShell.tsx`, add the import:

```typescript
import { UserMenu } from "./UserMenu";
```

Inside the `<header className="topbar">` element, add `<UserMenu />` at the end of `.topbar-controls`:

```tsx
          <div className="topbar-controls" aria-label="Global context">
            <GlobalDateRangePicker />
            {/* ... existing time-format, tenant, environment selects ... */}
            <UserMenu />
          </div>
```

- [ ] **Step 3: Run the frontend dev server and verify**

```
docker compose up --wait && cd apps/frontend && npm run dev
```

Open `http://localhost:5173`. Expected: "Sign in" link appears in the topbar. Clicking it redirects to Zitadel login at `http://localhost:8082`. After logging in as `admin@dev.observable`, the callback returns to `/` with the UserMenu showing the email and a "Sign out" button.

- [ ] **Step 4: Commit**

```
git add apps/frontend/src/components/UserMenu.tsx apps/frontend/src/components/AppShell.tsx
git commit -m "feat(frontend): add UserMenu with sign-in/sign-out to AppShell"
```

---

## Task 15: Frontend — Admin identity settings view

**Files:**
- Create: `apps/frontend/src/pages/IdentitySettingsPage.tsx`
- Modify: `apps/frontend/src/router.ts`

- [ ] **Step 1: Write IdentitySettingsPage.tsx**

```tsx
// apps/frontend/src/pages/IdentitySettingsPage.tsx
import { useQuery } from "@tanstack/react-query";
import { me } from "../api/auth";

export default function IdentitySettingsPage() {
  const { data: user, isLoading } = useQuery({
    queryKey: ["me"],
    queryFn: me,
    retry: false,
  });

  if (isLoading) return <div className="content-shell">Loading…</div>;

  const isTenantAdmin = user?.tenants.some((t) => t.role === "tenant_admin");
  if (!isTenantAdmin) {
    return (
      <div className="content-shell" style={{ padding: "1.5rem" }}>
        <p>Only tenant administrators can view identity settings.</p>
      </div>
    );
  }

  // These values come from the Zitadel instance; for v1 they are read from
  // environment variables injected at build time or via a /v1/admin/identity endpoint.
  const issuer =
    typeof window !== "undefined"
      ? window.__OBSERVABLE_ZITADEL_ISSUER__ ?? "http://localhost:8082"
      : "http://localhost:8082";

  return (
    <div style={{ padding: "1.5rem", maxWidth: "640px", display: "flex", flexDirection: "column", gap: "1rem" }}>
      <h1 style={{ fontSize: "1.1rem", fontWeight: 700, margin: 0 }}>Identity Settings</h1>

      <table style={{ borderCollapse: "collapse", width: "100%", fontSize: "0.9rem" }}>
        <tbody>
          <tr>
            <td style={{ padding: "0.4rem 1rem 0.4rem 0", fontWeight: 600, whiteSpace: "nowrap" }}>Provider</td>
            <td>Zitadel 2.71.x</td>
          </tr>
          <tr>
            <td style={{ padding: "0.4rem 1rem 0.4rem 0", fontWeight: 600, whiteSpace: "nowrap" }}>Issuer URL</td>
            <td>
              <code style={{ fontFamily: "monospace", fontSize: "0.85rem" }}>{issuer}</code>
            </td>
          </tr>
          <tr>
            <td style={{ padding: "0.4rem 1rem 0.4rem 0", fontWeight: 600, whiteSpace: "nowrap" }}>OIDC Discovery</td>
            <td>
              <a href={`${issuer}/.well-known/openid-configuration`} target="_blank" rel="noreferrer"
                style={{ color: "var(--accent, #3b82f6)" }}>
                {issuer}/.well-known/openid-configuration
              </a>
            </td>
          </tr>
          <tr>
            <td style={{ padding: "0.4rem 1rem 0.4rem 0", fontWeight: 600, whiteSpace: "nowrap" }}>Redirect URI</td>
            <td>
              <code style={{ fontFamily: "monospace", fontSize: "0.85rem" }}>
                {window.location.origin}/auth/callback
              </code>
            </td>
          </tr>
          <tr>
            <td style={{ padding: "0.4rem 1rem 0.4rem 0", fontWeight: 600, whiteSpace: "nowrap" }}>SCIM 2.0 (planned)</td>
            <td>
              <code style={{ fontFamily: "monospace", fontSize: "0.85rem" }}>
                {issuer}/scim/v2/&#x3C;org-id&#x3E;/
              </code>
              <span style={{ marginLeft: "0.5rem", color: "var(--text-muted, #888)", fontSize: "0.8rem" }}>
                — enable per-org in Zitadel Admin Console
              </span>
            </td>
          </tr>
        </tbody>
      </table>
    </div>
  );
}

declare global {
  interface Window {
    __OBSERVABLE_ZITADEL_ISSUER__?: string;
  }
}
```

- [ ] **Step 2: Add the route to router.ts**

Add import:

```typescript
import IdentitySettingsPage from "./pages/IdentitySettingsPage";
```

Add route:

```typescript
const identitySettingsRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/admin/identity",
  component: IdentitySettingsPage,
});
```

Add to routeTree array: `identitySettingsRoute`.

- [ ] **Step 3: Build**

```
cd apps/frontend && npm run build
```

Expected: no TypeScript errors.

- [ ] **Step 4: Commit**

```
git add apps/frontend/src/pages/IdentitySettingsPage.tsx apps/frontend/src/router.ts
git commit -m "feat(frontend): add read-only identity settings page at /admin/identity"
```

---

## Task 16: Helm chart values for Zitadel

**Files:**
- Create: `charts/observable/charts/zitadel/values.yaml`

- [ ] **Step 1: Write the Helm values file**

```yaml
# charts/observable/charts/zitadel/values.yaml
# Helm chart: zitadel/zitadel (pin to chart version matching Zitadel 2.71.x)
#
# Add this as a dependency in charts/observable/Chart.yaml:
#   - name: zitadel
#     version: "7.x.x"       # check https://charts.zitadel.com for the version pinned to 2.71.x
#     repository: "https://charts.zitadel.com"
#     condition: zitadel.enabled

zitadel:
  enabled: true

  image:
    tag: "v2.71.0"

  zitadel:
    masterKey:
      existingSecret: zitadel-masterkey
      existingSecretKey: masterkey

    configmapConfig:
      ExternalDomain: ""          # set per-environment in values override
      ExternalPort: 443
      ExternalSecure: true
      TLS:
        Enabled: false            # TLS terminated by ingress

    dbSslCaCrt: ""

  env:
    - name: ZITADEL_DATABASE_POSTGRES_HOST
      valueFrom:
        secretKeyRef:
          name: postgres-credentials
          key: host
    - name: ZITADEL_DATABASE_POSTGRES_PORT
      value: "5432"
    - name: ZITADEL_DATABASE_POSTGRES_DATABASE
      value: zitadel
    - name: ZITADEL_DATABASE_POSTGRES_USER_USERNAME
      valueFrom:
        secretKeyRef:
          name: postgres-credentials
          key: username
    - name: ZITADEL_DATABASE_POSTGRES_USER_PASSWORD
      valueFrom:
        secretKeyRef:
          name: postgres-credentials
          key: password
    - name: ZITADEL_DATABASE_POSTGRES_USER_SSL_MODE
      value: require
    - name: ZITADEL_DATABASE_POSTGRES_ADMIN_USERNAME
      valueFrom:
        secretKeyRef:
          name: postgres-credentials
          key: username
    - name: ZITADEL_DATABASE_POSTGRES_ADMIN_PASSWORD
      valueFrom:
        secretKeyRef:
          name: postgres-credentials
          key: password
    - name: ZITADEL_DATABASE_POSTGRES_ADMIN_SSL_MODE
      value: require

  ingress:
    enabled: true
    annotations:
      nginx.ingress.kubernetes.io/proxy-read-timeout: "3600"
      nginx.ingress.kubernetes.io/proxy-send-timeout: "3600"
    hosts:
      - host: ""                  # set per-environment, e.g. id.example.com
        paths:
          - path: /
            pathType: Prefix
    tls:
      - secretName: zitadel-tls
        hosts:
          - ""                    # matches host above

  resources:
    requests:
      cpu: 100m
      memory: 256Mi
    limits:
      cpu: 500m
      memory: 512Mi

  replicaCount: 2

  podDisruptionBudget:
    enabled: true
    minAvailable: 1
```

- [ ] **Step 2: Commit**

```
git add charts/observable/charts/zitadel/values.yaml
git commit -m "feat(helm): add Zitadel 2.71.x Helm values for Kubernetes deployment"
```

---

## Verification Checklist

Run through these in order after all tasks complete:

- [ ] `docker compose up --build --wait` — all services healthy, including `zitadel`
- [ ] `bash scripts/setup-dev-idp.sh` — `.env.local` created with `ZITADEL_CLIENT_ID`
- [ ] `docker compose up --build auth-service --wait` — restarts auth-service with `.env.local` values
- [ ] `curl -si http://localhost:4319/v1/auth/login | head -5` — response is `302 Found` with `Location: http://localhost:8082/oauth/v2/authorize?...`
- [ ] Open `http://localhost:5173` — "Sign in" link visible in topbar
- [ ] Click "Sign in" → Zitadel login page at `http://localhost:8082` → enter `admin@dev.observable` / `Dev@Admin1234!` → redirected back, email shown in topbar
- [ ] `GET /v1/auth/me` returns `{ user_id, email, tenants: [...] }` with `dev-tenant` membership
- [ ] `POST /v1/auth/logout` → redirects to `/login`, UserMenu shows "Sign in" link
- [ ] API key still works: `curl -s -X POST http://localhost:4319/internal/validate -H 'Content-Type: application/json' -d '{"api_key":"dev-api-key-0000"}'` — returns 200 with tenant/role/environment
- [ ] `SELECT * FROM credential_audit_log WHERE auth_method = 'oidc_session' ORDER BY occurred_at DESC LIMIT 5;` — login row present
- [ ] Navigate to `http://localhost:5173/admin/identity` — identity settings table visible for tenant_admin

---

---

## Errata: Axum state typing for OIDC routes (fix before Task 9 build)

The plan as written uses per-route `.with_state()` with two different state types, which Axum 0.7 does not support without `FromRef`. The simplest fix is a single unified state. Apply this before running `cargo build` in Task 9.

**In `services/auth-service/src/oidc.rs`**, change the handler signatures to use a combined state:

```rust
// Replace the per-handler State types with this unified type:
#[derive(Clone)]
pub struct OidcState {
    pub db: sqlx::PgPool,
    pub config: OidcConfig,
}
```

Replace every `State<OidcConfig>` with `State<OidcState>` and every `State<(PgPool, OidcConfig)>` with `State<OidcState>`, renaming `cfg` to `state.config` and `pool` to `state.db` throughout.

**In `services/auth-service/src/main.rs`**, remove the separate `oidc_state` tuple. Build one `OidcState` and use it for all OIDC routes:

```rust
use oidc::{OidcConfig, OidcState};

// ...inside main():
let oidc_config = OidcConfig {
    issuer: std::env::var("ZITADEL_ISSUER")
        .unwrap_or_else(|_| "http://localhost:8082".into()),
    client_id: std::env::var("ZITADEL_CLIENT_ID")
        .unwrap_or_else(|_| "dev-client-id".into()),
    redirect_uri: std::env::var("ZITADEL_REDIRECT_URI")
        .unwrap_or_else(|_| "http://localhost:5173/auth/callback".into()),
    session_secret: std::env::var("SESSION_SECRET")
        .unwrap_or_else(|_| "dev-session-secret-change-in-prod!!".into()),
};
let oidc_state = OidcState { db: db.clone(), config: oidc_config.clone() };

let app = Router::new()
    .route("/health", get(|| async { StatusCode::OK }))
    .route("/internal/validate", post(validate_handler))
    .route("/v1/auth/login",    get(oidc::login_handler))
    .route("/v1/auth/callback", get(oidc::callback_handler))
    .route("/v1/auth/logout",   post(oidc::logout_handler))
    .route("/v1/auth/me",       get(oidc::me_handler))
    .layer(TraceLayer::new_for_http()
        .make_span_with(domain::telemetry::OtelMakeSpan::new(Level::INFO)))
    .with_state(oidc_state);
// Note: validate_handler uses State<AppState> — move AppState to use OidcState or
// keep validate_handler stateless using only the db from OidcState.
// Simplest: pass db via OidcState.db in validate_handler too:

async fn validate_handler(
    State(state): State<OidcState>,
    Json(req): Json<ValidateRequest>,
) -> Result<Json<ValidateResponse>, StatusCode> {
    // replace state.db references — same body as before
```

Also fix the `logout_handler` bug where `Uuid::parse_str(&claims.sub)` result was unused:

```rust
pub async fn logout_handler(
    State(state): State<OidcState>,
    cookies: axum::http::HeaderMap,
) -> Response {
    if let Some(token) = extract_cookie(&cookies, "session") {
        if let Ok(claims) = verify_session_jwt(&state.config.session_secret, &token) {
            if let (Ok(user_id), Ok(tenant_id)) = (
                Uuid::parse_str(&claims.sub),
                Uuid::parse_str(&claims.tid),
            ) {
                let _ = sqlx::query(
                    "UPDATE user_sessions SET revoked_at = now() \
                     WHERE user_id = $1 AND tenant_id = $2 AND revoked_at IS NULL",
                )
                .bind(user_id)
                .bind(tenant_id)
                .execute(&state.db)
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
```

---

## Task 17: auth-service — internal session validation endpoint

The query-api (and any future service) must be able to validate a session JWT without holding the `SESSION_SECRET`. Add a lightweight internal endpoint to auth-service for this.

**Files:**
- Modify: `services/auth-service/src/main.rs`
- Modify: `services/auth-service/src/oidc.rs`

- [ ] **Step 1: Write a failing test for the new endpoint**

Add to `services/auth-service/tests/session_integration.rs`:

```rust
use auth_service::session::{sign_session_jwt, verify_session_jwt};
use uuid::Uuid;

#[test]
fn validate_session_returns_claims_for_valid_token() {
    let secret = "testsecretfortests1234567890abc";
    let user_id = Uuid::new_v4();
    let tenant_id = Uuid::new_v4();
    let token = sign_session_jwt(secret, user_id, tenant_id, "member", "production").unwrap();

    let claims = verify_session_jwt(secret, &token).unwrap();
    assert_eq!(claims.tid, tenant_id.to_string());
    assert_eq!(claims.role, "member");
}
```

- [ ] **Step 2: Run to confirm pass** (this tests `session.rs` which already exists)

```
cargo test --test session_integration validate_session
```

Expected: passes (logic already implemented in `session.rs`).

- [ ] **Step 3: Add the HTTP endpoint to oidc.rs**

Add to `services/auth-service/src/oidc.rs`:

```rust
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
/// Called by query-api and other internal services to validate a session JWT
/// without holding the SESSION_SECRET.
pub async fn validate_session_handler(
    State(state): State<OidcState>,
    Json(req): Json<ValidateSessionRequest>,
) -> Result<Json<ValidateSessionResponse>, StatusCode> {
    let claims = verify_session_jwt(&state.config.session_secret, &req.session_token)
        .map_err(|_| StatusCode::UNAUTHORIZED)?;

    // Verify the session row exists and is not revoked.
    let user_id = Uuid::parse_str(&claims.sub).map_err(|_| StatusCode::UNAUTHORIZED)?;
    let tenant_id = Uuid::parse_str(&claims.tid).map_err(|_| StatusCode::UNAUTHORIZED)?;

    let active: Option<Uuid> = sqlx::query_scalar(
        "SELECT id FROM user_sessions \
         WHERE user_id = $1 AND tenant_id = $2 \
           AND expires_at > now() AND revoked_at IS NULL \
         LIMIT 1",
    )
    .bind(user_id)
    .bind(tenant_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if active.is_none() {
        return Err(StatusCode::UNAUTHORIZED);
    }

    Ok(Json(ValidateSessionResponse {
        user_id: claims.sub,
        tenant_id: claims.tid,
        role: claims.role,
        environment: claims.env,
    }))
}
```

- [ ] **Step 4: Register the route in main.rs**

Add to the router in `main()`:

```rust
.route("/internal/validate-session", post(oidc::validate_session_handler))
```

- [ ] **Step 5: Build and test**

```
cargo build -p auth-service
cargo test --test postgres_integration
```

Expected: all pass.

- [ ] **Step 6: Commit**

```
git add services/auth-service/src/oidc.rs services/auth-service/src/main.rs
git commit -m "feat(auth-service): add /internal/validate-session endpoint for inter-service auth"
```

---

## Task 18: query-api — filter GET /v1/tenants by authenticated user

The `tenants.rs` handler already has a comment noting this is required when auth is in place. Extend it to filter by the caller's session when a `session` cookie is present.

**Files:**
- Modify: `services/query-api/src/tenants.rs`
- Modify: `services/query-api/src/config.rs` (add auth-service URL)

- [ ] **Step 1: Write the failing test**

Add to `services/query-api/` tests (or inline in `tenants.rs`):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;

    #[test]
    fn extract_session_cookie_returns_none_when_absent() {
        use axum::http::Request;
        let req = Request::builder().body(Body::empty()).unwrap();
        assert!(extract_session_cookie(req.headers()).is_none());
    }

    #[test]
    fn extract_session_cookie_returns_value() {
        use axum::http::{Request, header};
        let req = Request::builder()
            .header(header::COOKIE, "session=tok123; other=x")
            .body(Body::empty())
            .unwrap();
        assert_eq!(extract_session_cookie(req.headers()), Some("tok123".to_string()));
    }
}
```

- [ ] **Step 2: Run to confirm failure**

```
cargo test -p query-api -- extract_session 2>&1 | head -20
```

Expected: compile error — `extract_session_cookie` not defined.

- [ ] **Step 3: Extend tenants.rs**

Replace the full content of `services/query-api/src/tenants.rs`:

```rust
use crate::traces::AppState;
use axum::{
    extract::{Path, State},
    http::{header, HeaderMap, StatusCode},
    Json,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Serialize)]
pub struct TenantRecord {
    pub id: Uuid,
    pub name: String,
}

#[derive(Serialize)]
pub struct TenantListResponse {
    pub tenants: Vec<TenantRecord>,
}

#[derive(Serialize)]
pub struct EnvironmentRecord {
    pub environment: String,
}

#[derive(Serialize)]
pub struct EnvironmentListResponse {
    pub environments: Vec<EnvironmentRecord>,
}

#[derive(Deserialize)]
struct ValidateSessionResponse {
    user_id: String,
    tenant_id: String,
}

/// GET /v1/tenants
/// Without a session cookie: returns all tenants (backwards-compatible for API-key callers).
/// With a session cookie: filters to only the tenants the authenticated user belongs to.
pub async fn list_tenants(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<TenantListResponse>, StatusCode> {
    if let Some(session_token) = extract_session_cookie(&headers) {
        // Validate the session and get the user's tenant memberships.
        let user_id = validate_session_with_auth_service(&state.auth_service_url, &session_token)
            .await
            .map_err(|_| StatusCode::UNAUTHORIZED)?;

        let rows = sqlx::query!(
            r#"
            SELECT t.id, t.name
            FROM tenants t
            JOIN user_tenant_roles utr ON utr.tenant_id = t.id
            WHERE utr.user_id = $1
            ORDER BY t.name ASC
            "#,
            user_id,
        )
        .fetch_all(&state.db)
        .await
        .map_err(|e| {
            tracing::error!(error = ?e, "Failed to list user tenants");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

        return Ok(Json(TenantListResponse {
            tenants: rows.into_iter().map(|r| TenantRecord { id: r.id, name: r.name }).collect(),
        }));
    }

    // No session cookie — legacy path: return all tenants.
    let rows = sqlx::query!(r#"SELECT id, name FROM tenants ORDER BY name ASC"#)
        .fetch_all(&state.db)
        .await
        .map_err(|e| {
            tracing::error!(error = ?e, "Failed to list tenants");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok(Json(TenantListResponse {
        tenants: rows.into_iter().map(|r| TenantRecord { id: r.id, name: r.name }).collect(),
    }))
}

/// GET /v1/tenants/:id/environments
pub async fn list_tenant_environments(
    State(state): State<AppState>,
    Path(tenant_id): Path<Uuid>,
) -> Result<Json<EnvironmentListResponse>, StatusCode> {
    let rows = sqlx::query_scalar!(
        r#"
        SELECT DISTINCT environment
        FROM api_keys
        WHERE tenant_id = $1
          AND revoked_at IS NULL
          AND environment != ''
        ORDER BY environment ASC
        "#,
        tenant_id,
    )
    .fetch_all(&state.db)
    .await
    .map_err(|e| {
        tracing::error!(error = ?e, "Failed to list tenant environments");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(Json(EnvironmentListResponse {
        environments: rows.into_iter().map(|e| EnvironmentRecord { environment: e }).collect(),
    }))
}

// ── Helpers ───────────────────────────────────────────────────────────────────

pub fn extract_session_cookie(headers: &HeaderMap) -> Option<String> {
    let cookie = headers.get(header::COOKIE)?.to_str().ok()?;
    cookie.split(';').map(str::trim).find_map(|part| {
        let (k, v) = part.split_once('=')?;
        if k.trim() == "session" { Some(v.trim().to_owned()) } else { None }
    })
}

async fn validate_session_with_auth_service(
    auth_service_url: &str,
    session_token: &str,
) -> anyhow::Result<Uuid> {
    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{auth_service_url}/internal/validate-session"))
        .json(&serde_json::json!({ "session_token": session_token }))
        .send()
        .await?;

    if !resp.status().is_success() {
        anyhow::bail!("session validation failed: {}", resp.status());
    }

    let body: ValidateSessionResponse = resp.json().await?;
    Ok(Uuid::parse_str(&body.user_id)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::{Request, header};
    use axum::body::Body;

    #[test]
    fn extract_session_cookie_returns_none_when_absent() {
        let req = Request::builder().body(Body::empty()).unwrap();
        assert!(extract_session_cookie(req.headers()).is_none());
    }

    #[test]
    fn extract_session_cookie_returns_value() {
        let req = Request::builder()
            .header(header::COOKIE, "session=tok123; other=x")
            .body(Body::empty())
            .unwrap();
        assert_eq!(extract_session_cookie(req.headers()), Some("tok123".to_string()));
    }
}
```

- [ ] **Step 4: Add `auth_service_url` to `AppState` in query-api**

Find `AppState` in `services/query-api/src/traces.rs` (or wherever it's defined). Add the new field:

```rust
// In the AppState struct, add:
pub auth_service_url: String,
```

In `services/query-api/src/main.rs` (or wherever AppState is built), read from env:

```rust
auth_service_url: std::env::var("AUTH_SERVICE_URL")
    .unwrap_or_else(|_| "http://auth-service:4319".into()),
```

Also add `reqwest = { workspace = true }` to `services/query-api/Cargo.toml` if not already present.

- [ ] **Step 5: Run query-api tests**

```
cargo test -p query-api -- extract_session
```

Expected: both `extract_session_cookie` tests pass.

- [ ] **Step 6: Build**

```
cargo build -p query-api
```

Expected: no errors.

- [ ] **Step 7: Commit**

```
git add services/query-api/src/tenants.rs services/query-api/src/traces.rs services/query-api/src/main.rs services/query-api/Cargo.toml
git commit -m "feat(query-api): filter GET /v1/tenants by authenticated user's memberships"
```

---

## SCIM Reference (P4-S3b, not implemented)

When a customer needs SCIM provisioning, no Observable code changes are needed. The configuration is:

1. In Zitadel Admin Console → their organization → **Security** → **SCIM** → Enable
2. The SCIM 2.0 endpoint is: `https://<zitadel-host>/scim/v2/<org-id>/`
3. The customer configures their Azure AD / Okta directory connector to point at this URL with a service-account PAT
4. Zitadel handles user and group lifecycle; Observable sees up-to-date user records on next OIDC login (no webhook needed for P4-S3b)

If role sync from SCIM groups to `user_tenant_roles` is required, add a Zitadel webhook action that calls a new `POST /internal/sync-user-role` endpoint on auth-service.

---

## Future: Migrate to Zitadel Login V2

**Context (discovered during v4.13.1 upgrade, 2026-05-14):**
Zitadel v4 defaults to its new "Login V2" UI (`/ui/v2/login`), which is a standalone Next.js application (`ghcr.io/zitadel/zitadel-login`) that must be deployed separately — it is not bundled in the main Zitadel container. Upgrading docker-compose to v4.13.1 broke login with a 404 because nothing was serving that path. The workaround in `scripts/zitadel-bootstrap.sh` disables the Login V2 feature flag via `PUT /v2/features/instance` so the classic login UI at `/ui/login` (still bundled in the main container) is used instead.

**What migration involves:**
1. Add `ghcr.io/zitadel/zitadel-login:latest` as a service in docker-compose and Helm.
2. Add a reverse proxy (nginx or Caddy) to route `/ui/v2/login/*` to the login service and everything else to Zitadel — both must be accessible on the same origin (`localhost:8082` / the external domain) because the authorize redirect is host-relative.
3. Generate a PAT for the bootstrap service account and write it to a shared volume so the login service can authenticate to Zitadel's API (`ZITADEL_SERVICE_USER_TOKEN_FILE`).
4. Remove the `PUT /v2/features/instance` disable call from the bootstrap script and instead call it to enable Login V2 with the correct base URI.
5. Update the Helm values for production to deploy the login service and proxy alongside Zitadel.

**Why bother:** Login V2 is the Zitadel-supported path forward; the classic UI will eventually be deprecated. Login V2 is also easier to brand (React/Next.js) and supports newer Zitadel features first.
