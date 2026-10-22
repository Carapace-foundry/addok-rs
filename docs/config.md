# Configuration

`addok-rs` is designed to work out-of-the-box with sane defaults matching the official French BAN and Python Addok.

When customization is needed, `addok-rs` loads configuration in the following order of precedence:
1. **Environment Variables** (e.g. `ADDOK_PLUGINS`, `ADDOK_BUCKET_MIN`)
2. **Configuration file** (`addok.toml` in the current directory or pointed by `ADDOK_CONFIG_PATH`)
3. **Built-in Defaults** (matching Python Addok `default.py`)

---

## Example `addok.toml`

```toml
# ==============================================================================
# Addok Configuration (addok.toml)
# ==============================================================================

# Search collection bucket bounds
bucket_min = 10
bucket_max = 100

# High-frequency token thresholds
common_threshold = 10000
intersect_limit = 100000

# Relevance scoring parameters
match_threshold = 0.9
min_score = 0.1
query_max_length = 200

# Autocomplete N-Gram sizing
min_edge_ngrams = 3
max_edge_ngrams = 20

# Filter multi-value delimiter (e.g., /search/?type=street+city)
filters_multi_value_separator = " "

# Active filters enabled for /search/ and /reverse/
filters = ["type", "postcode", "citycode"]

# Metadata returned in GeoCodeJSON responses
attribution = "BAN (Base Adresse Nationale)"
licence = "ETALAB-2.0"

# Active built-in plugins
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

# Indexed document fields with relevance boosts
[[fields]]
key = "name"
boost = 4.0
null = false

[[fields]]
key = "street"
boost = 1.0
null = true

[[fields]]
key = "city"
boost = 1.0
null = true

[[fields]]
key = "postcode"
boost = 1.0
null = true

[[fields]]
key = "housenumbers"
boost = 1.0
null = true

[[fields]]
key = "context"
boost = 1.0
null = true
```

---

## Built-in Plugins Reference

In `addok-rs`, plugins are compiled directly into the binary and can be toggled via `plugins` list or `ADDOK_PLUGINS`:

| Plugin | Description | If Disabled |
|---|---|---|
| **`addok_france`** | French address rules: CEDEX/BP cleaning, thoroughfare expansion (`bd` $\rightarrow$ `boulevard`), ordinals (`1 er` $\rightarrow$ `1er`), date protection (`8 mai 1945`), repetition keywords (`bis`, `ter`). | Runs in international mode (no French-specific address rules). |
| **`addok_csv`** | Batch CSV streaming endpoints `/search/csv/` and `/reverse/csv/`. | CSV endpoints return 404. |
| **`addok.autocomplete`** | Edge N-Gram prefix autocompletion for last token. | Autocomplete parameter ignored. |
| **`addok.fuzzy`** | Damerau-Levenshtein typo correction for missing tokens. | No fuzzy matching if token misspelled. |
| **`addok.pairs`** | Fast candidate filtering via token co-occurrence index. | Scans raw candidate intersection. |

---

## Overriding via Environment Variables

Any parameter can be overridden using the `ADDOK_` prefix:

```bash
# Example: Disable French-specific normalizers for generic international usage
export ADDOK_PLUGINS="addok.shell,addok.http,addok.batch,addok.pairs,addok.fuzzy,addok.autocomplete"

# Example: Increase candidate collection bucket size
export ADDOK_BUCKET_MAX=200
```
