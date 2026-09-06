use async_graphql::{Context, EmptyMutation, EmptySubscription, Object, Schema};
use async_graphql_axum::{GraphQLRequest, GraphQLResponse};
use axum::{
    body::Bytes,
    extract::{Query, State},
    http::{header, StatusCode},
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use quick_xml::{events::Event, Reader};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    net::SocketAddr,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tonic::{Request, Response, Status};

type AppSchema = Schema<QueryRoot, EmptyMutation, EmptySubscription>;

pub mod hello {
    tonic::include_proto!("hello");
}

#[derive(Clone, Default)]
pub struct Telemetry {
    protocols: Arc<Mutex<BTreeMap<String, ProtocolTelemetry>>>,
}

#[derive(Clone, Default, Serialize)]
struct ProtocolTelemetry {
    requests: u64,
    failures: u64,
    total_duration_ns: u64,
    request_bytes: u64,
    response_bytes: u64,
}

#[derive(Clone, Default, Serialize)]
pub struct TelemetrySnapshot {
    pub protocols: BTreeMap<String, ProtocolSnapshot>,
}

#[derive(Clone, Default, Serialize)]
pub struct ProtocolSnapshot {
    pub requests: u64,
    pub failures: u64,
    pub average_duration_us: f64,
    pub request_bytes: u64,
    pub response_bytes: u64,
}

impl Telemetry {
    fn record(
        &self,
        protocol: &str,
        success: bool,
        duration: Duration,
        request_bytes: u64,
        response_bytes: u64,
    ) {
        let mut protocols = self.protocols.lock().expect("telemetry lock poisoned");
        let entry = protocols.entry(protocol.to_string()).or_default();
        entry.requests += 1;
        if !success {
            entry.failures += 1;
        }
        entry.total_duration_ns += duration.as_nanos() as u64;
        entry.request_bytes += request_bytes;
        entry.response_bytes += response_bytes;
    }

    pub fn snapshot(&self) -> TelemetrySnapshot {
        let protocols = self.protocols.lock().expect("telemetry lock poisoned");
        TelemetrySnapshot {
            protocols: protocols
                .iter()
                .map(|(protocol, metrics)| {
                    (
                        protocol.clone(),
                        ProtocolSnapshot {
                            requests: metrics.requests,
                            failures: metrics.failures,
                            average_duration_us: if metrics.requests == 0 {
                                0.0
                            } else {
                                metrics.total_duration_ns as f64 / metrics.requests as f64 / 1_000.0
                            },
                            request_bytes: metrics.request_bytes,
                            response_bytes: metrics.response_bytes,
                        },
                    )
                })
                .collect(),
        }
    }
}

#[derive(Clone)]
struct AppState {
    schema: AppSchema,
    telemetry: Telemetry,
}

#[derive(Debug, Deserialize)]
struct PayloadQuery {
    payload: Option<String>,
}

struct QueryRoot;

#[Object]
impl QueryRoot {
    async fn hello(&self, context: &Context<'_>, payload: Option<String>) -> String {
        let started = context
            .data_opt::<Instant>()
            .copied()
            .unwrap_or_else(Instant::now);
        protocol_response(
            "GraphQL",
            started.elapsed(),
            payload.as_deref().unwrap_or(""),
        )
    }
}

pub fn app() -> Router {
    app_with_telemetry(Telemetry::default())
}

pub fn app_with_telemetry(telemetry: Telemetry) -> Router {
    let schema = Schema::build(QueryRoot, EmptyMutation, EmptySubscription).finish();
    let state = AppState { schema, telemetry };

    Router::new()
        .route("/health", get(health_handler))
        .route("/hello", get(rest_hello_handler))
        .route("/graphql", post(graphql_handler))
        .route("/soap", post(handle_soap_request))
        .route("/telemetry", get(telemetry_handler))
        .with_state(state)
}

async fn health_handler(State(state): State<AppState>) -> &'static str {
    let started = Instant::now();
    let duration = started.elapsed();
    state.telemetry.record("health", true, duration, 0, 2);
    "ok"
}

async fn rest_hello_handler(
    State(state): State<AppState>,
    Query(query): Query<PayloadQuery>,
) -> String {
    let started = Instant::now();
    let duration = started.elapsed();
    let payload = query.payload.unwrap_or_default();
    let response = protocol_response("REST", duration, &payload);
    state.telemetry.record(
        "rest",
        true,
        duration,
        payload.len() as u64,
        response.len() as u64,
    );
    response
}

async fn graphql_handler(
    State(state): State<AppState>,
    request: GraphQLRequest,
) -> GraphQLResponse {
    let started = Instant::now();
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

async fn handle_soap_request(State(state): State<AppState>, body: Bytes) -> impl IntoResponse {
    let started = Instant::now();
    let Some(payload) = soap_ping_payload(&body) else {
        state
            .telemetry
            .record("soap", false, started.elapsed(), body.len() as u64, 0);
        return (
            StatusCode::BAD_REQUEST,
            [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
            "Request received, but it was not recognized as SOAP".to_string(),
        )
            .into_response();
    };

    let duration = started.elapsed();
    let response_body = soap_acknowledgement(protocol_response("SOAP", duration, &payload));
    state.telemetry.record(
        "soap",
        true,
        duration,
        body.len() as u64,
        response_body.len() as u64,
    );
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "text/xml; charset=utf-8")],
        response_body,
    )
        .into_response()
}

async fn telemetry_handler(State(state): State<AppState>) -> Json<TelemetrySnapshot> {
    Json(state.telemetry.snapshot())
}

fn soap_ping_payload(body: &[u8]) -> Option<String> {
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

fn soap_acknowledgement(message: String) -> String {
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

fn protocol_response(protocol: &str, duration: Duration, payload: &str) -> String {
    format!("{protocol} in {} us: {payload}", duration.as_micros())
}

pub async fn run(addr: SocketAddr) {
    let telemetry = Telemetry::default();
    let grpc_addr = grpc_addr(addr);
    let grpc_telemetry = telemetry.clone();

    tokio::spawn(async move {
        tonic::transport::Server::builder()
            .add_service(hello::hello_server::HelloServer::new(GrpcHello {
                telemetry: grpc_telemetry,
            }))
            .serve(grpc_addr)
            .await
            .expect("gRPC server failed");
    });

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .expect("failed to bind address");

    axum::serve(listener, app_with_telemetry(telemetry))
        .await
        .expect("server failed");
}

pub fn grpc_addr(http_addr: SocketAddr) -> SocketAddr {
    SocketAddr::new(http_addr.ip(), http_addr.port() + 1)
}

struct GrpcHello {
    telemetry: Telemetry,
}

#[tonic::async_trait]
impl hello::hello_server::Hello for GrpcHello {
    async fn say_hello(
        &self,
        request: Request<hello::HelloRequest>,
    ) -> Result<Response<hello::HelloReply>, Status> {
        let started = Instant::now();
        let request = request.into_inner();
        let name = request.name;
        let payload = request.payload;
        let duration = started.elapsed();
        let response_message = protocol_response("gRPC", duration, &payload);
        self.telemetry.record(
            "grpc",
            true,
            duration,
            (name.len() + payload.len()) as u64,
            response_message.len() as u64,
        );
        Ok(Response::new(hello::HelloReply {
            message: response_message,
        }))
    }
}
