use std::time::Duration;

use bdd_rust_multiprotocol_server::hello::{hello_client::HelloClient, HelloRequest};
use cucumber::{given, then, when, World};
use tokio::task::JoinHandle;

#[derive(Debug, Default, World)]
struct GrpcWorld {
    server_handle: Option<JoinHandle<()>>,
    response_message: Option<String>,
}

#[given("the server is running")]
async fn server_is_running(world: &mut GrpcWorld) {
    let address = "127.0.0.1:8080".parse().unwrap();
    let handle = tokio::spawn(async move {
        bdd_rust_multiprotocol_server::run(address).await;
    });

    world.server_handle = Some(handle);
    tokio::time::sleep(Duration::from_millis(500)).await;
}

#[when(expr = "I call the gRPC hello method with the name {string}")]
async fn call_grpc_hello(world: &mut GrpcWorld, name: String) {
    let mut client = HelloClient::connect("http://127.0.0.1:8081")
        .await
        .expect("Failed to connect to the gRPC server");
    let response = client
        .say_hello(HelloRequest {
            name,
            payload: String::new(),
        })
        .await
        .expect("Failed to call the gRPC hello method");

    world.response_message = Some(response.into_inner().message);
}

#[then(expr = "the gRPC response message should start with {string}")]
async fn grpc_response_should_start_with(world: &mut GrpcWorld, expected: String) {
    let actual = world
        .response_message
        .as_ref()
        .expect("No gRPC response message was captured");
    assert!(actual.starts_with(&expected));
}

#[tokio::main]
async fn main() {
    GrpcWorld::cucumber()
        .run("./features/grpc_hello.feature")
        .await;
}
