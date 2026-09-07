use std::net::SocketAddr;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_target(false)
        .with_thread_ids(false)
        .init();

    let addr: SocketAddr = "127.0.0.1:8080".parse().unwrap();

    println!("Server running at http://{}", addr);

    bdd_rust_multiprotocol_server::run(addr).await;
}
