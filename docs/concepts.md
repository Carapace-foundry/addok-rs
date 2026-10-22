# Concepts & Architecture

`addok-rs` is a specialized address geocoder. Its primary purpose is to convert unstructured, user-submitted text queries into structured geographical documents and coordinates.

---

## 🔄 The 3-Phase Geocoding Pipeline

Whenever an address query is submitted via `/search/` or the CLI, it flows through three sequential phases:

```
[ Raw Query: "8 bd st michel paris" ]
                │
                ▼
1. NORMALIZATION & TOKENIZATION
   (Deunicode, lowercase, expand "bd" -> "boulevard", extract housenumber "8")
                │
                ▼
2. COLLECTION PHASE (Candidate Buckets)
   (Intersect inverted index posting lists, prune via pairs.bin, target 10-100 candidates)
                │
                ▼
3. SCORING & RANKING
   (Text similarity + token coverage + housenumber match + geo decay)
                │
                ▼
[ Ranked GeoCodeJSON Results ]
```

---

## 1. Normalization & Tokenization

Address queries in the real world are filled with abbreviations, missing accents, ordinals, and administrative noise.

The normalizer applies:
- **Unicode Flattening**: Transliteration of accented characters (e.g., `é` $\rightarrow$ `e`, `î` $\rightarrow$ `i`) via `deunicode`.
- **Thoroughfare Expansions** (`addok_france`):
  - `bd` $\rightarrow$ `boulevard`
  - `av` $\rightarrow$ `avenue`
  - `r` $\rightarrow$ `rue`
  - `pl` $\rightarrow$ `place`
  - `all` $\rightarrow$ `allee`
- **Housenumber & Ordinal Gluing**:
  - `1 er` $\rightarrow$ `1er`
  - `8 bis` $\rightarrow$ `8bis`
- **Administrative Cleaning**:
  - Removal of `cedex`, `bp`, `cs`, `lieu-dit`.
- **Historical Date Protection**:
  - Protecting date-streets (e.g. `8 mai 1945`, `11 novembre`) from being misinterpreted as street numbers.

---

## 2. Collection Phase (The Candidate Bucket)

Rather than scoring all 25+ million addresses in France, Addok collects a small set of promising candidates (typically between `10` and `100` documents):

1. **Token Intersection**: Addok finds documents containing the query tokens in the inverted index.
2. **Co-occurrence Pruning (`addok.pairs`)**: Checks `pairs.bin` to eliminate impossible token combinations.
3. **Adaptive Bucketing**:
   - If the candidate count is $< 10$, Addok relaxes constraints, tries autocomplete prefix expansion on the trailing token (`addok.autocomplete`), or generates typo variants (`addok.fuzzy`).
   - If the candidate count is $> 100$, Addok tightens constraints using rarer tokens.

---

## 3. Scoring & Ranking Phase

Each candidate document in the bucket is evaluated:

$$\text{Final Score} = \text{Score}_{\text{text}} \times \text{Weight}_{\text{hn}} + \text{Importance} + \text{GeoBias}$$

- **Text Similarity**: Measures character-level similarity between the query tokens and the document name.
- **Token Coverage**: Fraction of query tokens present in the document.
- **Housenumber Match**: If the user specified a housenumber that exists on that street, a significant relevance boost is awarded.
- **Geographic Proximity Decay**:
  When a center coordinate (`lat`, `lon`) is provided, score decays with distance:
  $$S(d) = 2^{-d/50}$$
  *(where $d$ is the distance in kilometers; score halves every 50 km).*
