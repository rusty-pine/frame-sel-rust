#![allow(dead_code)]
//! Phase 1 benchmark harness (scaffold).
//! Adjust test runner to your environment. This harness is intended to be run
//! on the target where seL4 userspace processes execute.
//!
//! For local host testing (without sel4 syscalls), consider creating a mock
//! transport that simulates `call_sync` and `serve_once`.

use std::time::Instant;

const ITERATIONS: usize = 1000;

fn print_stats(label: &str, samples: &Vec<u128>) {
    let mut s = samples.clone();
    s.sort();
    let p50 = s[s.len() / 2];
    let p95 = s[(s.len() * 95) / 100];
    let p99 = s[(s.len() * 99) / 100];
    println!("{}: p50={}us p95={}us p99={}us", label, p50, p95, p99);
}

fn main() {
    // NOTE: This test harness is a scaffold. You must implement client/server setup
    // to create endpoints and start the server loop (Sel4DirectTransport::serve_once).
    //
    // The harness will run the six tasks and produce latency percentiles; results
    // should be logged to Supabase via scripts/telemetry.py (call it per-iteration
    // or in batches).
    //
    // Pseudocode:
    //
    // let (client, server) = setup_loopback();
    // spawn server.serve_loop();
    //
    // For each task:
    //  - execute operation ITERATIONS times (measure Instant::now.elapsed)
    //  - accumulate timings in Vec<u128>
    //  - print_stats()
    //
    // Implement actual calls using Sel4DirectTransport and Cap'n Proto builders.

    println!("Phase1 harness scaffold ready. Implement target-specific setup and run the tasks.");
}
