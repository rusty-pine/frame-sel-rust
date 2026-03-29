# Phase 1 Lite — Implementation & Runbook

## Summary

| Area | Implementation |
|---|---|
| Transport | Synchronous `seL4_Call`; one `extraCap` slot; seL4 Notification caps for async completion |
| Serialisation | Cap'n Proto (`bridge.capnp` with stable `@0x` file ID) |
| Benchmarks | 6 tasks — latency assertions in `tests/phase1_benchmark.rs` |
| Telemetry | Async Rust uploader (`src/telemetry.rs`) + Python uploader (`scripts/telemetry.py`) |
| Verification | 5 Kani harnesses in `src/verification.rs`; cargo-fuzz target in `fuzz/` |
| CI | `.github/workflows/ci.yml` — build → test → Kani → fuzz → bench |

---

## File Checklist

- [x] `src/schemas/bridge.capnp` — schema with stable file ID
- [x] `src/error.rs` — `CapError` + `TelemetryError`
- [x] `src/cap_management.rs` — `SessionCaps` / `CapBundle` with `Drop` impls
- [x] `src/bridge.rs` — `Sel4DirectTransport` with full dispatch
- [x] `src/telemetry.rs` — async, retrying, typed errors
- [x] `src/verification.rs` — 5 Kani proofs with `kani::assert!`
- [x] `src/sel4_mock.rs` — host-side mock with thread-local MR storage
- [x] `build.rs` — `$OUT_DIR`, `rerun-if-changed`, `capnpc`
- [x] `Cargo.toml` — `capnpc` in `[build-dependencies]`, no `kani` dep
- [x] `tests/phase1_benchmark.rs` — 6 real tests, `#[ignore]` gated
- [x] `benches/latency_distribution.rs` — actual operations, not synthetic data
- [x] `scripts/supabase_migration.sql` — RLS, constraints, percentile view
- [x] `scripts/telemetry.py` — key masking, 4xx vs 5xx retry, batch upload
- [x] `.github/workflows/ci.yml` — full pipeline

---

## Environment Setup

### Toolchain

```bash
# Rust stable
rustup update stable

# Cap'n Proto compiler
sudo apt-get install capnproto        # Debian/Ubuntu
brew install capnp                    # macOS

# Kani model checker
cargo install kani-verifier

# cargo-fuzz (requires nightly)
rustup toolchain install nightly
cargo install cargo-fuzz

# Python telemetry uploader
pip3 install requests
```

### Secrets

**Never commit keys.** Set in GitHub: Settings → Secrets and variables → Actions:

```
SUPABASE_URL  = https://<project-ref>.supabase.co
SUPABASE_KEY  = <service role key>
```

For local runs: export in your shell or use a `.env` file listed in `.gitignore`.

### Supabase Schema

```bash
psql $DATABASE_URL -f scripts/supabase_migration.sql
```

Creates `phase1_benchmarks` (with RLS + constraints) and the `phase1_percentiles` view.

---

## Build

```bash
# Host build (mock, no seL4 SDK required)
cargo build --features mock

# On-target build (requires seL4 SDK + cross-compiler)
cargo build --release --target <sel4-target>
```

---

## Running the Phase 1 Harness

### Step 1 — Host smoke test

```bash
cargo test --features mock
```

All tests pass; seL4 hardware tests are `#[ignore]`-d.

### Step 2 — Kani verification

```bash
cargo kani --harness verify_depth_limit_enforced
cargo kani --harness verify_derivation_quota_enforced
cargo kani --harness verify_derivation_count_bounded
cargo kani --harness verify_message_too_large_rejected
cargo kani --harness verify_message_parsing_no_panic
```

All harnesses must report VERIFICATION SUCCESSFUL before merging.

### Step 3 — Fuzz (local)

```bash
cargo +nightly fuzz run fuzz_capnp_parse -- -max_total_time=300
```

### Step 4 — On-target benchmark

1. Build release binary for seL4 target.
2. Launch seL4 kernel (QEMU or hardware).
3. Start server loop (`Sel4DirectTransport::serve_once` in a loop).
4. Run benchmark client (`cargo test --release --test phase1_benchmark`).
5. Collect `results.ndjson`.

### Step 5 — Upload results

```bash
export SUPABASE_URL=https://<ref>.supabase.co
export SUPABASE_KEY=<service-role-key>
python3 scripts/telemetry.py --batch results.ndjson
```

### Step 6 — Verify in Supabase

```sql
SELECT * FROM phase1_percentiles;
```

---

## Success Criteria

| Task | Criterion |
|---|---|
| 1 — openSession | p99 < 10 µs |
| 2 — preGrantMemory | p99 < 10 µs |
| 3 — registerQueue + notify | p95 < 6 µs |
| 4 — raw notification | p99 < 2 µs |
| 5 — kick + complete | p99 < 50 µs |
| 6 — revocation 500 caps | p99 < 120 µs |
| All Kani harnesses | VERIFICATION SUCCESSFUL |
| No capability use-after-revoke | Enforced by `Drop` + revocation tests |
| No dropped messages | Enforced by `serve_once` always-reply guarantee |

---

## Known TODOs (Phase 1 → Phase 2 Boundary)

- `mint_with_depth_check`: real `seL4_CNode_Mint` call (currently returns `Err(TransferFailed)`)
- `mint_notification_cap`: real untyped retype → Notification → mint with badge
- `openSession`: allocate a real session endpoint slot, not a placeholder cptr
- `preGrantMemory`: map received frame caps into the session CSpace
- Badge assignment: use `siphasher` for `(clientId, monotonic_counter)` → badge

These are intentionally deferred to Phase 2 which focuses on the hot datapath.

---

## Notes

- Phase 1 focuses on the control plane (synchronous calls + notification handover).
- Hot datapath optimisations (shared-memory event flags, zero-copy) are Phase 2.
- DO NOT commit `SUPABASE_URL` or `SUPABASE_KEY` under any circumstances.
