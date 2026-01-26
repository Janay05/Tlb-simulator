use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{BufRead, BufReader};

#[derive(Clone, Copy, Debug)]
struct TlbEntry {
    vpn: u64,
    tag: u64,
    pfn: u64,
    valid: bool,
    last_access: u64,
    is_huge: bool,
}

struct Tlb {
    sets: Vec<Vec<TlbEntry>>,
    num_sets: usize,
    associativity: usize,

    //Stats
    hits: u64,
    misses: u64,

    //Global Clock variable for LRU
    timer: u64,
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
        };

        let sets = vec![vec![blank_entry; associativity]; num_sets];

        Tlb {
            sets,
            num_sets,
            associativity,
            hits: 0,
            misses: 0,
            timer: 0,
        }
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
            if !entry.valid {
                continue;
            }

            if entry.is_huge {
                if tag_2mb == entry.tag {
                    self.hits += 1;
                    entry.last_access = self.timer;
                    return true;
                }
            } else {
                if tag_4kb == entry.tag {
                    self.hits = self.hits + 1;
                    entry.last_access = self.timer; //update the last_access variable
                    return true;
                }
            }
        }
        self.misses = self.misses + 1;
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
    }

    pub fn insert_huge(&mut self, address: u64, pfn: u64) {
        let index = (address >> 21) as usize % self.num_sets;
        let tag = address >> 21;

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
        self.sets[index][victim_way].is_huge = true;
        self.sets[index][victim_way].valid = true;
    }
    pub fn invalidate_region(&mut self, base_address: u64) -> () {
        let start_vpn = base_address >> 12;
        let end_vpn = start_vpn + 511;
        for index in 0..self.num_sets {
            for entry in &mut self.sets[index] {
                if entry.is_huge == false && entry.vpn <= end_vpn && entry.vpn >= start_vpn {
                    entry.valid = false;
                }
            }
        }
    }
}

struct TlbHierarchy {
    l1d: Tlb,
    l1i: Tlb,
    l2: Tlb,
    total_latency: u64,
    tracker: HashMap<u64, HashSet<u64>>,
    total_huge_pages: u64,
    unique_4kb_pages_touched: HashSet<u64>,
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
        TlbHierarchy {
            l1i: Tlb::new(l1i_sets, l1i_assoc),
            l1d: Tlb::new(l1d_sets, l1d_assoc),
            l2: Tlb::new(l2_sets, l2_assoc),
            total_latency: 0,
            tracker: HashMap::<u64, HashSet<u64>>::new(),
            total_huge_pages: 0,
            unique_4kb_pages_touched: HashSet::<u64>::new(),
        }
    }

    pub fn access(&mut self, addr: u64, is_instruction: bool) {
        const PROMOTION_THRESHOLD: usize = 64;
        self.unique_4kb_pages_touched.insert(addr >> 12);;
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
            let set = self.tracker.entry(region_key).or_insert_with(HashSet::new);
            set.insert(vpn_4kb);

            if set.len() >= PROMOTION_THRESHOLD {
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
        "I" => Some((addr, true)),              // Instruction fetch
        "L" | "S" | "M" => Some((addr, false)), // Data access
        _ => None,
    }
}

fn main() {
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
        if line_num % 200000 == 0 && line_num > 0 {
            println!("processed {} lines...", line_num);
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

    // 2. RESEARCHER (MEMORY WASTE) METRICS
    // We assume the OS allocates memory in 4KB chunks unless it's a Huge Page.
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
    println!("        TLB HIERARCHY SIMULATION REPORT       ");;
    println!("Total CPU Requests:      {}", total_accesses);
    println!("AMAT (Avg Latency):      {:.4} cycles/access", amat);
    println!("L1 global hit ratio:     {:.4}%", l1_hit_rate);
    println!("L2 local hit ratio:      {:.4}%", l2_hit_rate);
    println!("Huge pages promoted:     {}", tlb.total_huge_pages);
    println!("Actual memory touched:   {:.2} MB", actual_usage_mb);
    println!("Physical footprint:      {:.2} MB", physical_footprint_mb);
    println!("Internal fragmentation:  {:.2} MB", fragmentation_mb);
}
