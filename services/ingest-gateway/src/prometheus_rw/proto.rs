/// Hand-rolled prost Message types for Prometheus remote_write v1.
///
/// Field numbers match the official prometheus/prometheus prompb/types.proto
/// and remote.proto (stable since 2019 — no build.rs needed).

#[derive(prost::Message)]
pub struct WriteRequest {
    #[prost(message, repeated, tag = "1")]
    pub timeseries: Vec<TimeSeries>,
    // tag 3 (metadata) intentionally omitted — not needed for ingestion
}

#[derive(prost::Message)]
pub struct TimeSeries {
    #[prost(message, repeated, tag = "1")]
    pub labels: Vec<Label>,
    #[prost(message, repeated, tag = "2")]
    pub samples: Vec<Sample>,
}

#[derive(prost::Message, Clone)]
pub struct Label {
    #[prost(string, tag = "1")]
    pub name: String,
    #[prost(string, tag = "2")]
    pub value: String,
}

#[derive(prost::Message, Clone)]
pub struct Sample {
    #[prost(double, tag = "1")]
    pub value: f64,
    /// Milliseconds since Unix epoch (not nanoseconds).
    #[prost(int64, tag = "2")]
    pub timestamp: i64,
}

#[cfg(test)]
mod tests {
    use prost::Message;

    use super::*;

    #[test]
    fn round_trip_write_request() {
        let original = WriteRequest {
            timeseries: vec![TimeSeries {
                labels: vec![
                    Label {
                        name: "__name__".into(),
                        value: "http_requests_total".into(),
                    },
                    Label {
                        name: "job".into(),
                        value: "api-server".into(),
                    },
                ],
                samples: vec![Sample {
                    value: 42.0,
                    timestamp: 1_700_000_000_000,
                }],
            }],
        };

        let mut buf = Vec::new();
        original.encode(&mut buf).unwrap();

        let decoded = WriteRequest::decode(buf.as_slice()).unwrap();
        assert_eq!(decoded.timeseries.len(), 1);
        assert_eq!(decoded.timeseries[0].labels[0].name, "__name__");
        assert_eq!(decoded.timeseries[0].labels[0].value, "http_requests_total");
        assert_eq!(decoded.timeseries[0].samples[0].value, 42.0);
        assert_eq!(
            decoded.timeseries[0].samples[0].timestamp,
            1_700_000_000_000
        );
    }
}
