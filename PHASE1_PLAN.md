# Phase 1 Lite — Implementation & Runbook

Summary:
- Transport: direct synchronous `seL4_Call` for control messages; one `extraCap` slot for capability transfer; seL4 Notification caps used for async completion signals.
- Benchmarks: six tasks to validate control-plane latency, bundle pre-grant, notification latency, full kick cycle, and revocation cost.
- Telemetry: Supabase schema + client for logging distributions and outliers.
- Verification: unit tests + Kani harness candidates for queue processing functions.

Next steps:
1. Add the files in this PR to the repository.
2. Build and run the integration tests locally (QEMU or physical hardware).
3. Execute the benchmark harness and ingest results into Supabase (use repo secrets).
4. Share Supabase dashboard link and iteration results for Grok review.

Notes:
- DO NOT COMMIT SECRETS. Use GitHub Secrets or your CI secret store to set `SUPABASE_URL` and `SUPABASE_KEY` (service role key) for telemetry ingestion.
- This Phase 1 focuses on the control plane (synchronous calls + notification handover). Hot datapath optimizations (shared memory event flags) are Phase 2.

Run instructions (concise)
- Toolchain / environment
  - Rust toolchain compatible with `sel4` crate (stable or pinned).
  - sel4 SDK and cross-compiler to build userspace for target (QEMU or bare metal).
  - Cap'n Proto codegen: `capnp compile -orust schemas/bridge.capnp`
  - Python: `pip3 install requests`
- Supabase
  - Create project, run `scripts/supabase_migration.sql` (psql or Supabase UI).
  - Store `SUPABASE_URL` and `SUPABASE_KEY` as repo secrets.
- Build
  - Generate Cap'n Proto code then `cargo build --release`.
- Running Phase 1 harness
  1. Launch target (QEMU or hardware) with seL4 kernel.
  2. Start backend server (transport `serve_once` loop).
  3. Run the benchmark client (the harness runs the 6 tasks).
  4. Upload results to Supabase via `scripts/telemetry.py` (it reads `SUPABASE_URL` / `SUPABASE_KEY` from env).

Success criteria (high-level)
- p50/p95/p99 latencies logged for each task, with targets noted in README and PHASE1 plan.
- No capability use-after-revoke, no dropped messages, notification latencies within expected bounds.