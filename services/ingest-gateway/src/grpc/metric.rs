use crate::AppState;
use crate::queue::producer::build_envelope;
use opentelemetry_proto::tonic::collector::metrics::v1::{
    ExportMetricsServiceRequest, ExportMetricsServiceResponse,
    metrics_service_server::MetricsService,
};
use tonic::{Request, Response, Status};
use tracing::Instrument as _;

pub struct OltpMetricService {
    state: AppState,
}

impl OltpMetricService {
    pub fn new(state: AppState) -> Self {
        Self { state }
    }
}

#[tonic::async_trait]
impl MetricsService for OltpMetricService {
    async fn export(
        &self,
        request: Request<ExportMetricsServiceRequest>,
    ) -> Result<Response<ExportMetricsServiceResponse>, Status> {
        let metadata = request.metadata();
        let auth_header = metadata
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
            .ok_or_else(|| Status::unauthenticated("missing authorization token"))?;

        let observable_auth::ApiKeyContext {
            tenant_id,
            role,
            environment,
        } = self
            .state
            .validate_api_key(auth_header)
            .await
            .map_err(|_| Status::unauthenticated("invalid authorization token"))?;

        if role == "viewer" {
            return Err(Status::permission_denied(
                "viewer role cannot ingest metrics",
            ));
        }

        if self
            .state
            .metric_rate_limiter
            .check_key(&tenant_id)
            .is_err()
        {
            return Err(Status::resource_exhausted(
                "metric ingest rate limit exceeded",
            ));
        }

        let inner = request.into_inner();
        let producer = self.state.producer.clone();
        let span = if domain::telemetry::is_self_telemetry_env(&environment) {
            tracing::Span::none()
        } else {
            tracing::info_span!("grpc.export.metrics", %tenant_id, %environment)
        };
        async move {
            let (series, points, rejected_data_points) = super::convert::proto_metrics_to_domain(
                &inner.resource_metrics,
                tenant_id,
                &environment,
            );
            tracing::info!(
                tenant_id = %tenant_id,
                series_count = series.len(),
                point_count = points.len(),
                "received gRPC metric export"
            );
            if let Some(ref producer) = producer {
                let envelope = build_envelope(
                    tenant_id,
                    &environment,
                    domain::EnvelopePayload::Metrics { series, points },
                );
                producer
                    .publish(&envelope)
                    .await
                    .map_err(|_| Status::internal("failed to publish metrics"))?;
            }
            let partial_success = if rejected_data_points > 0 {
                use opentelemetry_proto::tonic::collector::metrics::v1::ExportMetricsPartialSuccess;
                Some(ExportMetricsPartialSuccess {
                    rejected_data_points: rejected_data_points as i64,
                    error_message: "ExponentialHistogram bucket detail and Summary quantile values not stored".to_string(),
                })
            } else {
                None
            };
            Ok(Response::new(ExportMetricsServiceResponse { partial_success }))
        }
        .instrument(span)
        .await
    }
}
