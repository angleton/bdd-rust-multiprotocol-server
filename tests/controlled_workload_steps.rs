use std::time::Duration;

use cucumber::{given, then, when, World};
use reqwest::Client;
use serde_json::Value;
use sysinfo::System;
use tokio::task::JoinHandle;

const MAX_STARTUP_CPU_PERCENT: f32 = 20.0;
const RESOURCE_WAIT_TIMEOUT_SECS: u64 = 30;

const PROTOCOLS: [&str; 6] = ["fix", "grpc", "graphql", "rest", "soap", "websocket"];

#[derive(Debug, Default, World)]
struct ControlledWorkloadWorld {
    server_handle: Option<JoinHandle<()>>,
    target_cpu_percent: Option<u8>,
    target_duration_ms: Option<u64>,
    responses_succeeded: bool,
    telemetry: Option<Value>,
}

async fn wait_for_system_resources() {
    let start = tokio::time::Instant::now();

    loop {
        let mut system = System::new();
        system.refresh_cpu();
        tokio::time::sleep(Duration::from_millis(500)).await;
        system.refresh_cpu();

        let cpu_percent = system.global_cpu_info().cpu_usage();

        if cpu_percent <= MAX_STARTUP_CPU_PERCENT {
            println!("System resources available: CPU usage is {cpu_percent:.1}%");
            return;
        }

        println!(
            "Waiting for system resource: CPU usage is {cpu_percent:.1}% \
             (target: <= {MAX_STARTUP_CPU_PERCENT:.1}%)"
        );

        if start.elapsed().as_secs() >= RESOURCE_WAIT_TIMEOUT_SECS {
            panic!(
                "Timed out waiting for system resources: CPU usage remained \
                 above {MAX_STARTUP_CPU_PERCENT:.1}%"
            );
        }
    }
}

#[given("the server is running")]
async fn server_is_running(world: &mut ControlledWorkloadWorld) {
    wait_for_system_resources().await;

    let address = "127.0.0.1:8080".parse().unwrap();
    let listener = bdd_rust_multiprotocol_server::bind_http(address)
        .await
        .expect("Cannot start test server: port 8080 is already in use. Stop the standalone server (`cargo run`) before running BDD tests.");

    let handle = tokio::spawn(async move {
        bdd_rust_multiprotocol_server::run_with_listener(address, listener).await;
    });

    world.server_handle = Some(handle);
    tokio::time::sleep(Duration::from_millis(500)).await;
}

#[given(expr = "I request a target CPU load of {int} percent for {int} milliseconds")]
async fn request_target_load(
    world: &mut ControlledWorkloadWorld,
    target_cpu_percent: u8,
    target_duration_ms: u64,
) {
    world.target_cpu_percent = Some(target_cpu_percent);
    world.target_duration_ms = Some(target_duration_ms);
}

#[when("I send the controlled workload through every protocol")]
async fn send_controlled_workload(world: &mut ControlledWorkloadWorld) {
    let client = Client::new();
    let response = client
        .get("http://127.0.0.1:8080/hello")
        .header(
            "x-workload-cpu-percent",
            world.target_cpu_percent.unwrap().to_string(),
        )
        .header(
            "x-workload-duration-ms",
            world.target_duration_ms.unwrap().to_string(),
        )
        .send()
        .await
        .expect("Failed to send controlled workload request");

    world.responses_succeeded = response.status().is_success();
}

#[then("every protocol response should succeed")]
async fn every_protocol_response_should_succeed(world: &mut ControlledWorkloadWorld) {
    assert!(world.responses_succeeded);
}

#[then(expr = "telemetry should report the requested {int} percent load for {int} milliseconds")]
async fn telemetry_should_report_requested_load(
    world: &mut ControlledWorkloadWorld,
    target_cpu_percent: u8,
    target_duration_ms: u64,
) {
    let telemetry = Client::new()
        .get("http://127.0.0.1:8080/telemetry")
        .send()
        .await
        .expect("Failed to retrieve telemetry")
        .json::<Value>()
        .await
        .expect("Failed to decode telemetry");

    for protocol in PROTOCOLS {
        assert_eq!(
            telemetry["protocols"][protocol]["workload"]["target_cpu_percent"],
            target_cpu_percent
        );
        assert_eq!(
            telemetry["protocols"][protocol]["workload"]["target_duration_ms"],
            target_duration_ms
        );
        assert!(
            telemetry["protocols"][protocol]["workload"]["observed_duration_ms"]
                .as_u64()
                .expect("missing observed workload duration")
                >= target_duration_ms
        );
        let actual_cpu_percent = telemetry["protocols"][protocol]["workload"]
            ["observed_cpu_percent"]
            .as_f64()
            .expect("missing observed CPU utilization");
        assert!((actual_cpu_percent - f64::from(target_cpu_percent)).abs() <= 1.0);
    }
    world.telemetry = Some(telemetry);
}

#[tokio::test]
async fn controlled_workload_feature() {
    ControlledWorkloadWorld::cucumber()
        .run_and_exit("./features/controlled_workload.feature")
        .await;
}
