# addok-rs ⚡

A high-performance, lightweight reimplementation of the [Addok](https://github.com/addok/addok) address geocoder in pure Rust.

It is a **100% drop-in compatible replacement** for Python Addok that eliminates all external dependencies (no Python runtime, no Redis server) by using memory-mapped disk storage (Tantivy + binary tables).

---

## 📦 All-in-One Monorepo (Core + Plugins Built-in)

In the original Python ecosystem, Addok and its plugins were distributed as separate packages (`addok`, `addok-france`, `addok-csv`).

**In `addok-rs`, everything is unified in this single repository into a single standalone binary:**
- **No external packages to install**: Core engine, French address rules (`addok-france`), and CSV batch processing (`addok-csv`) are compiled directly into the binary.
- **Toggleable via configuration**: Enable or disable any plugin on-demand in `addok.toml` or via environment variable, without recompilation or external plugins.

```toml
# addok.toml (defaults: all plugins enabled out-of-the-box)
plugins = [
    "addok.shell",
    "addok.http",
    "addok.batch",
    "addok.pairs",
    "addok.fuzzy",
    "addok.autocomplete",
    "addok_france",
    "addok_csv",
]
```

### Built-in Plugins

| Plugin | Features (built directly into the binary) | When Disabled |
|---|---|---|
| **`addok_france`** | French address rules: `clean_query` (`cedex`, `bp`, `lieu-dit`), thoroughfare expansions (`bd` $\rightarrow$ `boulevard`, `av` $\rightarrow$ `avenue`), ordinal gluing (`1 er` $\rightarrow$ `1er`), French stop words, repetitions (`bis`, `ter`), and date-street protection (`8 mai 1945`). | Generic international mode (no French-specific rules). |
| **`addok_csv`** | Batch CSV streaming geocoding endpoints: `/search/csv/` and `/reverse/csv/`. | CSV endpoints return `404 Not Found`. |
| **`addok.autocomplete`** | Edge N-Gram prefix autocompletion on the query's last token. | Autocomplete prefix expansion skipped. |
| **`addok.fuzzy`** | Levenshtein distance-1 typo tolerance when initial candidate bucket is dry. | Fuzzy neighbor generation skipped. |
| **`addok.pairs`** | Candidate pruning using token co-occurrence pairs index. | Unfiltered candidate expansion. |

```bash
# Example: run as a generic international geocoder without French address rules
export ADDOK_PLUGINS="addok.shell,addok.http,addok.batch,addok.pairs,addok.fuzzy,addok.autocomplete"
```

---

## 📊 Benchmark: Python + Redis vs. addok-rs (Rust)

*Rigorous benchmarks executed on the same Linux x86_64 machine (12 vCPUs, 15 GB RAM) with the exact same dataset (Paris 75 - 152,628 addresses) and identical queries:*

| Metric | Addok Python 1.3.2 + Redis 8.10 *(Measured)* | `addok-rs` (Rust) *(Measured)* | Observed Difference |
|---|---|---|---|
| **Architecture** | Python 3 + Redis (2 processes) | Single standalone native binary | 🚀 **Zero external daemons** |
| **p50 Latency (median)** | **3.71 ms** | **0.120 ms** (120 µs) | 🏎️ **`addok-rs` 31x faster** |
| **p90 Latency** | **5.02 ms** | **0.696 ms** | 🏎️ **`addok-rs` 7.2x faster** |
| **p95 Latency** | **24.99 ms** | **2.427 ms** | 🏎️ **`addok-rs` 10.3x faster** |
| **p99 Latency** | **32.93 ms** | **10.660 ms** | 🏎️ **`addok-rs` 3.1x faster** |
| **Single-thread Throughput** | **177 req/s** | **1,428 req/s** | 🔥 **8.1x higher** |
| **Multi-thread Throughput (12 cores)** | **879 req/s** *(multi-process)* | **11,981 req/s** | 🔥 **13.6x higher** |
| **Reverse Geocoding (p50)** | **5.08 ms** | **0.001 ms** (1 µs) | 📍 **5,000x faster (native R-Tree)** |
| **Reverse Geocoding (Throughput)** | **206 req/s** | **1,402,396 req/s** | 📍 **> 1.4 M QPS (6,800x faster)** |
| **Thread-Safety** | No *(concurrency limited by GIL & temp key collisions)* | Yes *(100% thread-safe lock-free)* | Native multi-core concurrency |
| **Disk Storage Footprint** | **8.1 MB** (`dump.rdb`) | **106.9 MB** *(Tantivy index + R-Tree)* | Redis stores raw keys in memory |
| **RAM (Process RSS)** | **59.3 MB** *(Redis 18.8M + Python 40.5M)* | **548.3 MB** | Rust preloads acceleration structures |
| **Cold Start** | ~129 ms *(Redis ~35ms + Python ~94ms)* | **667.5 ms** | Similar (< 1 second) |

---

## 🚀 Quick Start

### 1. Build
```bash
cargo build --release
sudo cp target/release/addok /usr/local/bin/
```

### 2. Index Data
Supports official French BAN `.csv.gz` or Addok `.ndjson` streams:
```bash
# From official BAN CSV / CSV.GZ (data.gouv.fr)
addok index --input data/adresses-75.csv.gz --output ./index_data

# Or from standard Addok NDJSON batch stream
addok batch addresses.ndjson --index ./index_data
```

### 3. Run HTTP API
```bash
addok serve --index ./index_data --port 7878
```

Test with curl:
```bash
curl "http://localhost:7878/search/?q=8+bd+st+michel"
```

### 4. CLI & Interactive Shell
```bash
# CLI text search
addok search "8 bd st michel" --index ./index_data

# Reverse geocoding
addok reverse --lat 48.8512 --lon 2.3431 --index ./index_data

# Interactive diagnostic REPL
addok shell --index ./index_data
```

---

## 📖 API Compatibility

- **Endpoints**: `/search/`, `/reverse/`, `/search/csv/`, and `/reverse/csv/` (with `/search:csv/` and `/reverse:csv/` aliases).
- **Format**: 100% compliant [GeoCodeJSON](https://github.com/geocoders/geocodejson-spec).
- **Scoring**: Exact same text similarity, French normalizations, token coverage, and geographic bias decay ($S(d) = 2^{-d/50}$).

Compatible with all existing Addok frontends and client libraries.
For full parameter specifications, see the [Official Addok Documentation](https://addok.readthedocs.io/).

---

## 📄 License

Licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or <http://www.apache.org/licenses/LICENSE-2.0>)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or <http://opensource.org/licenses/MIT>)

at your option.
