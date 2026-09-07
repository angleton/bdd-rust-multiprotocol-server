use std::{
    env,
    time::{Duration, Instant},
};

use bdd_rust_multiprotocol_server::hello::{hello_client::HelloClient, HelloRequest};
use rand::{seq::SliceRandom, SeedableRng};
use reqwest::Client;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
};
use tokio_tungstenite::{
    connect_async,
    tungstenite::{client::IntoClientRequest, Message},
};

const HTTP_BASE: &str = "http://127.0.0.1:18080";
const GRPC_ENDPOINT: &str = "http://127.0.0.1:18081";
const FIX_ENDPOINT: &str = "127.0.0.1:18082";
const WEBSOCKET_ENDPOINT: &str = "ws://127.0.0.1:18080/ws";
const WARMUP_REQUESTS: usize = 20;
const DEFAULT_ITERATIONS: usize = 1_000;
const DEFAULT_RUNS: usize = 20;
const DEFAULT_PAYLOAD_BYTES: usize = 4_096;
const DEFAULT_CPU_PERCENT: u8 = 0;
const DEFAULT_DURATION_MS: u64 = 0;

#[derive(Default)]
struct Stats {
    samples_us: Vec<u128>,
    run_averages_us: Vec<f64>,
    failures: usize,
}

impl Stats {
    fn record(&mut self, started: Instant, success: bool) {
        if success {
            self.samples_us.push(started.elapsed().as_micros());
        } else {
            self.failures += 1;
        }
    }

    fn extend(&mut self, mut other: Stats) {
        if !other.samples_us.is_empty() {
            self.run_averages_us.push(other.average());
        }
        self.samples_us.append(&mut other.samples_us);
        self.failures += other.failures;
    }

    fn average(&self) -> f64 {
        if self.samples_us.is_empty() {
            0.0
        } else {
            self.samples_us.iter().sum::<u128>() as f64 / self.samples_us.len() as f64
        }
    }

    fn standard_deviation(&self) -> f64 {
        if self.samples_us.len() < 2 {
            return 0.0;
        }
        let average = self.average();
        let squared_differences = self
            .samples_us
            .iter()
            .map(|sample| (*sample as f64 - average).powi(2))
            .sum::<f64>();
        (squared_differences / (self.samples_us.len() - 1) as f64).sqrt()
    }

    fn confidence_interval_95(&self) -> (f64, f64) {
        if self.run_averages_us.is_empty() {
            return (0.0, 0.0);
        }
        let average = self.run_averages_us.iter().sum::<f64>() / self.run_averages_us.len() as f64;
        let variance = if self.run_averages_us.len() < 2 {
            0.0
        } else {
            self.run_averages_us
                .iter()
                .map(|sample| (sample - average).powi(2))
                .sum::<f64>()
                / (self.run_averages_us.len() - 1) as f64
        };
        let margin = 1.96 * variance.sqrt() / (self.run_averages_us.len() as f64).sqrt();
        (average - margin, average + margin)
    }

    fn percentile(&self, percentile: usize) -> u128 {
        if self.samples_us.is_empty() {
            return 0;
        }
        let mut samples = self.samples_us.clone();
        samples.sort_unstable();
        let index = (samples.len() - 1) * percentile / 100;
        samples[index]
    }
}

