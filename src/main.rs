use std::path::Path;
use clap::{Parser, ValueEnum};
use serde_json::json;

use tlb_sim_lib::multicore::{
    generate_workload_custom, simulate_multicore, SimStats,
};
use tlb_sim_lib::trace::{run_trace_simulation_file, TraceReport};

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
enum SimMode {
    /// Auto-detect based on arguments
    Auto,
    /// Multi-core synthetic workload simulation with configured parameters
    Multicore,
    /// Side-by-side comparison of Tagged (ASID) vs Untagged TLB
    Compare,
    /// Trace-driven single-core THP simulation
    Trace,
    /// Run both multicore and trace-driven simulation
    All,
}

#[derive(Parser, Debug)]
#[command(
    name = "tlb_sim",
    author = "TLB Simulator Team",
    version = "0.2.0",
    about = "Cycle-Accurate Multi-Core TLB Simulator with ASID Tagging, Shootdowns & THP Promotion",
    long_about = "A high-performance cycle-accurate TLB simulator in Rust modeling:\n  - Set-associative L1I, L1D, and L2 unified TLBs with LRU victim selection\n  - ASID / PCID tagging across context switches (eliminating O(N) TLB flushes)\n  - Cross-core TLB Shootdown IPI latency and coherence stalls\n  - Transparent Huge Page (THP) 4KB-to-2MB promotion heuristics and internal memory fragmentation"
)]
struct Args {
    /// Path to a memory access trace file (Valgrind Lackey format)
    #[arg(short, long)]
    trace: Option<String>,

    /// Enable Address-Space ID (ASID) tagging
    #[arg(long, num_args = 0..=1, default_missing_value = "true", default_value = "true")]
    enable_asid: bool,

    /// Explicitly disable ASID tagging (forces full TLB flush on every context switch)
    #[arg(long)]
    no_asid: bool,

    /// Number of simulated CPU cores (default: 4)
    #[arg(short, long, default_value_t = 4)]
    cores: usize,

    /// Number of synthetic events to generate in multicore simulation
    #[arg(short, long, default_value_t = 200_000)]
    events: usize,

    /// Number of processes time-sharing the multi-core system
    #[arg(short, long, default_value_t = 6)]
    processes: u32,

    /// THP promotion threshold (unique 4KB page touches before 2MB promotion)
    #[arg(short = 'T', long, default_value_t = 64)]
    threshold: usize,

    /// Context switch probability (per mille, e.g. 5 = 0.5%)
    #[arg(long, default_value_t = 5)]
    switch_rate: u64,

    /// Unmap / shootdown trigger probability (per mille, e.g. 3 = 0.3%)
    #[arg(long, default_value_t = 3)]
    unmap_rate: u64,

    /// Simulation mode
    #[arg(short, long, value_enum, default_value_t = SimMode::Auto)]
    mode: SimMode,

    /// Output metrics in JSON format (machine-readable)
    #[arg(long)]
    json: bool,
}

fn format_num(val: u64) -> String {
    let s = val.to_string();
    let mut out = String::new();
    let chars: Vec<char> = s.chars().rev().collect();
    for (i, c) in chars.iter().enumerate() {
        if i > 0 && i % 3 == 0 {
            out.push(',');
        }
        out.push(*c);
    }
    out.chars().rev().collect()
}

fn print_multicore_table(cores: usize, asid_enabled: bool, stats: &SimStats) {
    let mode_str = if asid_enabled { "ENABLED (ASID)" } else { "DISABLED (Untagged)" };
    println!("┌──────────────────────────────────────────────────────────────┐");
    println!("│             TLB MULTI-CORE SIMULATION REPORT                 │");
    println!("├──────────────────────────────────────────────┬───────────────┤");
    println!("│ Metric                                       │ Value         │");
    println!("├──────────────────────────────────────────────┼───────────────┤");
    println!("│ Simulated CPU Cores                          │ {:>13} │", cores);
    println!("│ ASID / PCID Tagging                          │ {:>13} │", mode_str);
    println!("│ Total CPU Accesses                           │ {:>13} │", format_num(stats.total_requests));
    println!("│ L1 TLB Hits (Instruction + Data)             │ {:>13} │", format_num(stats.l1_hits));
    println!("│ L2 TLB Hits                                  │ {:>13} │", format_num(stats.l2_hits));
    println!("│ Total TLB Hits                               │ {:>13} │", format_num(stats.total_hits()));
    println!("│ Global TLB Hit Rate                          │ {:>12.4}% │", stats.hit_rate());
    println!("│ TLB Misses (Page Table Walks)                │ {:>13} │", format_num(stats.misses));
    println!("│ TLB Cache Evictions                          │ {:>13} │", format_num(stats.evictions));
    println!("│ Context-Switch Full Flushes                  │ {:>13} │", format_num(stats.flushes));
    println!("│ Cross-Core TLB Shootdowns Triggered          │ {:>13} │", format_num(stats.shootdown_count));
    println!("│ Shootdown Remote IPIs Sent                   │ {:>13} │", format_num(stats.shootdown_ipis));
    println!("│ Shootdown Stall Overhead Latency             │ {:>9} cyc │", format_num(stats.shootdown_latency));
    println!("│ Shootdown Latency Overhead Share             │ {:>12.2}% │", stats.shootdown_overhead_pct());
    println!("│ Total Simulation Latency                     │ {:>9} cyc │", format_num(stats.total_latency));
    println!("│ AMAT (Average Memory Access Time)            │ {:>9.4} cyc │", stats.amat());
    println!("└──────────────────────────────────────────────┴───────────────┘");
}

