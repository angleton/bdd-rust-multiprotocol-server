use crate::{protocols::protocol_response, AppState};
use axum::{
    extract::{Query, State},
    http::HeaderMap,
};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub(crate) struct PayloadQuery {
    pub(crate) payload: Option<String>,
}

pub(crate) async fn hello_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<PayloadQuery>,
) -> String {
    let started = std::time::Instant::now();
    crate::workload::run_from_headers(&headers, &state.telemetry).await;
    let payload = query.payload.unwrap_or_default();
    let response = protocol_response("REST");
    state.telemetry.record(
        "rest",
        true,
        started.elapsed(),
        payload.len() as u64,
        response.len() as u64,
    );
    response
}
