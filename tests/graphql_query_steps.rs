use std::time::Duration;

use cucumber::{given, then, when, World};
use reqwest::Client;
use serde_json::Value;
use tokio::task::JoinHandle;

#[derive(Debug, Default, World)]
struct GraphqlWorld {
    server_handle: Option<JoinHandle<()>>,
    response_body: Option<Value>,
}

#[given("the server is running")]
async fn server_is_running(world: &mut GraphqlWorld) {
    let app = bdd_rust_multiprotocol_server::app();

    let listener = tokio::net::TcpListener::bind("127.0.0.1:8080")
        .await
        .expect("Failed to bind test server to port 8080");

    let handle = tokio::spawn(async move {
        axum::serve(listener, app).await.expect("Server failed");
    });

    world.server_handle = Some(handle);
    tokio::time::sleep(Duration::from_millis(250)).await;
}

#[when(expr = "I send a GraphQL query for {string}")]
async fn send_graphql_query(world: &mut GraphqlWorld, field: String) {
    let query = format!("{{ {field} }}");
    let response = Client::new()
        .post("http://127.0.0.1:8080/graphql")
        .json(&serde_json::json!({ "query": query }))
        .send()
        .await
        .expect("Failed to send GraphQL request");

    assert!(response.status().is_success());
    world.response_body = Some(
        response
            .json()
            .await
            .expect("Failed to decode GraphQL response"),
    );
}

#[then(expr = "the GraphQL response data should be {string}")]
async fn graphql_response_should_be(world: &mut GraphqlWorld, expected: String) {
    let actual = world
        .response_body
        .as_ref()
        .and_then(|body| body.get("data"))
        .and_then(|data| data.get("hello"))
        .and_then(Value::as_str);

    assert_eq!(actual, Some(expected.as_str()));
}

#[tokio::test]
async fn graphql_query_feature() {
    GraphqlWorld::cucumber()
        .run("./features/graphql_query.feature")
        .await;
}
