use std::net::SocketAddr;
use tonic::{Request, Response, Status};

use crate::{activity, hello, protocols::protocol_response, telemetry::Telemetry};

pub(crate) async fn serve(addr: SocketAddr, telemetry: Telemetry) {
    tonic::transport::Server::builder()
        .add_service(hello::hello_server::HelloServer::new(GrpcHello {
            telemetry,
        }))
        .serve(addr)
        .await
        .expect("gRPC server failed");
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
        let started = std::time::Instant::now();
        let request = request.into_inner();
        activity::request(
            "grpc",
            format!("SayHello name_bytes={} payload_bytes={}", request.name.len(), request.payload.len()),
        );
        let response_message = protocol_response("gRPC");
        let duration = started.elapsed();
        self.telemetry.record(
            "grpc",
            true,
            duration,
            (request.name.len() + request.payload.len()) as u64,
            response_message.len() as u64,
        );
        activity::response("grpc", &response_message, duration, response_message.len());
        Ok(Response::new(hello::HelloReply {
            message: response_message,
        }))
    }
}
