use crate::Telemetry;
use axum::http::HeaderMap;
use serde::Serialize;
use std::time::{Duration, Instant};

const CPU_HEADER: &str = "x-workload-cpu-percent";
const DURATION_HEADER: &str = "x-workload-duration-ms";
const MAX_CPU_PERCENT: u8 = 100;
const MAX_DURATION_MS: u64 = 60_000;
const SLICE_MS: u64 = 10;

#[derive(Clone, Debug, Serialize)]
pub struct WorkloadSnapshot {
    pub target_cpu_percent: u8,
    pub target_duration_ms: u64,
    pub observed_duration_ms: u64,
    pub observed_cpu_percent: f64,
}

pub fn from_headers(headers: &HeaderMap) -> Option<(u8, u64)> {
    let cpu_percent = headers.get(CPU_HEADER)?.to_str().ok()?.parse().ok()?;
    let duration_ms = headers.get(DURATION_HEADER)?.to_str().ok()?.parse().ok()?;

    from_values(cpu_percent, duration_ms)
}

pub fn from_values(cpu_percent: u8, duration_ms: u64) -> Option<(u8, u64)> {
    (cpu_percent <= MAX_CPU_PERCENT && duration_ms <= MAX_DURATION_MS)
        .then_some((cpu_percent, duration_ms))
}

pub async fn run_from_headers(headers: &HeaderMap, telemetry: &Telemetry) {
    if let Some((cpu_percent, duration_ms)) = from_headers(headers) {
        telemetry.record_workload(run(cpu_percent, duration_ms).await);
    }
}

pub async fn run(cpu_percent: u8, duration_ms: u64) -> WorkloadSnapshot {
    let started = Instant::now();
    let target = Duration::from_millis(duration_ms);
    // Coarse OS timer wakeups add idle time on Windows, so compensate the
    // requested duty cycle while retaining the measured result in telemetry.
    let busy_duration =
        Duration::from_micros(SLICE_MS * 1_000 * u64::from(cpu_percent) * 3 / 2 / 100);
    let idle_duration = Duration::from_millis(SLICE_MS).saturating_sub(busy_duration);
    let mut busy = Duration::ZERO;

    while started.elapsed() < target {
        let busy_started = Instant::now();
        while busy_started.elapsed() < busy_duration {
            std::hint::black_box(std::hint::black_box(1_u64).wrapping_mul(31));
        }
        busy += busy_started.elapsed().min(busy_duration);

        if idle_duration > Duration::ZERO {
            tokio::time::sleep(idle_duration).await;
        }
    }

    let observed_duration = started.elapsed();
    let observed_cpu_percent = if observed_duration.is_zero() {
        0.0
    } else {
        busy.as_secs_f64() / observed_duration.as_secs_f64() * 100.0
    };

    WorkloadSnapshot {
        target_cpu_percent: cpu_percent,
        target_duration_ms: duration_ms,
        observed_duration_ms: observed_duration.as_millis() as u64,
        observed_cpu_percent,
    }
}
