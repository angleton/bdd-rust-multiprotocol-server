pub(crate) mod graphql;
pub(crate) mod grpc;
pub(crate) mod rest;
pub(crate) mod soap;

pub(crate) fn protocol_response(protocol: &str) -> String {
    format!("{protocol} message")
}
