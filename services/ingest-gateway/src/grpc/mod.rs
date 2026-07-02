pub mod convert;
pub mod log;
pub mod metric;
pub mod trace;

use opentelemetry_proto::tonic::collector::logs::v1::logs_service_server::LogsServiceServer;
use opentelemetry_proto::tonic::collector::metrics::v1::metrics_service_server::MetricsServiceServer;
use opentelemetry_proto::tonic::collector::trace::v1::trace_service_server::TraceServiceServer;
use tonic::codec::CompressionEncoding;
use tonic::transport::Server;

use crate::AppState;

pub async fn start_grpc_server(state: AppState, port: u16) -> anyhow::Result<()> {
    let addr = format!("0.0.0.0:{}", port).parse()?;

    let trace_service = trace::OltpTraceService::new(state.clone());
    let log_service = log::OltpLogService::new(state.clone());
    let metric_service = metric::OltpMetricService::new(state.clone());

    tracing::info!(port, "ingest-gateway gRPC listening");

    Server::builder()
        .add_service(TraceServiceServer::new(trace_service).accept_compressed(CompressionEncoding::Gzip))
        .add_service(LogsServiceServer::new(log_service).accept_compressed(CompressionEncoding::Gzip))
        .add_service(MetricsServiceServer::new(metric_service).accept_compressed(CompressionEncoding::Gzip))
        .serve(addr)
        .await?;

    Ok(())
}
