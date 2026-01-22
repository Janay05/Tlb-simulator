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
        let index_bits = (self.num_sets).ilog2() as u32; //used ilog2() because it performs integer logarithms as while using normal log by casting an integer as float, the float can sometimes return 2 as 1.9999999 which wiould then be truncated to 1 in the next casting conversion
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

fn main() {
    let mut tlb = Tlb::new(4,   2);

    let workload = vec![0x1000, 0x1004,
        0x2000,
        0x5000,
        0x1008,
        0x9000,
        0x1000,
        ];

    println!("{:<15} | {:<8} | {:<5} | {:<5}", "Address", "Result", "Set", "Tag");

    for addr in workload {
        let (index,tag) = tlb.get_indices(addr);
        
        let hit = tlb.lookup(addr);
        let result_str = if hit {"HIT"} else {"MISS"};
        println!("{:<15} | {:<8} | {:<5} | {:<5}",addr,result_str,index,tag);

        if !hit {
            let pfn = addr >> 12;
            tlb.insert(addr,pfn);
        }
    }

    println!("final result");
    println!("Hits : {}", tlb.hits);
    println!("Miss : {}", tlb.misses);

    let hit_ratio = (tlb.hits as f64/(tlb.misses + tlb.hits) as f64) * 100.0;

    println!("Hit rate : {:.2}%", hit_ratio);

}
