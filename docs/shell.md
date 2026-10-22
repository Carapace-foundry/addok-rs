# Interactive Diagnostic Shell

`addok-rs` includes an interactive REPL shell designed for testing queries, inspecting index internals, and understanding string normalization and candidate scoring.

---

## Launching the Shell

```bash
addok shell --index ./index_data
```

The shell provides syntax highlighting, command autocompletion, and persistent command history saved to `~/.addok_shell_history`.

```text
⚡ Addok-rs Interactive Geocoding Shell
Loaded index: "./index_paris"
Type any query to search, HELP for command list, EXIT to quit.

addok> 8 bd st michel
```

---

## Commands Reference

### `<query>` (Default Action)
Typing any string without a command prefix runs an immediate search:
```text
addok> 8 bd st michel
🔎 Search results for: "8 bd st michel" (limit: 5)
  1. [0.98] 8 Boulevard Saint-Michel, 75005 Paris (ID: 75105_0001_8) - coords: (48.85120, 2.34310)
  2. [0.85] 10 Boulevard Saint-Michel, 75005 Paris (ID: 75105_0001_10) - coords: (48.85135, 2.34325)
```

### `EXPLAIN <query>`
Breaks down the entire 3-step pipeline: query normalization, tokenization, stop word filtering, candidate bucket collection, and scoring:
```text
addok> EXPLAIN 8 bd st michel
Query: "8 bd st michel"
Normalized: "8 boulevard saint michel"
Tokens: ["8", "boulevard", "saint", "michel"]
Cleaned Housenumber: Some("8")
Stop words removed: []
Candidate Bucket Size: 12 candidates
Top scored hit: 75105_0001 (score: 0.98, text_sim: 1.0, coverage: 1.0)
```

### `TOKENIZE <query>`
Displays how the normalizer and French address rules process the text:
```text
addok> TOKENIZE 8 bis bd saint-michel 75005 paris
```

### `REVERSE <lat> <lon>`
Runs a spatial R-Tree nearest-neighbor lookup:
```text
addok> REVERSE 48.8512 2.3431
```

### `DOC <id>` or `GET <id>`
Fetches the raw document JSON stored in `docs.bin`:
```text
addok> DOC 75105_0001
```

### `FUZZY <token>`
Generates Damerau-Levenshtein distance-1 variants and checks if they exist in the index:
```text
addok> FUZZY michel
```

### `PAIR <token>`
Displays co-occurring tokens from `pairs.bin`:
```text
addok> PAIR michel
```

### `NGRAM <token>` or `AUTOCOMPLETE <token>`
Shows edge n-grams and prefix completions:
```text
addok> NGRAM champs
```

### `FREQUENCY <token>`
Displays the document occurrence frequency of a token in the Tantivy index:
```text
addok> FREQUENCY michel
```

### `GEODISTANCE <lat1> <lon1> <lat2> <lon2>`
Computes the exact Haversine distance in meters and resulting decay score ($2^{-d/50}$):
```text
addok> GEODISTANCE 48.8512 2.3431 48.8698 2.3082
Distance: 3241.2 meters (3.24 km)
Geographic decay score factor: 0.000000...
```

### `HELP` / `?`
Displays command help and examples.

### `EXIT` / `QUIT`
Exits the interactive shell session.
