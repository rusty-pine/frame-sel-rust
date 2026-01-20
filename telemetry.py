#!/usr/bin/env python3
"""
Simple Supabase logger for benchmark results. Requires environment variables:
  SUPABASE_URL and SUPABASE_KEY (service role key).

Usage:
  python3 scripts/telemetry.py '{"task_id":1,"task_name":"openSession","latency_us":2.4,"iteration":1,"hardware_config":{},"outlier":false}'
"""

import os
import sys
import json
import requests

SUPABASE_URL = os.environ.get("SUPABASE_URL")
SUPABASE_KEY = os.environ.get("SUPABASE_KEY")

if not SUPABASE_URL or not SUPABASE_KEY:
    print("ERROR: SUPABASE_URL and SUPABASE_KEY must be set in environment (do not commit keys).")
    sys.exit(1)

def log_row(row):
    url = f"{SUPABASE_URL}/rest/v1/phase1_benchmarks"
    headers = {
        "apikey": SUPABASE_KEY,
        "Authorization": f"Bearer {SUPABASE_KEY}",
        "Content-Type": "application/json",
        "Prefer": "return=representation"
    }
    resp = requests.post(url, headers=headers, data=json.dumps(row))
    resp.raise_for_status()
    return resp.json()

def main():
    if len(sys.argv) != 2:
        print("Usage: telemetry.py '<json_row>'")
        sys.exit(1)
    row = json.loads(sys.argv[1])
    out = log_row(row)
    print("Inserted:", out)

if __name__ == "__main__":
    main()
