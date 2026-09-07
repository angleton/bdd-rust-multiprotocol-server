use std::net::SocketAddr;
use tonic::{Request, Response, Status};

use crate::{hello, protocols::protocol_response, telemetry::Telemetry};

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
        if let (Some(cpu), Some(duration)) = (
            request
                .metadata()
                .get("x-workload-cpu-percent")
                .and_then(|value| value.to_str().ok())
                .and_then(|value| value.parse().ok()),
            request
                .metadata()
                .get("x-workload-duration-ms")
                .and_then(|value| value.to_str().ok())
                .and_then(|value| value.parse().ok()),
        ) {
            if let Some((cpu, duration)) = crate::workload::from_values(cpu, duration) {
                self.telemetry
                    .record_workload(crate::workload::run(cpu, duration).await);
            }
        }
        let request = request.into_inner();
        let response_message = protocol_response("gRPC");
        self.telemetry.record(
            "grpc",
            true,
            started.elapsed(),
            (request.name.len() + request.payload.len()) as u64,
            response_message.len() as u64,
        );
        Ok(Response::new(hello::HelloReply {
            message: response_message,
        }))
    }
}
