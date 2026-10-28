use tlb_sim_lib::multicore::{generate_workload, simulate_multicore};
use tlb_sim_lib::tlb::Tlb;

#[test]
fn test_asid_isolation() {
    let mut tlb = Tlb::with_tagging(16, 4, true);
    
    // Process 1 accesses address 0x1000
    tlb.switch_process(1);
    tlb.insert(0x1000, 0x1, false);
    assert!(tlb.lookup(0x1000), "Process 1 should hit its own mapping");

    // Switch to Process 2: same virtual address should NOT hit
    tlb.switch_process(2);
    assert!(!tlb.lookup(0x1000), "Process 2 should NOT hit Process 1's mapping with ASID enabled");

    // Switch back to Process 1: entry should still be valid (no flush occurred!)
    tlb.switch_process(1);
    assert!(tlb.lookup(0x1000), "Process 1 mapping should survive context switch with ASID");
}

#[test]
fn test_untagged_flush_on_switch() {
    let mut tlb = Tlb::with_tagging(16, 4, false); // Untagged mode
    
    tlb.switch_process(1);
    let base_flushes = tlb.flushes;
    tlb.insert(0x2000, 0x2, false);
    assert!(tlb.lookup(0x2000));

    // Switch to Process 2 forces full TLB flush
    tlb.switch_process(2);
    assert_eq!(tlb.flushes - base_flushes, 1, "Untagged switch must record a flush");

    // Switch back to Process 1: entry was destroyed by flush, so it must miss
    tlb.switch_process(1);
    assert_eq!(tlb.flushes - base_flushes, 2);
    assert!(!tlb.lookup(0x2000), "Entry must be gone due to context switch flush");
}

#[test]
fn test_huge_page_lookup() {
    let mut tlb = Tlb::new(16, 4);
    let base_2mb = 0x200000; // 2MB aligned
    
    tlb.insert_huge(base_2mb, base_2mb >> 12);

    // Any 4KB page within that 2MB region should hit!
    assert!(tlb.lookup(base_2mb + 0x0000), "Offset 0 should hit huge page");
    assert!(tlb.lookup(base_2mb + 0x4000), "Offset 16KB should hit huge page");
    assert!(tlb.lookup(base_2mb + 0x1FE000), "Offset near 2MB boundary should hit huge page");

    // An address outside the 2MB region should miss
    assert!(!tlb.lookup(base_2mb + 0x200000), "Address in next 2MB region must miss");
}

#[test]
fn test_multicore_asid_savings() {
    let events = generate_workload(4, 50000, 42);
    let tagged = simulate_multicore(4, &events, true);
    let untagged = simulate_multicore(4, &events, false);

    assert!(tagged.hit_rate() > untagged.hit_rate(), "Tagged hit rate must exceed untagged");
    assert!(tagged.amat() < untagged.amat(), "Tagged AMAT must be lower than untagged");
    assert_eq!(tagged.flushes, 0, "Tagged simulation must have 0 context-switch flushes");
    assert!(untagged.flushes > 0, "Untagged simulation must record flushes");
}
