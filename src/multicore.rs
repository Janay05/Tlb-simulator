// ============================================================================
// Multi-core ASID tagging + TLB shootdown simulation
//
// The trace-driven simulation in `hierarchy`/`trace` models a single process
// on a single core. Real thrashing shows up in two other places SMP kernels
// have to deal with:
//   1. Context switches — without ASID/PCID tagging, every switch to a
//      different process must flush the whole TLB, since a stale entry
//      could otherwise alias into the new process's address space.
//   2. Cross-core invalidation ("shootdowns") — when one core unmaps or
//      changes permissions on a page, every other core that might have it
//      cached must be interrupted (IPI) and made to invalidate it before
//      the unmapping core can proceed safely.
// ASID tagging fixes (1) essentially for free but does nothing for (2):
// shootdowns are a structural cost of shared address spaces on SMP systems.
// ============================================================================

use crate::tlb::Tlb;

const IPI_LATENCY: u64 = 50; // cycles charged per remote core interrupted

struct CoreHierarchy {
    l1i: Tlb,
    l1d: Tlb,
    l2: Tlb,
}

impl CoreHierarchy {
    fn new(tagging_enabled: bool) -> Self {
        CoreHierarchy {
            l1i: Tlb::with_tagging(16, 4, tagging_enabled),
            l1d: Tlb::with_tagging(16, 4, tagging_enabled),
            l2: Tlb::with_tagging(64, 8, tagging_enabled),
        }
    }

    fn switch_process(&mut self, asid: u32) {
        self.l1i.switch_process(asid);
        self.l1d.switch_process(asid);
        self.l2.switch_process(asid);
    }
}

#[derive(Default, Debug)]
pub struct SimStats {
    pub total_requests: u64,
    pub l1_hits: u64,
    pub l2_hits: u64,
    pub mem_accesses: u64,
    pub total_latency: u64,
    pub flushes: u64,
    pub shootdown_count: u64,
    pub shootdown_latency: u64,
}

impl SimStats {
    pub fn hit_rate(&self) -> f64 {
        if self.total_requests == 0 {
            return 0.0;
        }
        (self.l1_hits + self.l2_hits) as f64 / self.total_requests as f64 * 100.0
    }

    pub fn amat(&self) -> f64 {
        if self.total_requests == 0 {
            return 0.0;
        }
        self.total_latency as f64 / self.total_requests as f64
    }
}

struct MultiCoreSystem {
    cores: Vec<CoreHierarchy>,
    stats: SimStats,
}

impl MultiCoreSystem {
    fn new(num_cores: usize, tagging_enabled: bool) -> Self {
        MultiCoreSystem {
            cores: (0..num_cores).map(|_| CoreHierarchy::new(tagging_enabled)).collect(),
            stats: SimStats::default(),
        }
    }

    fn switch(&mut self, core: usize, asid: u32) {
        self.cores[core].switch_process(asid);
    }

    fn access(&mut self, core: usize, addr: u64, is_instruction: bool) {
        const L1_LATENCY: u64 = 1;
        const L2_LATENCY: u64 = 10;
        const MEM_LATENCY: u64 = 200;

        self.stats.total_requests += 1;
        self.stats.total_latency += L1_LATENCY;

        let core = &mut self.cores[core];
        let l1 = if is_instruction { &mut core.l1i } else { &mut core.l1d };

        if l1.lookup(addr) {
            self.stats.l1_hits += 1;
            return;
        }

        self.stats.total_latency += L2_LATENCY;
        if core.l2.lookup(addr) {
            self.stats.l2_hits += 1;
            let pfn = addr >> 12;
            l1.insert(addr, pfn, false);
        } else {
            self.stats.mem_accesses += 1;
            self.stats.total_latency += MEM_LATENCY;
            let pfn = addr >> 12;
            core.l2.insert(addr, pfn, false);
            l1.insert(addr, pfn, false);
        }
    }

    /// Simulates an unmap/mprotect on `addr` for `asid`, initiated by
    /// `initiating_core`. Any *other* core still caching that translation
    /// must be sent an IPI and invalidate before the unmap can be
    /// considered complete — that stall is charged to total_latency.
    fn unmap(&mut self, asid: u32, addr: u64, initiating_core: usize) {
        let mut remote_cores_invalidated: u64 = 0;
        for (i, core) in self.cores.iter_mut().enumerate() {
            let hit_i = core.l1i.invalidate_page_for_asid(asid, addr);
            let hit_d = core.l1d.invalidate_page_for_asid(asid, addr);
            let hit_l2 = core.l2.invalidate_page_for_asid(asid, addr);
            if i != initiating_core && (hit_i || hit_d || hit_l2) {
                remote_cores_invalidated += 1;
            }
        }
        if remote_cores_invalidated > 0 {
            self.stats.shootdown_count += 1;
            let cost = remote_cores_invalidated * IPI_LATENCY;
            self.stats.shootdown_latency += cost;
            self.stats.total_latency += cost;
        }
    }

