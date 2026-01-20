# frame-sel-rust — Phase 1: Direct seL4_Call Transport (Prototype)

This repository contains Phase‑1 artifacts for the seL4 + Cap'n Proto capability‑marshaling bridge and a Phase‑1 benchmark harness.

Design summary
- Transport: synchronous `seL4_Call` for control messages; single `extraCap` used when transferring a capability.
- Async completion: seL4 `Notification` caps (minted per-queue) for event signaling.
- Bulk datapath: pre‑grant memory using a temporary CNode bundle (single extraCap transfer).
- Safety: session-root revocation, bounded derivation depth, typed capability wrappers (Rust).

Quick list
- `PHASE1_PLAN.md` — runbook, milestones, success criteria.
- `schemas/bridge.capnp` — Cap'n Proto schema for bridge RPCs.
- `src/cap_management.rs` — SessionCaps / CapBundle scaffolding.
- `src/bridge.rs` — Sel4DirectTransport scaffold (call/serve/dispatch).
- `tests/phase1_benchmark.rs` — benchmark harness scaffold.
- `scripts/supabase_migration.sql` — Supabase telemetry schema.
- `scripts/telemetry.py` — example uploader (reads SUPABASE_URL / SUPABASE_KEY).

Getting started (developer machine)
1. Prereqs
   - Rust toolchain (via rustup).
   - Cap'n Proto (`capnp` compiler).
   - sel4 SDK / cross-compiler to build the userspace program for seL4 (QEMU or bare metal).
   - Python 3 and `requests` for telemetry: `pip3 install requests`.
2. Generate Cap'n Proto Rust code:
   - `capnp compile -orust schemas/bridge.capnp`
3. Build:
   - `cargo build --release` (for host-mode tests; building for seL4 requires cross-target config)
4. Secrets (do not commit):
   - Set `SUPABASE_URL` and `SUPABASE_KEY` as repository/CI secrets (GitHub: Settings → Secrets → Actions)
5. Run the Phase‑1 harness on the seL4 target:
   - Start the server (backend) on target (call `Sel4DirectTransport::serve_once()` in a loop).
   - Run benchmark client on target.
   - Use `scripts/telemetry.py` to upload results to Supabase (via the secrets).

Benchmarks & targets (Phase 1)
- openSession (cap transfer): p50/p95/p99 targets <5µs / <6µs / ~<10µs
- preGrantMemory (100 frames): target <10µs end-to-end
- registerQueue + notify handover: <5–6µs
- notification (seL4_Signal + wait): <1–2µs
- full kick+complete cycle: <50µs target
- revocation (500 derived caps): <100–120µs target

Verification & safety notes
- Add Kani harnesses for `process_tx_queue` / descriptor parsing (pure Rust, small wrappers).
- Enforce session derivation depth and derivation-count limits at mint time.
- Badge validation on receive (userspace check) + kernel badge guarantees.

Contribution / workflow
- Branch: `phase1/direct-transport`
- Open PR with testing checklist and hardware details (M2, Xe3, QEMU, etc.)


