use pyo3::prelude::*;
use pyo3::types::PyDict;
use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;
use rayon::prelude::*; // Pustaka Dewa untuk Multi-threading

// --- FUNGSI HELPER: Ekstraksi Digit Pertama (Benford) ---
fn get_first_digit(mut n: u64) -> Option<usize> {
    if n == 0 { return None; } // Benford mengabaikan angka 0
    while n >= 10 {
        n /= 10;
    }
    Some(n as usize)
}

// --- 1. STRUKTUR DATA PEMBACA CSV ---
#[derive(Debug, Deserialize)]
struct RawTransaction {
    tx_id: String,
    timestamp: u64,
    vin: String,
    vout: String,
}

// Ditambahkan derive Clone agar bisa disalin antar thread
#[derive(Debug, Deserialize, Clone)] 
struct VoutData {
    address: String,
    value_satoshi: u64,
}

// --- 2. MESIN UNION-FIND (SYBIL CLUSTERING) ---
struct UnionFind {
    parent: Vec<usize>,
    size: Vec<usize>,
    pub address_to_id: HashMap<String, usize>,
    pub id_to_address: Vec<String>,
}

impl UnionFind {
    fn new() -> Self {
        Self {
            parent: Vec::new(),
            size: Vec::new(),
            address_to_id: HashMap::new(),
            id_to_address: Vec::new(),
        }
    }

    fn get_or_create_id(&mut self, address: String) -> usize {
        if let Some(&id) = self.address_to_id.get(&address) {
            id
        } else {
            let new_id = self.parent.len();
            self.parent.push(new_id);
            self.size.push(1);
            self.address_to_id.insert(address.clone(), new_id);
            self.id_to_address.push(address);
            new_id
        }
    }

    fn find(&mut self, mut i: usize) -> usize {
        while i != self.parent[i] {
            self.parent[i] = self.parent[self.parent[i]]; // Path compression
            i = self.parent[i];
        }
        i
    }

    fn union(&mut self, i: usize, j: usize) {
        let root_i = self.find(i);
        let root_j = self.find(j);
        if root_i != root_j {
            if self.size[root_i] < self.size[root_j] {
                self.parent[root_i] = root_j;
                self.size[root_j] += self.size[root_i];
            } else {
                self.parent[root_j] = root_i;
                self.size[root_i] += self.size[root_j];
            }
        }
    }
}

// --- 3. KELAS MESIN FORENSIK (DIPANGGIL OLEH PYTHON) ---
#[pyclass]
struct ForensicEngine {
    uf: UnionFind,
    adj_list: HashMap<String, Vec<String>>,
    
    // --- STATE BARU: Pelacakan Hukum Benford ---
    global_benford: [u64; 9],
    address_benford: HashMap<String, [u64; 9]>,
}

#[pymethods]
impl ForensicEngine {
    #[new]
    fn new() -> Self {
        ForensicEngine {
            uf: UnionFind::new(),
            adj_list: HashMap::new(),
            global_benford: [0; 9],
            address_benford: HashMap::new(),
        }
    }

    // FUNGSI 1: MEMUAT DATA DENGAN ARSITEKTUR MAP-REDUCE
    fn load_data(&mut self, folder_path: String) -> PyResult<usize> {
        let paths = fs::read_dir(Path::new(&folder_path)).expect("Gagal membaca direktori!");
        
        let mut csv_files = Vec::new();
        for path in paths {
            let file_path = path.unwrap().path();
            if file_path.extension().and_then(|s| s.to_str()) == Some("csv") {
                csv_files.push(file_path);
            }
        }

        println!("Memulai ekstraksi paralel pada {} file CSV...", csv_files.len());

        // ====================================================================
        // FASE MAP (PARALEL)
        // ====================================================================
        let extracted_data: Vec<(Vec<String>, Vec<VoutData>)> = csv_files.par_iter().flat_map(|file_path| {
            let mut reader = csv::ReaderBuilder::new()
                .buffer_capacity(8 * 1024 * 1024) 
                .from_path(file_path)
                .unwrap();
                
            let mut file_results = Vec::new();

            for result in reader.deserialize() {
                if let Ok(record) = result {
                    let record: RawTransaction = record;
                    
                    let vin_addrs: Vec<String> = serde_json::from_str(&record.vin).unwrap_or_default();
                    let vout_data: Vec<VoutData> = serde_json::from_str(&record.vout).unwrap_or_default();

                    let valid_vins: Vec<String> = vin_addrs.into_iter()
                        .filter(|a| !a.trim().is_empty() && a != "null").collect();
                    
                    // Modifikasi: Kita pertahankan struktur VoutData (termasuk nilai transaksi)
                    let valid_vouts: Vec<VoutData> = vout_data.into_iter()
                        .filter(|v| !v.address.trim().is_empty() && v.address != "null").collect();

                    if !valid_vins.is_empty() && !valid_vouts.is_empty() {
                        file_results.push((valid_vins, valid_vouts));
                    }
                }
            }
            file_results
        }).collect();

        let total_tx = extracted_data.len();
        println!("Fase Map selesai. {} transaksi siap dirakit...", total_tx);

        // ====================================================================
        // FASE REDUCE (SEKUENSIAL)
        // ====================================================================
        for (valid_vins, valid_vouts) in extracted_data {
            // A. Logika Sybil Clustering
            if valid_vins.len() > 1 {
                let first_id = self.uf.get_or_create_id(valid_vins[0].clone());
                for addr in valid_vins.iter().skip(1) {
                    let current_id = self.uf.get_or_create_id(addr.clone());
                    self.uf.union(first_id, current_id);
                }
            }

            // B. Logika Graf DFS
            for sender in &valid_vins {
                let neighbors = self.adj_list.entry(sender.clone()).or_insert_with(Vec::new);
                for receiver in &valid_vouts {
                    neighbors.push(receiver.address.clone());
                }
            }

            // C. Logika Perhitungan Hukum Benford (Hanya berdasarkan nilai penerimaan)
            for receiver in &valid_vouts {
                if let Some(digit) = get_first_digit(receiver.value_satoshi) {
                    let idx = digit - 1; // digit 1 masuk index 0
                    
                    // Update skor global
                    self.global_benford[idx] += 1;
                    
                    // Update skor individual alamat
                    let entry = self.address_benford.entry(receiver.address.clone()).or_insert([0; 9]);
                    entry[idx] += 1;
                }
            }
        }

        println!("Fase Reduce selesai. Graf & Benford State siap di RAM!");
        Ok(total_tx)
    }

