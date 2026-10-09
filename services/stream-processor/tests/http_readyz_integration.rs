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
    use std::net::TcpListener;
    use testcontainers::{
        GenericImage, ImageExt,
        core::{IntoContainerPort, WaitFor},
        runners::AsyncRunner,
    };

    // Bind to port 0 to let the OS pick a free host port, then release it and tell
    // Redpanda to advertise exactly that address. This keeps rdkafka's metadata
    // discovery working for the single-node setup: it follows the advertised broker
    // address from the metadata response, which must resolve back to the mapped port.
    let host_port = {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind to 0 to get free port");
        listener.local_addr().unwrap().port()
    };
    let advertise_addr = format!("127.0.0.1:{host_port}");

    let _container = GenericImage::new("redpandadata/redpanda", "v26.2.4")
        .with_wait_for(WaitFor::message_on_stderr("Successfully started Redpanda!"))
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
            "--kafka-addr",
            "0.0.0.0:9092",
            "--advertise-kafka-addr",
            &advertise_addr,
        ])
        .with_mapped_port(host_port, 9092_u16.tcp())
        .start()
        .await
        .expect("redpanda started");

    let app = test_probe_app(&advertise_addr);

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
