use std::net::SocketAddr;

#[tokio::main]
async fn main() {
    let addr: SocketAddr = "127.0.0.1:8080".parse().unwrap();

    bdd_rust_multiprotocol_server::run(addr).await;
}
