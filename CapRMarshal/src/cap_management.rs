// src/cap_management.rs
//
// Session and capability management.
//
// Every seL4 syscall return value is checked. Neither `revoke_root` nor
// `CapBundle::revoke` silently swallows errors any longer (was issue #1 / #2).
//
// `mint_with_depth_check` never returns a null cap (cptr == 0) as `Ok`
// (was issue #3). While the real mint call is still a TODO, it returns
// `Err(CapError::TransferFailed)` so callers are never misled.
//
// Drop impls are provided for both `SessionCaps` and `CapBundle` so that
// CSPACE_STRATEGY.md's "root revocation on session drop" promise is actually
// kept (was issue #7).
//
// `AtomicU64` / `Ordering` removed (were imported but unused — issue #21).

use std::time::Instant;
use serde::Serialize;
use sel4::cap::{Cap, CNode};
use sel4::sys::*;
use crate::error::CapError;

// ---------------------------------------------------------------------------
// CSpace telemetry snapshot
// ---------------------------------------------------------------------------

#[derive(Serialize, Debug, Clone)]
pub struct CSpaceMetrics {
    pub session_id: u64,
    pub max_depth: usize,
    pub derivation_count: usize,
    /// Latency of the most recent `revoke_root` call in microseconds.
    /// Zero if revocation has not yet been performed.
    pub revoke_latency_us: u64,
    pub timestamp_ms: u128,
}

// ---------------------------------------------------------------------------
// SessionCaps
// ---------------------------------------------------------------------------

/// Session-level capability container with enforced constraints.
///
/// # Invariants (enforced by code, verified by Kani proofs in verification.rs)
/// - `derivation_count` never exceeds `max_derivations`.
/// - No `mint_with_depth_check` call succeeds at `depth > max_depth`.
/// - On `Drop`, `revoke_root` is called unconditionally to prevent cap leaks.
pub struct SessionCaps {
    pub session_id: u64,
    pub root_cap: Cap<CNode>,
    /// Tracks derived caps for telemetry; does not own their lifetimes (the
    /// kernel CDT is the authoritative record).
    pub derived_caps: Vec<Cap<sel4::cap_type::Untyped>>,
    pub max_depth: usize,
    pub max_derivations: usize,
    pub derivation_count: usize,
    pub created_at: Instant,
    /// Cached latency from the last revoke call; exposed via `record_metrics`.
    last_revoke_latency_us: u64,
}

impl SessionCaps {
    /// Construct a new `SessionCaps` with conservative defaults:
    /// - max derivation depth: 3
    /// - max derivations per session: 500
    pub fn new(session_id: u64, root_cap: Cap<CNode>) -> Self {
        Self {
            session_id,
            root_cap,
            derived_caps: Vec::new(),
            max_depth: 3,
            max_derivations: 500,
            derivation_count: 0,
            created_at: Instant::now(),
            last_revoke_latency_us: 0,
        }
    }

    /// Attempt to mint or copy a derived capability, enforcing depth and quota
    /// limits.
    ///
    /// # Errors
    /// - `MaxDepthExceeded` — if `depth > self.max_depth`
    /// - `MaxDerivationsExceeded` — if the session quota is exhausted
    /// - `TransferFailed` — placeholder until the real seL4_CNode_Mint call is
    ///   implemented; callers must treat this as "not yet available" rather than
    ///   a permanent failure
    ///
    /// # Never returns a null cap
    /// The previous scaffold returned `Cap::new(0)` (cptr == 0, the seL4 null
    /// slot) as `Ok`. That is now replaced with an explicit error so callers
    /// are not misled into using a worthless capability.
    pub fn mint_with_depth_check(
        &mut self,
        _source: Cap<sel4::cap_type::Untyped>,
        _rights: sel4::cap::CapRights,
        depth: usize,
    ) -> Result<Cap<sel4::cap_type::Untyped>, CapError> {
        if depth > self.max_depth {
            return Err(CapError::MaxDepthExceeded);
        }
        if self.derivation_count >= self.max_derivations {
            return Err(CapError::MaxDerivationsExceeded);
        }

        // TODO: allocate a destination slot in the session CSpace and call
        // seL4_CNode_Mint (or seL4_CNode_Copy) via the sel4 crate wrappers.
        // Until that is implemented, we must NOT increment derivation_count
        // or push a null cap — both would corrupt state visible to telemetry
        // and the Drop path.
        Err(CapError::TransferFailed)
    }

