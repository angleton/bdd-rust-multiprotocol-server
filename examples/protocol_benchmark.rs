use std::{
    env,
    time::{Duration, Instant},
};

use bdd_rust_multiprotocol_server::hello::{hello_client::HelloClient, HelloRequest};
use rand::{seq::SliceRandom, SeedableRng};
use reqwest::Client;

const HTTP_BASE: &str = "http://127.0.0.1:18080";
const GRPC_ENDPOINT: &str = "http://127.0.0.1:18081";
const WARMUP_REQUESTS: usize = 20;
const DEFAULT_ITERATIONS: usize = 1_000;
const DEFAULT_RUNS: usize = 20;
const DEFAULT_PAYLOAD_BYTES: usize = 4_096;

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
    ];
    let mut randomizer = rand::rngs::StdRng::seed_from_u64(0xBDD_2026);

    for run in 0..runs {
        let mut order = ["REST", "GraphQL", "SOAP", "gRPC"];
        order.shuffle(&mut randomizer);
        println!("run {}/{}: {}", run + 1, runs, order.join(", "));

        for protocol in order {
            let stats = match protocol {
                "REST" => benchmark_rest(&client, iterations, &payload).await,
                "GraphQL" => benchmark_graphql(&client, iterations, &payload).await,
                "SOAP" => benchmark_soap(&client, iterations, &soap_request).await,
                "gRPC" => benchmark_grpc(iterations, &payload).await,
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

    println!("Protocol benchmark ({runs} runs x {iterations} requests each; payload {payload_size} bytes; {WARMUP_REQUESTS} warm-ups excluded)");
    println!("protocol | samples | average_us | median_us | p95_us | 95%_ci_us       | stddev_us | failures");
    println!("---------|---------|------------|-----------|--------|------------------|-----------|---------");
    for (protocol, stats) in &results {
        let (ci_low, ci_high) = stats.confidence_interval_95();
        println!(
            "{protocol:8} | {:7} | {:10.1} | {:9} | {:6} | {:6.1} - {:6.1} | {:9.1} | {}",
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

async fn benchmark_rest(client: &Client, iterations: usize, payload: &str) -> Stats {
    let mut stats = Stats::default();
    for _ in 0..WARMUP_REQUESTS {
        let _ = client
            .get(format!("{HTTP_BASE}/hello"))
            .query(&[("payload", payload)])
            .send()
            .await;
    }
    for _ in 0..iterations {
        let started = Instant::now();
        let success = client
            .get(format!("{HTTP_BASE}/hello"))
            .query(&[("payload", payload)])
            .send()
            .await
            .and_then(|response| response.error_for_status())
            .is_ok();
        stats.record(started, success);
    }
    stats
}

async fn benchmark_graphql(client: &Client, iterations: usize, payload: &str) -> Stats {
    let mut stats = Stats::default();
    let request = serde_json::json!({
        "query": "query($payload: String!) { hello(payload: $payload) }",
        "variables": {"payload": payload}
    });
    for _ in 0..WARMUP_REQUESTS {
        let _ = client
            .post(format!("{HTTP_BASE}/graphql"))
            .json(&request)
            .send()
            .await;
    }
    for _ in 0..iterations {
        let started = Instant::now();
        let success = client
            .post(format!("{HTTP_BASE}/graphql"))
            .json(&request)
            .send()
            .await
            .and_then(|response| response.error_for_status())
            .is_ok();
        stats.record(started, success);
    }
    stats
}

async fn benchmark_soap(client: &Client, iterations: usize, request: &str) -> Stats {
    let mut stats = Stats::default();
    for _ in 0..WARMUP_REQUESTS {
        let _ = client
            .post(format!("{HTTP_BASE}/soap"))
            .header("content-type", "text/xml")
            .body(request.to_owned())
            .send()
            .await;
    }
    for _ in 0..iterations {
        let started = Instant::now();
        let success = client
            .post(format!("{HTTP_BASE}/soap"))
            .header("content-type", "text/xml")
            .body(request.to_owned())
            .send()
            .await
            .and_then(|response| response.error_for_status())
            .is_ok();
        stats.record(started, success);
    }
    stats
}

async fn benchmark_grpc(iterations: usize, payload: &str) -> Stats {
    let mut client = HelloClient::connect(GRPC_ENDPOINT)
        .await
        .expect("failed to connect to gRPC server");
    for _ in 0..WARMUP_REQUESTS {
        let _ = client
            .say_hello(HelloRequest {
                name: "warmup".to_string(),
                payload: payload.to_string(),
            })
            .await;
    }
    let mut stats = Stats::default();
    for _ in 0..iterations {
        let started = Instant::now();
        let success = client
            .say_hello(HelloRequest {
                name: "benchmark".to_string(),
                payload: payload.to_string(),
            })
            .await
            .is_ok();
        stats.record(started, success);
    }
    stats
}
