use std::time::Duration;

use bdd_rust_multiprotocol_server::fix_addr;
use cucumber::{given, then, when, World};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
    task::JoinHandle,
};

#[derive(Debug, Default, World)]
struct FixWorld {
    server_handle: Option<JoinHandle<()>>,
    response: Option<String>,
    connection_closed: bool,
}

#[when("I connect to FIX without sending a heartbeat")]
async fn connect_without_heartbeat(world: &mut FixWorld) {
    let address = fix_addr("127.0.0.1:8080".parse().unwrap());
    let mut stream = TcpStream::connect(address)
        .await
        .expect("Failed to connect to the FIX acceptor");
    let mut response = Vec::new();
    tokio::time::timeout(
        Duration::from_secs(4),
        stream.read_to_end(&mut response),
    )
    .await
    .expect("FIX acceptor did not close the silent session in time")
    .expect("Failed to read FIX timeout response");

    world.connection_closed = true;
    world.response = Some(String::from_utf8_lossy(&response).into_owned());
}

#[given("the server is running")]
async fn server_is_running(world: &mut FixWorld) {
    let address = "127.0.0.1:8080".parse().unwrap();
    let handle = tokio::spawn(async move {
        bdd_rust_multiprotocol_server::run(address).await;
    });

    world.server_handle = Some(handle);
    tokio::time::sleep(Duration::from_millis(500)).await;
}

#[when("I send a FIX heartbeat message")]
async fn send_fix_heartbeat(world: &mut FixWorld) {
    let address = fix_addr("127.0.0.1:8080".parse().unwrap());
    let mut stream = TcpStream::connect(address)
        .await
        .expect("Failed to connect to the FIX acceptor");
    let heartbeat = b"8=FIX.4.4\x019=5\x0135=0\x0110=000\x01\n";
    stream
        .write_all(heartbeat)
        .await
        .expect("Failed to send FIX heartbeat");

    let mut response = Vec::new();
    stream
        .read_to_end(&mut response)
        .await
        .expect("Failed to read FIX response");
    world.response = Some(String::from_utf8_lossy(&response).into_owned());
}

#[then(expr = "the FIX response should contain message type {string}")]
async fn fix_response_should_contain_message_type(world: &mut FixWorld, message_type: String) {
    let response = world.response.as_ref().expect("No FIX response was captured");
    assert!(response.contains(&format!("35={message_type}")));
}

#[then("the FIX connection should be closed")]
async fn fix_connection_should_be_closed(world: &mut FixWorld) {
    assert!(world.connection_closed);
}

#[tokio::test]
async fn fix_heartbeat_feature() {
    FixWorld::cucumber()
        .run("./features/fix_heartbeat.feature")
        .await;
}