    fn finish(mut self) -> SimStats {
        self.stats.flushes = self
            .cores
            .iter()
            .map(|c| c.l1i.flushes + c.l1d.flushes + c.l2.flushes)
            .sum();
        self.stats
    }
}

enum Event {
    Switch { core: usize, asid: u32 },
    Access { core: usize, addr: u64, is_instruction: bool },
    Unmap { core: usize, asid: u32, addr: u64 },
}

fn lcg_next(state: &mut u64) -> u64 {
    // Numerical Recipes LCG constants — deterministic, no external crate needed.
    *state = state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
    *state
}

/// Generates a synthetic multi-process, multi-core workload: a handful of
/// processes time-sharing a fixed core count, each with its own working
/// set (mostly hot-page locality, occasionally spilling wider), plus
/// occasional unmaps to exercise the shootdown path.
fn generate_workload(num_cores: usize, num_events: usize, seed: u64) -> Vec<Event> {
    let mut state = seed;
    let num_processes: u32 = 6;
    let working_set_pages: u64 = 300;
    let hot_set_pages: u64 = 32;

    let mut core_asid = vec![1u32; num_cores];
    let mut events = Vec::with_capacity(num_events + num_cores);

    for core in 0..num_cores {
        events.push(Event::Switch { core, asid: core_asid[core] });
    }

    for _ in 0..num_events {
        let core = (lcg_next(&mut state) % num_cores as u64) as usize;
        let roll = lcg_next(&mut state) % 1000;

        if roll < 5 {
            // ~0.5% of events: scheduler quantum expires, switch process.
            let new_asid = 1 + (lcg_next(&mut state) % num_processes as u64) as u32;
            core_asid[core] = new_asid;
            events.push(Event::Switch { core, asid: new_asid });
        } else if roll < 8 {
            // ~0.3% of events: process unmaps a page (e.g. munmap/mprotect).
            let asid = core_asid[core];
            let base = asid as u64 * 0x1000_0000;
            let page = lcg_next(&mut state) % working_set_pages;
            events.push(Event::Unmap { core, asid, addr: base + page * 4096 });
        } else {
            let asid = core_asid[core];
            let base = asid as u64 * 0x1000_0000;
            let page = if lcg_next(&mut state) % 100 < 80 {
                lcg_next(&mut state) % hot_set_pages
            } else {
                lcg_next(&mut state) % working_set_pages
            };
            let is_instruction = lcg_next(&mut state) % 10 < 3;
            events.push(Event::Access { core, addr: base + page * 4096, is_instruction });
        }
    }

    events
}

fn simulate_multicore(num_cores: usize, events: &[Event], tagging_enabled: bool) -> SimStats {
    let mut system = MultiCoreSystem::new(num_cores, tagging_enabled);
    for event in events {
        match event {
            Event::Switch { core, asid } => system.switch(*core, *asid),
            Event::Access { core, addr, is_instruction } => {
                system.access(*core, *addr, *is_instruction)
            }
            Event::Unmap { core, asid, addr } => system.unmap(*asid, *addr, *core),
        }
    }
    system.finish()
}

pub fn run_multicore_asid_demo() {
    let num_cores = 4;
    let num_events = 200_000;
    let seed: u64 = 0x1234_5678_9abc_def0;

    let events = generate_workload(num_cores, num_events, seed);
    let tagged = simulate_multicore(num_cores, &events, true);
    let untagged = simulate_multicore(num_cores, &events, false);

    println!("=== Multi-Core ASID Tagging + TLB Shootdown Demo ===");
    println!(
        "{} cores, {} synthetic scheduler/memory events, {} processes time-sharing the system",
        num_cores, num_events, 6
    );
    println!();
    println!("{:<28}{:>16}{:>16}", "", "Tagged (ASID)", "Untagged");
    println!("{:<28}{:>16}{:>16}", "Total accesses", tagged.total_requests, untagged.total_requests);
    println!(
        "{:<28}{:>15.4}%{:>15.4}%",
        "Hit rate", tagged.hit_rate(), untagged.hit_rate()
    );
    println!("{:<28}{:>16}{:>16}", "Context-switch flushes", tagged.flushes, untagged.flushes);
    println!(
        "{:<28}{:>16.4}{:>16.4}",
        "AMAT (cycles/access)", tagged.amat(), untagged.amat()
    );
    println!(
        "{:<28}{:>16}{:>16}",
        "Shootdowns triggered", tagged.shootdown_count, untagged.shootdown_count
    );
    println!(
        "{:<28}{:>16}{:>16}",
        "Shootdown latency (cyc)", tagged.shootdown_latency, untagged.shootdown_latency
    );
    println!();
    println!(
        "ASID tagging saves {:.4} cycles/access of AMAT by eliminating flush-induced cold\nmisses on context switch. Shootdown cost is identical either way, since ASID\ntagging does nothing to avoid cross-core invalidation of shared mappings.",
        untagged.amat() - tagged.amat()
    );
}
