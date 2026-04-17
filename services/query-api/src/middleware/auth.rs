use axum::{extract::Request, http::StatusCode, middleware::Next, response::Response};
use uuid::Uuid;

#[derive(Clone)]
pub struct TenantContext {
    #[allow(dead_code)]
    pub tenant_id: Uuid,
}

pub async fn require_tenant(mut req: Request, next: Next) -> Result<Response, StatusCode> {
    // Phase 1: accept X-Tenant-ID header (set by ingest-gateway for internal calls).
    // Phase 2: replace with bearer token validation via auth-service.
    let tenant_id: Uuid = req
        .headers()
        .get("X-Tenant-ID")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse().ok())
        // Fallback to dev tenant for local development
        .unwrap_or_else(|| Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap());

    req.extensions_mut().insert(TenantContext { tenant_id });
    Ok(next.run(req).await)
}
