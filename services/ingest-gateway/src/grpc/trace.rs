use crate::queue::producer::build_envelope;
use crate::AppState;
use opentelemetry_proto::tonic::collector::trace::v1::{
    trace_service_server::TraceService, ExportTraceServiceRequest, ExportTraceServiceResponse,
};
use tonic::{Request, Response, Status};

pub struct OltpTraceService {
    state: AppState,
}

impl OltpTraceService {
    pub fn new(state: AppState) -> Self {
        Self { state }
    }
}

#[tonic::async_trait]
impl TraceService for OltpTraceService {
    async fn export(
        &self,
        request: Request<ExportTraceServiceRequest>,
    ) -> Result<Response<ExportTraceServiceResponse>, Status> {
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
            return Err(Status::permission_denied(
                "viewer role cannot ingest traces",
            ));
        }

        if self.state.trace_rate_limiter.check_key(&tenant_id).is_err() {
            return Err(Status::resource_exhausted(
                "trace ingest rate limit exceeded",
            ));
        }

        let inner = request.into_inner();
        let spans = super::convert::proto_spans_to_domain(&inner.resource_spans, tenant_id);

        tracing::info!(tenant_id = %tenant_id, span_count = spans.len(), "received gRPC trace export");

        if let Some(producer) = &self.state.producer {
            let envelope = build_envelope(tenant_id, domain::EnvelopePayload::Spans(spans));
            producer
                .publish(&envelope)
                .await
                .map_err(|_| Status::internal("failed to publish spans"))?;
        }

        Ok(Response::new(ExportTraceServiceResponse {
            partial_success: None,
        }))
    }
}
