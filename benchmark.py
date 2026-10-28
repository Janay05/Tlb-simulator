#!/usr/bin/env python3
"""
Automated Sensitivity Benchmark Pipeline for THP Promotion Thresholds (T)
Executes tlb_sim with various thresholds and outputs formatted markdown table.
"""

import subprocess
import json

thresholds = [16, 32, 64, 128, 256]
results = []

print(f"{'Threshold (T)':<15} | {'Huge Pages':<12} | {'Huge Hits':<12} | {'Frag (MB)':<12} | {'AMAT (Cyc)':<10}")
print("-" * 72)

for t in thresholds:
    cmd = ["cargo", "run", "--release", "--", "--mode", "trace", "--threshold", str(t), "--json"]
    try:
        raw = subprocess.check_output(cmd, stderr=subprocess.DEVNULL).decode()
        data = json.loads(raw)
        hp = data.get("huge_pages_promoted", 0)
        hh = data.get("huge_page_hits", 0)
        frag = data.get("fragmentation_mb", 0.0)
        amat = data.get("amat", 0.0)
        results.append((t, hp, hh, frag, amat))
        print(f"{t:<15} | {hp:<12} | {hh:<12} | {frag:<12.2f} | {amat:<10.4f}")
    except Exception as e:
        print(f"Error running threshold {t}: {e}")

print("\nBenchmark completed successfully.")
