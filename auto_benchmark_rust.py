"""
Script Controller: auto_benchmark_rust.py
Fokus Pengujian: Load Time dan Load RAM untuk RUST HYBRID (30 Iterasi)
Fitur Ekstra: Penghapusan hasil eksplisit & Log Isolasi OS.
"""
import os
import subprocess
import time
import gc
import csv
import sys

# Konfigurasi Benchmark
N_ITERATIONS = 30
WARMUP_RUNS = 2
TARGET_SCRIPT = "run_rust_hybrid.py"                  # <- Target diubah ke Rust
OUTPUT_CSV = "benchmark_rust_load_results.csv"      # <- Nama output CSV dibedakan

def hapus_hasil_lama_secara_eksplisit():
    """Menghapus file CSV hasil benchmark sebelumnya jika ada."""
    if os.path.exists(OUTPUT_CSV):
        print(f"[*] Menemukan file hasil lama ('{OUTPUT_CSV}').")
        os.remove(OUTPUT_CSV)
        print(f"[*] File lama berhasil DIHAPUS. Memulai dengan memori penyimpanan bersih.")
    else:
        print(f"[*] Tidak ada file hasil lama. Penyimpanan sudah bersih.")

def run_single_iteration(iter_name):
    """Menjalankan subprocess dengan log pembersihan memori yang eksplisit."""
    
    # 1. Pembersihan RAM Controller secara Eksplisit
    print(f"   [{iter_name}] -> Membersihkan Garbage Collector di Controller...")
    gc.collect() 
    
    try:
        # 2. Membuka Subproses (Restart OS Level)
        print(f"   [{iter_name}] -> Membuka Sub-Proses OS Baru untuk Rust...")
        result = subprocess.run(
            [sys.executable, TARGET_SCRIPT],
            capture_output=True,
            text=True,
            check=True
        )
        
        # 3. Subproses selesai, OS otomatis mematikan memori
        print(f"   [{iter_name}] -> Sub-Proses selesai. OS telah mereset alokasi RAM.")
        
        # Mencari baris yang mengandung output metrik
        for line in result.stdout.split('\n'):
            if line.startswith("BENCHMARK_LOAD|"):
                parts = line.split('|')
                return {
                    "load_time": float(parts[1]),
                    "load_ram": float(parts[2])
                }
                
    except subprocess.CalledProcessError as e:
        print(f"   [ERROR] Script gagal dieksekusi!\n{e.stderr}")
        return None
        
    print(f"   [ERROR] Output BENCHMARK_LOAD tidak ditemukan di console.")
    return None

if __name__ == "__main__":
    print("======================================================")
    print(f"=== MEMULAI BENCHMARK OTOMATIS RUST HYBRID (LOAD) ===")
    print("======================================================")
    print(f"Target Script : {TARGET_SCRIPT}")
    print(f"Iterasi       : {N_ITERATIONS}x")
    print("-" * 54)
    
    # [EKSPLISIT] Hapus data lama sebelum memulai apa pun
    hapus_hasil_lama_secara_eksplisit()
    print("-" * 54)
    
    # 1. Fase Pemanasan (Warm-up)
    print(f"Menjalankan fase pemanasan ({WARMUP_RUNS} iterasi) untuk melatih CPU Cache...")
    for w in range(WARMUP_RUNS):
        run_single_iteration(f"Warm-up {w+1}")
        print("   ---")
        
    print("\n[✓] Fase pemanasan selesai. Memulai PENGUKURAN UTAMA 30 Iterasi...\n")
    
    results_history = []
    
    # 2. Fase Pengukuran Utama (30 Iterasi)
    for i in range(N_ITERATIONS):
        iteration_label = f"Iterasi {i+1}/{N_ITERATIONS}"
        print(f"-> Memulai {iteration_label}")
        
        metrics = run_single_iteration(iteration_label)
        
        if metrics:
            metrics["iteration"] = i + 1
            results_history.append(metrics)
            print(f"   [BERHASIL] Load Time: {metrics['load_time']:.2f} detik | RAM Used: {metrics['load_ram']:.2f} MB\n")
        
        # Jeda 3 detik untuk mendinginkan CPU
        time.sleep(3) 
        
    # 3. Menyimpan hasil ke CSV
    if results_history:
        with open(OUTPUT_CSV, mode='w', newline='') as file:
            writer = csv.DictWriter(file, fieldnames=["iteration", "load_time", "load_ram"])
            writer.writeheader()
            writer.writerows(results_history)
            
        print("======================================================")
        print(f"[✓] SELESAI! Hasil 30 iterasi RUST telah DISIMPAN ke '{OUTPUT_CSV}'.")
        print("======================================================")