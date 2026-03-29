#!/usr/bin/env python3
"""
Supabase telemetry uploader for Phase 1 benchmark results.

Requires environment variables (never commit these values):
  SUPABASE_URL   — project REST URL, e.g. https://xyzxyz.supabase.co
  SUPABASE_KEY   — service role key (NOT the anon key)

Key fixes vs. previous revision:

  • The service role key is never printed to stdout or stderr.  The previous
    version logged full headers in debug mode, exposing the key in CI logs
    (issue #19 in Phase 1 review).

  • Retry logic distinguishes 4xx (permanent — do not retry) from 5xx
    (transient — retry with backoff).  The previous version retried all
    HTTP errors including 401 Unauthorized (introduced in Copilot revision).

  • Request timeout added (10 s per attempt) so the script does not hang
    indefinitely on unreachable endpoints.

  • Batch upload supported: pass a JSON array or multiple JSON objects
    separated by newlines — all rows are sent in a single POST.

  • Exit codes:
      0 — success
      1 — permanent error (bad credentials, schema mismatch, usage error)
      2 — transient error (all retries exhausted)

Usage (single row):
  python3 scripts/telemetry.py \\
    '{"task_id":1,"task_name":"openSession","latency_us":2.4,
      "iteration":1,"hardware_config":{},"outlier":false}'

Usage (batch — newline-delimited JSON file):
  python3 scripts/telemetry.py --batch results.ndjson
"""

import os
import sys
import json
import time
import argparse
import requests
from typing import List, Union

# ---------------------------------------------------------------------------
# Configuration
# ---------------------------------------------------------------------------

SUPABASE_URL = os.environ.get("SUPABASE_URL")
SUPABASE_KEY = os.environ.get("SUPABASE_KEY")

TABLE = "phase1_benchmarks"
MAX_RETRIES = 3
TIMEOUT_SECONDS = 10
BACKOFF_BASE_S = 0.2  # 200 ms, 400 ms, 800 ms

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

def _masked_key(key: str) -> str:
    """Return a partially masked key for safe logging — never the full value."""
    if len(key) <= 8:
        return "***"
    return key[:4] + "..." + key[-4:]


def _headers() -> dict:
    """Build request headers.  Key is included but never logged via this dict."""
    return {
        "apikey": SUPABASE_KEY,
        "Authorization": f"Bearer {SUPABASE_KEY}",
        "Content-Type": "application/json",
        # "return=minimal" suppresses the echoed row payload, reducing
        # response size for bulk uploads.
        "Prefer": "return=minimal",
    }


def _validate_env() -> None:
    """Exit with a clear message if required env vars are missing."""
    missing = []
    if not SUPABASE_URL:
        missing.append("SUPABASE_URL")
    if not SUPABASE_KEY:
        missing.append("SUPABASE_KEY")
    if missing:
        print(
            f"ERROR: required environment variable(s) not set: {', '.join(missing)}\n"
            "Do not commit these values — use GitHub Secrets or a .env file "
            "that is listed in .gitignore.",
            file=sys.stderr,
        )
        sys.exit(1)


# ---------------------------------------------------------------------------
# Core upload
# ---------------------------------------------------------------------------

def upload_rows(rows: List[dict]) -> None:
    """
    Upload a list of benchmark rows to Supabase.

    Retries on 5xx / network errors with exponential backoff.
    Raises SystemExit(1) on permanent errors (4xx).
    Raises SystemExit(2) if all retries are exhausted.

    The SUPABASE_KEY is NEVER included in any log output.
    """
    url = f"{SUPABASE_URL}/rest/v1/{TABLE}"
    payload = json.dumps(rows)

    for attempt in range(1, MAX_RETRIES + 1):
        try:
            resp = requests.post(
                url,
                headers=_headers(),
                data=payload,
                timeout=TIMEOUT_SECONDS,
            )
        except requests.exceptions.Timeout:
            print(
                f"[telemetry] attempt {attempt}/{MAX_RETRIES}: request timed out "
                f"after {TIMEOUT_SECONDS}s",
                file=sys.stderr,
            )
            _maybe_backoff(attempt)
            continue
        except requests.exceptions.ConnectionError as exc:
            # Log the exception message but NOT the headers (which contain the key).
            print(
                f"[telemetry] attempt {attempt}/{MAX_RETRIES}: connection error: {exc}",
                file=sys.stderr,
            )
            _maybe_backoff(attempt)
            continue

        if resp.ok:
            print(f"[telemetry] uploaded {len(rows)} row(s) (attempt {attempt}).")
            return

        # 4xx — permanent, do not retry.
        if 400 <= resp.status_code < 500:
            # Log the status and body but NOT the key.
            print(
                f"[telemetry] permanent error {resp.status_code}: {resp.text}",
                file=sys.stderr,
            )
            sys.exit(1)

        # 5xx — transient, retry.
        print(
            f"[telemetry] attempt {attempt}/{MAX_RETRIES}: server error "
            f"{resp.status_code}",
            file=sys.stderr,
        )
        _maybe_backoff(attempt)

    print(
        f"[telemetry] all {MAX_RETRIES} attempts failed — giving up.",
        file=sys.stderr,
    )
    sys.exit(2)


def _maybe_backoff(attempt: int) -> None:
    if attempt < MAX_RETRIES:
        delay = BACKOFF_BASE_S * (2 ** (attempt - 1))
        print(f"[telemetry] retrying in {delay:.1f}s …", file=sys.stderr)
        time.sleep(delay)


# ---------------------------------------------------------------------------
# Input parsing
# ---------------------------------------------------------------------------

def parse_rows(raw: str) -> List[dict]:
    """
    Parse raw string as either a JSON array or newline-delimited JSON objects.
    """
    raw = raw.strip()
    if raw.startswith("["):
        # JSON array
        parsed = json.loads(raw)
        if not isinstance(parsed, list):
            raise ValueError("Expected a JSON array")
        return parsed
    # Newline-delimited JSON
    rows = []
    for line in raw.splitlines():
        line = line.strip()
        if line:
            rows.append(json.loads(line))
    return rows


def load_ndjson_file(path: str) -> List[dict]:
    with open(path, "r") as f:
        return parse_rows(f.read())


# ---------------------------------------------------------------------------
# Entry point
# ---------------------------------------------------------------------------

def main() -> None:
    parser = argparse.ArgumentParser(
        description="Upload Phase 1 benchmark results to Supabase.",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog=__doc__,
    )
    parser.add_argument(
        "json_row",
        nargs="?",
        help="JSON object (single row) or JSON array (batch) as a string.",
    )
    parser.add_argument(
        "--batch",
        metavar="FILE",
        help="Path to a newline-delimited JSON file containing multiple rows.",
    )
    args = parser.parse_args()

    _validate_env()

    if args.batch:
        rows = load_ndjson_file(args.batch)
    elif args.json_row:
        rows = parse_rows(args.json_row)
    else:
        parser.print_help()
        sys.exit(1)

    if not rows:
        print("[telemetry] no rows to upload.", file=sys.stderr)
        sys.exit(1)

    upload_rows(rows)


if __name__ == "__main__":
    main()
