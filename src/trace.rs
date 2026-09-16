use std::fs::File;
use std::io::{BufRead, BufReader};

use crate::hierarchy::TlbHierarchy;

pub fn parsing_logic(line: &str) -> Option<(u64, bool)> {
    let parts: Vec<&str> = line.split_whitespace().collect();
    if parts.len() < 2 {
        return None;
    }

    let op = parts[0];
    let addr_str = parts[1].split(',').next()?;
    let addr = u64::from_str_radix(addr_str, 16).ok()?;

    match op {
        "I" => Some((addr, true)),// Instruction fetch
        "L" | "S" | "M" => Some((addr, false)), // Data access
        _ => None,
    }
}

pub fn run_trace_simulation() {
    let mut tlb = TlbHierarchy::new(16, 4, 16, 4, 128, 8);

    let file = File::open("trace.txt").expect("could not find trace.txt");
    let reader = BufReader::new(file);

    println!("file processing starts");

    for (line_num, line_result) in reader.lines().enumerate() {
        let line: String = line_result.expect("error reading");

        if let Some((addr, is_instr)) = parsing_logic(&line) {
            tlb.access(addr, is_instr);

            if line.starts_with(" M") {
                tlb.access(addr, is_instr);
            }
        }

        //to track progress while executing
        if line_num % 100000 == 0 && line_num > 0 {
            let current_actual_mb = (tlb.unique_4kb_pages_touched.len() as f64 * 4096.0) / 1_048_576.0;
            let current_physical_mb = (tlb.total_huge_pages as f64 * 2.0 * 1024.0 * 1024.0) / 1_048_576.0;
            let current_frag = if current_physical_mb > current_actual_mb {
            current_physical_mb - current_actual_mb
            }
            else {
                0.0
            };

    println!("DATA_POINT,{},{:.2}", line_num, current_frag);
}
    }

    let total_accesses = tlb.l1d.hits + tlb.l1i.hits + tlb.l1i.misses + tlb.l1d.misses;

    if total_accesses == 0 {
        println!("No memory accesses recorded.");
        return;
    }

    let l1_hit_rate = (tlb.l1d.hits + tlb.l1i.hits) as f64 / total_accesses as f64 * 100.0;

    let l1_misses = tlb.l1i.misses + tlb.l1d.misses;
    let l2_hit_rate = if l1_misses > 0 {
        tlb.l2.hits as f64 / l1_misses as f64 * 100.0
    } else {
        0.0
    };
    let amat = tlb.total_latency as f64 / total_accesses as f64;

    //2.MEMORY WASTE METRICS
    //We assume the OS allocates memory in 4KB chunks unless it's a Huge Page.
    let page_4kb_size: f64 = 4096.0;
    let huge_page_size: f64 = 2.0 * 1024.0 * 1024.0; // 2MB

    // Minimal memory actually required by the process (Unique 4KB pages touched)
    let actual_usage_mb = (tlb.unique_4kb_pages_touched.len() as f64 * page_4kb_size) / 1_048_576.0;

    // Total physical footprint allocated by your promotion policy
    let physical_footprint_mb = (tlb.total_huge_pages as f64 * huge_page_size) / 1_048_576.0;

    // Fragmentation (Waste)
    let fragmentation_mb = if physical_footprint_mb > actual_usage_mb {
        physical_footprint_mb - actual_usage_mb
    } else {
        0.0
    };
    println!("        TLB SIMULATION REPORT       ");
    println!("Total CPU Requests:      {}", total_accesses);
    println!("AMAT (Avg Latency):      {:.4} cycles/access", amat);
    println!("L1 global hit ratio:     {:.4}%", l1_hit_rate);
    println!("L2 local hit ratio:      {:.4}%", l2_hit_rate);
    println!("Huge pages promoted:     {}", tlb.total_huge_pages);
    println!("Huge page hits:          {}", tlb.l1d.huge_hits + tlb.l1i.huge_hits + tlb.l2.huge_hits);
    println!("Actual memory touched:   {:.2} MB", actual_usage_mb);
    println!("Physical footprint:      {:.2} MB", physical_footprint_mb);
    println!("Internal fragmentation:  {:.2} MB", fragmentation_mb);
}
