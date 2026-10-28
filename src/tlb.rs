use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct TlbEntrySnapshot {
    pub valid: bool,
    pub vpn: u64,
    pub tag: u64,
    pub pfn: u64,
    pub asid: u32,
    pub is_huge: bool,
    pub last_access: u64,
}

#[derive(Clone, Copy, Debug)]
pub struct TlbEntry {
    pub vpn: u64,
    pub tag: u64,
    pub pfn: u64,
    pub valid: bool,
    pub last_access: u64,
    pub is_huge: bool,
    // Address-Space ID this translation belongs to. Lets entries from
    // different processes coexist in the same TLB without a flush.
    pub asid: u32,
}

#[derive(Clone, Debug)]
pub struct Tlb {
    sets: Vec<Vec<TlbEntry>>,
    num_sets: usize,
    associativity: usize,

    // Statistics
    pub hits: u64,
    pub huge_hits: u64,
    pub misses: u64,
    pub evictions: u64,
    // Full-TLB flushes forced by an untagged context switch.
    pub flushes: u64,

    // Global Clock variable for LRU
    timer: u64,

    // ASID of the process currently scheduled on this TLB's core.
    current_asid: u32,
    // If false, a switch_process() call to a different ASID flushes the
    // whole TLB (models hardware without PCID/ASID tagging support).
    tagging_enabled: bool,
}

impl Tlb {
    pub fn new(num_sets: usize, associativity: usize) -> Self {
        assert!(num_sets > 0);
        assert!(associativity > 0);
        let blank_entry = TlbEntry {
            vpn: 0,
            tag: 0,
            pfn: 0,
            valid: false,
            last_access: 0,
            is_huge: false,
            asid: 0,
        };

        let sets = vec![vec![blank_entry; associativity]; num_sets];

        Tlb {
            sets,
            num_sets,
            associativity,
            hits: 0,
            huge_hits: 0,
            misses: 0,
            evictions: 0,
            flushes: 0,
            timer: 0,
            current_asid: 0,
            tagging_enabled: true,
        }
    }

    pub fn with_tagging(num_sets: usize, associativity: usize, tagging_enabled: bool) -> Self {
        let mut tlb = Self::new(num_sets, associativity);
        tlb.tagging_enabled = tagging_enabled;
        tlb
    }

    pub fn associativity(&self) -> usize {
        self.associativity
    }

    pub fn num_sets(&self) -> usize {
        self.num_sets
    }

    pub fn tagging_enabled(&self) -> bool {
        self.tagging_enabled
    }

    pub fn get_sets_snapshot(&self) -> Vec<Vec<TlbEntrySnapshot>> {
        self.sets
            .iter()
            .map(|set| {
                set.iter()
                    .map(|e| TlbEntrySnapshot {
                        valid: e.valid,
                        vpn: e.vpn,
                        tag: e.tag,
                        pfn: e.pfn,
                        asid: e.asid,
                        is_huge: e.is_huge,
                        last_access: e.last_access,
                    })
                    .collect()
            })
            .collect()
    }

    pub fn current_asid(&self) -> u32 {
        self.current_asid
    }

    /// Switches the process scheduled on this core. With ASID tagging this
    /// is O(1) — stale entries from other processes simply stop matching.
    /// Without it, every switch must flush the whole TLB to avoid aliasing
    /// between address spaces, which is exactly the cost PCID was
    /// (re)introduced to eliminate for KPTI in the Meltdown mitigations.
    pub fn switch_process(&mut self, new_asid: u32) {
        if new_asid == self.current_asid {
            return;
        }
        if !self.tagging_enabled {
            for set in &mut self.sets {
                for entry in set.iter_mut() {
                    entry.valid = false;
                }
            }
            self.flushes += 1;
        }
        self.current_asid = new_asid;
    }

    fn get_indices_4kb(&self, address: u64) -> (usize, u64) {
        let offset_bits = 12; // Standard 4KB page
        let index = (address >> offset_bits) as usize % self.num_sets;
        let index_bits = (self.num_sets as f64).log2().ceil() as u32;
        let tag = address >> (offset_bits + index_bits);
        (index, tag)
    }

