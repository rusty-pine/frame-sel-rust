# frame-sel-rust — Phase 1: Direct seL4_Call Transport

Phase 1 capability-marshaling bridge: seL4 + Cap'n Proto, synchronous control plane, telemetry via Supabase, formal verification via Kani.

---

## Design Summary

| Component | Detail |
|---|---|
| Transport | Synchronous `seL4_Call` for control messages; single `extraCap` slot for capability transfer |
| Async completion | seL4 `Notification` caps, minted per-queue, badged with `(sessionId << 16 \| queueId)` |
| Bulk memory | Pre-grant via temporary CNode bundle transferred as one `extraCap` |
| Safety | Session-root revocation on `Drop`, bounded derivation depth (3), bounded derivation quota (500), typed capability wrappers |
| Telemetry | Async upload to Supabase `phase1_benchmarks` table; retry with exponential backoff; 4xx not retried |
| Verification | Kani model-checking harnesses for all cap management invariants; cargo-fuzz for deserialisation |

---

## Repository Layout

```
capability-marshaling/
├── build.rs                    — Cap'n Proto codegen → $OUT_DIR
├── Cargo.toml
├── src/
│   ├── lib.rs
│   ├── main.rs
│   ├── error.rs                — CapError + TelemetryError (thiserror)
│   ├── cap_management.rs       — SessionCaps / CapBundle with Drop impls
│   ├── bridge.rs               — Sel4DirectTransport (call_sync / serve_once)
│   ├── telemetry.rs            — async log_to_supabase (no nested runtime)
│   ├── verification.rs         — Kani harnesses (5 proofs)
│   ├── sel4_mock.rs            — Host-side seL4 mock (feature = "mock")
│   └── schemas/
│       └── bridge.capnp        — Cap'n Proto schema (stable @0x file ID)
├── tests/
│   └── phase1_benchmark.rs     — 6 tasks; seL4 tests gated with #[ignore]
├── benches/
│   └── latency_distribution.rs — Criterion: serialisation + telemetry path
├── fuzz/
│   └── fuzz_targets/
│       └── fuzz_capnp_parse.rs — cargo-fuzz: arbitrary byte → dispatch
├── scripts/
│   ├── supabase_migration.sql  — Table + RLS + percentile view
│   └── telemetry.py            — Python uploader (batch, masked key, backoff)
├── .github/workflows/ci.yml    — Build → Test → Kani → Fuzz → Bench
├── CSPACE_STRATEGY.md
└── PHASE1_PLAN.md
```

---

## Prerequisites

| Tool | Purpose |
|---|---|
| `rustup` | Rust toolchain (stable) |
| `capnp` compiler | Cap'n Proto codegen (`apt install capnproto`) |
| seL4 SDK + cross-compiler | Required for on-target builds; not needed for mock/host builds |
| Python 3 + `requests` | `pip3 install requests` (telemetry.py) |
| `cargo kani` | Formal verification (`cargo install kani-verifier`) |
| `cargo +nightly fuzz` | Fuzz testing (`cargo install cargo-fuzz`) |

---

## Quick Start (Host / Mock)

```bash
# 1. Generate Cap'n Proto bindings
capnp compile -orust src/schemas/bridge.capnp

# 2. Build with mock seL4 layer
cargo build --features mock

# 3. Run all host tests (seL4 hardware tests are #[ignore]-d)
cargo test --features mock

# 4. Run Kani proofs
cargo kani --harness verify_depth_limit_enforced
cargo kani --harness verify_derivation_quota_enforced
cargo kani --harness verify_derivation_count_bounded
cargo kani --harness verify_message_too_large_rejected
cargo kani --harness verify_message_parsing_no_panic

# 5. Run fuzz target (60 s)
cargo +nightly fuzz run fuzz_capnp_parse -- -max_total_time=60

# 6. Run benchmarks
cargo bench --features mock
```

---

## Secrets Setup (GitHub Actions)

**Do not commit keys.** Set as repository secrets:

```
Settings → Secrets and variables → Actions → New repository secret

SUPABASE_URL   = https://<project-ref>.supabase.co
SUPABASE_KEY   = <service role key>   ← NOT the anon key
```

---

## On-Target Run (seL4 QEMU or Hardware)

```bash
# 1. Cross-compile
cargo build --release --target aarch64-sel4   # adjust target as needed

# 2. Launch seL4 image in QEMU
# (target-specific; see your seL4 SDK documentation)

# 3. Start the server loop
# The binary calls Sel4DirectTransport::serve_once() in a loop.

# 4. Run the benchmark harness
cargo test --release --test phase1_benchmark

# 5. Upload results
export SUPABASE_URL=https://<ref>.supabase.co
export SUPABASE_KEY=<service-role-key>
python3 scripts/telemetry.py --batch results.ndjson
```

---

## Supabase Schema Setup

```bash
psql $DATABASE_URL -f scripts/supabase_migration.sql
```

Or paste the file into the Supabase SQL editor.

The migration creates:
- `phase1_benchmarks` table with CHECK constraints and RLS
- Indexes on `(task_id, timestamp)`, `(task_id, latency_us)`, and `outlier`
- `phase1_percentiles` view for p50/p95/p99 per task

---

## Phase 1 Latency Targets

| Task | Operation | p50 | p95 | p99 |
|---|---|---|---|---|
| 1 | `openSession` (cap transfer) | < 5 µs | < 6 µs | < 10 µs |
| 2 | `preGrantMemory` (100 frames) | — | — | < 10 µs |
| 3 | `registerQueue` + notify handover | — | < 6 µs | — |
| 4 | `seL4_Signal` + wait (raw notification) | — | — | < 2 µs |
| 5 | Full kick + complete cycle | — | — | < 50 µs |
| 6 | Revocation (500 derived caps) | — | — | < 120 µs |

---

## Formal Verification Coverage

| Property | Kani Harness |
|---|---|
| Depth limit always enforced | `verify_depth_limit_enforced` |
| Derivation quota always enforced | `verify_derivation_quota_enforced` |
| `derivation_count` never exceeds `max_derivations` | `verify_derivation_count_bounded` |
| Oversized messages always rejected | `verify_message_too_large_rejected` |
| Parser never panics on arbitrary input | `verify_message_parsing_no_panic` |

---

## Safety Notes

- `SessionCaps::drop` calls `revoke_root()` — all derived caps are revoked when a session ends
- `CapBundle::drop` calls `revoke()` — bundle CNode revoked after transfer
- Badge validation in `serve_once` rejects messages from unknown sessions before dispatch
- `mint_with_depth_check` never returns a null cap (`cptr == 0`) as `Ok`
- All `seL4_CNode_Revoke` return codes are checked and propagated

---

## Phase 2 (Planned)

Hot datapath optimisations: shared-memory event flags, zero-copy bulk transfers, lock-free queue descriptors.  Phase 1 control plane is intentionally conservative; Phase 2 builds on the verified foundation here.

---

## Contribution

Branch: `phase1/direct-transport`

Open a PR with:
- Hardware details (M2, Xe3, QEMU target, arch)
- Benchmark output (p50/p95/p99 for all 6 tasks)
- Kani proof output (`cargo kani` passing)
- Supabase dashboard link for results
