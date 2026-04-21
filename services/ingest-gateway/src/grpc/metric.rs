use crate::queue::producer::build_envelope;
use crate::AppState;
use opentelemetry_proto::tonic::collector::metrics::v1::{
    metrics_service_server::MetricsService, ExportMetricsServiceRequest,
    ExportMetricsServiceResponse,
};
use tonic::{Request, Response, Status};

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

        let (tenant_id, role) = self
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
        let (series, points) =
            super::convert::proto_metrics_to_domain(&inner.resource_metrics, tenant_id);

        tracing::info!(
            tenant_id = %tenant_id,
            series_count = series.len(),
            point_count = points.len(),
            "received gRPC metric export"
        );

        if let Some(producer) = &self.state.producer {
            let envelope = build_envelope(
                tenant_id,
                domain::EnvelopePayload::Metrics { series, points },
            );
            producer
                .publish(&envelope)
                .await
                .map_err(|_| Status::internal("failed to publish metrics"))?;
        }

        Ok(Response::new(ExportMetricsServiceResponse {
            partial_success: None,
        }))
    }
}
