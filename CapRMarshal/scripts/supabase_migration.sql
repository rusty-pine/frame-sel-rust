-- scripts/supabase_migration.sql
--
-- Phase 1 benchmark telemetry schema.
--
-- Key fixes vs. previous revision:
--
--  • Row Level Security (RLS) enabled on phase1_benchmarks.
--    Without RLS any holder of the anon key can SELECT, UPDATE, or DELETE
--    all rows (issue #18 in Phase 1 review).
--
--  • Two policies defined:
--    - service_insert: allows INSERT for the service role (used by telemetry.rs).
--    - anon_select_own: blocks reads from the anon key by default.
--      Adjust or add a select policy if a Supabase dashboard query user is needed.
--
-- Run via:
--   psql $DATABASE_URL -f scripts/supabase_migration.sql
-- or paste into the Supabase SQL editor.

-- ============================================================
-- Table
-- ============================================================

CREATE TABLE IF NOT EXISTS phase1_benchmarks (
    id              BIGSERIAL PRIMARY KEY,
    task_id         INT           NOT NULL,
    task_name       TEXT          NOT NULL,
    latency_us      REAL          NOT NULL,
    iteration       INT           NOT NULL,
    timestamp       TIMESTAMPTZ   NOT NULL DEFAULT NOW(),
    hardware_config JSONB,
    outlier         BOOLEAN       NOT NULL DEFAULT FALSE,

    -- Enforce that latency is non-negative.
    CONSTRAINT latency_non_negative CHECK (latency_us >= 0),
    -- task_id must be 1–6 for Phase 1.
    CONSTRAINT valid_task_id CHECK (task_id BETWEEN 1 AND 6)
);

-- ============================================================
-- Indexes
-- ============================================================

CREATE INDEX IF NOT EXISTS idx_task_timestamp
    ON phase1_benchmarks (task_id, timestamp DESC);

CREATE INDEX IF NOT EXISTS idx_outlier
    ON phase1_benchmarks (outlier)
    WHERE outlier = TRUE;

-- Composite index for p50/p95/p99 aggregation queries.
CREATE INDEX IF NOT EXISTS idx_task_latency
    ON phase1_benchmarks (task_id, latency_us);

-- ============================================================
-- Row Level Security
-- ============================================================

ALTER TABLE phase1_benchmarks ENABLE ROW LEVEL SECURITY;

-- Allow the service role (used by telemetry.rs / telemetry.py) to insert rows.
-- The service role bypasses RLS by default in Supabase, but the explicit
-- policy documents the intended access pattern.
CREATE POLICY service_insert
    ON phase1_benchmarks
    FOR INSERT
    TO service_role
    WITH CHECK (true);

-- Deny all access via the anon key by default.
-- To allow dashboard reads, add a SELECT policy for the anon role:
--
--   CREATE POLICY anon_select
--       ON phase1_benchmarks
--       FOR SELECT
--       TO anon
--       USING (true);
--
-- Leave commented out until a read-only dashboard user is configured.

-- ============================================================
-- Aggregation view (optional convenience)
-- ============================================================

CREATE OR REPLACE VIEW phase1_percentiles AS
SELECT
    task_id,
    task_name,
    COUNT(*)                                                    AS sample_count,
    PERCENTILE_CONT(0.50) WITHIN GROUP (ORDER BY latency_us)   AS p50_us,
    PERCENTILE_CONT(0.95) WITHIN GROUP (ORDER BY latency_us)   AS p95_us,
    PERCENTILE_CONT(0.99) WITHIN GROUP (ORDER BY latency_us)   AS p99_us,
    MIN(latency_us)                                             AS min_us,
    MAX(latency_us)                                             AS max_us
FROM phase1_benchmarks
GROUP BY task_id, task_name
ORDER BY task_id;