fn print_unified_demo_table(
    cores: usize,
    asid_enabled: bool,
    mc_stats: &SimStats,
    trace_path: &str,
    trace_rep: &TraceReport,
    threshold: usize,
) {
    let mode_str = if asid_enabled { "ENABLED (ASID)" } else { "DISABLED (Untagged)" };
    println!("┌──────────────────────────────────────────────────────────────┐");
    println!("│           ZERO-FRICTION TLB SIMULATOR REPORT                 │");
    println!("├──────────────────────────────────────────────┬───────────────┤");
    println!("│ Configuration                                │ Setting       │");
    println!("├──────────────────────────────────────────────┼───────────────┤");
    println!("│ Simulated CPU Cores                          │ {:>13} │", cores);
    println!("│ ASID / PCID Tagging                          │ {:>13} │", mode_str);
    println!("│ Memory Trace File                            │ {:>13} │", trace_path);
    println!("│ THP Promotion Threshold (T)                  │ {:>13} │", threshold);
    println!("├──────────────────────────────────────────────┼───────────────┤");
    println!("│ Multi-Core SMP Metrics                       │ Metric Value  │");
    println!("├──────────────────────────────────────────────┼───────────────┤");
    println!("│ Multi-Core Total Accesses                    │ {:>13} │", format_num(mc_stats.total_requests));
    println!("│ Multi-Core Hits (L1 + L2)                    │ {:>13} │", format_num(mc_stats.total_hits()));
    println!("│ Multi-Core Misses (Page Walks)               │ {:>13} │", format_num(mc_stats.misses));
    println!("│ Multi-Core Hit Rate                          │ {:>12.4}% │", mc_stats.hit_rate());
    println!("│ Multi-Core Cache Evictions                   │ {:>13} │", format_num(mc_stats.evictions));
    println!("│ Context-Switch TLB Flushes                   │ {:>13} │", format_num(mc_stats.flushes));
    println!("│ Cross-Core TLB Shootdowns                    │ {:>13} │", format_num(mc_stats.shootdown_count));
    println!("│ Shootdown Remote IPIs Sent                   │ {:>13} │", format_num(mc_stats.shootdown_ipis));
    println!("│ Shootdown Overhead Latency                   │ {:>9} cyc │", format_num(mc_stats.shootdown_latency));
    println!("│ Multi-Core AMAT                              │ {:>9.4} cyc │", mc_stats.amat());
    println!("├──────────────────────────────────────────────┼───────────────┤");
    println!("│ Trace-Driven THP Metrics                     │ Metric Value  │");
    println!("├──────────────────────────────────────────────┼───────────────┤");
    println!("│ Trace Requests Processed                     │ {:>13} │", format_num(trace_rep.total_accesses));
    println!("│ Trace Overall Hit Rate                       │ {:>12.4}% │", trace_rep.global_hit_rate);
    println!("│ Huge Pages Promoted (2MB)                    │ {:>13} │", format_num(trace_rep.huge_pages_promoted));
    println!("│ Huge Page Hits                               │ {:>13} │", format_num(trace_rep.huge_page_hits));
    println!("│ Trace Cache Evictions                        │ {:>13} │", format_num(trace_rep.evictions));
    println!("│ Memory Footprint (Actual / Alloc)            │ {:.1}/{:.1} MB │", trace_rep.actual_usage_mb, trace_rep.physical_footprint_mb);
    println!("│ Internal Memory Fragmentation                │ {:>10.2} MB │", trace_rep.fragmentation_mb);
    println!("│ Trace AMAT                                   │ {:>9.4} cyc │", trace_rep.amat);
    println!("└──────────────────────────────────────────────┴───────────────┘");
}

