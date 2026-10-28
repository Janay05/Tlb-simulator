use std::collections::{HashMap, HashSet};

use crate::tlb::Tlb;

pub struct TlbHierarchy {
    pub l1d: Tlb,
    pub l1i: Tlb,
    pub l2: Tlb,
    pub total_latency: u64,
    pub total_requests: u64,
    pub promotion_threshold: usize,
    tracker: HashMap<u64, HashSet<u64>>,
    pub total_huge_pages: u64,
    pub unique_4kb_pages_touched: HashSet<u64>,
}

impl TlbHierarchy {
    pub fn new(
        l1i_sets: usize,
        l1i_assoc: usize,
        l1d_sets: usize,
        l1d_assoc: usize,
        l2_sets: usize,
        l2_assoc: usize,
    ) -> Self {
        Self::with_threshold(l1i_sets, l1i_assoc, l1d_sets, l1d_assoc, l2_sets, l2_assoc, 64)
    }

    pub fn with_threshold(
        l1i_sets: usize,
        l1i_assoc: usize,
        l1d_sets: usize,
        l1d_assoc: usize,
        l2_sets: usize,
        l2_assoc: usize,
        threshold: usize,
    ) -> Self {
        TlbHierarchy {
            l1i: Tlb::new(l1i_sets, l1i_assoc),
            l1d: Tlb::new(l1d_sets, l1d_assoc),
            l2: Tlb::new(l2_sets, l2_assoc),
            total_latency: 0,
            total_requests: 0,
            promotion_threshold: threshold,
            tracker: HashMap::<u64, HashSet<u64>>::new(),
            total_huge_pages: 0,
            unique_4kb_pages_touched: HashSet::<u64>::new(),
        }
    }

    pub fn access(&mut self, addr: u64, is_instruction: bool) {
        self.total_requests += 1;
        self.unique_4kb_pages_touched.insert(addr >> 12);
        let l1_latency = 1;
        let l2_latency = 10;
        let mem_latency = 200;

        let region_key = addr & !0x1FFFFF;

        let l1 = if is_instruction {
            &mut self.l1i
        } else {
            &mut self.l1d
        };

        self.total_latency += l1_latency;

        if l1.lookup(addr) {
            return;
        }

        self.total_latency += l2_latency;
        if self.l2.lookup(addr) {
            let pfn = addr >> 12;
            l1.insert(addr, pfn, false);
        } else {
            self.total_latency += mem_latency;
            let pfn = addr >> 12;
            self.l2.insert(addr, pfn, false);
            l1.insert(addr, pfn, false);

            let vpn_4kb = addr >> 12;
            let set = self.tracker.entry(region_key).or_default();
            set.insert(vpn_4kb);

            if set.len() >= self.promotion_threshold {
                self.promote(region_key, is_instruction);
                self.tracker.remove(&region_key);
            }
        }
    }

    pub fn promote(&mut self, base_addr: u64, is_instruction: bool) {
        self.total_huge_pages += 1;
        self.l1i.invalidate_region(base_addr);
        self.l1d.invalidate_region(base_addr);
        self.l2.invalidate_region(base_addr);

        let pfn_huge = base_addr >> 12;
        self.l2.insert_huge(base_addr, pfn_huge);

        let l1 = if is_instruction {
            &mut self.l1i
        } else {
            &mut self.l1d
        };
        l1.insert_huge(base_addr, pfn_huge);
    }

    pub fn total_hits(&self) -> u64 {
        self.l1i.hits + self.l1d.hits + self.l2.hits
    }

    pub fn total_evictions(&self) -> u64 {
        self.l1i.evictions + self.l1d.evictions + self.l2.evictions
    }

    pub fn total_huge_hits(&self) -> u64 {
        self.l1i.huge_hits + self.l1d.huge_hits + self.l2.huge_hits
    }

    pub fn hit_rate(&self) -> f64 {
        if self.total_requests == 0 {
            return 0.0;
        }
        self.total_hits() as f64 / self.total_requests as f64 * 100.0
    }

    pub fn amat(&self) -> f64 {
        if self.total_requests == 0 {
            return 0.0;
        }
        self.total_latency as f64 / self.total_requests as f64
    }
}
