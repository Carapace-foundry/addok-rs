# Data Import

`addok-rs` supports two primary methods for importing geographic address data:

1. **Direct BAN CSV / CSV.GZ Ingestion** (`addok index`): High-speed streaming parser with automatic housenumber aggregation.
2. **Standard Addok NDJSON Batch Stream** (`addok batch`): 100% compatible with existing Python Addok export pipelines.

---

## 1. Direct BAN CSV / CSV.GZ Ingestion

The fastest way to index French addresses is using the official BAN (Base Adresse Nationale) CSV dumps from [adresse.data.gouv.fr](https://adresse.data.gouv.fr/donnees-nationales):

```bash
# Ingest and index directly from compressed CSV
addok index --input data/adresses-75.csv.gz --output ./index_paris
```

### Options for `addok index`

- `--input`, `-i`: Path to the `.csv` or `.csv.gz` input file (required).
- `--output`, `-o`: Path to the destination index directory (required).
- `--batch-size`: Batch size for multi-threaded processing (default: `10,000`).
- `--memory-budget-mb`: Memory budget allocated for Tantivy segment indexing (default: `50 MB`).

During indexing, `addok-rs` automatically streams rows, aggregates all housenumbers belonging to each street, computes street centroid coordinates, builds the spatial R-Tree, and flushes Tantivy segments in parallel using Rayon.

---

## 2. Standard Addok NDJSON Batch Stream

If you have an existing data pipeline targeting Python Addok, `addok-rs` accepts the exact same **Line-Delimited JSON (NDJSON)** stream via file or stdin:

```bash
# From file
addok batch addresses.ndjson --index ./index_data

# Or from standard input pipe
cat addresses.ndjson | addok batch --index ./index_data
```

### Expected NDJSON Document Format

Each line must be a valid JSON object:

```json
{
  "id": "75105_0001",
  "type": "street",
  "name": "Boulevard Saint-Michel",
  "postcode": "75005",
  "city": "Paris",
  "lat": 48.8512,
  "lon": 2.3431,
  "importance": 0.65,
  "housenumbers": {
    "8": {
      "lat": 48.8512,
      "lon": 2.3431
    },
    "10": {
      "lat": 48.8514,
      "lon": 2.3433
    },
    "12 bis": {
      "lat": 48.8515,
      "lon": 2.3434
    }
  }
}
```

### Document Attributes

- **`id`** *(required)*: Unique identifier string for the street or municipality.
- **`type`** *(required)*: Document type (`"street"`, `"housenumber"`, `"municipality"`, `"locality"`).
- **`name`** *(required)*: Street or municipality name.
- **`postcode`**: French 5-digit postal code.
- **`city`**: Name of the municipality.
- **`lat` / `lon`**: WGS84 coordinates.
- **`importance`**: Float between `0.0` and `1.0` representing static relevance weight.
- **`housenumbers`**: Dictionary mapping housenumber numbers (e.g. `"8"`, `"12 bis"`) to coordinate objects `{lat, lon}`.

---

## 3. Resetting an Index

To wipe an index directory before re-indexing:

```bash
addok reset --index ./index_data -y
```