    fn get_indices_huge(&self, address: u64) -> (usize, u64) {
        let offset_bits = 21; // 2MB huge page
        let index = (address >> offset_bits) as usize % self.num_sets;
        let index_bits = (self.num_sets as f64).log2().ceil() as u32;
        let tag = address >> (offset_bits + index_bits);
        (index, tag)
    }

    pub fn lookup(&mut self, address: u64) -> bool {
        self.timer += 1;

        // 1. Check for 4KB mapping in standard set index
        let (index_4k, tag_4k) = self.get_indices_4kb(address);
        for entry in &mut self.sets[index_4k] {
            if entry.valid && entry.asid == self.current_asid && !entry.is_huge && entry.tag == tag_4k {
                self.hits += 1;
                entry.last_access = self.timer;
                return true;
            }
        }

        // 2. Check for 2MB Huge Page mapping in huge-page set index
        let (index_huge, tag_huge) = self.get_indices_huge(address);
        for entry in &mut self.sets[index_huge] {
            if entry.valid && entry.asid == self.current_asid && entry.is_huge && entry.tag == tag_huge {
                self.hits += 1;
                self.huge_hits += 1;
                entry.last_access = self.timer;
                return true;
            }
        }

        self.misses += 1;
        false
    }

    pub fn insert(&mut self, address: u64, pfn: u64, is_huge: bool) {
        if is_huge {
            self.insert_huge(address, pfn);
            return;
        }

        let (index, tag) = self.get_indices_4kb(address);
        let mut victim_way: usize = 0;
        let mut min_time = u64::MAX;

        for (way_index, entry) in self.sets[index].iter().enumerate() {
            if !entry.valid {
                victim_way = way_index;
                break;
            } else if entry.last_access < min_time {
                min_time = entry.last_access;
                victim_way = way_index;
            }
        }

        if self.sets[index][victim_way].valid {
            self.evictions += 1;
        }

        self.sets[index][victim_way] = TlbEntry {
            vpn: address >> 12,
            tag,
            pfn,
            valid: true,
            last_access: self.timer,
            is_huge: false,
            asid: self.current_asid,
        };
    }

    pub fn insert_huge(&mut self, address: u64, pfn: u64) {
        let (index, tag) = self.get_indices_huge(address);
        let mut victim_way: usize = 0;
        let mut min_time = u64::MAX;

        for (way_index, entry) in self.sets[index].iter().enumerate() {
            if !entry.valid {
                victim_way = way_index;
                break;
            } else if entry.last_access < min_time {
                min_time = entry.last_access;
                victim_way = way_index;
            }
        }

        if self.sets[index][victim_way].valid {
            self.evictions += 1;
        }

        self.sets[index][victim_way] = TlbEntry {
            vpn: address >> 12,
            tag,
            pfn,
            valid: true,
            last_access: self.timer,
            is_huge: true,
            asid: self.current_asid,
        };
    }

    /// Invalidates a single 4KB translation for a specific ASID, if cached
    /// here. Returns true if an entry was actually invalidated, so callers
    /// can tell whether this core needed a shootdown at all.
    pub fn invalidate_page_for_asid(&mut self, asid: u32, address: u64) -> bool {
        let (index, tag) = self.get_indices_4kb(address);
        let mut invalidated = false;
        for entry in &mut self.sets[index] {
            if entry.valid && !entry.is_huge && entry.asid == asid && entry.tag == tag {
                entry.valid = false;
                invalidated = true;
            }
        }
        invalidated
    }

    /// Invalidate any 4KB mappings belonging to the 2MB region being promoted
    pub fn invalidate_region(&mut self, base_address: u64) {
        let start_vpn = base_address >> 12;
        let end_vpn = start_vpn + 511;
        for set in &mut self.sets {
            for entry in set.iter_mut() {
                if entry.valid && !entry.is_huge && entry.vpn >= start_vpn && entry.vpn <= end_vpn {
                    entry.valid = false;
                }
            }
        }
    }
}
