// src/error.rs
// Consolidated error types for the capability-marshaling crate.
// Uses `thiserror` for Display + From derivation; all variants are
// exhaustive and documented so call-sites never need bare `.unwrap()`.

use thiserror::Error;

// ---------------------------------------------------------------------------
// Capability / transport errors
// ---------------------------------------------------------------------------

#[derive(Error, Debug, PartialEq)]
pub enum CapError {
    /// Raw seL4 syscall returned a non-zero error code.
    #[error("seL4 syscall failed with code {0}")]
    Sel4Error(u32),

    /// The serialised Cap'n Proto message would exceed seL4's IPC word limit.
    #[error("message too large for seL4 IPC buffer")]
    MessageTooLarge,

    /// An expected extraCap was absent from the received message.
    #[error("missing required capability in extraCaps")]
    MissingCapability,

    /// A mint/copy would exceed the session's configured derivation depth.
    #[error("maximum derivation depth exceeded")]
    MaxDepthExceeded,

    /// A mint/copy would exceed the session's per-session derivation quota.
    #[error("maximum derivations per session exceeded")]
    MaxDerivationsExceeded,

    /// seL4_CNode_Revoke returned a non-zero error code.
    #[error("capability revocation failed")]
    RevokeFailed,

    /// Cap'n Proto serialisation or deserialisation error.
    /// Wraps capnp::Error so callers receive the inner message via `?`.
    #[error("Cap'n Proto error: {0}")]
    Capnp(#[from] capnp::Error),

    /// An inbound badge did not match any live session in the session manager.
    #[error("unknown badge {0}: no matching session")]
    UnknownBadge(u64),

    /// A capability transfer or mint placeholder returned without completing.
    /// Used exclusively in scaffold code that has not been fully implemented.
    #[error("capability transfer failed (not yet implemented)")]
    TransferFailed,

    /// Bridge-level fault — used when dispatch encounters an internal
    /// inconsistency that is not directly attributable to seL4 or Cap'n Proto.
    #[error("bridge service fault")]
    BridgeFault,
}

// ---------------------------------------------------------------------------
// Telemetry errors (separate type so telemetry failures cannot be confused
// with cap management failures at the type level)
// ---------------------------------------------------------------------------

#[derive(Error, Debug)]
pub enum TelemetryError {
    /// A required environment variable was absent.
    #[error("missing environment variable: {0}")]
    MissingEnvVar(&'static str),

    /// The HTTP client could not be constructed or the request failed at the
    /// network layer (connection refused, DNS failure, timeout, etc.).
    #[error("HTTP client error: {0}")]
    HttpClient(#[from] reqwest::Error),

    /// The Supabase REST API returned a 4xx status (permanent error — do not
    /// retry).
    #[error("Supabase returned client error {0}")]
    ClientError(reqwest::StatusCode),

    /// The Supabase REST API returned repeated 5xx responses and all retry
    /// attempts were exhausted.
    #[error("Supabase unavailable after all retries")]
    Unavailable,
}
