# Porting from Python Addok

`addok-rs` is designed as a **100% drop-in replacement** for the original Python+Redis Addok.

If you are already running Addok in production or development, migrating to `addok-rs` requires zero changes to your frontends, API clients, or data pipelines.

---

## 🧭 Command Mapping

| Python Addok Command | `addok-rs` Equivalent | Notes |
|---|---|---|
| `addok batch <file.ndjson>` | `addok batch <file.ndjson> --index ./index_data` | Reads identical NDJSON format from file or pipe. |
| *(None - requires csv plugin)* | `addok index --input ban.csv.gz --output ./index_data` | Direct ingestion of official BAN `.csv.gz`. |
| `addok serve` / `gunicorn addok.http.wsgi` | `addok serve --index ./index_data --port 7878` | Built-in async HTTP server (no Gunicorn needed). |
| `addok shell` | `addok shell --index ./index_data` | Enhanced interactive REPL with color formatting. |
| `addok reset` | `addok reset --index ./index_data -y` | Safely wipes the target index directory. |
| `addok ngrams` | Handled automatically during index build | `addok ngrams` subcommand also available. |

---

## ⚙️ Configuration Mapping

| Python Addok (`addok.conf`) | `addok-rs` (`addok.toml` or Env) |
|---|---|
| `PLUGINS = ["addok.shell", ...]` | `plugins = ["addok.shell", ...]` or `ADDOK_PLUGINS="..."` |
| `FIELDS = [{"key": "name", "boost": 4}, ...]` | `[[fields]] key = "name" boost = 4.0` |
| `FILTERS = ["type", "postcode"]` | `filters = ["type", "postcode"]` |
| `BUCKET_MIN = 10` | `bucket_min = 10` or `ADDOK_BUCKET_MIN=10` |
| `BUCKET_MAX = 100` | `bucket_max = 100` or `ADDOK_BUCKET_MAX=100` |
| `MATCH_THRESHOLD = 0.9` | `match_threshold = 0.9` |

---

## 🔌 API & Client Compatibility

Because `addok-rs` implements the exact same [GeoCodeJSON](https://github.com/geocoders/geocodejson-spec) specification and query parameters as Python Addok:

- **Web frontends** (e.g. [addok-web](https://github.com/addok/addok-web), Leaflet geocoding plugins, autocomplete dropdowns) work immediately without modification.
- **Client libraries** (Python `geopy`, JavaScript `fetch`, cURL scripts) continue to function identically.
- **CSV batch uploaders** can post to `/search/csv/` without changing field names or headers.

---

## 💡 Migration Steps Checklist

1. **Stop Redis**: You no longer need `redis-server` running for Addok.
2. **Build or download the `addok` binary**: Place it on your server.
3. **Build the index**:
   ```bash
   addok index --input adresses-france.csv.gz --output /var/lib/addok/index
   ```
4. **Update systemd service**: Switch `ExecStart` from `gunicorn addok.http.wsgi` to `addok serve --index /var/lib/addok/index --port 7878`.
5. **Reload service**: Restart your service and enjoy 30x lower search latencies!
