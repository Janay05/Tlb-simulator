import os
import subprocess
import re

thresholds = [16, 32, 64, 128, 256]
results = []

def update_threshold(val):
    with open("src/main.rs", "r") as f:
        content = f.read()
    
    new_content = re.sub(r'const PROMOTION_THRESHOLD: usize = \d+;', 
                         f'const PROMOTION_THRESHOLD: usize = {val};', 
                         content)
    with open("src/main.rs", "w") as f:
        f.write(new_content)

print(f"{'Threshold':<10} | {'Huge Pages':<12} | {'Huge Hits':<10} | {'Frag (MB)':<10} | {'AMAT':<10}")
print("-" * 65)

for t in thresholds:
    update_threshold(t)
    
    output = subprocess.check_output("cargo run --release", shell=True).decode()
    
    huge_pages = re.search(r"Huge pages promoted:\s+(\d+)", output).group(1)
    huge_hits = re.search(r"Huge page hits:\s+(\d+)", output).group(1)
    frag = re.search(r"Internal fragmentation:\s+([\d.]+)", output).group(1)
    amat = re.search(r"AMAT \(Avg Latency\):\s+([\d.]+)", output).group(1)
    
    results.append((t, huge_pages, huge_hits, frag, amat))
    print(f"{t:<10} | {huge_pages:<12} | {huge_hits:<10} | {frag:<10} | {amat:<10}")

update_threshold(64)