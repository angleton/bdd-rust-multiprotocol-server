use serde::Serialize;
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
    time::Duration,
};

#[derive(Clone, Default)]
pub struct Telemetry {
    protocols: Arc<Mutex<BTreeMap<String, ProtocolTelemetry>>>,
}

#[derive(Clone, Default, Serialize)]
struct ProtocolTelemetry {
    requests: u64,
    failures: u64,
    total_duration_ns: u64,
    request_bytes: u64,
    response_bytes: u64,
}

#[derive(Clone, Default, Serialize)]
pub struct TelemetrySnapshot {
    pub protocols: BTreeMap<String, ProtocolSnapshot>,
}

#[derive(Clone, Default, Serialize)]
pub struct ProtocolSnapshot {
    pub requests: u64,
    pub failures: u64,
    pub average_duration_us: f64,
    pub request_bytes: u64,
    pub response_bytes: u64,
}

impl Telemetry {
    pub(crate) fn record(
        &self,
        protocol: &str,
        success: bool,
        duration: Duration,
        request_bytes: u64,
        response_bytes: u64,
    ) {
        let mut protocols = self.protocols.lock().expect("telemetry lock poisoned");
        let entry = protocols.entry(protocol.to_string()).or_default();
        entry.requests += 1;
        if !success {
            entry.failures += 1;
        }
        entry.total_duration_ns += duration.as_nanos() as u64;
        entry.request_bytes += request_bytes;
        entry.response_bytes += response_bytes;
    }

    pub fn snapshot(&self) -> TelemetrySnapshot {
        let protocols = self.protocols.lock().expect("telemetry lock poisoned");
        TelemetrySnapshot {
            protocols: protocols
                .iter()
                .map(|(protocol, metrics)| {
                    (
                        protocol.clone(),
                        ProtocolSnapshot {
                            requests: metrics.requests,
                            failures: metrics.failures,
                            average_duration_us: if metrics.requests == 0 {
                                0.0
                            } else {
                                metrics.total_duration_ns as f64 / metrics.requests as f64 / 1_000.0
                            },
                            request_bytes: metrics.request_bytes,
                            response_bytes: metrics.response_bytes,
                        },
                    )
                })
                .collect(),
        }
    }
}
