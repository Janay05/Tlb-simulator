mod tlb;
mod hierarchy;
mod multicore;
mod trace;

fn main() {
    multicore::run_multicore_asid_demo();
    println!();

    if std::path::Path::new("trace.txt").exists() {
        trace::run_trace_simulation();
    } else {
        println!(
            "(trace.txt not found — skipping the trace-driven THP simulation; see README for how to generate one)"
        );
    }
}
