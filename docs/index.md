# Welcome to Addok (Rust) ⚡

*Search engine for address. Only address. Pure Rust high-performance engine.*

[addok-rs](https://github.com/carapace-foundry/addok-rs) is a high-performance, single-binary reimplementation of the [Addok](https://github.com/addok/addok) address geocoder in pure Rust.

It serves as a **100% drop-in replacement** for Python Addok that eliminates all external dependencies (no Python runtime, no Redis server, no virtualenv) while delivering **10x to 30x lower search latencies** and **> 5,000x faster reverse geocoding**.

---

## 🚀 Overview & Key Features

- **Standalone Native Binary**: Everything compiles into a single executable (`addok`). No external services or daemons to manage.
- **Zero Redis Dependency**: Replaces Redis with memory-mapped disk storage (Tantivy full-text index + binary acceleration tables + R-Tree).
- **All-in-One Monorepo**: Core geocoder, French address normalizers (`addok_france`), and CSV batch processing (`addok_csv`) are compiled directly into the binary.
- **100% API Compatible**: Exposes the exact same REST API endpoints (`/search/`, `/reverse/`, `/search/csv/`, `/reverse/csv/`) returning standard [GeoCodeJSON](https://github.com/geocoders/geocodejson-spec).
- **Built-in Interactive Shell**: Full diagnostic REPL (`addok shell`) for inspecting tokenization, candidate buckets, fuzzy variants, and scores.
- **Streaming BAN Ingestion**: Direct indexing of official French BAN `.csv.gz` files and standard Addok `.ndjson` streams.
- **Concurrent & Thread-Safe**: Full multi-core parallelism powered by Tokio and Rayon without GIL bottlenecks.

---

## ⚡ Performance at a Glance

| Metric | Python Addok + Redis | `addok-rs` (Rust) | Improvement |
|---|---|---|---|
| **Runtime Architecture** | Python 3 + Redis (2 daemons) | Single native binary | **Zero external dependencies** |
| **Median Search Latency (p50)** | **3.71 ms** | **0.120 ms** (120 µs) | 🏎️ **31x faster** |
| **95th Percentile (p95)** | **24.99 ms** | **2.427 ms** | 🏎️ **10.3x faster** |
| **Search Throughput (12 cores)** | **879 req/s** | **11,981 req/s** | 🔥 **13.6x higher** |
| **Reverse Geocoding (p50)** | **5.08 ms** | **0.001 ms** (1 µs) | 📍 **5,000x faster (R-Tree)** |
| **Reverse Geocoding QPS** | **206 req/s** | **1,402,396 req/s** | 📍 **> 1.4 M QPS** |

---

## 📚 Navigation

- [Installation](install.md) — How to build and install `addok-rs`.
- [Tutorial](tutorial.md) — Step-by-step guide to index sample data and run your first geocoder.
- [Configuration](config.md) — Tuning fields, filters, plugins, and scoring.
- [Storage & Architecture](storage.md) — How memory-mapped tables and Tantivy replace Redis.
- [Data Import](import.md) — Importing BAN CSV files or Addok NDJSON batch streams.
- [HTTP API Reference](api.md) — Full REST API specifications and GeoCodeJSON format.
- [Interactive Shell](shell.md) — Using the diagnostic REPL.
- [Concepts](concepts.md) — Geocoding philosophy, tokenization, buckets, and scoring.
- [Advanced](advanced.md) — Production deployment, systemd, and performance tuning.
- [Porting from Python Addok](porting.md) — Seamless migration guide.
- [FAQ](faq.md) — Frequently asked questions.
