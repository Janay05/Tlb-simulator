#[derive(Clone, Copy, Debug)]
struct TlbEntry {
    vpn: u64,
    tag: u64,
    pfn: u64,
    valid: bool,
    last_access: u64,
    is_huge: bool,
    // Address-Space ID this translation belongs to. Lets entries from
    // different processes coexist in the same TLB without a flush.
    asid: u32,
}

pub struct Tlb {
    sets: Vec<Vec<TlbEntry>>,
    num_sets: usize,
    associativity: usize,

    //Stats
    pub hits: u64,
    pub huge_hits: u64,
    pub misses: u64,
    // Full-TLB flushes forced by an untagged context switch.
    pub flushes: u64,

    //Global Clock variable for LRU
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
    fn get_indices(&self, address: u64) -> (usize, u64) {
        let offset_bits = 12; //Assuming a standard 4KB page

        let index = (address >> offset_bits) as usize % self.num_sets; //(address >> offset_bits) as usize & (self.num_sets - 1) this is faster than using % as it simulates the absolute speed of the hardware
        let index_bits = (self.num_sets as f64).log2().ceil() as u32;
        let tag = address >> (offset_bits + index_bits);

        (index, tag)
    }
    pub fn lookup(&mut self, address: u64) -> bool {
        self.timer += 1;
        let (index, tag_4kb) = self.get_indices(address);

        let tag_2mb = address >> 21;

        for entry in &mut self.sets[index] {
            if !entry.valid || entry.asid != self.current_asid {
                continue;
            }

            if entry.is_huge {
                if tag_2mb == entry.tag {
                    self.hits += 1;
                    self.huge_hits += 1;
                    entry.last_access = self.timer;
                    return true;
                }
            } else {
                if tag_4kb == entry.tag {
                    self.hits += 1;
                    entry.last_access = self.timer; //update the last_access variable
                    return true;
                }
            }
        }
        self.misses += 1;
        return false;
    }
    pub fn insert(&mut self, address: u64, pfn: u64, is_huge: bool) {
        let (index, tag) = self.get_indices(address);
        let mut victim_way: usize = 0;
        let mut min_time = u64::MAX;
        for (way_index, entry) in self.sets[index].iter_mut().enumerate() {
            if entry.valid == false {
                victim_way = way_index;
                break;
            } else {
                if entry.last_access < min_time {
                    min_time = entry.last_access;
                    victim_way = way_index;
                }
            }
        }
        self.sets[index][victim_way].tag = tag;
        self.sets[index][victim_way].pfn = pfn;
        self.sets[index][victim_way].vpn = address >> 12;
        self.sets[index][victim_way].valid = true;
        self.sets[index][victim_way].is_huge = is_huge;
        self.sets[index][victim_way].last_access = self.timer;
        self.sets[index][victim_way].asid = self.current_asid;
    }

    pub fn insert_huge(&mut self, address: u64, pfn: u64) {
        let index = (address >> 21) as usize % self.num_sets;
        let tag = address >> 21;

        let mut victim_way: usize = 0;
        let mut min_time = u64::MAX;
        for (way_index, entry) in self.sets[index].iter_mut().enumerate() {
            if !entry.valid {
                victim_way = way_index;
                break;
            } else {
                if entry.last_access < min_time {
                    min_time = entry.last_access;
                    victim_way = way_index;
                }
            }
        }
        self.sets[index][victim_way].tag = tag;
        self.sets[index][victim_way].pfn = pfn;
        self.sets[index][victim_way].vpn = address >> 12;
        self.sets[index][victim_way].is_huge = true;
        self.sets[index][victim_way].valid = true;
        self.sets[index][victim_way].asid = self.current_asid;
    }
    /// Invalidates a single 4KB translation for a specific ASID, if cached
    /// here. Returns true if an entry was actually invalidated, so callers
    /// can tell whether this core needed a shootdown at all.
    pub fn invalidate_page_for_asid(&mut self, asid: u32, address: u64) -> bool {
        let (index, tag) = self.get_indices(address);
        let mut invalidated = false;
        for entry in &mut self.sets[index] {
            if entry.valid && !entry.is_huge && entry.asid == asid && entry.tag == tag {
                entry.valid = false;
                invalidated = true;
            }
        }
        invalidated
    }

    pub fn invalidate_region(&mut self, base_address: u64) -> () {
        let start_vpn = base_address >> 12;
        let end_vpn = start_vpn + 511;
        for index in 0..self.num_sets {
            for entry in &mut self.sets[index] {
                if !entry.is_huge && entry.vpn <= end_vpn && entry.vpn >= start_vpn {
                    entry.valid = false;
                }
            }
        }
    }
}
