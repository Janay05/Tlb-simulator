use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;
use serde::{Deserialize, Serialize};

use crate::hierarchy::TlbHierarchy;

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct TraceReport {
    pub file_path: String,
    pub total_accesses: u64,
    pub l1_hits: u64,
    pub l2_hits: u64,
    pub l1_misses: u64,
    pub l2_misses: u64,
    pub huge_pages_promoted: u64,
    pub huge_page_hits: u64,
    pub evictions: u64,
    pub l1_hit_rate: f64,
    pub l2_hit_rate: f64,
    pub global_hit_rate: f64,
    pub total_latency: u64,
    pub amat: f64,
    pub actual_usage_mb: f64,
    pub physical_footprint_mb: f64,
    pub fragmentation_mb: f64,
}

pub fn parsing_logic(line: &str) -> Option<(u64, bool)> {
    let parts: Vec<&str> = line.split_whitespace().collect();
    if parts.len() < 2 {
        return None;
    }

    let op = parts[0];
    let addr_str = parts[1].split(',').next()?;
    let addr = u64::from_str_radix(addr_str, 16).ok()?;

    match op {
        "I" => Some((addr, true)),             // Instruction fetch
        "L" | "S" | "M" => Some((addr, false)), // Data access
        _ => None,
    }
}

pub fn run_trace_simulation_from_reader<R: BufRead>(
    reader: R,
    file_name: &str,
    threshold: usize,
    emit_progress: bool,
) -> TraceReport {
    let mut tlb = TlbHierarchy::with_threshold(16, 4, 16, 4, 128, 8, threshold);

    for (line_num, line_result) in reader.lines().enumerate() {
        let line = match line_result {
            Ok(l) => l,
            Err(_) => continue,
        };

        if let Some((addr, is_instr)) = parsing_logic(&line) {
            tlb.access(addr, is_instr);

            if line.starts_with(" M") {
                tlb.access(addr, is_instr);
            }
        }

        if emit_progress && line_num % 100_000 == 0 && line_num > 0 {
            let current_actual_mb =
                (tlb.unique_4kb_pages_touched.len() as f64 * 4096.0) / 1_048_576.0;
            let current_physical_mb =
                (tlb.total_huge_pages as f64 * 2.0 * 1024.0 * 1024.0) / 1_048_576.0;
            let current_frag = if current_physical_mb > current_actual_mb {
                current_physical_mb - current_actual_mb
            } else {
                0.0
            };
            println!("DATA_POINT,{},{:.2}", line_num, current_frag);
        }
    }

    let total_accesses = tlb.total_requests;
    let l1_hits = tlb.l1d.hits + tlb.l1i.hits;
    let l1_misses = tlb.l1i.misses + tlb.l1d.misses;
    let l2_hits = tlb.l2.hits;
    let l2_misses = tlb.l2.misses;
    let total_hits = l1_hits + l2_hits;

    let l1_hit_rate = if total_accesses > 0 {
        l1_hits as f64 / total_accesses as f64 * 100.0
    } else {
        0.0
    };

    let l2_hit_rate = if l1_misses > 0 {
        l2_hits as f64 / l1_misses as f64 * 100.0
    } else {
        0.0
    };

    let global_hit_rate = if total_accesses > 0 {
        total_hits as f64 / total_accesses as f64 * 100.0
    } else {
        0.0
    };

    let amat = if total_accesses > 0 {
        tlb.total_latency as f64 / total_accesses as f64
    } else {
        0.0
    };

    let page_4kb_size: f64 = 4096.0;
    let huge_page_size: f64 = 2.0 * 1024.0 * 1024.0;

    let actual_usage_mb = (tlb.unique_4kb_pages_touched.len() as f64 * page_4kb_size) / 1_048_576.0;
    let physical_footprint_mb = (tlb.total_huge_pages as f64 * huge_page_size) / 1_048_576.0;
    let fragmentation_mb = if physical_footprint_mb > actual_usage_mb {
        physical_footprint_mb - actual_usage_mb
    } else {
        0.0
    };

    TraceReport {
        file_path: file_name.to_string(),
        total_accesses,
        l1_hits,
        l2_hits,
        l1_misses,
        l2_misses,
        huge_pages_promoted: tlb.total_huge_pages,
        huge_page_hits: tlb.total_huge_hits(),
        evictions: tlb.total_evictions(),
        l1_hit_rate,
        l2_hit_rate,
        global_hit_rate,
        total_latency: tlb.total_latency,
        amat,
        actual_usage_mb,
        physical_footprint_mb,
        fragmentation_mb,
    }
}

pub fn run_trace_simulation_file(path: &str, threshold: usize) -> Result<TraceReport, String> {
    let file = File::open(path).map_err(|e| format!("Could not open '{}': {}", path, e))?;
    let reader = BufReader::new(file);
    Ok(run_trace_simulation_from_reader(reader, path, threshold, false))
}

pub fn run_trace_simulation() {
    let candidates = ["trace.txt", "traces/sample_workload.txt", "sample_workload.txt"];
    let mut chosen = None;
    for cand in candidates {
        if Path::new(cand).exists() {
            chosen = Some(cand);
            break;
        }
    }

    match chosen {
        Some(path) => {
            println!("Simulating trace from: {}", path);
            match run_trace_simulation_file(path, 64) {
                Ok(rep) => {
                    println!("        TLB SIMULATION REPORT       ");
                    println!("Total CPU Requests:      {}", rep.total_accesses);
                    println!("AMAT (Avg Latency):      {:.4} cycles/access", rep.amat);
                    println!("L1 global hit ratio:     {:.4}%", rep.l1_hit_rate);
                    println!("L2 local hit ratio:      {:.4}%", rep.l2_hit_rate);
                    println!("Huge pages promoted:     {}", rep.huge_pages_promoted);
                    println!("Huge page hits:          {}", rep.huge_page_hits);
                    println!("TLB evictions:           {}", rep.evictions);
                    println!("Actual memory touched:   {:.2} MB", rep.actual_usage_mb);
                    println!("Physical footprint:      {:.2} MB", rep.physical_footprint_mb);
                    println!("Internal fragmentation:  {:.2} MB", rep.fragmentation_mb);
                }
                Err(e) => eprintln!("Error running trace: {}", e),
            }
        }
        None => {
            println!(
                "(No trace file found — skipping trace-driven THP simulation. Use --trace <FILE> or see README)"
            );
        }
    }
}
