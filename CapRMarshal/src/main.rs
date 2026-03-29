// src/main.rs
//
// Entry point for the capability-marshaling binary.
//
// Uses #[tokio::main] — log_to_supabase is now an async fn and must be
// .await-ed (not called via Runtime::block_on inside an already-running
// runtime — that would panic).

use capability_marshaling::telemetry::{BenchmarkResult, log_to_supabase_best_effort};
use capability_marshaling::bridge::Sel4DirectTransport;
use std::env;

#[tokio::main]
async fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() > 1 && args[1] == "benchmark" {
        println!("Running Phase 1 Lite Benchmarks…");
        println!("Use: cargo test --test phase1_benchmark");
    } else if args.len() > 1 && args[1] == "telemetry-smoke" {
        // Quick smoke-test: upload one synthetic row.
        let results = vec![BenchmarkResult {
            task_id: 0,
            task_name: "smoke".into(),
            latency_us: 0.0,
            iteration: 0,
            hardware_config: serde_json::json!({"env": "smoke"}),
            outlier: false,
        }];
        log_to_supabase_best_effort(&results).await;
    } else {
        println!("capability-marshaling server starting…");
        // TODO: initialise seL4 endpoint, construct Sel4DirectTransport,
        // and loop over serve_once():
        //
        // loop {
        //     if let Err(e) = transport.serve_once() {
        //         eprintln!("[main] serve_once error: {e}");
        //     }
        // }
    }
}
