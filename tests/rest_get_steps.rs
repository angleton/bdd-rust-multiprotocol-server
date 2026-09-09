use cucumber::{given, then, when, World};
use std::net::SocketAddr;
use std::time::Duration;

#[derive(Debug, Default, World)]
struct RestGetWorld {
    response_body: Option<String>,
}

#[given("the server is running")]
async fn server_is_running(_world: &mut RestGetWorld) {
    let addr: SocketAddr = "127.0.0.1:8080".parse().unwrap();
    let listener = bdd_rust_multiprotocol_server::bind_http(addr)
        .await
        .expect("Cannot start test server: port 8080 is already in use. Stop the standalone server (`cargo run`) before running BDD tests.");

    tokio::spawn(async move {
        bdd_rust_multiprotocol_server::run_with_listener(addr, listener).await;
    });

    tokio::time::sleep(Duration::from_millis(500)).await;
}

#[when(expr = "I send a REST GET request to {string}")]
async fn send_rest_get_request(world: &mut RestGetWorld, path: String) {
    let url = format!("http://127.0.0.1:8080{}", path);

    let response = reqwest::get(url)
        .await
        .expect("failed to send REST GET request");

    let body = response.text().await.expect("failed to read response body");

    world.response_body = Some(body);
}

#[then(expr = "the response body should be {string}")]
async fn response_body_should_be(world: &mut RestGetWorld, expected: String) {
    assert_eq!(world.response_body.as_deref(), Some(expected.as_str()));
}

#[tokio::test]
async fn rest_get_feature() {
    RestGetWorld::run("features/rest_get.feature").await;
}
