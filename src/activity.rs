use std::fmt::Display;
use std::time::Duration;

pub(crate) fn request(protocol: &str, details: impl Display) {
    tracing::info!(protocol, request = %details, "Request");
}

pub(crate) fn response(
    protocol: &str,
    details: impl Display,
    duration: Duration,
    response_bytes: usize,
) {
    tracing::info!(
        protocol,
        response = %details,
        duration_us = duration.as_micros(),
        response_bytes,
        "Response"
    );
}
