# Step-by-Step Tutorial

This tutorial guides you through indexing a sample address dataset and launching your first `addok-rs` search instance.

---

## 1. Getting Data

You can download official French BAN (Base Adresse Nationale) datasets directly from [adresse.data.gouv.fr](https://adresse.data.gouv.fr/donnees-nationales):

```bash
mkdir -p data
# Download Paris (75) department data (~6 MB compressed, ~150,000 addresses)
curl -L -o data/adresses-75.csv.gz https://adresse.data.gouv.fr/data/ban/adresses/latest/csv/adresses-75.csv.gz
```

For quick experimentation, the repository also includes a ready-to-use sample fixture:
`data/sample_adresses_75.csv` (18 representative Paris addresses).

---

## 2. Building the Index

Run the `index` subcommand to process the CSV stream, aggregate housenumbers, and create the memory-mapped Tantivy and spatial indices:

```bash
addok index --input data/adresses-75.csv.gz --output ./index_paris
```

You will see the progress bar streaming records:
```
Starting indexing from "data/adresses-75.csv.gz"
Output index directory: "index_paris"
Indexed 152,628 documents in 1.42s (107,484 docs/sec)
Done! Index is ready for searches.
```

---

## 3. Testing Address Search from the CLI

Before starting the web server, you can test searches directly from your terminal:

```bash
addok search "8 bd st michel" --index ./index_paris
```

Output:
```text
Found 5 results:
1. [0.98] 8 Boulevard Saint-Michel, 75005 Paris (ID: 75105_0001_8) - coords: (48.85120, 2.34310)
2. [0.85] 10 Boulevard Saint-Michel, 75005 Paris (ID: 75105_0001_10) - coords: (48.85135, 2.34325)
...
```

You can format the CLI output as standard GeoJSON with `--json`:
```bash
addok search "8 bd st michel" --index ./index_paris --json
```

---

## 4. Testing Reverse Geocoding

Find the closest street and housenumber given GPS coordinates (WGS84):

```bash
addok reverse --lat 48.8512 --lon 2.3431 --index ./index_paris
```

---

## 5. Running the REST API Server

Launch the Axum-powered HTTP server on port 7878:

```bash
addok serve --index ./index_paris --port 7878
```

You will see:
```
INFO addok_server: 🚀 Addok HTTP API listening on 127.0.0.1:7878
INFO addok_server: Loaded index from "index_paris" (Tantivy + R-Tree)
```

Now query the API using `curl`:

```bash
curl -s "http://localhost:7878/search/?q=8+bd+st+michel" | jq .
```

The server returns a standard GeoCodeJSON response:
```json
{
  "type": "FeatureCollection",
  "version": "draft-02",
  "features": [
    {
      "type": "Feature",
      "geometry": {
        "type": "Point",
        "coordinates": [2.3431, 48.8512]
      },
      "properties": {
        "id": "75105_0001_8",
        "label": "8 Boulevard Saint-Michel 75005 Paris",
        "name": "8 Boulevard Saint-Michel",
        "score": 0.98,
        "housenumber": "8",
        "postcode": "75005",
        "citycode": "75105",
        "city": "Paris",
        "context": "75, Paris, Île-de-France",
        "type": "housenumber"
      }
    }
  ],
  "attribution": "BAN",
  "licence": "ETALAB-2.0",
  "query": "8 bd st michel",
  "limit": 5
}
```

---

## 6. Launching the Interactive Shell

`addok-rs` includes an interactive REPL for diagnosing queries, tokenization, and candidate collection:

```bash
addok shell --index ./index_paris
```

In the shell:
```text
addok> 8 bd st michel
addok> EXPLAIN 8 bd st michel
addok> TOKENIZE 8 bis bd saint michel 75005 paris
addok> REVERSE 48.8512 2.3431
addok> EXIT
```
