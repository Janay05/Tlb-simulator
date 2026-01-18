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
impl Tlb {
    fn get_indices(&self, address: u64) -> (usize, u64) {
        let offset_bits = 12; //Assuming a standard 4KB page

        let index = (address >> offset_bits) as usize % self.num_sets; //(address >> offset_bits) as usize & (self.num_sets - 1) this is faster than using % as it simulates the absolute speed of the hardware
        let index_bits = (self.num_sets as f64).log2() as u32;
        let tag = address >> (offset_bits + index_bits); 

        (index,tag) 


    }
    pub fn lookup(&mut self, address: u64) -> bool {

        let (index,tag) = self.get_indices(address);
        for entry in &mut self.sets[index] {
            if tag == entry.tag && entry.valid == true {
                self.hits = self.hits + 1 ;
                //update the last_access variable
                return true;
            }
        }
        self.misses = self.misses + 1 ;
        return false;
    }
 }

fn main() {

}
