use crate::AppState;
use crate::queue::producer::build_envelope;
use opentelemetry_proto::tonic::collector::trace::v1::{
    ExportTraceServiceRequest, ExportTraceServiceResponse, trace_service_server::TraceService,
};
use tonic::{Request, Response, Status};
use tracing::Instrument as _;

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

        let (tenant_id, role, environment) = self
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
        let producer = self.state.producer.clone();
        let span = if domain::telemetry::is_self_telemetry_env(&environment) {
            tracing::Span::none()
        } else {
            tracing::info_span!("grpc.export.traces", %tenant_id, %environment)
        };
        async move {
            let spans = super::convert::proto_spans_to_domain(
                &inner.resource_spans,
                tenant_id,
                &environment,
            );
            tracing::info!(tenant_id = %tenant_id, span_count = spans.len(), "received gRPC trace export");
            if let Some(ref producer) = producer {
                let envelope = build_envelope(
                    tenant_id,
                    &environment,
                    domain::EnvelopePayload::Spans(spans),
                );
                producer
                    .publish(&envelope)
                    .await
                    .map_err(|_| Status::internal("failed to publish spans"))?;
            }
            Ok(Response::new(ExportTraceServiceResponse {
                partial_success: None,
            }))
        }
        .instrument(span)
        .await
    }
}
