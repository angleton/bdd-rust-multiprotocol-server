use std::time::Duration;

use cucumber::{given, then, when, World};
use reqwest::Client;
use tokio::task::JoinHandle;

#[derive(Debug, Default, World)]
struct SoapWorld {
    server_handle: Option<JoinHandle<()>>,
    response_body: Option<String>,
    content_type: Option<String>,
}

#[given("the server is running")]
async fn server_is_running(world: &mut SoapWorld) {
    let app = bdd_rust_multiprotocol_server::app();

    let listener = bdd_rust_multiprotocol_server::bind_http("127.0.0.1:8080".parse().unwrap())
        .await
        .expect("Cannot start test server: port 8080 is already in use. Stop the standalone server (`cargo run`) before running BDD tests.");

    let handle = tokio::spawn(async move {
        axum::serve(listener, app).await.expect("Server failed");
    });

    world.server_handle = Some(handle);

    tokio::time::sleep(Duration::from_millis(250)).await;
}

#[when(regex = r#"I send a SOAP request to "([^"]+)""#)]
async fn send_soap_request(world: &mut SoapWorld, path: String) {
    let url = format!("http://127.0.0.1:8080{}", path);

    let soap_body = r#"
        <soap:Envelope xmlns:soap="http://schemas.xmlsoap.org/soap/envelope/">
            <soap:Body>
                <PingRequest>
                    <Message>Hello from BDD SOAP test</Message>
                </PingRequest>
            </soap:Body>
        </soap:Envelope>
    "#;

    let client = Client::new();

    let response = client
        .post(url)
        .header("Content-Type", "text/xml")
        .body(soap_body)
        .send()
        .await
        .expect("Failed to send SOAP request");

    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);

    let body = response
        .text()
        .await
        .expect("Failed to read SOAP response body");

    world.content_type = content_type;
    world.response_body = Some(body);
}

#[then(regex = r#"the SOAP response content type should be "([^"]+)""#)]
async fn soap_response_content_type_should_be(world: &mut SoapWorld, expected: String) {
    let actual = world
        .content_type
        .as_ref()
        .expect("No SOAP response content type was captured");

    assert!(actual.starts_with(&expected));
}

#[then(regex = r#"the SOAP response body should contain "([^"]+)""#)]
async fn soap_response_body_should_contain(world: &mut SoapWorld, expected: String) {
    let actual = world
        .response_body
        .as_ref()
        .expect("No response body was captured");

    assert!(actual.contains(&expected));
}

#[tokio::test]
async fn soap_acknowledgement_feature() {
    SoapWorld::cucumber()
        .run("./features/soap_acknowledgement.feature")
        .await;
}
