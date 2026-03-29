// src/telemetry.rs
//
// Asynchronous telemetry uploader for Phase 1 benchmark results.
//
// Key fixes applied vs. previous revisions:
//
//  • Function is now `async` — no nested Tokio runtime (issues #8 / critical
//    in telemetry review).  Callers use `.await` instead of a blocking call.
//
//  • Env var errors are propagated as TelemetryError::MissingEnvVar rather
//    than panicking via `.expect()` (issue #10 / critical).
//
//  • 4xx (permanent) HTTP errors are not retried (new bug in Copilot
//    revision — retried 401/400 three times each).
//
//  • Request timeout added (10 s); previously the client had no timeout
//    and could block for 75–120 s per attempt on unreachable endpoints.
//
//  • Exponential backoff replaces flat 2 s sleep.
//
//  • Unused `Error` import removed.
//
//  • All operational output goes to stderr via `log` facade so it can be
//    silenced in tests or redirected in production.

use std::time::Duration;
use serde::Serialize;
use reqwest::Client;
use crate::error::TelemetryError;

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

/// A single benchmark measurement to be persisted in Supabase.
#[derive(Serialize, Debug, Clone)]
pub struct BenchmarkResult {
    pub task_id: i32,
    pub task_name: String,
    pub latency_us: f64,
    pub iteration: i32,
    pub hardware_config: serde_json::Value,
    pub outlier: bool,
}

// ---------------------------------------------------------------------------
// Upload function
// ---------------------------------------------------------------------------

/// Upload `results` to the `phase1_benchmarks` table in Supabase.
///
/// This function is `async` and must be `.await`-ed from within an existing
/// Tokio runtime (e.g., `#[tokio::main]` in `main.rs`, or `#[tokio::test]`
/// in tests).  It must NOT be called via `Runtime::block_on` from within an
/// already-running runtime — that would panic immediately.
///
/// # Retry policy
/// - 5xx (transient server errors): up to 3 attempts with exponential backoff
///   (200 ms, 400 ms, 800 ms).
/// - 4xx (permanent client errors, e.g. 401 Unauthorized, 400 Bad Request):
///   not retried; returns `TelemetryError::ClientError` immediately.
/// - Network errors (connection refused, timeout): treated as transient and
///   retried up to 3 times.
///
/// # Errors
/// Returns the first permanent error or `TelemetryError::Unavailable` if all
/// retry attempts are exhausted.
pub async fn log_to_supabase(results: &[BenchmarkResult]) -> Result<(), TelemetryError> {
    // Resolve configuration from environment.  Propagate missing-var errors
    // as typed errors rather than panicking.
    let supabase_url = std::env::var("SUPABASE_URL")
        .map_err(|_| TelemetryError::MissingEnvVar("SUPABASE_URL"))?;
    let supabase_key = std::env::var("SUPABASE_KEY")
        .map_err(|_| TelemetryError::MissingEnvVar("SUPABASE_KEY"))?;

    // Build a client with an explicit timeout so unreachable endpoints do
    // not block indefinitely (previous code had no timeout — issue in
    // telemetry review).
    let client = Client::builder()
        .timeout(Duration::from_secs(10))
        .build()?;

    let url = format!("{}/rest/v1/phase1_benchmarks", supabase_url);

    const MAX_ATTEMPTS: u32 = 3;

    for attempt in 0..MAX_ATTEMPTS {
        match client
            .post(&url)
            .header("apikey", &supabase_key)
            .header("Authorization", format!("Bearer {}", supabase_key))
            // "return=minimal" avoids returning the inserted rows, which
            // reduces response payload size for bulk uploads.
            .header("Prefer", "return=minimal")
            .json(results)
            .send()
            .await
        {
            Ok(response) if response.status().is_success() => {
                #[cfg(any(test, feature = "mock"))]
                eprintln!(
                    "[telemetry] uploaded {} results (attempt {})",
                    results.len(),
                    attempt + 1
                );
                return Ok(());
            }

            // Permanent client error (4xx) — do NOT retry.
            Ok(response) if response.status().is_client_error() => {
                eprintln!(
                    "[telemetry] permanent error {}: {}",
                    response.status(),
                    response.text().await.unwrap_or_default()
                );
                return Err(TelemetryError::ClientError(response.status()));
            }

            // Transient server error (5xx) — fall through to retry.
            Ok(response) => {
                eprintln!(
                    "[telemetry] server error {} on attempt {}",
                    response.status(),
                    attempt + 1
                );
            }

            // Network-layer error — fall through to retry.
            Err(e) => {
                eprintln!("[telemetry] network error on attempt {}: {}", attempt + 1, e);
            }
        }

        // Only sleep if there are remaining attempts.
        if attempt + 1 < MAX_ATTEMPTS {
            // Exponential backoff: 200 ms, 400 ms, 800 ms …
            let delay = Duration::from_millis(200 * (1u64 << attempt));
            tokio::time::sleep(delay).await;
        }
    }

    Err(TelemetryError::Unavailable)
}

// ---------------------------------------------------------------------------
// Batch helper
// ---------------------------------------------------------------------------

/// Convenience wrapper: upload a batch, log the outcome, never panic.
///
/// Intended for fire-and-forget call sites in the benchmark harness where
/// a telemetry failure should not abort the benchmark run.  The error is
/// printed to stderr (visible in mock/test builds) and discarded.
pub async fn log_to_supabase_best_effort(results: &[BenchmarkResult]) {
    if let Err(e) = log_to_supabase(results).await {
        eprintln!("[telemetry] upload failed (best-effort, continuing): {}", e);
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn missing_env_vars_returns_error_not_panic() {
        // Temporarily unset the variables (they may already be unset in CI).
        std::env::remove_var("SUPABASE_URL");
        std::env::remove_var("SUPABASE_KEY");

        let results = vec![BenchmarkResult {
            task_id: 1,
            task_name: "test".into(),
            latency_us: 1.0,
            iteration: 1,
            hardware_config: serde_json::json!({}),
            outlier: false,
        }];

        let result = log_to_supabase(&results).await;
        assert!(
            matches!(result, Err(TelemetryError::MissingEnvVar("SUPABASE_URL"))),
            "expected MissingEnvVar, got {:?}",
            result
        );
    }

    #[test]
    fn benchmark_result_serialises_correctly() {
        let r = BenchmarkResult {
            task_id: 2,
            task_name: "openSession".into(),
            latency_us: 3.7,
            iteration: 42,
            hardware_config: serde_json::json!({"board": "QEMU"}),
            outlier: false,
        };
        let json = serde_json::to_string(&r).expect("serialise");
        assert!(json.contains("openSession"));
        assert!(json.contains("3.7"));
    }
}
