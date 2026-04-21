use crate::AppState;
use opentelemetry_proto::tonic::collector::logs::v1::{
    logs_service_server::LogsService, ExportLogsServiceRequest, ExportLogsServiceResponse,
};
use tonic::{Request, Response, Status};

pub struct OltpLogService {
    state: AppState,
}

impl OltpLogService {
    pub fn new(state: AppState) -> Self {
        Self { state }
    }
}

#[tonic::async_trait]
impl LogsService for OltpLogService {
    async fn export(
        &self,
        request: Request<ExportLogsServiceRequest>,
    ) -> Result<Response<ExportLogsServiceResponse>, Status> {
        let metadata = request.metadata();
        let auth_header = metadata
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
            .ok_or_else(|| Status::unauthenticated("missing authorization token"))?;

        let (tenant_id, role) = self
            .state
            .validate_api_key(auth_header)
            .await
            .map_err(|_| Status::unauthenticated("invalid authorization token"))?;

        if role == "viewer" {
            return Err(Status::permission_denied("viewer role cannot ingest logs"));
        }

        if self.state.log_rate_limiter.check_key(&tenant_id).is_err() {
            return Err(Status::resource_exhausted("log ingest rate limit exceeded"));
        }

        tracing::info!(tenant_id = %tenant_id, "received gRPC log export");

        Ok(Response::new(ExportLogsServiceResponse {
            partial_success: None,
        }))
    }
}
