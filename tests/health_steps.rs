use cucumber::{given, then, when, World};
use std::net::SocketAddr;
use std::time::Duration;

#[derive(Debug, Default, World)]
struct HealthWorld {
    response_body: Option<String>,
}

#[given("the server is running")]
async fn server_is_running(_world: &mut HealthWorld) {
    let addr: SocketAddr = "127.0.0.1:8080".parse().unwrap();

    tokio::spawn(async move {
        bdd_rust_multiprotocol_server::run(addr).await;
    });

    tokio::time::sleep(Duration::from_millis(500)).await;
}

#[when("I request the health endpoint")]
async fn request_health_endpoint(world: &mut HealthWorld) {
    let response = reqwest::get("http://127.0.0.1:8080/health")
        .await
        .expect("failed to call health endpoint");

    let body = response
        .text()
        .await
        .expect("failed to read response body");

    world.response_body = Some(body);
}

#[then("the response should be ok")]
async fn response_should_be_ok(world: &mut HealthWorld) {
    assert_eq!(world.response_body.as_deref(), Some("ok"));
}

#[tokio::test]
async fn health_feature() {
    HealthWorld::run("features/health.feature").await;
}
