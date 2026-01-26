# Cycle-Accurate TLB Hierarchy Simulator with THP Promotion

A high-performance systems-level simulator implemented in **Rust** to analyze the performance and memory overhead of **Transparent Huge Page (THP)** promotion heuristics. This project quantifies the **Space-Time Trade-off** between average memory access latency (AMAT) and internal fragmentation in modern Memory Management Units (MMUs).

---

## 🚀 Project Motivation
Modern operating systems utilize **Huge Pages (2MB)** to minimize TLB misses and shorten page table walks. However, allocating large blocks can lead to **Internal Fragmentation** if the workload lacks spatial density. 

As a **B.Tech CSE student** focused on **systems and performance programming**, I developed this simulator to:
1.  **Hardware Modeling**: Simulate a realistic 3-level TLB hierarchy (L1I, L1D, and L2).
2.  **Heuristic Evaluation**: Test a demand-based promotion policy using a **HashMap-based sparse tracker**.
3.  **Data Visualization**: Quantify the cost of performance gains in terms of physical memory waste.



---

## 🛠️ Tech Stack & Methodology
* **Language**: **Rust** — Selected for zero-cost abstractions and memory safety, essential for cycle-accurate simulation.
* **Trace Generation**: **Valgrind (Lackey)** — Used to capture real-world memory access patterns (Instruction, Load, Store, Modify).
* **Analysis**: **Python (Matplotlib)** — Used to parse simulation logs and visualize fragmentation dynamics.
* **Replacement Policy**: **Least Recently Used (LRU)** — Implemented via a global timer to maintain temporal locality.

---

## 📊 Architecture & Promotion Logic

### TLB Hierarchy Configuration
* **L1 Instruction (L1I)**: 16 sets, 4-way associative.
* **L1 Data (L1D)**: 16 sets, 4-way associative.
* **L2 Unified**: 128 sets, 8-way associative.

### Promotion Heuristic
The simulator monitors 2MB virtual regions. When a region reaches a threshold of **64 unique 4KB page touches**, the simulator:
1.  Invalidates specific 4KB entries across the hierarchy.
2.  Promotes the region to a **2MB Huge Page**.
3.  Calculates **Internal Fragmentation** based on the delta between physical footprint and actual usage.



---

## 📈 Performance Analysis (Workload: grep)
Analysis of a trace containing **3.5 million memory accesses**:

| Metric | Result |
| :--- | :--- |
| **Total CPU Requests** | 3,515,365 |
| **Huge Page Hits** | 59,311 |
| **L1 Global Hit Ratio** | 99.98% |
| **AMAT** | 1.0239 cycles |
| **Internal Fragmentation** | 4.64 MB |

### Dynamic Fragmentation Analysis
The "Sawtooth" behavior in the graph below illustrates the policy's reaction. Spikes represent new 2MB promotions, while the subsequent downward slopes represent the workload "filling in" the Huge Page, thereby increasing utilization and reducing waste.
![alt text](results/fragmentation_graph.png)



---

## 🛠️ How to Run
1.  **Generate Trace**:
    ```bash
    valgrind --tool=lackey --trace-mem=yes ./stressor > trace.txt 2>&1
    ```
2.  **Run Simulator**:
    ```bash
    cargo run > simulation_log.txt
    ```
3.  **Generate Plot**:
    ```bash
    python plot_frag.py
    ```

---

## 🔮 Future Improvements & Research Directions
Implementing these features would further refine the simulator's accuracy and utility in a research environment:

* **Huge Page Demotion Logic**: Implement an aging mechanism to "split" Huge Pages back into 4KB entries if access density falls below a critical threshold, reclaiming fragmented RAM.
* **Adaptive Thresholding**: Dynamically adjust the promotion threshold (currently 64) based on real-time system memory pressure or hit-rate feedback.
* **Multi-Core TLB Coherence**: Extend the hierarchy to support multiple cores, necessitating the simulation of **TLB Shootdowns** and cache coherence protocols.
* **Prefetching Support**: Integrate spatial prefetchers to predict upcoming page accesses and trigger promotions before the CPU requests them.