    // FUNGSI 2: MENDAPATKAN KLASTER SYBIL
    fn get_sybil_clusters(&mut self, py: Python, min_size: usize) -> PyResult<PyObject> {
        let mut cluster_counts: HashMap<usize, usize> = HashMap::new();
        let total_nodes = self.uf.parent.len();
        
        for i in 0..total_nodes {
            let root = self.uf.find(i);
            *cluster_counts.entry(root).or_insert(0) += 1;
        }

        let py_dict = PyDict::new(py);
        for (root_id, count) in cluster_counts.iter() {
            if *count >= min_size {
                let root_address = &self.uf.id_to_address[*root_id];
                py_dict.set_item(root_address, count).unwrap();
            }
        }
        Ok(py_dict.into())
    }

    // FUNGSI 3: PENELUSURAN DFS
    fn run_dfs(&self, start_node: String, max_depth: usize, high_degree_limit: usize) -> PyResult<Vec<Vec<String>>> {
        let mut suspicious_paths = Vec::new();
        let mut current_path = Vec::new();
        let mut visited = HashSet::new();

        self.execute_dfs_recursive(
            &start_node, &mut current_path, &mut visited, &mut suspicious_paths,
            max_depth, high_degree_limit,
        );

        Ok(suspicious_paths)
    }

    // FUNGSI 4 BARU: BENFORD STATS EXTRACTION
    // Mengekstrak statistik dari klaster target
    #[pyo3(signature = (top_wallets))]
    fn get_benford_stats(&mut self, py: Python, top_wallets: Vec<String>) -> PyResult<Py<PyDict>> {
        let result_dict = PyDict::new(py);

        // 1. Masukkan data global
        result_dict.set_item("overall", self.global_benford.to_vec())?;

        // 2. Kalkulasi statistik untuk setiap klaster dari daftar target
        let targets_dict = PyDict::new(py);

        for wallet in top_wallets {
            let mut cluster_benford = [0u64; 9];

            // Cek apakah target terdaftar sebagai Root ID di mesin Union-Find
            if let Some(&root_id) = self.uf.address_to_id.get(&wallet) {
                // Iterasi secara native di Rust untuk mencari seluruh anggota klaster tersebut
                for i in 0..self.uf.parent.len() {
                    if self.uf.find(i) == root_id {
                        let member_addr = &self.uf.id_to_address[i];
                        if let Some(b_array) = self.address_benford.get(member_addr) {
                            for d in 0..9 {
                                cluster_benford[d] += b_array[d];
                            }
                        }
                    }
                }
            } else {
                // Jika target sekadar single-address (bukan klaster)
                if let Some(b_array) = self.address_benford.get(&wallet) {
                     cluster_benford = *b_array;
                }
            }

            targets_dict.set_item(&wallet, cluster_benford.to_vec())?;
        }

        result_dict.set_item("targets", targets_dict)?;
        Ok(result_dict.into())
    }
}

// Logika Rekursif Internal untuk DFS
impl ForensicEngine {
    fn execute_dfs_recursive(
        &self,
        current_node: &String,
        current_path: &mut Vec<String>,
        visited: &mut HashSet<String>,
        suspicious_paths: &mut Vec<Vec<String>>,
        max_depth: usize,
        high_degree_limit: usize,
    ) {
        if current_path.len() >= max_depth {
            suspicious_paths.push(current_path.clone());
            return;
        }

        if let Some(neighbors) = self.adj_list.get(current_node) {
            if neighbors.len() > high_degree_limit {
                let mut exchange_path = current_path.clone();
                exchange_path.push("EXCHANGE_STOP".to_string());
                suspicious_paths.push(exchange_path);
                return;
            }

            for neighbor in neighbors {
                if visited.contains(neighbor) {
                    let mut cycle_path = current_path.clone();
                    cycle_path.push(neighbor.clone());
                    cycle_path.push("CYCLE_DETECTED".to_string());
                    suspicious_paths.push(cycle_path);
                    continue;
                }

                visited.insert(neighbor.clone());
                current_path.push(neighbor.clone());

                self.execute_dfs_recursive(neighbor, current_path, visited, suspicious_paths, max_depth, high_degree_limit);

                current_path.pop();
                visited.remove(neighbor);
            }
        } else {
            if !current_path.is_empty() {
                suspicious_paths.push(current_path.clone());
            }
        }
    }
}

#[pymodule]
fn skripsi_forensik(_py: Python, m: &PyModule) -> PyResult<()> {
    m.add_class::<ForensicEngine>()?;
    Ok(())
}