fn print_comparison_table(cores: usize, _events: usize, tagged: &SimStats, untagged: &SimStats) {
    println!("┌────────────────────────────────────────────────────────────────────────┐");
    println!("│         TLB MULTI-CORE ASID TAGGING COMPARISON ({} CORES)            │", cores);
    println!("├────────────────────────────────────────┬───────────────┬───────────────┤");
    println!("│ Metric                                 │ Tagged (ASID) │ Untagged      │");
    println!("├────────────────────────────────────────┼───────────────┼───────────────┤");
    println!("│ Total CPU Accesses                     │ {:>13} │ {:>13} │", format_num(tagged.total_requests), format_num(untagged.total_requests));
    println!("│ L1 TLB Hits                            │ {:>13} │ {:>13} │", format_num(tagged.l1_hits), format_num(untagged.l1_hits));
    println!("│ L2 TLB Hits                            │ {:>13} │ {:>13} │", format_num(tagged.l2_hits), format_num(untagged.l2_hits));
    println!("│ Total TLB Hits                         │ {:>13} │ {:>13} │", format_num(tagged.total_hits()), format_num(untagged.total_hits()));
    println!("│ Global Hit Rate                        │ {:>12.4}% │ {:>12.4}% │", tagged.hit_rate(), untagged.hit_rate());
    println!("│ TLB Misses (Page Table Walks)          │ {:>13} │ {:>13} │", format_num(tagged.misses), format_num(untagged.misses));
    println!("│ TLB Cache Evictions                    │ {:>13} │ {:>13} │", format_num(tagged.evictions), format_num(untagged.evictions));
    println!("│ Context-Switch Full Flushes            │ {:>13} │ {:>13} │", format_num(tagged.flushes), format_num(untagged.flushes));
    println!("│ Shootdowns Triggered                   │ {:>13} │ {:>13} │", format_num(tagged.shootdown_count), format_num(untagged.shootdown_count));
    println!("│ Shootdown Remote IPIs Sent             │ {:>13} │ {:>13} │", format_num(tagged.shootdown_ipis), format_num(untagged.shootdown_ipis));
    println!("│ Shootdown Stall Latency                │ {:>9} cyc │ {:>9} cyc │", format_num(tagged.shootdown_latency), format_num(untagged.shootdown_latency));
    println!("│ AMAT (Average Memory Access Time)      │ {:>9.4} cyc │ {:>9.4} cyc │", tagged.amat(), untagged.amat());
    println!("├────────────────────────────────────────┴───────────────┴───────────────┤");
    let speedup = (untagged.amat() - tagged.amat()) / untagged.amat() * 100.0;
    println!("│ Architectural Summary:                                                 │");
    println!("│   - ASID Tagging eliminates {:>5} context-switch flushes               │", format_num(untagged.flushes));
    println!("│   - AMAT reduced by {:.4} cycles/access ({:.2}% latency reduction)       │", untagged.amat() - tagged.amat(), speedup);
    println!("│   - Cross-core shootdown cost remains identical (SMP invalidation)    │");
    println!("└────────────────────────────────────────────────────────────────────────┘");
}

fn print_trace_table(report: &TraceReport, threshold: usize) {
    println!("┌──────────────────────────────────────────────────────────────┐");
    println!("│          TRACE-DRIVEN THP SIMULATION REPORT                  │");
    println!("├──────────────────────────────────────────────┬───────────────┤");
    println!("│ Metric                                       │ Value         │");
    println!("├──────────────────────────────────────────────┼───────────────┤");
    println!("│ Trace Source                                 │ {:>13} │", report.file_path);
    println!("│ THP Promotion Threshold (T)                  │ {:>13} │", threshold);
    println!("│ Total CPU Accesses                           │ {:>13} │", format_num(report.total_accesses));
    println!("│ L1 Global Hit Rate                           │ {:>12.4}% │", report.l1_hit_rate);
    println!("│ L2 Local Hit Rate                            │ {:>12.4}% │", report.l2_hit_rate);
    println!("│ Global Overall Hit Rate                      │ {:>12.4}% │", report.global_hit_rate);
    println!("│ Huge Pages Promoted (2MB)                    │ {:>13} │", format_num(report.huge_pages_promoted));
    println!("│ Huge Page Hits                               │ {:>13} │", format_num(report.huge_page_hits));
    println!("│ TLB Cache Evictions                          │ {:>13} │", format_num(report.evictions));
    println!("│ Unique 4KB Memory Touched                    │ {:>10.2} MB │", report.actual_usage_mb);
    println!("│ Physical Memory Allocated                    │ {:>10.2} MB │", report.physical_footprint_mb);
    println!("│ Internal Memory Fragmentation                │ {:>10.2} MB │", report.fragmentation_mb);
    println!("│ Total Execution Latency                      │ {:>9} cyc │", format_num(report.total_latency));
    println!("│ AMAT (Average Memory Access Time)            │ {:>9.4} cyc │", report.amat);
    println!("└──────────────────────────────────────────────┴───────────────┘");
}

