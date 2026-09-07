use crate::{activity, protocols::protocol_response, AppState};
use axum::extract::{Query, State};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub(crate) struct PayloadQuery {
    pub(crate) payload: Option<String>,
}

pub(crate) async fn hello_handler(
    State(state): State<AppState>,
    Query(query): Query<PayloadQuery>,
) -> String {
    let started = std::time::Instant::now();
    let payload = query.payload.unwrap_or_default();
    activity::request("rest", format!("GET /hello payload_bytes={}", payload.len()));
    let response = protocol_response("REST");
    let duration = started.elapsed();
    state.telemetry.record(
        "rest",
        true,
        duration,
        payload.len() as u64,
        response.len() as u64,
    );
    activity::response("rest", &response, duration, response.len());
    response
}
