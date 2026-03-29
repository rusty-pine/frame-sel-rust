# CSpace and Capability Management Strategy

## Design Constraints (Enforced by Code)

1. **Max derivation depth: 3 layers** — enforced by `mint_with_depth_check`; formally verified by Kani harness `verify_depth_limit_enforced`.
2. **Max derived caps per session: 500** — enforced by `derivation_count` guard; formally verified by Kani harness `verify_derivation_quota_enforced` and `verify_derivation_count_bounded`.
3. **Batch transfers: Temporary CNodes + single extraCap** — keeps per-frame IPC round-trips at O(1) regardless of frame count.
4. **Revocation on session drop** — `Drop` impl on `SessionCaps` calls `revoke_root()` unconditionally. ✅ **Implemented** (`src/cap_management.rs`).
5. **Revocation on bundle drop** — `Drop` impl on `CapBundle` calls `revoke()` unconditionally. ✅ **Implemented** (`src/cap_management.rs`).

> **Previous state:** Points 4 and 5 were documented here but not implemented in code. This has been corrected — the `Drop` impls now exist and are tested.

---

## Error Propagation for seL4 Syscalls

All `seL4_CNode_Revoke` call sites check the return code and propagate
`CapError::Sel4Error(rc)` on failure. Previously both `revoke_root()` and
`CapBundle::revoke()` silently discarded the return value, making capability
leaks invisible.

---

## Why No Runtime Balancing?

- seL4 CNodes do not fragment like heaps.
- Deep trees and excessive derivations are design smells; bounded depths
  enforce good CSpace hygiene by construction.
- Genode / CAmkES deployments succeed with shallow trees + root revocation.

---

## Telemetry & Alerting

Track per-session: `derivation_count`, `max_depth`, `revoke_latency_us` → Supabase `phase1_benchmarks`.

Alert thresholds:
- `depth > 3`
- `derivations > 500`
- `p95_revoke > 200 µs`

The `phase1_percentiles` view in Supabase provides aggregate p50/p95/p99
per task without requiring application-side aggregation.

---

## Kani Verification Coverage

| Property | Harness | Status |
|---|---|---|
| Depth limit always enforced | `verify_depth_limit_enforced` | ✅ |
| Quota always enforced | `verify_derivation_quota_enforced` | ✅ |
| Count never exceeds max | `verify_derivation_count_bounded` | ✅ |
| Message size check | `verify_message_too_large_rejected` | ✅ |
| Parser never panics | `verify_message_parsing_no_panic` | ✅ |

---

## References

- seL4 Manual: https://docs.sel4.systems
- Genode Foundations: https://genode.org/documentation/
- Kani Model Checker: https://model-checking.github.io/kani/
