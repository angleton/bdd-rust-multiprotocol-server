use std::{io, net::SocketAddr};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::{TcpListener, TcpStream},
    time::{timeout, Duration},
};

use crate::{activity, telemetry::Telemetry};

pub(crate) const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(1);

pub(crate) async fn serve(addr: SocketAddr, telemetry: Telemetry) {
    let listener = TcpListener::bind(addr)
        .await
        .expect("FIX server failed to bind address");

    loop {
        let (stream, _) = listener
            .accept()
            .await
            .expect("FIX server failed to accept connection");
        let telemetry = telemetry.clone();

        tokio::spawn(async move {
            if let Err(error) = handle_connection(stream, telemetry).await {
                eprintln!("FIX connection failed: {error}");
            }
        });
    }
}

async fn handle_connection(stream: TcpStream, telemetry: Telemetry) -> io::Result<()> {
    let (reader, mut writer) = stream.into_split();
    let mut reader = BufReader::new(reader);
    let mut request = Vec::new();
    let read_result = timeout(
        HEARTBEAT_INTERVAL,
        reader.read_until(b'\n', &mut request),
    )
    .await;

    if read_result.is_err() {
        activity::request("fix", "heartbeat timeout");
        let test_request = test_request_message();
        writer.write_all(&test_request).await?;
        telemetry.record(
            "fix",
            false,
            HEARTBEAT_INTERVAL,
            0,
            test_request.len() as u64,
        );
        activity::response(
            "fix",
            "Test Request",
            HEARTBEAT_INTERVAL,
            test_request.len(),
        );

        let mut follow_up = Vec::new();
        if timeout(
            HEARTBEAT_INTERVAL,
            reader.read_until(b'\n', &mut follow_up),
        )
        .await
        .is_err()
        {
            activity::request("fix", "heartbeat response timeout");
            return Ok(());
        }
    }

    let started = std::time::Instant::now();
    activity::request("fix", format!("message_bytes={}", request.len()));
    let response = if is_heartbeat(&request) {
        heartbeat_message()
    } else {
        reject_message()
    };
    let success = is_heartbeat(&request);

    writer.write_all(&response).await?;
    let duration = started.elapsed();
    telemetry.record(
        "fix",
        success,
        duration,
        request.len() as u64,
        response.len() as u64,
    );
    activity::response(
        "fix",
        if success { "Heartbeat" } else { "Reject" },
        duration,
        response.len(),
    );
    Ok(())
}

fn is_heartbeat(message: &[u8]) -> bool {
    message
        .split(|byte| *byte == 1 || *byte == b'\n')
        .any(|field| field == b"35=0")
}

fn heartbeat_message() -> Vec<u8> {
    build_message(b"35=0\x01")
}

fn test_request_message() -> Vec<u8> {
    build_message(b"35=1\x01112=heartbeat-timeout\x01")
}

fn reject_message() -> Vec<u8> {
    build_message(b"35=3\x0158=Unsupported FIX message\x01")
}

fn build_message(body: &[u8]) -> Vec<u8> {
    let mut message = format!("8=FIX.4.4\x019={}\x01", body.len()).into_bytes();
    message.extend_from_slice(body);
    let checksum = message
        .iter()
        .map(|byte| *byte as u32)
        .sum::<u32>()
        % 256;
    message.extend_from_slice(format!("10={checksum:03}\x01\n").as_bytes());
    message
}
