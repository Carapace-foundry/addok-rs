# Storage & Architecture

One of the most significant evolutions in `addok-rs` compared to Python Addok is the **complete elimination of Redis and external databases**, replaced by zero-copy, memory-mapped disk structures.

---

## 🏛️ Python Addok vs. `addok-rs` Storage Architecture

```
Python Addok:
[ Client ] ---> [ Gunicorn / Falcon ] ---> [ Redis (RAM) ] (Tokens + Documents)
                                     \--> [ SQLite / Postgres ] (Optional Doc Plugin)

addok-rs:
[ Client ] ---> [ Axum / Tokio ] ---> [ Memory-Mapped Index (memmap2) ]
                                      ├── Tantivy Inverted Index
                                      ├── spatial.bin (R-Tree)
                                      ├── docs.bin (Document Store)
                                      ├── housenumbers.bin (HN Table)
                                      ├── pairs.bin (Co-occurrences)
                                      └── ngram.bin (Edge N-Grams)
```

---

## 📁 Storage Components on Disk

When `addok index` completes, the output index directory contains compact, self-contained binary files:

| File | Purpose | Storage Technology |
|---|---|---|
| **Tantivy Segments** (`*.idx`, `*.store`, `*.term`) | Full-text inverted index for normalized tokens, document IDs, and term frequencies. | [Tantivy](https://github.com/quickwit-oss/tantivy) (Rust Lucene alternative) |
| **`spatial.bin`** | 2D R-Tree index for spatial nearest-neighbor search (`/reverse/`). | [`rstar`](https://crates.io/crates/rstar) serialized tree |
| **`docs.bin`** | Raw address document payloads, labels, and metadata. | Byte-offset packed binary store |
| **`housenumbers.bin`** | Map of street document IDs to housenumber numbers, coordinates, and positions. | Packed binary table |
| **`pairs.bin`** | Token co-occurrence pair index used for aggressive candidate pruning. | Serialized token adjacency table |
| **`ngram.bin`** | Edge N-Gram prefix table used for instant autocompletion. | Prefix lookup index |
| **`filters.bin`** | Inverted bitsets for fast postcode, type, and citycode filtering. | Compressed bitmap index |

---

## 🚀 Why Memory-Mapped Storage (`mmap`)?

1. **Zero External Daemons**: No need to configure, monitor, or manage a Redis daemon or deal with Redis RDB snapshots and AOF persistence.
2. **Instant Warmup**: Memory-mapped files (`memmap2`) leverage the operating system's unified page cache. Pages are faulted into memory on demand without long loading pauses.
3. **Lock-Free Concurrency**: Multiple read threads across any number of CPU cores access the index concurrently without global locks or mutex contention.
4. **Huge Throughput**: Eliminates TCP roundtrips, Unix socket overhead, and serialization/deserialization latency between Python and Redis.