#[tokio::main]
async fn main() {
    let iterations = env::args()
        .nth(1)
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(DEFAULT_ITERATIONS);
    let runs = env::args()
        .nth(2)
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(DEFAULT_RUNS);
    let payload_size = env::args()
        .nth(3)
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(DEFAULT_PAYLOAD_BYTES);
    let cpu_percent = env::args()
        .nth(4)
        .and_then(|value| value.parse::<u8>().ok())
        .unwrap_or(DEFAULT_CPU_PERCENT);
    let duration_ms = env::args()
        .nth(5)
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(DEFAULT_DURATION_MS);
    let payload = generate_payload(payload_size);
    let soap_request = soap_request(&payload);
    let server_address = "127.0.0.1:18080".parse().unwrap();

    tokio::spawn(async move {
        bdd_rust_multiprotocol_server::run(server_address).await;
    });
    wait_for_http().await;

    let client = Client::new();
    let mut results = [
        ("REST", Stats::default()),
        ("GraphQL", Stats::default()),
        ("SOAP", Stats::default()),
        ("gRPC", Stats::default()),
        ("FIX", Stats::default()),
        ("WebSocket", Stats::default()),
    ];
    let mut randomizer = rand::rngs::StdRng::seed_from_u64(0xBDD_2026);

    for run in 0..runs {
        let mut order = ["REST", "GraphQL", "SOAP", "gRPC", "FIX", "WebSocket"];
        order.shuffle(&mut randomizer);
        println!("run {}/{}: {}", run + 1, runs, order.join(", "));

        for protocol in order {
            let stats = match protocol {
                "REST" => {
                    benchmark_rest(&client, iterations, &payload, cpu_percent, duration_ms).await
                }
                "GraphQL" => {
                    benchmark_graphql(&client, iterations, &payload, cpu_percent, duration_ms).await
                }
                "SOAP" => {
                    benchmark_soap(&client, iterations, &soap_request, cpu_percent, duration_ms)
                        .await
                }
                "gRPC" => benchmark_grpc(iterations, &payload, cpu_percent, duration_ms).await,
                "FIX" => benchmark_fix(iterations, cpu_percent, duration_ms).await,
                "WebSocket" => {
                    benchmark_websocket(iterations, &payload, cpu_percent, duration_ms).await
                }
                _ => unreachable!(),
            };
            results
                .iter_mut()
                .find(|(name, _)| *name == protocol)
                .expect("unknown protocol")
                .1
                .extend(stats);
        }
    }

    println!("Protocol benchmark ({runs} runs x {iterations} requests each; payload {payload_size} bytes; workload {cpu_percent}% for {duration_ms} ms; {WARMUP_REQUESTS} warm-ups excluded)");
    println!("protocol | samples | average_us | median_us | p95_us | 95%_ci_us       | stddev_us | failures");
    println!("---------|---------|------------|-----------|--------|------------------|-----------|---------");
    for (protocol, stats) in &results {
        let (ci_low, ci_high) = stats.confidence_interval_95();
        println!(
            "{protocol:10} | {:7} | {:10.1} | {:9} | {:6} | {:6.1} - {:6.1} | {:9.1} | {}",
            stats.samples_us.len(),
            stats.average(),
            stats.percentile(50),
            stats.percentile(95),
            ci_low,
            ci_high,
            stats.standard_deviation(),
            stats.failures
        );
    }

    let winner = results
        .iter()
        .filter(|(_, stats)| stats.failures == 0)
        .min_by(|(_, left), (_, right)| left.average().total_cmp(&right.average()))
        .map(|(protocol, _)| *protocol)
        .unwrap_or("none");
    println!("winner (lowest average latency with zero failures): {winner}");

    let telemetry: serde_json::Value = client
        .get(format!("{HTTP_BASE}/telemetry"))
        .send()
        .await
        .expect("failed to fetch telemetry")
        .json()
        .await
        .expect("failed to decode telemetry");
    println!("server telemetry:");
    println!("{}", serde_json::to_string_pretty(&telemetry).unwrap());
}

fn generate_payload(size: usize) -> String {
    const ALPHABET: &[u8] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";
    (0..size)
        .map(|index| ALPHABET[index % ALPHABET.len()] as char)
        .collect()
}

fn soap_request(payload: &str) -> String {
    format!(
        "<soap:Envelope xmlns:soap=\"http://schemas.xmlsoap.org/soap/envelope/\"><soap:Body><PingRequest><Message>{payload}</Message></PingRequest></soap:Body></soap:Envelope>"
    )
}

