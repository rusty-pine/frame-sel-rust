// src/lib.rs
//
// Crate root.
//
// Key fixes vs. previous revision:
//
//  • Generated bridge_capnp.rs is now included from OUT_DIR via
//    concat!(env!("OUT_DIR"), ...) instead of a relative src/ path.
//    This matches the corrected build.rs output path (issue #4).
//
//  • Removed `#` pseudo-comments — those are not valid Rust syntax and
//    caused compile errors in the original file (issue #26 in Phase 1
//    review).

pub mod error;
pub mod cap_management;
pub mod bridge;
pub mod telemetry;
pub mod verification;
pub mod sel4_mock;

/// Generated Cap'n Proto bindings.
/// The file is produced by build.rs into $OUT_DIR and included here so it
/// is never committed to version control.
pub mod schemas {
    include!(concat!(env!("OUT_DIR"), "/bridge_capnp.rs"));
}

// Convenience re-exports for crate consumers.
pub use error::{CapError, TelemetryError};
pub use cap_management::{SessionCaps, CapBundle, CSpaceMetrics};
pub use bridge::Sel4DirectTransport;
pub use telemetry::{BenchmarkResult, log_to_supabase, log_to_supabase_best_effort};
