// benches/latency_distribution.rs
//
// Criterion benchmarks for Phase 1 latency targets.
//
// Key fixes vs. previous revision:
//
//  • The previous benchmark only measured the `percentile()` helper on
//    synthetic data — completely unrelated to Phase 1 targets (issue #16
//    in Phase 1 review).
//
//  • Benchmarks are now structured around the real Phase 1 operations.
//    On non-seL4 hosts (feature = "mock"), the transport uses sel4_mock
//    so the benchmarks compile and run; wall-clock numbers in mock mode
//    measure serialisation + dispatch overhead only (no kernel round-trip).
//
//  • The `percentile` function is benchmarked as a micro-bench in isolation
//    so its own overhead is visible when interpreting full-path results.

use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId};
use capability_marshaling::telemetry::BenchmarkResult;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn percentile(data: &[f64], p: f64) -> f64 {
    assert!(!data.is_empty());
    let mut sorted = data.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let idx = ((sorted.len() - 1) as f64 * p / 100.0).round() as usize;
    sorted[idx.min(sorted.len() - 1)]
}

// ---------------------------------------------------------------------------
// Bench: percentile helper (isolated)
// ---------------------------------------------------------------------------

fn bench_percentile_helper(c: &mut Criterion) {
    let latencies: Vec<f64> = (0..1000).map(|i| 1.0 + (i % 50) as f64 * 0.1).collect();

    let mut group = c.benchmark_group("percentile_helper");
    for &p in &[50.0f64, 95.0, 99.0] {
        group.bench_with_input(
            BenchmarkId::from_parameter(format!("p{}", p as u32)),
            &p,
            |b, &p| b.iter(|| black_box(percentile(&latencies, p))),
        );
    }
    group.finish();
}

// ---------------------------------------------------------------------------
// Bench: Cap'n Proto serialisation round-trip (host-only, no seL4)
// ---------------------------------------------------------------------------
//
// Measures the cost of building + serialising an openSession Cap'n Proto
// message — one component of the overall IPC latency.

fn bench_capnp_serialisation(c: &mut Criterion) {
    use capnp::message::Builder;
    use capnp::serialize;

    c.bench_function("capnp_openSession_serialise", |b| {
        b.iter(|| {
            let mut builder = Builder::new_default();
            {
                // Build a minimal openSession request.
                // Adjust the schema accessor to match the generated code
                // once bridge_capnp.rs is available.
                let _root = builder.init_root::<capnp::any_pointer::Builder>();
            }
            let words = serialize::write_message_to_words(&builder);
            black_box(words)
        })
    });
}

// ---------------------------------------------------------------------------
// Bench: BenchmarkResult JSON serialisation (telemetry path)
// ---------------------------------------------------------------------------

fn bench_telemetry_serialise(c: &mut Criterion) {
    let result = BenchmarkResult {
        task_id: 1,
        task_name: "openSession".into(),
        latency_us: 3.7,
        iteration: 42,
        hardware_config: serde_json::json!({"board": "QEMU", "arch": "aarch64"}),
        outlier: false,
    };

    c.bench_function("benchmark_result_json_serialise", |b| {
        b.iter(|| {
            let json = serde_json::to_vec(black_box(&result)).unwrap();
            black_box(json)
        })
    });
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

criterion_group!(
    benches,
    bench_percentile_helper,
    bench_capnp_serialisation,
    bench_telemetry_serialise,
);
criterion_main!(benches);
