# HTTP REST API Reference

`addok-rs` implements an Axum HTTP API that is **100% compliant with the official Addok / French BAN API specification**.

It can be started with:
```bash
addok serve --index ./index_data --host 127.0.0.1 --port 7878
```

---

## 1. Full-Text Search: `/search/`

Executes a full-text address query with normalizations, candidate collection, and fuzzy ranking.

```http
GET /search/?q=8+bd+st+michel+75005+paris&limit=5 HTTP/1.1
Host: localhost:7878
```

### Query Parameters

| Parameter | Type | Default | Description |
|---|---|---|---|
| **`q`** | `string` | *(required)* | Unstructured address query text (e.g. `"8 bd st michel"`). |
| **`limit`** | `integer` | `5` | Maximum number of results to return. |
| **`autocomplete`** | `integer` | `1` | Enable (`1`) or disable (`0`) edge n-gram autocompletion on the query's last token. |
| **`lat`**, **`lon`** (or **`lng`**) | `float` | `None` | WGS84 coordinates to bias results geographically towards this center. |
| **`type`** | `string` | `None` | Filter by document type (`"housenumber"`, `"street"`, `"municipality"`, `"locality"`). |
| **`postcode`** | `string` | `None` | Filter by 5-digit postal code (e.g. `"75005"`). |
| **`citycode`** | `string` | `None` | Filter by 5-digit INSEE commune code (e.g. `"75105"`). |

### Multi-Value Filters

Filters support multiple values with `OR` logic:

- **Repeated parameters**: `/search/?q=Paris&type=street&type=municipality`
- **Space or `+` separator**: `/search/?q=Paris&type=street+municipality`

---

## 2. Reverse Geocoding: `/reverse/`

Looks up nearest geographic addresses from latitude and longitude coordinates using the spatial R-Tree index.

```http
GET /reverse/?lat=48.8512&lon=2.3431&limit=3 HTTP/1.1
Host: localhost:7878
```

### Query Parameters

| Parameter | Type | Default | Description |
|---|---|---|---|
| **`lat`** | `float` | *(required)* | Latitude in WGS84 decimal degrees. |
| **`lon`** (or **`lng`**) | `float` | *(required)* | Longitude in WGS84 decimal degrees. |
| **`limit`** | `integer` | `5` | Maximum number of nearest results. |
| **`type`**, **`postcode`**, **`citycode`** | `string` | `None` | Optional filters applied to the nearest candidates. |

---

## 3. CSV Batch Streaming: `/search/csv/` and `/reverse/csv/`

Powered by the built-in `addok_csv` plugin, these endpoints stream-geocode batch CSV files using HTTP multipart file upload.

```bash
# Batch geocode a CSV file:
curl -X POST "http://localhost:7878/search/csv/" \
  -F "data=@input_addresses.csv" \
  -F "columns=address" \
  -F "postcode=postal_code" \
  > geocoded_output.csv
```

### Supported Aliases
- `/search/csv/` and `/search:csv/`
- `/reverse/csv/` and `/reverse:csv/`

---

## 4. Response Format (GeoCodeJSON)

All search endpoints return standard [GeoCodeJSON](https://github.com/geocoders/geocodejson-spec) (an extension of GeoJSON FeatureCollection):

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
        "type": "housenumber",
        "importance": 0.65,
        "distance": 0
      }
    }
  ],
  "attribution": "BAN (Base Adresse Nationale)",
  "licence": "ETALAB-2.0",
  "query": "8 bd st michel",
  "limit": 5
}
```
