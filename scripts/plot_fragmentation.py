import matplotlib.pyplot as plt

lines_processed = []
fragmentation = []

print("Starting to parse simulation_log.txt...")

# We use 'utf-16' as a fallback because PowerShell '>' often uses it
# 'errors=ignore' skips any weird characters that might break the parser
try:
    with open("simulation_log.txt", "r", encoding="utf-16", errors="ignore") as f:
        content = f.readlines()
    if len(content) < 5: # If UTF-16 failed, try standard UTF-8
        raise ValueError
except:
    with open("simulation_log.txt", "r", encoding="utf-8", errors="ignore") as f:
        content = f.readlines()

for line in content:
    clean_line = line.strip()
    if "DATA_POINT" in clean_line:
        try:
            # Split by comma and handle potential extra spaces
            parts = [p.strip() for p in clean_line.split(",")]
            # The word DATA_POINT might be parts[0], so we need 1 and 2
            l_num = int(parts[1])
            f_val = float(parts[2])
            
            lines_processed.append(l_num)
            fragmentation.append(f_val)
        except (ValueError, IndexError):
            continue

if not lines_processed:
    print("ERROR: Still no DATA_POINT lines found. Try running 'cargo run | Out-File -Encoding utf8 simulation_log.txt' next time.")
else:
    plt.figure(figsize=(10, 6))
    plt.plot(lines_processed, fragmentation, marker='o', linestyle='-', color='b')
    plt.title("Memory Fragmentation Over Time (THP Promotion Strategy)")
    plt.xlabel("Total Memory Accesses")
    plt.ylabel("Internal Fragmentation (MB)")
    plt.grid(True)
    plt.savefig("fragmentation_graph.png")
    print(f"Successfully plotted {len(lines_processed)} points to fragmentation_graph.png")