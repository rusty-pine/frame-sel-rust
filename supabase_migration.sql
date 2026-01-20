-- Supabase migration for Phase 1 benchmark telemetry
CREATE TABLE IF NOT EXISTS phase1_benchmarks (
    id BIGSERIAL PRIMARY KEY,
    task_id INT,
    task_name TEXT,
    latency_us REAL,
    iteration INT,
    timestamp TIMESTAMPTZ DEFAULT NOW(),
    hardware_config JSONB,
    outlier BOOLEAN DEFAULT FALSE
);

CREATE INDEX IF NOT EXISTS idx_task_timestamp ON phase1_benchmarks(task_id, timestamp DESC);
CREATE INDEX IF NOT EXISTS idx_outlier ON phase1_benchmarks(outlier) WHERE outlier = TRUE;
