#!/usr/bin/env python3
"""
Custom Memory Access Trace Generator (Valgrind Lackey Format)
Generates realistic memory access sequences to test TLB caching and THP heuristics:
- Instruction fetches ('I') with tight loop temporal locality
- Stack manipulations ('L', 'S') for function call frame allocation
- Multi-page heap traversals ('L', 'M') crossing 4KB and 2MB page boundaries
"""

import sys
import argparse
import random

def generate_trace(filename="traces/sample_workload.txt", total_accesses=60000, pattern="thp"):
    random.seed(42)
    
    code_base = 0x00400000
    stack_base = 0x7fff0000
    heap_base_1 = 0x01200000  # 2MB region 1
    heap_base_2 = 0x01400000  # 2MB region 2 (promotes to THP)
    
    lines = []
    
    if pattern == "thp":
        # Loop simulating dense heap allocations across a 2MB chunk
        for i in range(total_accesses // 6):
            pc = code_base + (i % 8) * 4
            lines.append(f"I {pc:08x},4")
            
            sp = stack_base - (i % 16) * 8
            lines.append(f"L {sp:08x},8")
            
            pc = code_base + ((i + 1) % 8) * 4
            lines.append(f"I {pc:08x},4")
            
            addr1 = heap_base_1 + ((i * 64) % (128 * 1024))
            lines.append(f"L {addr1:08x},8")
            
            # Sequential touches across >70 unique 4KB pages in 2MB region
            page_idx = (i // 50) % 90
            offset = (i * 32) % 4096
            addr2 = heap_base_2 + (page_idx * 4096) + offset
            lines.append(f"M {addr2:08x},8")
            
            lines.append(f"S {sp:08x},8")
    elif pattern == "locality":
        # High temporal/spatial locality (e.g., matrix multiply or inner loop)
        for i in range(total_accesses // 4):
            pc = code_base + (i % 4) * 4
            lines.append(f"I {pc:08x},4")
            lines.append(f"L {stack_base - (i % 4) * 8:08x},8")
            lines.append(f"L {heap_base_1 + (i % 16) * 8:08x},8")
            lines.append(f"S {heap_base_1 + (i % 16) * 8:08x},8")
    else:
        # Streaming access across vast virtual address range (stress misses)
        for i in range(total_accesses // 2):
            pc = code_base + (i % 32) * 4
            lines.append(f"I {pc:08x},4")
            addr = heap_base_1 + (i * 4096)
            lines.append(f"L {addr:08x},8")

    with open(filename, "w") as f:
        f.write("\n".join(lines[:total_accesses]) + "\n")
        
    print(f"Generated {min(len(lines), total_accesses)} trace events into '{filename}' (pattern: {pattern})")

def main():
    parser = argparse.ArgumentParser(description="Generate synthetic Valgrind Lackey memory traces")
    parser.add_argument("-o", "--output", default="traces/sample_workload.txt", help="Output trace file path")
    parser.add_argument("-n", "--accesses", type=int, default=60000, help="Number of memory access events to generate")
    parser.add_argument("-p", "--pattern", choices=["thp", "locality", "streaming"], default="thp", help="Access pattern profile")
    args = parser.parse_args()
    
    generate_trace(args.output, args.accesses, args.pattern)

if __name__ == "__main__":
    main()
