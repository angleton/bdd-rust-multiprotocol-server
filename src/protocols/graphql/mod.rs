use async_graphql::{EmptyMutation, EmptySubscription, Object, Schema};
use async_graphql_axum::{GraphQLRequest, GraphQLResponse};
use axum::extract::State;

use crate::{activity, protocols::protocol_response, AppState};

pub(crate) type AppSchema = Schema<QueryRoot, EmptyMutation, EmptySubscription>;

pub(crate) fn schema() -> AppSchema {
    Schema::build(QueryRoot, EmptyMutation, EmptySubscription).finish()
}

pub(crate) struct QueryRoot;

#[Object]
impl QueryRoot {
    async fn hello(&self, _payload: Option<String>) -> String {
        protocol_response("GraphQL")
    }
}

pub(crate) async fn handler(
    State(state): State<AppState>,
    request: GraphQLRequest,
) -> GraphQLResponse {
    let started = std::time::Instant::now();
    let request = request.into_inner().data(started);
    let request_bytes = request.query.len() as u64;
    activity::request("graphql", format!("POST /graphql query_bytes={request_bytes}"));
    let response = state.schema.execute(request).await;
    let response_bytes = serde_json::to_vec(&response).map_or(0, |body| body.len());
    let duration = started.elapsed();
    state.telemetry.record(
        "graphql",
        response.errors.is_empty(),
        duration,
        request_bytes,
        response_bytes as u64,
    );
    activity::response("graphql", "GraphQL response", duration, response_bytes);
    response.into()
}
