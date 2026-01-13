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
}

fn main() {
    //awawdaddda
}
