use async_graphql::{EmptyMutation, EmptySubscription, Object, Schema};
use async_graphql_axum::{GraphQLRequest, GraphQLResponse};
use axum::extract::State;

use crate::{protocols::protocol_response, AppState};

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
    let response = state.schema.execute(request).await;
    let response_bytes = serde_json::to_vec(&response).map_or(0, |body| body.len() as u64);
    state.telemetry.record(
        "graphql",
        response.errors.is_empty(),
        started.elapsed(),
        request_bytes,
        response_bytes,
    );
    response.into()
}
