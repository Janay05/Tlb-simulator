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

pub fn parsing_logic(tlb:&mut  Tlb, line: &str) -> () {
    let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 2 {return;}

        let addr_str = match parts[1].split(',').next() {
            Some(s) => s,
            None => return
        };

        if let Ok(addr) = u64::from_str_radix(addr_str, 16) {
            if !tlb.lookup(addr) {
                let pfn = addr >> 12;
                tlb.insert(addr, pfn);
            }
        }
}

fn main() {
    let mut tlb = Tlb::new(16, 4);

    let file = File::open("trace.txt").expect("could not find trace.txt");
    let reader = BufReader::new(file);

    println!("file processing starts");

    for (line_num, line_result) in reader.lines().enumerate() {
        let line: String  = line_result.expect("error reading");

        if line.starts_with(" L") || line.starts_with(" S") {
            parsing_logic(&mut tlb, &line);
        }
        else if line.starts_with(" M") {
            parsing_logic(&mut tlb, &line);
            tlb.timer += 1;
            tlb.hits += 1; 
            
        }  

        //to track progress while executing
        if line_num % 200000 == 0 && line_num > 0 {
            println!("processed {} lines...", line_num);
        }
    }


    let total = tlb.hits + tlb.misses;
    let hit_ratio = (tlb.hits as f64 / total as f64) * 100.0;

    println!("total accesses: {}", total);
    println!("TLB hits:       {}", tlb.hits);
    println!("TLB misses:     {}", tlb.misses);
    println!("hit ratio:      {:.4}%", hit_ratio);

}
