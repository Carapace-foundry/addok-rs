# Frequently Asked Questions (FAQ)

### Do I need Redis or any external database to run `addok-rs`?
**No.** `addok-rs` completely eliminates the Redis dependency. All full-text indices, document tables, and spatial R-Trees are stored in compact, memory-mapped files on disk. There are zero background daemons to configure or manage.

---

### Do I need a Python runtime or virtual environment?
**No.** `addok-rs` is written in pure Rust and compiles to a single standalone native binary (`addok`).

---

### Which datasets can I index?
1. **Official French BAN (Base Adresse Nationale)**: Ingests `.csv` or `.csv.gz` files from [adresse.data.gouv.fr](https://adresse.data.gouv.fr/donnees-nationales) directly via `addok index`.
2. **Addok NDJSON**: Ingests standard line-delimited JSON files or stdin streams via `addok batch`.
3. **Custom / International Datasets**: Any dataset converted to Addok NDJSON format can be indexed.

---

### Can I use `addok-rs` for non-French addresses?
**Yes.** While `addok_france` is enabled by default to handle French-specific address rules (CEDEX, thoroughfare expansions like `bd` $\rightarrow$ `boulevard`), you can run `addok-rs` as a generic international geocoder simply by disabling `addok_france`:

```bash
export ADDOK_PLUGINS="addok.shell,addok.http,addok.batch,addok.pairs,addok.fuzzy,addok.autocomplete"
```

---

### Is `addok-rs` compatible with existing Addok web frontends?
**Yes.** `addok-rs` serves the exact same REST API endpoints (`/search/`, `/reverse/`, `/search/csv/`, `/reverse/csv/`) returning standard GeoCodeJSON. Any web app or frontend (such as [addok-web](https://github.com/addok/addok-web) or Leaflet plugins) works immediately without configuration changes.

---

### How fast is reverse geocoding in `addok-rs`?
Reverse geocoding is powered by a memory-mapped 2D R-Tree index (`rstar`). Looking up the closest address from GPS coordinates takes approximately **1 microsecond (0.001 ms)**, achieving over **1.4 million queries per second (QPS)** on modern multi-core hardware.

---

### How are housenumbers handled?
Housenumbers are grouped hierarchically under their parent street document. When searching for an address with a street number (e.g. `"8 Boulevard Saint-Michel"`), the street is matched first, and the matching housenumber coordinates and point geometry are extracted and returned as a `"type": "housenumber"` GeoCodeJSON feature.

---

### How does memory usage compare?
`addok-rs` uses zero-copy memory mapping (`memmap2`). Rather than holding all uncompressed strings in Redis RAM, the Linux kernel page cache loads index pages on demand and shares them across worker threads. This ensures instant cold startup (< 1 second) and prevents out-of-memory crashes.
