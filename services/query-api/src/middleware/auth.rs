use axum::{extract::Request, http::StatusCode, middleware::Next, response::Response};
use uuid::Uuid;

#[derive(Clone, Debug)]
pub struct TenantContext {
    #[allow(dead_code)]
    pub tenant_id: Uuid,
}

pub async fn require_tenant(mut req: Request, next: Next) -> Result<Response, StatusCode> {
    let ctx = extract_tenant_context(&req)?;
    req.extensions_mut().insert(ctx);
    Ok(next.run(req).await)
}

fn extract_tenant_context(req: &Request) -> Result<TenantContext, StatusCode> {
    let tenant_id = req
        .headers()
        .get("X-Tenant-ID")
        .ok_or(StatusCode::UNAUTHORIZED)?
        .to_str()
        .ok()
        .and_then(|s| s.parse().ok())
        .ok_or(StatusCode::BAD_REQUEST)?;

    Ok(TenantContext { tenant_id })
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;

    #[test]
    fn missing_tenant_header_is_rejected() {
        let req = Request::builder().body(Body::empty()).unwrap();

        let err = extract_tenant_context(&req).unwrap_err();

        assert_eq!(err, StatusCode::UNAUTHORIZED);
    }

    #[test]
    fn invalid_tenant_header_is_rejected() {
        let req = Request::builder()
            .header("X-Tenant-ID", "not-a-uuid")
            .body(Body::empty())
            .unwrap();

        let err = extract_tenant_context(&req).unwrap_err();

        assert_eq!(err, StatusCode::BAD_REQUEST);
    }

    #[test]
    fn valid_tenant_header_builds_context() {
        let tenant_id = Uuid::new_v4();
        let req = Request::builder()
            .header("X-Tenant-ID", tenant_id.to_string())
            .body(Body::empty())
            .unwrap();

        let ctx = extract_tenant_context(&req).unwrap();

        assert_eq!(ctx.tenant_id, tenant_id);
    }
}
