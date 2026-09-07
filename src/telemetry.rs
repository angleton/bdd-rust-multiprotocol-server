use serde::Serialize;
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
    time::Duration,
};

use crate::workload::WorkloadSnapshot;

const PROTOCOLS: [&str; 6] = ["fix", "grpc", "graphql", "rest", "soap", "websocket"];

#[derive(Clone, Default)]
pub struct Telemetry {
    protocols: Arc<Mutex<BTreeMap<String, ProtocolTelemetry>>>,
    workload: Arc<Mutex<Option<WorkloadSnapshot>>>,
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
    pub workload: Option<WorkloadSnapshot>,
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

    pub(crate) fn record_workload(&self, workload: WorkloadSnapshot) {
        *self.workload.lock().expect("workload lock poisoned") = Some(workload);
    }

    pub fn snapshot(&self) -> TelemetrySnapshot {
        let protocols = self.protocols.lock().expect("telemetry lock poisoned");
        let workload = self
            .workload
            .lock()
            .expect("workload lock poisoned")
            .clone();
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
                            workload: workload.clone(),
                        },
                    )
                })
                .chain(
                    PROTOCOLS
                        .iter()
                        .filter(|protocol| !protocols.contains_key(**protocol))
                        .map(|protocol| {
                            (
                                (*protocol).to_string(),
                                ProtocolSnapshot {
                                    workload: workload.clone(),
                                    ..ProtocolSnapshot::default()
                                },
                            )
                        }),
                )
                .collect(),
        }
    }
}