fn find_default_trace() -> Option<String> {
    let candidates = [
        "traces/sample_workload.txt",
        "sample_workload.txt",
        "trace.txt",
    ];
    for path in candidates {
        if Path::new(path).exists() {
            return Some(path.to_string());
        }
    }
    None
}

fn main() {
    let args = Args::parse();
    let asid_enabled = if args.no_asid { false } else { args.enable_asid };

    let seed: u64 = 0x1234_5678_9abc_def0;

    // Check if user specifically requested trace + cores demo (like in the prompt example)
    if let Some(ref trace_path) = args.trace {
        if args.mode == SimMode::Auto {
            // Run multi-core simulation and trace simulation, and print unified table
            let events = generate_workload_custom(
                args.cores,
                args.events,
                seed,
                args.processes,
                args.switch_rate,
                args.unmap_rate,
            );
            let mc_stats = simulate_multicore(args.cores, &events, asid_enabled);
            let trace_rep = run_trace_simulation_file(trace_path, args.threshold)
                .expect("Failed to process trace file");

            if args.json {
                let out = json!({
                    "cores": args.cores,
                    "asid_enabled": asid_enabled,
                    "multicore_stats": mc_stats,
                    "trace_report": trace_rep
                });
                println!("{}", serde_json::to_string_pretty(&out).unwrap());
            } else {
                print_unified_demo_table(args.cores, asid_enabled, &mc_stats, trace_path, &trace_rep, args.threshold);
            }
            return;
        }
    }

    let mode = match args.mode {
        SimMode::Auto => SimMode::All,
        m => m,
    };

    match mode {
        SimMode::Multicore => {
            let events = generate_workload_custom(
                args.cores,
                args.events,
                seed,
                args.processes,
                args.switch_rate,
                args.unmap_rate,
            );
            let stats = simulate_multicore(args.cores, &events, asid_enabled);

            if args.json {
                let out = json!({
                    "mode": "multicore",
                    "cores": args.cores,
                    "asid_enabled": asid_enabled,
                    "stats": stats,
                });
                println!("{}", serde_json::to_string_pretty(&out).unwrap());
            } else {
                print_multicore_table(args.cores, asid_enabled, &stats);
            }
        }

        SimMode::Compare => {
            let events = generate_workload_custom(
                args.cores,
                args.events,
                seed,
                args.processes,
                args.switch_rate,
                args.unmap_rate,
            );
            let tagged = simulate_multicore(args.cores, &events, true);
            let untagged = simulate_multicore(args.cores, &events, false);

            if args.json {
                let out = json!({
                    "mode": "compare",
                    "cores": args.cores,
                    "tagged": tagged,
                    "untagged": untagged,
                });
                println!("{}", serde_json::to_string_pretty(&out).unwrap());
            } else {
                print_comparison_table(args.cores, args.events, &tagged, &untagged);
            }
        }

        SimMode::Trace => {
            let trace_file = args
                .trace
                .or_else(find_default_trace)
                .expect("No trace file found. Provide --trace <PATH> or create traces/sample_workload.txt");

            match run_trace_simulation_file(&trace_file, args.threshold) {
                Ok(report) => {
                    if args.json {
                        println!("{}", serde_json::to_string_pretty(&report).unwrap());
                    } else {
                        print_trace_table(&report, args.threshold);
                    }
                }
                Err(e) => {
                    eprintln!("Error executing trace simulation: {}", e);
                    std::process::exit(1);
                }
            }
        }

        SimMode::All => {
            // Run multi-core comparison first
            let events = generate_workload_custom(
                args.cores,
                args.events,
                seed,
                args.processes,
                args.switch_rate,
                args.unmap_rate,
            );
            let tagged = simulate_multicore(args.cores, &events, true);
            let untagged = simulate_multicore(args.cores, &events, false);

            if !args.json {
                print_comparison_table(args.cores, args.events, &tagged, &untagged);
                println!();
            }

            // Run trace simulation if available
            let trace_file = args.trace.or_else(find_default_trace);
            if let Some(path) = trace_file {
                match run_trace_simulation_file(&path, args.threshold) {
                    Ok(report) => {
                        if args.json {
                            let out = json!({
                                "multicore_comparison": {
                                    "cores": args.cores,
                                    "tagged": tagged,
                                    "untagged": untagged
                                },
                                "trace_simulation": report
                            });
                            println!("{}", serde_json::to_string_pretty(&out).unwrap());
                        } else {
                            print_trace_table(&report, args.threshold);
                        }
                    }
                    Err(e) => {
                        eprintln!("Error reading trace: {}", e);
                    }
                }
            } else if args.json {
                let out = json!({
                    "multicore_comparison": {
                        "cores": args.cores,
                        "tagged": tagged,
                        "untagged": untagged
                    }
                });
                println!("{}", serde_json::to_string_pretty(&out).unwrap());
            }
        }
        SimMode::Auto => unreachable!(),
    }
}
