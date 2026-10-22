use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Instant;

use addok_index::{AddokSearcher, SearchParams};
use anyhow::Result;

/// Get current process Resident Set Size (RSS) in megabytes
fn get_process_rss_mb() -> f64 {
    if let Ok(content) = fs::read_to_string("/proc/self/status") {
        for line in content.lines() {
            if line.starts_with("VmRSS:") {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 2 {
                    if let Ok(kb) = parts[1].parse::<f64>() {
                        return kb / 1024.0;
                    }
                }
            }
        }
    }
    0.0
}

/// Compute total size of all files in an index directory
fn get_index_disk_size(path: &Path) -> (f64, Vec<(String, f64)>) {
    let mut total_bytes = 0u64;
    let mut file_sizes = Vec::new();

    if let Ok(entries) = fs::read_dir(path) {
        for entry in entries.flatten() {
            if let Ok(meta) = entry.metadata() {
                if meta.is_file() {
                    let bytes = meta.len();
                    total_bytes += bytes;
                    let mb = bytes as f64 / (1024.0 * 1024.0);
                    file_sizes.push((entry.file_name().to_string_lossy().to_string(), mb));
                }
            }
        }
    }
    file_sizes.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
    let total_mb = total_bytes as f64 / (1024.0 * 1024.0);
    (total_mb, file_sizes)
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
struct BenchmarkStats {
    dataset_name: String,
    index_path: PathBuf,
    disk_size_mb: f64,
    cold_startup_ms: f64,
    ram_rss_mb: f64,
    total_queries: usize,
    duration_secs: f64,
    p50_ms: f64,
    p90_ms: f64,
    p95_ms: f64,
    p99_ms: f64,
    mean_ms: f64,
    min_ms: f64,
    max_ms: f64,
    single_thread_qps: f64,
    multi_thread_qps: f64,
    reverse_p50_ms: f64,
    reverse_p99_ms: f64,
    reverse_qps: f64,
}

fn benchmark_dataset(
    dataset_name: &str,
    index_dir: &Path,
    text_queries: &[&str],
    reverse_coords: &[(f64, f64)],
) -> Result<BenchmarkStats> {
    println!("\n=======================================================");
    println!(" 🚀 BENCHMARKING: {}", dataset_name);
    println!(" Index directory: {}", index_dir.display());
    println!("=======================================================");

    let (disk_size_mb, file_sizes) = get_index_disk_size(index_dir);
    println!("📁 Index size on disk: {:.2} MB", disk_size_mb);
    for (name, mb) in file_sizes.iter().take(8) {
        println!("   - {:<24} {:>8.2} MB", name, mb);
    }

    // 1. Cold Startup Benchmark
    let start_open = Instant::now();
    let searcher = AddokSearcher::open(index_dir)?;
    let cold_startup_ms = start_open.elapsed().as_secs_f64() * 1000.0;
    let ram_rss_mb = get_process_rss_mb();

    println!("⏱️  Cold startup time:     {:.2} ms", cold_startup_ms);
    println!("🧠 Process RAM (RSS):      {:.2} MB", ram_rss_mb);

    let searcher = Arc::new(searcher);

    // Warm-up: 200 queries
    for q in text_queries.iter().cycle().take(200) {
        let _ = searcher.search(&SearchParams::new(*q).with_limit(5));
    }

    // 2. Single-threaded latency measurement
    println!("\n📊 Measuring single-thread latency profile (5,000 queries)...");
    let mut latencies: Vec<f64> = Vec::with_capacity(5000);
    let run_start = Instant::now();

    for q in text_queries.iter().cycle().take(5000) {
        let t0 = Instant::now();
        let _ = searcher.search(&SearchParams::new(*q).with_limit(5))?;
        latencies.push(t0.elapsed().as_secs_f64() * 1000.0);
    }
    let duration_secs = run_start.elapsed().as_secs_f64();
    let single_thread_qps = 5000.0 / duration_secs;

    latencies.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let n = latencies.len();
    let p50_ms = latencies[n * 50 / 100];
    let p90_ms = latencies[n * 90 / 100];
    let p95_ms = latencies[n * 95 / 100];
    let p99_ms = latencies[n * 99 / 100];
    let min_ms = latencies[0];
    let max_ms = latencies[n - 1];
    let mean_ms = latencies.iter().sum::<f64>() / n as f64;

    println!("   p50 (median):  {:.3} ms", p50_ms);
    println!("   p90:           {:.3} ms", p90_ms);
    println!("   p95:           {:.3} ms", p95_ms);
    println!("   p99:           {:.3} ms", p99_ms);
    println!("   Mean:          {:.3} ms", mean_ms);
    println!("   Min:           {:.3} ms", min_ms);
    println!("   Max:           {:.3} ms", max_ms);
    println!("   Throughput 1T: {:.0} req/s", single_thread_qps);

    // 3. Multi-threaded throughput saturation
    let num_threads = std::thread::available_parallelism()
        .map(|p| p.get())
        .unwrap_or(8);
    println!(
        "\n🔥 Measuring multi-threaded throughput ({} worker threads, 20,000 queries)...",
        num_threads
    );

    let counter = Arc::new(AtomicUsize::new(0));
    let target_queries = 20_000;
    let mut handles = Vec::new();

    let mt_start = Instant::now();
    for _ in 0..num_threads {
        let s = Arc::clone(&searcher);
        let c = Arc::clone(&counter);
        let queries_vec: Vec<String> = text_queries.iter().map(|s| s.to_string()).collect();

        handles.push(std::thread::spawn(move || {
            let mut i = 0;
            while c.fetch_add(1, Ordering::Relaxed) < target_queries {
                let q = &queries_vec[i % queries_vec.len()];
                let _ = s.search(&SearchParams::new(q).with_limit(5));
                i += 1;
            }
        }));
    }

    for h in handles {
        h.join().unwrap();
    }
    let mt_duration = mt_start.elapsed().as_secs_f64();
    let multi_thread_qps = target_queries as f64 / mt_duration;
    println!(
        "   Multi-thread Throughput: {:.0} req/s (across {} cores)",
        multi_thread_qps, num_threads
    );

    // 4. Reverse Geocoding Benchmark
    println!("\n📍 Measuring Reverse Geocoding performance (R-Tree, 5,000 lookups)...");
    let mut rev_latencies = Vec::with_capacity(5000);
    let rev_start = Instant::now();

    for &(lat, lon) in reverse_coords.iter().cycle().take(5000) {
        let t0 = Instant::now();
        let _ = searcher.reverse(lat, lon, 1);
        rev_latencies.push(t0.elapsed().as_secs_f64() * 1000.0);
    }
    let rev_duration = rev_start.elapsed().as_secs_f64();
    let reverse_qps = 5000.0 / rev_duration;

    rev_latencies.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let rn = rev_latencies.len();
    let reverse_p50_ms = rev_latencies[rn * 50 / 100];
    let reverse_p99_ms = rev_latencies[rn * 99 / 100];

    println!("   Reverse p50:   {:.3} ms", reverse_p50_ms);
    println!("   Reverse p99:   {:.3} ms", reverse_p99_ms);
    println!("   Reverse QPS:   {:.0} req/s", reverse_qps);

    Ok(BenchmarkStats {
        dataset_name: dataset_name.to_string(),
        index_path: index_dir.to_path_buf(),
        disk_size_mb,
        cold_startup_ms,
        ram_rss_mb,
        total_queries: 5000,
        duration_secs,
        p50_ms,
        p90_ms,
        p95_ms,
        p99_ms,
        mean_ms,
        min_ms,
        max_ms,
        single_thread_qps,
        multi_thread_qps,
        reverse_p50_ms,
        reverse_p99_ms,
        reverse_qps,
    })
}

fn main() -> Result<()> {
    println!("=======================================================");
    println!("       ⚡ addok-rs COMPREHENSIVE BENCHMARK SUITE       ");
    println!("=======================================================");

    // Paris queries covering all real-world scenarios: exact, abbreviation, prefix, fuzzy, house numbers
    let paris_queries = vec![
        "8 bd st michel",
        "10 avenue des champs elysees",
        "12 rue de la paix",
        "5 place de la concorde",
        "18 cours victor hugo",
        "4 faubourg saint antoine",
        "1 allee des lilas",
        "25 quai branly",
        "3 impasse du moulin",
        "14 rue de rivoli",
        "boulevard saint germain",
        "place de la republique",
        "rue de l eglise",
        "8 bis bd st michel",
        "10 av champs elysees",
        // Autocomplete prefixes
        "boulev",
        "avenue des cha",
        "rue de la p",
        "place de la con",
        // Typos / Fuzzy
        "8 boulevrad saint michel",
        "10 avneu champ elysee",
        "12 r paix",
        "5 palce concorde",
    ];

    let paris_coords = vec![
        (48.8512, 2.3431),
        (48.8698, 2.3082),
        (48.8688, 2.3308),
        (48.8656, 2.3212),
        (48.8530, 2.3499),
        (48.8584, 2.2945),
        (48.8606, 2.3376),
        (48.8566, 2.3522),
    ];

    // France queries covering all regions: Lyon, Marseille, Bordeaux, Lille, Nantes, Toulouse, Strasbourg, rural towns
    let france_queries = vec![
        // Paris
        "8 bd st michel paris",
        "10 av champs elysees paris",
        // Lyon
        "15 rue de la republique lyon",
        "2 place bellecour lyon",
        // Marseille
        "10 la canebiere marseille",
        "5 rue de rome marseille",
        // Bordeaux
        "12 cours de l intendance bordeaux",
        "4 place de la bourse bordeaux",
        // Lille
        "20 rue faidherbe lille",
        "1 grand place lille",
        // Nantes
        "8 rue crebillon nantes",
        "14 cours des 50 otages nantes",
        // Toulouse
        "3 place du capitole toulouse",
        "10 rue alsace lorraine toulouse",
        // Strasbourg
        "5 place kleber strasbourg",
        "12 rue des grandes arcades strasbourg",
        // Nice
        "1 promenade des anglais nice",
        "8 avenue jean medecin nice",
        // Rural & towns (from department 01, 02, etc.)
        "la chèvre aberbergement clemenciat",
        "route de la fontaine 01400",
        "impasse des epis 01400",
        // Autocomplete prefixes
        "rue de la rep",
        "cours de l in",
        "place du cap",
        // Typos / Fuzzy
        "10 canbiere marseille",
        "15 r republiq lyon",
        "12 cours intendanc bordaux",
    ];

    let france_coords = vec![
        (48.8512, 2.3431),  // Paris
        (45.7640, 4.8357),  // Lyon
        (43.2965, 5.3698),  // Marseille
        (44.8378, -0.5792), // Bordeaux
        (50.6292, 3.0573),  // Lille
        (47.2184, -1.5536), // Nantes
        (43.6047, 1.4442),  // Toulouse
        (48.5734, 7.7521),  // Strasbourg
        (43.7102, 7.2620),  // Nice
        (46.1325, 4.9143),  // Ain
    ];

    // Run Paris Benchmark
    let paris_index = Path::new("index_paris");
    let paris_stats = benchmark_dataset(
        "PARIS (Department 75)",
        paris_index,
        &paris_queries,
        &paris_coords,
    )?;

    // Run France Benchmark
    let france_index = Path::new("/tmp/test_index_france_2m");
    let france_stats = if france_index.exists() {
        Some(benchmark_dataset(
            "FRANCE (National BAN 2,000,000 addresses)",
            france_index,
            &france_queries,
            &france_coords,
        )?)
    } else {
        None
    };

    // Print summary comparative table
    println!("\n\n=======================================================");
    println!(" 📊 FINAL BENCHMARK SUMMARY (REAL MEASUREMENTS)        ");
    println!("=======================================================");
    println!("\n| Metric | Paris (75) | France (National 2M) |");
    println!("|---|---|---|");
    println!(
        "| **Disk Index Size** | {:.1} MB | {:.1} MB |",
        paris_stats.disk_size_mb,
        france_stats.as_ref().map(|s| s.disk_size_mb).unwrap_or(0.0)
    );
    println!(
        "| **Cold Startup Time** | **{:.1} ms** | **{:.1} ms** |",
        paris_stats.cold_startup_ms,
        france_stats
            .as_ref()
            .map(|s| s.cold_startup_ms)
            .unwrap_or(0.0)
    );
    println!(
        "| **Process RAM (RSS)** | **{:.1} MB** | **{:.1} MB** |",
        paris_stats.ram_rss_mb,
        france_stats.as_ref().map(|s| s.ram_rss_mb).unwrap_or(0.0)
    );
    println!(
        "| **Text Search p50** | **{:.3} ms** | **{:.3} ms** |",
        paris_stats.p50_ms,
        france_stats.as_ref().map(|s| s.p50_ms).unwrap_or(0.0)
    );
    println!(
        "| **Text Search p90** | **{:.3} ms** | **{:.3} ms** |",
        paris_stats.p90_ms,
        france_stats.as_ref().map(|s| s.p90_ms).unwrap_or(0.0)
    );
    println!(
        "| **Text Search p99** | **{:.3} ms** | **{:.3} ms** |",
        paris_stats.p99_ms,
        france_stats.as_ref().map(|s| s.p99_ms).unwrap_or(0.0)
    );
    println!(
        "| **Single-thread QPS** | **{:.0} req/s** | **{:.0} req/s** |",
        paris_stats.single_thread_qps,
        france_stats
            .as_ref()
            .map(|s| s.single_thread_qps)
            .unwrap_or(0.0)
    );
    println!(
        "| **Multi-thread QPS** | **{:.0} req/s** | **{:.0} req/s** |",
        paris_stats.multi_thread_qps,
        france_stats
            .as_ref()
            .map(|s| s.multi_thread_qps)
            .unwrap_or(0.0)
    );
    println!(
        "| **Reverse Geo p50** | **{:.3} ms** | **{:.3} ms** |",
        paris_stats.reverse_p50_ms,
        france_stats
            .as_ref()
            .map(|s| s.reverse_p50_ms)
            .unwrap_or(0.0)
    );
    println!(
        "| **Reverse Geo p99** | **{:.3} ms** | **{:.3} ms** |",
        paris_stats.reverse_p99_ms,
        france_stats
            .as_ref()
            .map(|s| s.reverse_p99_ms)
            .unwrap_or(0.0)
    );
    println!(
        "| **Reverse Geo QPS** | **{:.0} req/s** | **{:.0} req/s** |",
        paris_stats.reverse_qps,
        france_stats.as_ref().map(|s| s.reverse_qps).unwrap_or(0.0)
    );

    Ok(())
}
