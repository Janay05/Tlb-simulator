use std::fs::File;
use std::io::{BufRead, BufReader};

#[derive(Clone,Copy,Debug)]
struct TlbEntry {
    tag: u64,
    pfn: u64,
    valid: bool,
    last_access: u64,
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
            tag: 0,
            pfn: 0,
            valid: false,
            last_access: 0,
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
        let index_bits = (self.num_sets as f64).log2().ceil() as u32; //used ilog2() because it performs integer logarithms as while using normal log by casting an integer as float, the float can sometimes return 2 as 1.9999999 which wiould then be truncated to 1 in the next casting conversion
        let tag = address >> (offset_bits + index_bits); 

        (index,tag) 


    }
    pub fn lookup(&mut self, address: u64) -> bool {
        self.timer += 1;
        let (index,tag) = self.get_indices(address);
        for entry in &mut self.sets[index] {
            if tag == entry.tag && entry.valid == true {
                self.hits = self.hits + 1 ;
                
                entry.last_access = self.timer;//update the last_access variable
                return true;
            }
        }
        self.misses = self.misses + 1 ;
        return false;
    }
    pub fn insert(&mut self, address: u64, pfn: u64) -> () {
        let (index,tag) = self.get_indices(address);
        let mut victim_way: usize = 0;
        let mut min_time = u64::MAX;
        for (way_index,entry) in self.sets[index].iter_mut().enumerate() {
            if entry.valid == false {
                victim_way = way_index;
                break;
            }
            else {
                if entry.last_access < min_time {
                    min_time = entry.last_access;
                    victim_way = way_index;
                }
            }
        }
        self.sets[index][victim_way].tag = tag;
        self.sets[index][victim_way].pfn = pfn;
        self.sets[index][victim_way].valid = true;
        self.sets[index][victim_way].last_access = self.timer;
    }
 }

struct TlbHierarchy {
    l1d: Tlb,
    l1i: Tlb,
    l2: Tlb,
    total_latency: u64,
}

impl TlbHierarchy {
    pub fn new(
        l1i_sets: usize, l1i_assoc: usize,
        l1d_sets: usize, l1d_assoc: usize,
        l2_sets: usize, l2_assoc: usize,
    ) -> Self {
        TlbHierarchy {
            l1i: Tlb::new(l1i_sets, l1i_assoc),
            l1d: Tlb::new(l1d_sets, l1d_assoc),
            l2: Tlb::new(l2_sets, l2_assoc),
            total_latency: 0,
        }
    }

    pub fn access(&mut self, addr: u64, is_instruction: bool) {
        let l1_latency = 1;
        let l2_latency = 10;
        let mem_latency = 200;

        let l1 = if is_instruction { &mut self.l1i } else { &mut self.l1d };

        self.total_latency += l1_latency;

        if l1.lookup(addr) {
            return;
        }

        self.total_latency += l2_latency;
        if self.l2.lookup(addr) {
            let pfn = addr >> 12;
            l1.insert(addr, pfn);
        } else {
            self.total_latency += mem_latency;
            let pfn = addr >> 12;
            self.l2.insert(addr, pfn);
            l1.insert(addr, pfn);
        }
    }
}

pub fn parsing_logic(line: &str) -> Option<(u64, bool)> {
    let parts: Vec<&str> = line.split_whitespace().collect();
    if parts.len() < 2 { return None; }

    let op = parts[0];
    let addr_str = parts[1].split(',').next()?;
    let addr = u64::from_str_radix(addr_str, 16).ok()?;

    match op {
        "I" => Some((addr, true)),  // Instruction fetch
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
        let line: String  = line_result.expect("error reading");

        if let Some((addr,is_instr)) = parsing_logic(&line) {
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

    let total_accesses = tlb.l1d.hits + tlb.l1i.hits + tlb.l1i.misses + tlb.l1d.misses ;
    let l1_hit_rate = (tlb.l1d.hits + tlb.l1i.hits) as f64/total_accesses as f64 * 100.00;
    let l2_hit_rate = tlb.l2.hits as f64/(tlb.l1i.misses + tlb.l1d.misses) as f64 * 100.00;

    let amat = tlb.total_latency as f64 / total_accesses as f64;
    println!("total accesses: {}", total_accesses);
    println!("l1 hit ratio:      {:.4}%", l1_hit_rate);
    println!("l2 hit ratio:      {:.4}%", l2_hit_rate);
    println!("AMAT: {}", amat);

}
