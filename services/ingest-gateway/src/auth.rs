use axum::{
    extract::{Request, State},
    http::StatusCode,
    middleware::Next,
    response::Response,
};
use uuid::Uuid;

#[derive(Clone, Debug)]
pub struct TenantContext {
    pub tenant_id: Uuid,
    pub role: String,
}

impl TenantContext {
    pub fn can_ingest(&self) -> bool {
        matches!(self.role.as_str(), "member" | "admin")
    }
}

pub async fn auth_middleware(
    State(state): State<crate::AppState>,
    mut req: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let bearer = req
        .headers()
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .ok_or(StatusCode::UNAUTHORIZED)?;

    let (tenant_id, role) = state
        .validate_api_key(bearer)
        .await
        .map_err(|_| StatusCode::UNAUTHORIZED)?;

    let ctx = TenantContext { tenant_id, role };
    if !ctx.can_ingest() {
        tracing::warn!(tenant_id = %ctx.tenant_id, role = %ctx.role, "ingest rejected: insufficient role");
        return Err(StatusCode::FORBIDDEN);
    }

    req.extensions_mut().insert(ctx);
    Ok(next.run(req).await)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn member_can_ingest() {
        let ctx = TenantContext {
            tenant_id: Uuid::new_v4(),
            role: "member".to_string(),
        };
        assert!(ctx.can_ingest());
    }

    #[test]
    fn admin_can_ingest() {
        let ctx = TenantContext {
            tenant_id: Uuid::new_v4(),
            role: "admin".to_string(),
        };
        assert!(ctx.can_ingest());
    }

    #[test]
    fn viewer_cannot_ingest() {
        let ctx = TenantContext {
            tenant_id: Uuid::new_v4(),
            role: "viewer".to_string(),
        };
        assert!(!ctx.can_ingest());
    }
}