async fn wait_for_http() {
    let client = Client::new();
    for _ in 0..100 {
        if client
            .get(format!("{HTTP_BASE}/health"))
            .send()
            .await
            .is_ok()
        {
            return;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("server did not become ready");
}

async fn benchmark_rest(
    client: &Client,
    iterations: usize,
    payload: &str,
    cpu_percent: u8,
    duration_ms: u64,
) -> Stats {
    let mut stats = Stats::default();
    for _ in 0..WARMUP_REQUESTS {
        let _ = client
            .get(format!("{HTTP_BASE}/hello"))
            .query(&[("payload", payload)])
            .header("x-workload-cpu-percent", cpu_percent.to_string())
            .header("x-workload-duration-ms", duration_ms.to_string())
            .send()
            .await;
    }
    for _ in 0..iterations {
        let started = Instant::now();
        let success = client
            .get(format!("{HTTP_BASE}/hello"))
            .query(&[("payload", payload)])
            .header("x-workload-cpu-percent", cpu_percent.to_string())
            .header("x-workload-duration-ms", duration_ms.to_string())
            .send()
            .await
            .and_then(|response| response.error_for_status())
            .is_ok();
        stats.record(started, success);
    }
    stats
}

async fn benchmark_graphql(
    client: &Client,
    iterations: usize,
    payload: &str,
    cpu_percent: u8,
    duration_ms: u64,
) -> Stats {
    let mut stats = Stats::default();
    let request = serde_json::json!({
        "query": "query($payload: String!) { hello(payload: $payload) }",
        "variables": {"payload": payload}
    });
    for _ in 0..WARMUP_REQUESTS {
        let _ = client
            .post(format!("{HTTP_BASE}/graphql"))
            .json(&request)
            .header("x-workload-cpu-percent", cpu_percent.to_string())
            .header("x-workload-duration-ms", duration_ms.to_string())
            .send()
            .await;
    }
    for _ in 0..iterations {
        let started = Instant::now();
        let success = client
            .post(format!("{HTTP_BASE}/graphql"))
            .json(&request)
            .header("x-workload-cpu-percent", cpu_percent.to_string())
            .header("x-workload-duration-ms", duration_ms.to_string())
            .send()
            .await
            .and_then(|response| response.error_for_status())
            .is_ok();
        stats.record(started, success);
    }
    stats
}

async fn benchmark_soap(
    client: &Client,
    iterations: usize,
    request: &str,
    cpu_percent: u8,
    duration_ms: u64,
) -> Stats {
    let mut stats = Stats::default();
    for _ in 0..WARMUP_REQUESTS {
        let _ = client
            .post(format!("{HTTP_BASE}/soap"))
            .header("content-type", "text/xml")
            .header("x-workload-cpu-percent", cpu_percent.to_string())
            .header("x-workload-duration-ms", duration_ms.to_string())
            .body(request.to_owned())
            .send()
            .await;
    }
    for _ in 0..iterations {
        let started = Instant::now();
        let success = client
            .post(format!("{HTTP_BASE}/soap"))
            .header("content-type", "text/xml")
            .header("x-workload-cpu-percent", cpu_percent.to_string())
            .header("x-workload-duration-ms", duration_ms.to_string())
            .body(request.to_owned())
            .send()
            .await
            .and_then(|response| response.error_for_status())
            .is_ok();
        stats.record(started, success);
    }
    stats
}

async fn benchmark_grpc(
    iterations: usize,
    payload: &str,
    cpu_percent: u8,
    duration_ms: u64,
) -> Stats {
    let mut client = HelloClient::connect(GRPC_ENDPOINT)
        .await
        .expect("failed to connect to gRPC server");
    for _ in 0..WARMUP_REQUESTS {
        let _ = client
            .say_hello(workload_grpc_request(
                "warmup",
                payload,
                cpu_percent,
                duration_ms,
            ))
            .await;
    }
    let mut stats = Stats::default();
    for _ in 0..iterations {
        let started = Instant::now();
        let success = client
            .say_hello(workload_grpc_request(
                "benchmark",
                payload,
                cpu_percent,
                duration_ms,
            ))
            .await
            .is_ok();
        stats.record(started, success);
    }
    stats
}

fn workload_grpc_request(
    name: &str,
    payload: &str,
    cpu_percent: u8,
    duration_ms: u64,
) -> tonic::Request<HelloRequest> {
    let mut request = tonic::Request::new(HelloRequest {
        name: name.to_string(),
        payload: payload.to_string(),
    });
    request.metadata_mut().insert(
        "x-workload-cpu-percent",
        cpu_percent.to_string().parse().unwrap(),
    );
    request.metadata_mut().insert(
        "x-workload-duration-ms",
        duration_ms.to_string().parse().unwrap(),
    );
    request
}

async fn benchmark_fix(iterations: usize, cpu_percent: u8, duration_ms: u64) -> Stats {
    let heartbeat = fix_heartbeat(cpu_percent, duration_ms);
    let mut stats = Stats::default();
    for _ in 0..WARMUP_REQUESTS {
        let _ = fix_request(&heartbeat).await;
    }
    for _ in 0..iterations {
        let started = Instant::now();
        let success = fix_request(&heartbeat).await;
        stats.record(started, success);
    }
    stats
}

fn fix_heartbeat(cpu_percent: u8, duration_ms: u64) -> Vec<u8> {
    format!("8=FIX.4.4\x019=17\x0135=0\x019000={cpu_percent}\x019001={duration_ms}\x0110=000\x01\n")
        .into_bytes()
}

async fn fix_request(request: &[u8]) -> bool {
    let Ok(mut stream) = TcpStream::connect(FIX_ENDPOINT).await else {
        return false;
    };
    if stream.write_all(request).await.is_err() {
        return false;
    }
    let mut response = Vec::new();
    stream.read_to_end(&mut response).await.is_ok()
        && response.windows(4).any(|field| field == b"35=0")
}

async fn benchmark_websocket(
    iterations: usize,
    payload: &str,
    cpu_percent: u8,
    duration_ms: u64,
) -> Stats {
    let mut request = WEBSOCKET_ENDPOINT.into_client_request().unwrap();
    request.headers_mut().insert(
        "x-workload-cpu-percent",
        cpu_percent.to_string().parse().unwrap(),
    );
    request.headers_mut().insert(
        "x-workload-duration-ms",
        duration_ms.to_string().parse().unwrap(),
    );
    let (mut socket, _) = match connect_async(request).await {
        Ok(connection) => connection,
        Err(error) => {
            eprintln!("WebSocket benchmark connection failed: {error}");
            return Stats {
                failures: WARMUP_REQUESTS + iterations,
                ..Stats::default()
            };
        }
    };
    for _ in 0..WARMUP_REQUESTS {
        let _ = websocket_request(&mut socket, payload).await;
    }
    let mut stats = Stats::default();
    for _ in 0..iterations {
        let started = Instant::now();
        let success = websocket_request(&mut socket, payload).await;
        stats.record(started, success);
    }
    let _ = socket.close(None).await;
    stats
}

async fn websocket_request(
    socket: &mut tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
    payload: &str,
) -> bool {
    use futures_util::{SinkExt, StreamExt};

    if socket
        .send(Message::Text(payload.to_owned().into()))
        .await
        .is_err()
    {
        return false;
    }
    matches!(socket.next().await, Some(Ok(Message::Text(_))))
}
