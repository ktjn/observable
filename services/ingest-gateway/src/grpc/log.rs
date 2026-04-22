use crate::queue::producer::build_envelope;
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

        let inner = request.into_inner();
        let logs = super::convert::proto_logs_to_domain(&inner.resource_logs, tenant_id);

        tracing::info!(tenant_id = %tenant_id, log_count = logs.len(), "received gRPC log export");

        if let Some(producer) = &self.state.producer {
            let envelope = build_envelope(tenant_id, domain::EnvelopePayload::Logs(logs));
            producer
                .publish(&envelope)
                .await
                .map_err(|_| Status::internal("failed to publish log records"))?;
        }

        Ok(Response::new(ExportLogsServiceResponse {
            partial_success: None,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use opentelemetry_proto::tonic::collector::logs::v1::logs_service_client::LogsServiceClient;
    use opentelemetry_proto::tonic::collector::logs::v1::logs_service_server::LogsServiceServer;
    use tonic::transport::Server;

    const TENANT: &str = "00000000-0000-0000-0000-000000000001";

    async fn start_test_server(state: AppState) -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let incoming = tokio_stream::wrappers::TcpListenerStream::new(listener);
        tokio::spawn(async move {
            Server::builder()
                .add_service(LogsServiceServer::new(OltpLogService::new(state)))
                .serve_with_incoming(incoming)
                .await
                .unwrap();
        });
        format!("http://127.0.0.1:{}", addr.port())
    }

    #[tokio::test]
    async fn grpc_logs_export_authenticated_returns_ok() {
        let addr = start_test_server(AppState::with_stub_auth(TENANT)).await;
        let mut client = LogsServiceClient::connect(addr).await.unwrap();
        let mut req = Request::new(ExportLogsServiceRequest {
            resource_logs: vec![],
        });
        req.metadata_mut()
            .insert("authorization", "Bearer dev-api-key-0000".parse().unwrap());
        let resp = client.export(req).await;
        assert!(resp.is_ok(), "expected OK, got: {:?}", resp);
    }

    #[tokio::test]
    async fn grpc_logs_export_no_token_returns_unauthenticated() {
        let addr = start_test_server(AppState::with_stub_auth(TENANT)).await;
        let mut client = LogsServiceClient::connect(addr).await.unwrap();
        let req = Request::new(ExportLogsServiceRequest {
            resource_logs: vec![],
        });
        let err = client.export(req).await.unwrap_err();
        assert_eq!(err.code(), tonic::Code::Unauthenticated);
    }

    #[tokio::test]
    async fn grpc_logs_export_invalid_token_returns_unauthenticated() {
        let addr = start_test_server(AppState::with_stub_auth(TENANT)).await;
        let mut client = LogsServiceClient::connect(addr).await.unwrap();
        let mut req = Request::new(ExportLogsServiceRequest {
            resource_logs: vec![],
        });
        req.metadata_mut()
            .insert("authorization", "Bearer invalid-token".parse().unwrap());
        let err = client.export(req).await.unwrap_err();
        assert_eq!(err.code(), tonic::Code::Unauthenticated);
    }
}
