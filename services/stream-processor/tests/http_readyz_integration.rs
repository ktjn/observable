use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
    routing::get,
};
use stream_processor::readyz::{StreamProcessorProbeState, readyz};
use tower::ServiceExt;

fn test_probe_app(brokers: &str) -> Router {
    let probe_state = StreamProcessorProbeState {
        brokers: brokers.to_string(),
        metrics_registry: None,
    };
    Router::new()
        .route("/health", get(|| async { StatusCode::OK }))
        .route("/readyz", get(readyz))
        .with_state(probe_state)
}

#[tokio::test]
async fn stream_processor_readyz_returns_503_when_redpanda_unavailable() {
    // Port 1 is never valid; rdkafka metadata fetch fails immediately.
    let app = test_probe_app("127.0.0.1:1");

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/readyz")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("router responded");

    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
}

#[tokio::test]
#[ignore]
async fn stream_processor_readyz_returns_200_when_redpanda_reachable() {
    use testcontainers::{
        GenericImage, ImageExt,
        core::{IntoContainerPort, WaitFor},
        runners::AsyncRunner,
    };

    let container = GenericImage::new("redpandadata/redpanda", "v24.3.1")
        .with_wait_for(WaitFor::message_on_stdout("Successfully started Redpanda!"))
        .with_exposed_port(9092_u16.tcp())
        .with_cmd([
            "redpanda",
            "start",
            "--overprovisioned",
            "--smp",
            "1",
            "--memory",
            "512M",
            "--reserve-memory",
            "0M",
            "--node-id",
            "0",
            "--check=false",
        ])
        .start()
        .await
        .expect("redpanda started");
    let port = container.get_host_port_ipv4(9092).await.unwrap();

    let app = test_probe_app(&format!("127.0.0.1:{port}"));

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/readyz")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("router responded");

    assert_eq!(response.status(), StatusCode::OK);
}
