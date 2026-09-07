use axum::{
    body::Bytes,
    extract::State,
    http::{header, StatusCode},
    response::IntoResponse,
};
use quick_xml::{events::Event, Reader};

use crate::{activity, protocols::protocol_response, AppState};

pub(crate) async fn handler(State(state): State<AppState>, body: Bytes) -> impl IntoResponse {
    let started = std::time::Instant::now();
    activity::request("soap", format!("POST /soap request_bytes={}", body.len()));
    let Some(_payload) = ping_payload(&body) else {
        let duration = started.elapsed();
        state
            .telemetry
            .record("soap", false, duration, body.len() as u64, 0);
        activity::response("soap", "invalid SOAP request", duration, 0);
        return (
            StatusCode::BAD_REQUEST,
            [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
            "Request received, but it was not recognized as SOAP".to_string(),
        )
            .into_response();
    };

    let response_body = acknowledgement(protocol_response("SOAP"));
    let duration = started.elapsed();
    state.telemetry.record(
        "soap",
        true,
        duration,
        body.len() as u64,
        response_body.len() as u64,
    );
    activity::response("soap", "SOAP response", duration, response_body.len());
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "text/xml; charset=utf-8")],
        response_body,
    )
        .into_response()
}

fn ping_payload(body: &[u8]) -> Option<String> {
    let mut reader = Reader::from_reader(body);
    reader.config_mut().trim_text(true);
    let mut buffer = Vec::new();
    let mut envelope = false;
    let mut soap_body = false;
    let mut ping_request = false;
    let mut message = false;
    let mut payload = String::new();

    loop {
        match reader.read_event_into(&mut buffer) {
            Ok(Event::Start(event)) => match local_name(event.name().as_ref()) {
                b"Envelope" => envelope = true,
                b"Body" if envelope => soap_body = true,
                b"PingRequest" if soap_body => ping_request = true,
                b"Message" if ping_request => message = true,
                _ => {}
            },
            Ok(Event::End(event)) => match local_name(event.name().as_ref()) {
                b"Body" => soap_body = false,
                b"Message" => message = false,
                b"Envelope" => break,
                _ => {}
            },
            Ok(Event::Eof) => break,
            Ok(Event::Text(event)) if message => {
                payload.push_str(&event.unescape().ok()?);
            }
            Err(_) => return None,
            _ => {}
        }
        buffer.clear();
    }

    if envelope && ping_request {
        Some(payload)
    } else {
        None
    }
}

fn local_name(name: &[u8]) -> &[u8] {
    name.rsplit(|byte| *byte == b':').next().unwrap_or(name)
}

fn acknowledgement(message: String) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<soap:Envelope xmlns:soap="http://schemas.xmlsoap.org/soap/envelope/">
  <soap:Body>
    <PingResponse>
      <Message>{message}</Message>
    </PingResponse>
  </soap:Body>
</soap:Envelope>"#
    )
}
