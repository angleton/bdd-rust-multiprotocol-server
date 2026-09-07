use axum::{extract::State, routing::get, Json, Router};
use std::net::SocketAddr;
use tokio::net::TcpListener;

mod protocols;
mod telemetry;
mod workload;

pub use telemetry::{ProtocolSnapshot, Telemetry, TelemetrySnapshot};

pub mod hello {
    tonic::include_proto!("hello");
}

#[derive(Clone)]
pub(crate) struct AppState {
    pub(crate) schema: protocols::graphql::AppSchema,
    pub(crate) telemetry: Telemetry,
}

pub fn app() -> Router {
    app_with_telemetry(Telemetry::default())
}

pub fn app_with_telemetry(telemetry: Telemetry) -> Router {
    let state = AppState {
        schema: protocols::graphql::schema(),
        telemetry,
    };

    Router::new()
        .route("/health", get(health_handler))
        .route("/hello", get(protocols::rest::hello_handler))
        .route("/graphql", axum::routing::post(protocols::graphql::handler))
        .route("/soap", axum::routing::post(protocols::soap::handler))
        .route("/ws", axum::routing::get(protocols::websocket::handler))
        .route("/telemetry", get(telemetry_handler))
        .with_state(state)
}

async fn health_handler(State(state): State<AppState>) -> &'static str {
    let started = std::time::Instant::now();
    let duration = started.elapsed();
    state.telemetry.record("health", true, duration, 0, 2);
    "ok"
}

async fn telemetry_handler(State(state): State<AppState>) -> Json<TelemetrySnapshot> {
    Json(state.telemetry.snapshot())
}

pub async fn bind_http(addr: SocketAddr) -> std::io::Result<TcpListener> {
    TcpListener::bind(addr).await.map_err(|error| {
        std::io::Error::new(
            error.kind(),
            format!("cannot bind HTTP server to {addr}: {error}"),
        )
    })
}

pub async fn run(addr: SocketAddr) {
    let listener = bind_http(addr).await.unwrap_or_else(|error| {
        panic!(
            "Cannot start standalone server on {addr}: another server may already be running. Stop it before running `cargo run`. Details: {error}"
        )
    });

    println!("Server running at http://{addr}");
    run_with_listener(addr, listener).await;
}

pub async fn run_with_listener(addr: SocketAddr, listener: TcpListener) {
    let telemetry = Telemetry::default();
    let grpc_addr = grpc_addr(addr);
    let fix_addr = fix_addr(addr);
    let grpc_telemetry = telemetry.clone();
    let fix_telemetry = telemetry.clone();

    tokio::spawn(async move {
        protocols::grpc::serve(grpc_addr, grpc_telemetry).await;
    });

    tokio::spawn(async move {
        protocols::fix::serve(fix_addr, fix_telemetry).await;
    });

    axum::serve(listener, app_with_telemetry(telemetry))
        .await
        .expect("server failed");
}

pub fn grpc_addr(http_addr: SocketAddr) -> SocketAddr {
    SocketAddr::new(http_addr.ip(), http_addr.port() + 1)
}

pub fn fix_addr(http_addr: SocketAddr) -> SocketAddr {
    SocketAddr::new(http_addr.ip(), http_addr.port() + 2)
}