    /// Revoke the session root capability.
    ///
    /// The kernel walks the CDT; cost is O(n) in the number of descendants.
    /// The latency is recorded for telemetry (`last_revoke_latency_us`).
    ///
    /// # Errors
    /// Returns `CapError::Sel4Error(rc)` if the kernel returns a non-zero
    /// error code. Previously the return value was silently discarded — that
    /// is corrected here (issue #1).
    pub fn revoke_root(&mut self) -> Result<(), CapError> {
        let start = Instant::now();
        let rc = unsafe { seL4_CNode_Revoke(self.root_cap.cptr()) };
        self.last_revoke_latency_us = start.elapsed().as_micros() as u64;

        if rc != seL4_NoError {
            return Err(CapError::Sel4Error(rc));
        }
        Ok(())
    }

    /// Snapshot metrics for the current session state.
    pub fn record_metrics(&self) -> CSpaceMetrics {
        CSpaceMetrics {
            session_id: self.session_id,
            max_depth: self.max_depth,
            derivation_count: self.derivation_count,
            revoke_latency_us: self.last_revoke_latency_us,
            timestamp_ms: self.created_at.elapsed().as_millis(),
        }
    }
}

/// On drop, attempt to revoke the session root.
///
/// This fulfils the CSPACE_STRATEGY.md invariant "Root revocation on session
/// drop (Drop impl)" which was previously documented but not implemented
/// (issue #7).
///
/// Errors from `revoke_root` are logged to stderr in mock/debug builds and
/// silently swallowed in release builds, because `drop` cannot propagate
/// errors. Callers that need to handle revocation failure must call
/// `revoke_root()` explicitly before allowing `SessionCaps` to drop.
impl Drop for SessionCaps {
    fn drop(&mut self) {
        if let Err(e) = self.revoke_root() {
            #[cfg(any(test, feature = "mock"))]
            eprintln!(
                "[SessionCaps::drop] session {} revocation failed: {}",
                self.session_id, e
            );
            // In production builds this is a best-effort operation.
            // The kernel will clean up when the process exits anyway,
            // but explicit revocation is preferred for defence-in-depth.
            let _ = e;
        }
    }
}

// ---------------------------------------------------------------------------
// CapBundle
// ---------------------------------------------------------------------------

/// Bundle of frame capabilities inside a temporary CNode.
///
/// Transferred to the server as a single `extraCap` to avoid per-frame IPC
/// round-trips for bulk memory grants.
pub struct CapBundle {
    pub bundle_cnode: Cap<CNode>,
    pub caps: Vec<Cap<sel4::cap_type::Frame>>,
}

impl CapBundle {
    pub fn new(bundle_cnode: Cap<CNode>, num_frames: usize) -> Self {
        Self {
            bundle_cnode,
            caps: Vec::with_capacity(num_frames),
        }
    }

    /// Revoke the bundle CNode (defence-in-depth after transfer).
    ///
    /// The return value of `seL4_CNode_Revoke` is now checked and propagated
    /// (issue #1 — previously silent in `CapBundle::revoke` as well).
    pub fn revoke(&self) -> Result<(), CapError> {
        let rc = unsafe { seL4_CNode_Revoke(self.bundle_cnode.cptr()) };
        if rc != seL4_NoError {
            return Err(CapError::Sel4Error(rc));
        }
        Ok(())
    }
}

/// Best-effort revocation on drop (mirrors SessionCaps pattern).
impl Drop for CapBundle {
    fn drop(&mut self) {
        if let Err(e) = self.revoke() {
            #[cfg(any(test, feature = "mock"))]
            eprintln!("[CapBundle::drop] bundle revocation failed: {}", e);
            let _ = e;
        }
    }
}
