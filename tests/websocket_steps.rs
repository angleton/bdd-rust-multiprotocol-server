use std::time::Duration;

use cucumber::{given, then, when, World};
use futures_util::{SinkExt, StreamExt};
use tokio::task::JoinHandle;
use tokio_tungstenite::{connect_async, tungstenite::Message, WebSocketStream};

type TestWebSocket = WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

#[derive(Debug, Default, World)]
struct WebSocketWorld {
    server_handle: Option<JoinHandle<()>>,
    websocket: Option<TestWebSocket>,
    response: Option<String>,
}

#[given("the server is running")]
async fn server_is_running(world: &mut WebSocketWorld) {
    let address = "127.0.0.1:8080".parse().unwrap();
    let app = bdd_rust_multiprotocol_server::app();
    let listener = bdd_rust_multiprotocol_server::bind_http(address)
        .await
        .expect("Cannot start test server: port 8080 is already in use. Stop the standalone server (`cargo run`) before running BDD tests.");

    let handle = tokio::spawn(async move {
        axum::serve(listener, app).await.expect("Server failed");
    });

    world.server_handle = Some(handle);
    tokio::time::sleep(Duration::from_millis(250)).await;
}

#[when("I connect to the WebSocket endpoint")]
async fn connect_to_websocket_endpoint(world: &mut WebSocketWorld) {
    let (websocket, _) = connect_async("ws://127.0.0.1:8080/ws")
        .await
        .expect("Failed to connect to the WebSocket endpoint");

    world.websocket = Some(websocket);
}

#[when(expr = "I send the WebSocket message {string}")]
async fn send_websocket_message(world: &mut WebSocketWorld, message: String) {
    let websocket = world
        .websocket
        .as_mut()
        .expect("WebSocket connection was not established");

    websocket
        .send(Message::Text(message.into()))
        .await
        .expect("Failed to send the WebSocket message");

    let response = websocket
        .next()
        .await
        .expect("WebSocket closed without a response")
        .expect("Failed to receive the WebSocket response");

    world.response = Some(
        response
            .into_text()
            .expect("Expected a text WebSocket response")
            .to_string(),
    );
}

#[then(expr = "the WebSocket response should be {string}")]
async fn websocket_response_should_be(world: &mut WebSocketWorld, expected: String) {
    assert_eq!(world.response.as_deref(), Some(expected.as_str()));
}

#[tokio::test]
async fn websocket_message_feature() {
    WebSocketWorld::cucumber()
        .run_and_exit("./features/websocket_message.feature")
        .await;
}
