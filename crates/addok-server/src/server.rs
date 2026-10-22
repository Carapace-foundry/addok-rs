use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

use addok_index::{km_to_score, AddokSearcher, SearchHit, SearchParams, SpatialResult};
use anyhow::Result;
use axum::{
    body::Body,
    extract::{Multipart, RawQuery, State},
    http::{header, Method, StatusCode},
    response::{IntoResponse, Json, Response},
    routing::{get, post},
    Router,
};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use tower_http::trace::TraceLayer;
use tracing::{info, warn};

use crate::api::{ApiError, FeatureCollection};

// Global Prometheus/Telemetry metrics counters
static SEARCH_REQUESTS: AtomicU64 = AtomicU64::new(0);
static REVERSE_REQUESTS: AtomicU64 = AtomicU64::new(0);
static CSV_SEARCH_REQUESTS: AtomicU64 = AtomicU64::new(0);
static CSV_REVERSE_REQUESTS: AtomicU64 = AtomicU64::new(0);

#[derive(Clone)]
pub struct AppState {
    pub searcher: Arc<AddokSearcher>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct SearchQuery {
    pub q: Option<String>,
    pub query: Option<String>,
    pub limit: Option<usize>,
    pub lat: Option<f64>,
    pub lon: Option<f64>,
    pub lng: Option<f64>,
    pub postcode: Option<String>,
    pub citycode: Option<String>,
    pub autocomplete: Option<String>,
    #[serde(rename = "type")]
    pub address_type: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct ReverseQuery {
    pub lat: Option<f64>,
    pub lon: Option<f64>,
    pub lng: Option<f64>,
    pub limit: Option<usize>,
}

const DEFAULT_SEARCH_CSV_COLUMNS: [&str; 12] = [
    "result_label",
    "result_score",
    "result_score_next",
    "result_type",
    "result_id",
    "result_housenumber",
    "result_postcode",
    "result_city",
    "result_citycode",
    "result_distance",
    "latitude",
    "longitude",
];

const DEFAULT_REVERSE_CSV_COLUMNS: [&str; 11] = [
    "result_label",
    "result_distance",
    "result_type",
    "result_id",
    "result_housenumber",
    "result_postcode",
    "result_city",
    "result_citycode",
    "result_latitude",
    "result_longitude",
    "result_context",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CsvResultField {
    Label,
    Score,
    ScoreNext,
    Type,
    Id,
    Housenumber,
    Postcode,
    City,
    Citycode,
    Distance,
    Latitude,
    Longitude,
    Context,
    Name,
    Street,
    Unknown,
}

impl CsvResultField {
    pub fn parse(name: &str) -> Self {
        let norm = name.trim().to_lowercase();
        let stripped = norm.strip_prefix("result_").unwrap_or(&norm);
        match stripped {
            "label" => Self::Label,
            "score_next" => Self::ScoreNext,
            "score" => Self::Score,
            "type" => Self::Type,
            "id" => Self::Id,
            "housenumber" => Self::Housenumber,
            "postcode" | "code_postal" | "cp" => Self::Postcode,
            "city" | "commune" => Self::City,
            "citycode" | "code_insee" | "insee" => Self::Citycode,
            "distance" => Self::Distance,
            "lat" | "latitude" => Self::Latitude,
            "lon" | "lng" | "longitude" => Self::Longitude,
            "context" => Self::Context,
            "name" => Self::Name,
            "street" | "voie" => Self::Street,
            _ => Self::Unknown,
        }
    }
}

fn extract_search_hit_fields(
    hit: Option<&SearchHit>,
    next_hit: Option<&SearchHit>,
    field: CsvResultField,
) -> String {
    let hit = match hit {
        Some(h) => h,
        None => return String::new(),
    };
    let rec = &hit.record;
    match field {
        CsvResultField::Label => rec.label(),
        CsvResultField::Score => format!("{:.2}", (hit.score * 100.0).round() / 100.0),
        CsvResultField::ScoreNext => next_hit
            .map(|h| format!("{:.2}", (h.score * 100.0).round() / 100.0))
            .unwrap_or_else(|| "0".to_string()),
        CsvResultField::Type => rec.ban_type().to_string(),
        CsvResultField::Id => rec.id.clone(),
        CsvResultField::Housenumber => rec.full_housenumber().unwrap_or_default(),
        CsvResultField::Postcode => rec.code_postal.clone(),
        CsvResultField::City => rec.nom_commune.clone(),
        CsvResultField::Citycode => rec.code_insee.clone().unwrap_or_default(),
        CsvResultField::Distance => hit
            .distance_meters
            .map(|d| d.round().to_string())
            .unwrap_or_default(),
        CsvResultField::Latitude => rec.lat.map(|l| l.to_string()).unwrap_or_default(),
        CsvResultField::Longitude => rec.lon.map(|l| l.to_string()).unwrap_or_default(),
        CsvResultField::Context => hit
            .document
            .as_ref()
            .and_then(|d| d.context.clone())
            .unwrap_or_default(),
        CsvResultField::Name => {
            let addr_type = rec.ban_type();
            if addr_type == addok_core::ban::BanType::Municipality {
                rec.nom_commune.clone()
            } else if addr_type == addok_core::ban::BanType::Housenumber
                || hit.matched_housenumber.is_some()
            {
                rec.street_and_number()
            } else {
                rec.nom_voie.clone()
            }
        }
        CsvResultField::Street => {
            let addr_type = rec.ban_type();
            if addr_type == addok_core::ban::BanType::Municipality {
                String::new()
            } else {
                rec.nom_voie.clone()
            }
        }
        CsvResultField::Unknown => String::new(),
    }
}

fn extract_spatial_result_field(
    res: Option<&SpatialResult>,
    doc_store: &addok_index::DocumentStore,
    field: CsvResultField,
) -> String {
    let res = match res {
        Some(r) => r,
        None => return String::new(),
    };
    let item = &res.item;
    let doc = doc_store.get_by_id(&item.id).or_else(|| {
        let (parent_id, _) = item.id.rsplit_once('_')?;
        doc_store.get_by_id(parent_id)
    });

    match field {
        CsvResultField::Label => item.label.clone(),
        CsvResultField::Score => {
            let dist_km = res.distance_meters / 1000.0;
            let score = km_to_score(dist_km) as f32;
            format!("{:.2}", (score * 100.0).round() / 100.0)
        }
        CsvResultField::ScoreNext => "0".to_string(),
        CsvResultField::Type => item.ban_type.clone(),
        CsvResultField::Id => item.id.clone(),
        CsvResultField::Housenumber => item.housenumber.clone().unwrap_or_default(),
        CsvResultField::Postcode => item.postcode.clone(),
        CsvResultField::City => item.city.clone(),
        CsvResultField::Citycode => item
            .citycode
            .clone()
            .or_else(|| doc.and_then(|d| d.citycode.clone()))
            .unwrap_or_default(),
        CsvResultField::Distance => format!("{}", res.distance_meters.round()),
        CsvResultField::Latitude => item.point[1].to_string(),
        CsvResultField::Longitude => item.point[0].to_string(),
        CsvResultField::Context => doc.and_then(|d| d.context.clone()).unwrap_or_default(),
        CsvResultField::Name => {
            let street_name = doc.map(|d| d.name.clone());
            match (&item.housenumber, &street_name) {
                (Some(hn), Some(st)) => format!("{} {}", hn, st),
                (None, Some(st)) => st.clone(),
                _ => {
                    if let Some(hn) = &item.housenumber {
                        let street_part = item
                            .label
                            .strip_prefix(hn)
                            .map(str::trim)
                            .and_then(|s| s.strip_suffix(&item.city))
                            .map(str::trim)
                            .and_then(|s| s.strip_suffix(&item.postcode))
                            .map(str::trim)
                            .unwrap_or(&item.label);
                        format!("{} {}", hn, street_part)
                    } else {
                        item.label.clone()
                    }
                }
            }
        }
        CsvResultField::Street => {
            if item.ban_type == "municipality" {
                String::new()
            } else {
                doc.map(|d| d.name.clone()).unwrap_or_default()
            }
        }
        CsvResultField::Unknown => String::new(),
    }
}

#[derive(Debug, Clone, Default)]
struct ParamResolution {
    col_idx: Option<usize>,
    static_val: Option<String>,
}

fn resolve_csv_param(
    param_val: Option<String>,
    headers: &csv::StringRecord,
    default_columns: &[&str],
) -> ParamResolution {
    if let Some(val) = param_val {
        if let Some(idx) = headers.iter().position(|h| h.eq_ignore_ascii_case(&val)) {
            ParamResolution {
                col_idx: Some(idx),
                static_val: None,
            }
        } else {
            ParamResolution {
                col_idx: None,
                static_val: Some(val),
            }
        }
    } else {
        let col_idx = default_columns
            .iter()
            .find_map(|&name| headers.iter().position(|h| h.eq_ignore_ascii_case(name)));
        ParamResolution {
            col_idx,
            static_val: None,
        }
    }
}

#[derive(Debug, Clone, Default)]
struct CoordParamResolution {
    col_idx: Option<usize>,
    static_val: Option<f64>,
}

fn resolve_csv_coord_param(
    param_val: Option<String>,
    headers: &csv::StringRecord,
    default_columns: &[&str],
) -> CoordParamResolution {
    if let Some(val) = param_val {
        if let Some(idx) = headers.iter().position(|h| h.eq_ignore_ascii_case(&val)) {
            CoordParamResolution {
                col_idx: Some(idx),
                static_val: None,
            }
        } else if let Ok(f) = val.replace(',', ".").trim().parse::<f64>() {
            CoordParamResolution {
                col_idx: None,
                static_val: Some(f),
            }
        } else {
            CoordParamResolution {
                col_idx: None,
                static_val: None,
            }
        }
    } else {
        let col_idx = default_columns
            .iter()
            .find_map(|&name| headers.iter().position(|h| h.eq_ignore_ascii_case(name)));
        CoordParamResolution {
            col_idx,
            static_val: None,
        }
    }
}

/// Transcodes CSV bytes to UTF-8 handling Latin-1 / Windows-1252 if specified or detected
fn decode_csv_bytes(bytes: &[u8], forced_encoding: Option<&str>) -> Vec<u8> {
    let is_latin1 = forced_encoding.is_some_and(|e| {
        let l = e.to_lowercase();
        l.contains("iso") || l.contains("latin") || l.contains("1252") || l.contains("windows")
    });

    if is_latin1 {
        let text: String = bytes.iter().map(|&b| b as char).collect();
        return text.into_bytes();
    }

    if std::str::from_utf8(bytes).is_ok() {
        bytes.to_vec()
    } else {
        let text = String::from_utf8_lossy(bytes);
        text.into_owned().into_bytes()
    }
}

/// Helper stream for asynchronous chunked HTTP streaming from an mpsc receiver
struct ReceiverStream<T> {
    inner: tokio::sync::mpsc::Receiver<T>,
}

impl<T> ReceiverStream<T> {
    pub fn new(inner: tokio::sync::mpsc::Receiver<T>) -> Self {
        Self { inner }
    }
}

impl<T> futures_core::Stream for ReceiverStream<T> {
    type Item = T;

    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        self.inner.poll_recv(cx)
    }
}

/// Detects the CSV delimiter (';', ',', or '\t') by inspecting the first line
fn detect_delimiter_from_bytes(bytes: &[u8]) -> u8 {
    let first_line = match bytes.iter().position(|&b| b == b'\n') {
        Some(pos) => &bytes[..pos],
        None => bytes,
    };
    let line_str = String::from_utf8_lossy(first_line);
    let semicolons = line_str.matches(';').count();
    let commas = line_str.matches(',').count();
    let tabs = line_str.matches('\t').count();

    if semicolons >= commas && semicolons >= tabs && semicolons > 0 {
        b';'
    } else if tabs > commas && tabs > 0 {
        b'\t'
    } else {
        b','
    }
}

/// CORS middleware ensuring Access-Control headers on every response, including OPTIONS preflights
async fn cors_middleware(
    req: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    if req.method() == Method::OPTIONS {
        let mut response = Response::new(Body::empty());
        let headers = response.headers_mut();
        headers.insert(
            header::ACCESS_CONTROL_ALLOW_ORIGIN,
            header::HeaderValue::from_static("*"),
        );
        headers.insert(
            header::ACCESS_CONTROL_ALLOW_HEADERS,
            header::HeaderValue::from_static("X-Requested-With, Content-Type, Accept"),
        );
        headers.insert(
            header::ACCESS_CONTROL_ALLOW_METHODS,
            header::HeaderValue::from_static("GET, POST, OPTIONS, HEAD"),
        );
        return response;
    }

    let mut response = next.run(req).await;
    let headers = response.headers_mut();
    headers.insert(
        header::ACCESS_CONTROL_ALLOW_ORIGIN,
        header::HeaderValue::from_static("*"),
    );
    headers.insert(
        header::ACCESS_CONTROL_ALLOW_HEADERS,
        header::HeaderValue::from_static("X-Requested-With, Content-Type, Accept"),
    );
    headers.insert(
        header::ACCESS_CONTROL_ALLOW_METHODS,
        header::HeaderValue::from_static("GET, POST, OPTIONS, HEAD"),
    );
    response
}

/// Handler returned when the CSV geocoding plugin (`addok_csv`) is not enabled
async fn csv_disabled_handler() -> Response {
    (
        StatusCode::NOT_FOUND,
        Json(serde_json::json!({
            "error": "Plugin 'addok_csv' is not enabled"
        })),
    )
        .into_response()
}

/// Create Axum router with `/search/`, `/reverse/`, and CSV batch streaming endpoints
pub fn create_router(state: AppState) -> Router {
    let mut router = Router::new()
        .route("/search", get(search_handler))
        .route("/search/", get(search_handler))
        .route("/reverse", get(reverse_handler))
        .route("/reverse/", get(reverse_handler))
        .route("/health", get(health_handler))
        .route("/metrics", get(metrics_handler));

    if state.searcher.config.has_csv() {
        router = router
            .route("/csv", post(search_csv_handler))
            .route("/csv/", post(search_csv_handler))
            .route("/search/csv", post(search_csv_handler))
            .route("/search/csv/", post(search_csv_handler))
            .route("/search:csv", post(search_csv_handler))
            .route("/search:csv/", post(search_csv_handler))
            .route("/reverse/csv", post(reverse_csv_handler))
            .route("/reverse/csv/", post(reverse_csv_handler))
            .route("/reverse:csv", post(reverse_csv_handler))
            .route("/reverse:csv/", post(reverse_csv_handler));
    } else {
        router = router
            .route("/csv", post(csv_disabled_handler))
            .route("/csv/", post(csv_disabled_handler))
            .route("/search/csv", post(csv_disabled_handler))
            .route("/search/csv/", post(csv_disabled_handler))
            .route("/search:csv", post(csv_disabled_handler))
            .route("/search:csv/", post(csv_disabled_handler))
            .route("/reverse/csv", post(csv_disabled_handler))
            .route("/reverse/csv/", post(csv_disabled_handler))
            .route("/reverse:csv", post(csv_disabled_handler))
            .route("/reverse:csv/", post(csv_disabled_handler));
    }

    router
        .layer(axum::middleware::from_fn(cors_middleware))
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

/// `/search` endpoint handler
async fn search_handler(State(state): State<AppState>, RawQuery(raw_query): RawQuery) -> Response {
    SEARCH_REQUESTS.fetch_add(1, Ordering::Relaxed);
    let mut q: Option<String> = None;
    let mut limit = 5usize;
    let mut autocomplete = state.searcher.config.has_autocomplete();
    let mut lat: Option<f64> = None;
    let mut lon: Option<f64> = None;
    let mut lng: Option<f64> = None;
    let mut postcode: Option<String> = None;
    let mut citycode: Option<String> = None;
    let mut filters: HashMap<String, String> = HashMap::new();

    if let Some(query_str) = raw_query {
        for (k, v) in form_urlencoded::parse(query_str.as_bytes()) {
            let key = k.trim().to_lowercase();
            let val = v.trim();
            if val.is_empty() {
                continue;
            }

            let norm_key = if let Some(inner) = key
                .strip_prefix("filter[")
                .and_then(|k| k.strip_suffix(']'))
            {
                inner.trim()
            } else {
                key.as_str()
            };

            match norm_key {
                "q" | "query" => {
                    q = Some(val.to_string());
                }
                "limit" => {
                    if let Ok(l) = val.parse::<usize>() {
                        limit = l.clamp(1, 100);
                    }
                }
                "autocomplete" => {
                    if state.searcher.config.has_autocomplete() {
                        autocomplete = val != "0" && !val.eq_ignore_ascii_case("false");
                    } else {
                        autocomplete = false;
                    }
                }
                "lat" | "latitude" => {
                    if let Ok(num) = val.replace(',', ".").parse::<f64>() {
                        lat = Some(num);
                    }
                }
                "lon" | "longitude" => {
                    if let Ok(num) = val.replace(',', ".").parse::<f64>() {
                        lon = Some(num);
                    }
                }
                "lng" => {
                    if let Ok(num) = val.replace(',', ".").parse::<f64>() {
                        lng = Some(num);
                    }
                }
                "postcode" => {
                    match filters.get_mut("postcode") {
                        Some(existing) => {
                            existing.push(' ');
                            existing.push_str(val);
                        }
                        None => {
                            filters.insert("postcode".to_string(), val.to_string());
                        }
                    }
                    postcode = Some(val.to_string());
                }
                "citycode" => {
                    match filters.get_mut("citycode") {
                        Some(existing) => {
                            existing.push(' ');
                            existing.push_str(val);
                        }
                        None => {
                            filters.insert("citycode".to_string(), val.to_string());
                        }
                    }
                    citycode = Some(val.to_string());
                }
                "type" => match filters.get_mut("type") {
                    Some(existing) => {
                        existing.push(' ');
                        existing.push_str(val);
                    }
                    None => {
                        filters.insert("type".to_string(), val.to_string());
                    }
                },
                custom_key => match filters.get_mut(custom_key) {
                    Some(existing) => {
                        existing.push(' ');
                        existing.push_str(val);
                    }
                    None => {
                        filters.insert(custom_key.to_string(), val.to_string());
                    }
                },
            }
        }
    }

    let lon = lon.or(lng);

    let raw_q = match q {
        Some(s) if !s.trim().is_empty() => s.trim().to_string(),
        _ => {
            return (
                StatusCode::BAD_REQUEST,
                Json(ApiError::invalid_query("Missing query parameter 'q'")),
            )
                .into_response();
        }
    };

    let mut search_params = SearchParams::new(&raw_q).with_limit(limit);
    search_params.autocomplete = autocomplete;
    search_params.postcode = postcode;
    if let (Some(la), Some(lo)) = (lat, lon) {
        search_params = search_params.with_geo_bias(la, lo);
    }
    for (k, v) in filters {
        search_params.filters.insert(k, v);
    }
    if let Some(cc) = citycode {
        search_params.filters.insert("citycode".to_string(), cc);
    }

    let start = Instant::now();
    let hits = match state.searcher.search(&search_params) {
        Ok(h) => h,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "type": "server_error",
                    "error": format!("Search error: {}", e),
                    "detail": e.to_string(),
                })),
            )
                .into_response();
        }
    };

    let duration = start.elapsed();
    let duration_ms = duration.as_secs_f64() * 1000.0;

    let log_queries = state.searcher.config.log_queries
        || std::env::var("LOG_QUERIES").is_ok_and(|v| v == "1" || v.eq_ignore_ascii_case("true"));
    let log_not_found = state.searcher.config.log_not_found
        || std::env::var("LOG_NOT_FOUND").is_ok_and(|v| v == "1" || v.eq_ignore_ascii_case("true"));
    let slow_threshold = state.searcher.config.slow_queries.or_else(|| {
        std::env::var("SLOW_QUERIES")
            .ok()
            .and_then(|v| v.parse().ok())
    });

    if log_queries {
        info!(
            query = %raw_q,
            hits = hits.len(),
            duration_ms = format!("{:.2}", duration_ms),
            "Search query executed"
        );
    }

    if let Some(threshold) = slow_threshold {
        if duration_ms >= threshold {
            warn!(
                query = %raw_q,
                duration_ms = format!("{:.2}", duration_ms),
                threshold_ms = threshold,
                "Slow search query detected"
            );
        }
    }

    if hits.is_empty() && log_not_found {
        info!(
            query = %raw_q,
            duration_ms = format!("{:.2}", duration_ms),
            "Search query returned no results"
        );
    }

    let center = match (lat, lon) {
        (Some(la), Some(lo)) => Some([lo, la]),
        _ => None,
    };

    let mut filter_map = HashMap::new();
    for (k, v) in &search_params.filters {
        filter_map.insert(k.clone(), serde_json::Value::String(v.clone()));
    }
    if let Some(ref pc) = search_params.postcode {
        filter_map.insert(
            "postcode".to_string(),
            serde_json::Value::String(pc.clone()),
        );
    }

    let collection = FeatureCollection::from_search_hits_with_center(hits, &raw_q, limit, center)
        .with_attribution_and_licence(
            &state.searcher.config.attribution,
            &state.searcher.config.licence,
        )
        .with_filters(filter_map);
    (StatusCode::OK, Json(collection)).into_response()
}

/// `/reverse` endpoint handler
async fn reverse_handler(State(state): State<AppState>, RawQuery(raw_query): RawQuery) -> Response {
    REVERSE_REQUESTS.fetch_add(1, Ordering::Relaxed);
    let mut limit = 1usize;
    let mut lat: Option<f64> = None;
    let mut lon: Option<f64> = None;
    let mut lng: Option<f64> = None;
    let mut type_filter: Vec<String> = Vec::new();
    let mut postcode_filter: Vec<String> = Vec::new();
    let mut citycode_filter: Vec<String> = Vec::new();

    if let Some(query_str) = raw_query {
        for (k, v) in form_urlencoded::parse(query_str.as_bytes()) {
            let key = k.trim().to_lowercase();
            let val = v.trim();
            if val.is_empty() {
                continue;
            }
            let norm_key = if let Some(inner) = key
                .strip_prefix("filter[")
                .and_then(|k| k.strip_suffix(']'))
            {
                inner.trim()
            } else {
                key.as_str()
            };
            match norm_key {
                "lat" | "latitude" => {
                    if let Ok(num) = val.replace(',', ".").parse::<f64>() {
                        lat = Some(num);
                    }
                }
                "lon" | "longitude" => {
                    if let Ok(num) = val.replace(',', ".").parse::<f64>() {
                        lon = Some(num);
                    }
                }
                "lng" => {
                    if let Ok(num) = val.replace(',', ".").parse::<f64>() {
                        lng = Some(num);
                    }
                }
                "limit" | "k" => {
                    if let Ok(l) = val.parse::<usize>() {
                        limit = l.clamp(1, 50);
                    }
                }
                "type" => {
                    for t in val.split(['+', ',', ' ']) {
                        let trimmed = t.trim();
                        if !trimmed.is_empty() {
                            type_filter.push(trimmed.to_string());
                        }
                    }
                }
                "postcode" | "code_postal" => {
                    for p in val.split(['+', ',', ' ']) {
                        let trimmed = p.trim();
                        if !trimmed.is_empty() {
                            postcode_filter.push(trimmed.to_string());
                        }
                    }
                }
                "citycode" | "code_insee" => {
                    for c in val.split(['+', ',', ' ']) {
                        let trimmed = c.trim();
                        if !trimmed.is_empty() {
                            citycode_filter.push(trimmed.to_string());
                        }
                    }
                }
                _ => {}
            }
        }
    }

    let lon = lon.or(lng);

    let (lat, lon) = match (lat, lon) {
        (Some(la), Some(lo)) => (la, lo),
        _ => {
            return (
                StatusCode::BAD_REQUEST,
                Json(ApiError::invalid_query("Missing lat or lon parameter")),
            )
                .into_response();
        }
    };

    let start = Instant::now();
    let results = state.searcher.reverse_with_filters(
        lat,
        lon,
        limit,
        if type_filter.is_empty() {
            None
        } else {
            Some(&type_filter)
        },
        if postcode_filter.is_empty() {
            None
        } else {
            Some(&postcode_filter)
        },
        if citycode_filter.is_empty() {
            None
        } else {
            Some(&citycode_filter)
        },
    );
    let duration = start.elapsed();
    let duration_ms = duration.as_secs_f64() * 1000.0;

    let log_queries = state.searcher.config.log_queries
        || std::env::var("LOG_QUERIES").is_ok_and(|v| v == "1" || v.eq_ignore_ascii_case("true"));
    let log_not_found = state.searcher.config.log_not_found
        || std::env::var("LOG_NOT_FOUND").is_ok_and(|v| v == "1" || v.eq_ignore_ascii_case("true"));
    let slow_threshold = state.searcher.config.slow_queries.or_else(|| {
        std::env::var("SLOW_QUERIES")
            .ok()
            .and_then(|v| v.parse().ok())
    });

    if log_queries {
        info!(
            lat = lat,
            lon = lon,
            hits = results.len(),
            duration_ms = format!("{:.2}", duration_ms),
            "Reverse query executed"
        );
    }

    if let Some(threshold) = slow_threshold {
        if duration_ms >= threshold {
            warn!(
                lat = lat,
                lon = lon,
                duration_ms = format!("{:.2}", duration_ms),
                threshold_ms = threshold,
                "Slow reverse query detected"
            );
        }
    }

    if results.is_empty() && log_not_found {
        info!(
            lat = lat,
            lon = lon,
            duration_ms = format!("{:.2}", duration_ms),
            "Reverse query returned no results"
        );
    }

    let mut filter_map = HashMap::new();
    if !type_filter.is_empty() {
        filter_map.insert("type".to_string(), serde_json::json!(type_filter));
    }
    if !postcode_filter.is_empty() {
        filter_map.insert("postcode".to_string(), serde_json::json!(postcode_filter));
    }
    if !citycode_filter.is_empty() {
        filter_map.insert("citycode".to_string(), serde_json::json!(citycode_filter));
    }

    let collection = FeatureCollection::from_spatial_results_with_details(
        results,
        limit,
        Some([lon, lat]),
        Some(&state.searcher.doc_store),
    )
    .with_attribution_and_licence(
        &state.searcher.config.attribution,
        &state.searcher.config.licence,
    )
    .with_filters(filter_map);
    (StatusCode::OK, Json(collection)).into_response()
}

/// High-Performance Streaming CSV Batch Geocoding: `POST /search/csv/`
async fn search_csv_handler(
    State(state): State<AppState>,
    RawQuery(raw_query): RawQuery,
    mut multipart: Multipart,
) -> Response {
    CSV_SEARCH_REQUESTS.fetch_add(1, Ordering::Relaxed);
    let mut query_columns: Vec<String> = Vec::new();
    let mut query_result_columns: Vec<String> = Vec::new();
    let mut query_postcode: Option<String> = None;
    let mut query_citycode: Option<String> = None;
    let mut query_type: Option<String> = None;
    let mut query_lat: Option<String> = None;
    let mut query_lon: Option<String> = None;
    let mut query_delimiter: Option<u8> = None;
    let mut query_encoding: Option<String> = None;

    if let Some(q_str) = raw_query {
        for (k, v) in form_urlencoded::parse(q_str.as_bytes()) {
            let key = k.trim().to_lowercase();
            let val = v.trim();
            if val.is_empty() {
                continue;
            }
            let norm_key = if let Some(inner) = key
                .strip_prefix("filter[")
                .and_then(|k| k.strip_suffix(']'))
            {
                inner.trim()
            } else {
                key.as_str()
            };
            match norm_key {
                "columns" => {
                    for c in val.split(',') {
                        let trimmed = c.trim();
                        if !trimmed.is_empty() {
                            query_columns.push(trimmed.to_string());
                        }
                    }
                }
                "result_columns" => {
                    for c in val.split(',') {
                        let trimmed = c.trim();
                        if !trimmed.is_empty() {
                            query_result_columns.push(trimmed.to_string());
                        }
                    }
                }
                "postcode" => query_postcode = Some(val.to_string()),
                "citycode" => query_citycode = Some(val.to_string()),
                "type" => query_type = Some(val.to_string()),
                "lat" | "latitude" => query_lat = Some(val.to_string()),
                "lon" | "lng" | "longitude" => query_lon = Some(val.to_string()),
                "encoding" => query_encoding = Some(val.to_string()),
                "delimiter" => {
                    if let Some(b) = val.bytes().next() {
                        query_delimiter = Some(b);
                    }
                }
                _ => {}
            }
        }
    }

    let mut csv_bytes: Option<Vec<u8>> = None;
    let mut form_columns: Vec<String> = Vec::new();
    let mut form_result_columns: Vec<String> = Vec::new();
    let mut form_postcode: Option<String> = None;
    let mut form_citycode: Option<String> = None;
    let mut form_type: Option<String> = None;
    let mut form_lat: Option<String> = None;
    let mut form_lon: Option<String> = None;
    let mut form_delimiter: Option<u8> = None;
    let mut form_encoding: Option<String> = None;

    while let Ok(Some(field)) = multipart.next_field().await {
        let name = field.name().unwrap_or("").to_string();
        let norm_name = if let Some(inner) = name
            .strip_prefix("filter[")
            .and_then(|k| k.strip_suffix(']'))
        {
            inner.trim()
        } else {
            name.as_str()
        };

        if norm_name == "data" || field.file_name().is_some() {
            match field.bytes().await {
                Ok(bytes) => csv_bytes = Some(bytes.to_vec()),
                Err(e) => {
                    return (
                        StatusCode::BAD_REQUEST,
                        Json(ApiError::invalid_query(format!(
                            "Failed to read CSV payload: {}",
                            e
                        ))),
                    )
                        .into_response();
                }
            }
        } else if norm_name == "columns" {
            if let Ok(text) = field.text().await {
                for c in text.split(',') {
                    let trimmed = c.trim();
                    if !trimmed.is_empty() {
                        form_columns.push(trimmed.to_string());
                    }
                }
            }
        } else if norm_name == "result_columns" {
            if let Ok(text) = field.text().await {
                for c in text.split(',') {
                    let trimmed = c.trim();
                    if !trimmed.is_empty() {
                        form_result_columns.push(trimmed.to_string());
                    }
                }
            }
        } else if norm_name == "postcode" {
            if let Ok(text) = field.text().await {
                form_postcode = Some(text.trim().to_string());
            }
        } else if norm_name == "citycode" {
            if let Ok(text) = field.text().await {
                form_citycode = Some(text.trim().to_string());
            }
        } else if norm_name == "type" {
            if let Ok(text) = field.text().await {
                form_type = Some(text.trim().to_string());
            }
        } else if norm_name == "lat" || norm_name == "latitude" {
            if let Ok(text) = field.text().await {
                form_lat = Some(text.trim().to_string());
            }
        } else if norm_name == "lon" || norm_name == "lng" || norm_name == "longitude" {
            if let Ok(text) = field.text().await {
                form_lon = Some(text.trim().to_string());
            }
        } else if norm_name == "encoding" {
            if let Ok(text) = field.text().await {
                form_encoding = Some(text.trim().to_string());
            }
        } else if norm_name == "delimiter" {
            if let Ok(text) = field.text().await {
                if let Some(b) = text.trim().bytes().next() {
                    form_delimiter = Some(b);
                }
            }
        }
    }

    let raw_data = match csv_bytes {
        Some(b) if !b.is_empty() => b,
        _ => {
            return (
                StatusCode::BAD_REQUEST,
                Json(ApiError::invalid_query("Missing CSV file in 'data' field")),
            )
                .into_response();
        }
    };

    let encoding = form_encoding.or(query_encoding);
    let clean_data = decode_csv_bytes(&raw_data, encoding.as_deref());

    let delimiter = form_delimiter
        .or(query_delimiter)
        .unwrap_or_else(|| detect_delimiter_from_bytes(&clean_data));

    let columns = if !form_columns.is_empty() {
        form_columns
    } else {
        query_columns
    };

    let mut reader = csv::ReaderBuilder::new()
        .delimiter(delimiter)
        .flexible(true)
        .has_headers(true)
        .from_reader(std::io::Cursor::new(clean_data));

    let headers = match reader.headers() {
        Ok(h) => h.clone(),
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(ApiError::invalid_query(format!(
                    "Failed to parse CSV headers: {}",
                    e
                ))),
            )
                .into_response();
        }
    };

    let mut query_col_indices: Vec<usize> = Vec::new();
    if !columns.is_empty() {
        for col in &columns {
            let lower_col = col.to_lowercase();
            if let Some(idx) = headers.iter().position(|h| h.to_lowercase() == lower_col) {
                query_col_indices.push(idx);
            }
        }
    }

    if query_col_indices.is_empty() {
        for candidate in &["q", "query", "adresse", "address", "voie"] {
            if let Some(idx) = headers
                .iter()
                .position(|h| h.eq_ignore_ascii_case(candidate))
            {
                query_col_indices.push(idx);
                break;
            }
        }
    }

    if query_col_indices.is_empty() {
        query_col_indices = (0..headers.len()).collect();
    }

    let postcode_res = resolve_csv_param(
        form_postcode.or(query_postcode),
        &headers,
        &["postcode", "code_postal", "cp"],
    );
    let citycode_res = resolve_csv_param(
        form_citycode.or(query_citycode),
        &headers,
        &["citycode", "code_insee", "insee"],
    );
    let type_res = resolve_csv_param(form_type.or(query_type), &headers, &["type"]);
    let lat_res =
        resolve_csv_coord_param(form_lat.or(query_lat), &headers, &["lat", "latitude", "y"]);
    let lon_res = resolve_csv_coord_param(
        form_lon.or(query_lon),
        &headers,
        &["lon", "lng", "longitude", "x"],
    );

    let raw_result_columns = if !form_result_columns.is_empty() {
        form_result_columns
    } else {
        query_result_columns
    };

    let output_fields: Vec<(String, CsvResultField)> = if !raw_result_columns.is_empty() {
        raw_result_columns
            .into_iter()
            .map(|col| {
                let field = CsvResultField::parse(&col);
                (col, field)
            })
            .collect()
    } else {
        DEFAULT_SEARCH_CSV_COLUMNS
            .iter()
            .map(|&col| (col.to_string(), CsvResultField::parse(col)))
            .collect()
    };

    let mut out_headers = headers.clone();
    for (col_name, _) in &output_fields {
        out_headers.push_field(col_name);
    }

    let mut header_writer = csv::WriterBuilder::new()
        .delimiter(delimiter)
        .from_writer(Vec::new());

    if let Err(e) = header_writer.write_record(&out_headers) {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError::invalid_query(format!(
                "CSV serialization error: {}",
                e
            ))),
        )
            .into_response();
    }
    let _ = header_writer.flush();
    let header_bytes = bytes::Bytes::from(header_writer.into_inner().unwrap());

    let (tx, rx) = tokio::sync::mpsc::channel::<Result<bytes::Bytes, std::io::Error>>(16);
    let _ = tx.send(Ok(header_bytes)).await;

    let searcher = Arc::clone(&state.searcher);

    tokio::task::spawn(async move {
        let mut record_iter = reader.into_records();
        loop {
            let mut chunk_vec = Vec::with_capacity(256);
            for _ in 0..256 {
                match record_iter.next() {
                    Some(Ok(rec)) => chunk_vec.push(rec),
                    Some(Err(_)) => continue,
                    None => break,
                }
            }
            if chunk_vec.is_empty() {
                break;
            }

            let searcher_cloned = Arc::clone(&searcher);
            let query_cols = query_col_indices.clone();
            let out_fields = output_fields.clone();
            let pc_res = postcode_res.clone();
            let cc_res = citycode_res.clone();
            let tp_res = type_res.clone();
            let lt_res = lat_res.clone();
            let ln_res = lon_res.clone();

            let batch_bytes_res = tokio::task::spawn_blocking(move || {
                let rows: Vec<Vec<String>> = chunk_vec
                    .into_par_iter()
                    .map(|record| {
                        let q_parts: Vec<&str> = query_cols
                            .iter()
                            .filter_map(|&idx| record.get(idx))
                            .filter(|s| !s.trim().is_empty())
                            .collect();
                        let q_str = q_parts.join(" ");

                        let pc = pc_res
                            .col_idx
                            .and_then(|idx| record.get(idx))
                            .map(str::trim)
                            .filter(|s| !s.is_empty())
                            .map(ToString::to_string)
                            .or_else(|| pc_res.static_val.clone());

                        let cc = cc_res
                            .col_idx
                            .and_then(|idx| record.get(idx))
                            .map(str::trim)
                            .filter(|s| !s.is_empty())
                            .map(ToString::to_string)
                            .or_else(|| cc_res.static_val.clone());

                        let tp = tp_res
                            .col_idx
                            .and_then(|idx| record.get(idx))
                            .map(str::trim)
                            .filter(|s| !s.is_empty())
                            .map(ToString::to_string)
                            .or_else(|| tp_res.static_val.clone());

                        let lat_val = lt_res
                            .col_idx
                            .and_then(|idx| record.get(idx))
                            .and_then(|s| s.replace(',', ".").trim().parse::<f64>().ok())
                            .or(lt_res.static_val);

                        let lon_val = ln_res
                            .col_idx
                            .and_then(|idx| record.get(idx))
                            .and_then(|s| s.replace(',', ".").trim().parse::<f64>().ok())
                            .or(ln_res.static_val);

                        let mut params = SearchParams::new(&q_str).with_limit(2);
                        params.postcode = pc;
                        if let Some(c) = cc {
                            params.filters.insert("citycode".to_string(), c);
                        }
                        if let Some(t) = tp {
                            params.filters.insert("type".to_string(), t);
                        }
                        if let (Some(la), Some(lo)) = (lat_val, lon_val) {
                            params = params.with_geo_bias(la, lo);
                        }

                        let hits = searcher_cloned.search(&params).unwrap_or_default();
                        let hit = hits.first();
                        let next_hit = hits.get(1);

                        let mut out_row: Vec<String> =
                            record.iter().map(ToString::to_string).collect();
                        for (_, field) in &out_fields {
                            out_row.push(extract_search_hit_fields(hit, next_hit, *field));
                        }
                        out_row
                    })
                    .collect();

                let mut writer = csv::WriterBuilder::new()
                    .delimiter(delimiter)
                    .from_writer(Vec::new());

                for row in rows {
                    writer.write_record(&row).map_err(std::io::Error::other)?;
                }
                writer.flush().map_err(std::io::Error::other)?;
                Ok::<bytes::Bytes, std::io::Error>(bytes::Bytes::from(writer.into_inner().unwrap()))
            })
            .await;

            match batch_bytes_res {
                Ok(Ok(bytes)) => {
                    if tx.send(Ok(bytes)).await.is_err() {
                        break;
                    }
                }
                Ok(Err(e)) => {
                    let _ = tx.send(Err(e)).await;
                    break;
                }
                Err(e) => {
                    let _ = tx.send(Err(std::io::Error::other(e))).await;
                    break;
                }
            }
        }
    });

    let body = Body::from_stream(ReceiverStream::new(rx));
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "text/csv; charset=utf-8")
        .header(
            header::CONTENT_DISPOSITION,
            "attachment; filename=\"geocoded.csv\"",
        )
        .header(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
        .header(
            header::ACCESS_CONTROL_ALLOW_HEADERS,
            "X-Requested-With, Content-Type, Accept",
        )
        .body(body)
        .unwrap()
}

/// High-Performance Streaming CSV Batch Reverse Geocoding: `POST /reverse/csv/`
async fn reverse_csv_handler(
    State(state): State<AppState>,
    RawQuery(raw_query): RawQuery,
    mut multipart: Multipart,
) -> Response {
    CSV_REVERSE_REQUESTS.fetch_add(1, Ordering::Relaxed);
    let mut query_result_columns: Vec<String> = Vec::new();
    let mut query_type: Option<String> = None;
    let mut query_postcode: Option<String> = None;
    let mut query_citycode: Option<String> = None;
    let mut query_lat: Option<String> = None;
    let mut query_lon: Option<String> = None;
    let mut query_encoding: Option<String> = None;
    let mut query_delimiter: Option<u8> = None;

    if let Some(q_str) = raw_query {
        for (k, v) in form_urlencoded::parse(q_str.as_bytes()) {
            let key = k.trim().to_lowercase();
            let val = v.trim();
            if val.is_empty() {
                continue;
            }
            let norm_key = if let Some(inner) = key
                .strip_prefix("filter[")
                .and_then(|k| k.strip_suffix(']'))
            {
                inner.trim()
            } else {
                key.as_str()
            };
            match norm_key {
                "result_columns" => {
                    for c in val.split(',') {
                        let trimmed = c.trim();
                        if !trimmed.is_empty() {
                            query_result_columns.push(trimmed.to_string());
                        }
                    }
                }
                "type" => query_type = Some(val.to_string()),
                "postcode" | "code_postal" | "cp" => query_postcode = Some(val.to_string()),
                "citycode" | "code_insee" | "insee" => query_citycode = Some(val.to_string()),
                "lat" | "latitude" => query_lat = Some(val.to_string()),
                "lon" | "lng" | "longitude" => query_lon = Some(val.to_string()),
                "encoding" => query_encoding = Some(val.to_string()),
                "delimiter" => {
                    if let Some(b) = val.bytes().next() {
                        query_delimiter = Some(b);
                    }
                }
                _ => {}
            }
        }
    }

    let mut csv_bytes: Option<Vec<u8>> = None;
    let mut form_result_columns: Vec<String> = Vec::new();
    let mut form_type: Option<String> = None;
    let mut form_postcode: Option<String> = None;
    let mut form_citycode: Option<String> = None;
    let mut form_lat: Option<String> = None;
    let mut form_lon: Option<String> = None;
    let mut form_encoding: Option<String> = None;
    let mut form_delimiter: Option<u8> = None;

    while let Ok(Some(field)) = multipart.next_field().await {
        let name = field.name().unwrap_or("").to_string();
        let norm_name = if let Some(inner) = name
            .strip_prefix("filter[")
            .and_then(|k| k.strip_suffix(']'))
        {
            inner.trim()
        } else {
            name.as_str()
        };

        if norm_name == "data" || field.file_name().is_some() {
            match field.bytes().await {
                Ok(bytes) => csv_bytes = Some(bytes.to_vec()),
                Err(e) => {
                    return (
                        StatusCode::BAD_REQUEST,
                        Json(ApiError::invalid_query(format!(
                            "Failed to read CSV payload: {}",
                            e
                        ))),
                    )
                        .into_response();
                }
            }
        } else if norm_name == "result_columns" {
            if let Ok(text) = field.text().await {
                for c in text.split(',') {
                    let trimmed = c.trim();
                    if !trimmed.is_empty() {
                        form_result_columns.push(trimmed.to_string());
                    }
                }
            }
        } else if norm_name == "type" {
            if let Ok(text) = field.text().await {
                form_type = Some(text.trim().to_string());
            }
        } else if norm_name == "postcode" || norm_name == "code_postal" || norm_name == "cp" {
            if let Ok(text) = field.text().await {
                form_postcode = Some(text.trim().to_string());
            }
        } else if norm_name == "citycode" || norm_name == "code_insee" || norm_name == "insee" {
            if let Ok(text) = field.text().await {
                form_citycode = Some(text.trim().to_string());
            }
        } else if norm_name == "lat" || norm_name == "latitude" {
            if let Ok(text) = field.text().await {
                form_lat = Some(text.trim().to_string());
            }
        } else if norm_name == "lon" || norm_name == "lng" || norm_name == "longitude" {
            if let Ok(text) = field.text().await {
                form_lon = Some(text.trim().to_string());
            }
        } else if norm_name == "encoding" {
            if let Ok(text) = field.text().await {
                form_encoding = Some(text.trim().to_string());
            }
        } else if norm_name == "delimiter" {
            if let Ok(text) = field.text().await {
                if let Some(b) = text.trim().bytes().next() {
                    form_delimiter = Some(b);
                }
            }
        }
    }

    let raw_data = match csv_bytes {
        Some(b) if !b.is_empty() => b,
        _ => {
            return (
                StatusCode::BAD_REQUEST,
                Json(ApiError::invalid_query("Missing CSV file in 'data' field")),
            )
                .into_response();
        }
    };

    let encoding = form_encoding.or(query_encoding);
    let clean_data = decode_csv_bytes(&raw_data, encoding.as_deref());

    let delimiter = form_delimiter
        .or(query_delimiter)
        .unwrap_or_else(|| detect_delimiter_from_bytes(&clean_data));

    let mut reader = csv::ReaderBuilder::new()
        .delimiter(delimiter)
        .flexible(true)
        .has_headers(true)
        .from_reader(std::io::Cursor::new(clean_data));

    let headers = match reader.headers() {
        Ok(h) => h.clone(),
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(ApiError::invalid_query(format!(
                    "Failed to parse CSV headers: {}",
                    e
                ))),
            )
                .into_response();
        }
    };

    let lat_res =
        resolve_csv_coord_param(form_lat.or(query_lat), &headers, &["lat", "latitude", "y"]);
    let lon_res = resolve_csv_coord_param(
        form_lon.or(query_lon),
        &headers,
        &["lon", "lng", "longitude", "x"],
    );

    if (lat_res.col_idx.is_none() && lat_res.static_val.is_none())
        || (lon_res.col_idx.is_none() && lon_res.static_val.is_none())
    {
        return (
            StatusCode::BAD_REQUEST,
            Json(ApiError::invalid_query(
                "Cannot identify latitude and longitude columns in CSV",
            )),
        )
            .into_response();
    }

    let type_res = resolve_csv_param(form_type.or(query_type), &headers, &["type"]);
    let postcode_res = resolve_csv_param(
        form_postcode.or(query_postcode),
        &headers,
        &["postcode", "code_postal", "cp"],
    );
    let citycode_res = resolve_csv_param(
        form_citycode.or(query_citycode),
        &headers,
        &["citycode", "code_insee", "insee"],
    );

    let raw_result_columns = if !form_result_columns.is_empty() {
        form_result_columns
    } else {
        query_result_columns
    };

    let output_fields: Vec<(String, CsvResultField)> = if !raw_result_columns.is_empty() {
        raw_result_columns
            .into_iter()
            .map(|col| {
                let field = CsvResultField::parse(&col);
                (col, field)
            })
            .collect()
    } else {
        DEFAULT_REVERSE_CSV_COLUMNS
            .iter()
            .map(|&col| (col.to_string(), CsvResultField::parse(col)))
            .collect()
    };

    let mut out_headers = headers.clone();
    for (col_name, _) in &output_fields {
        out_headers.push_field(col_name);
    }

    let mut header_writer = csv::WriterBuilder::new()
        .delimiter(delimiter)
        .from_writer(Vec::new());

    if let Err(e) = header_writer.write_record(&out_headers) {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError::invalid_query(format!(
                "CSV serialization error: {}",
                e
            ))),
        )
            .into_response();
    }
    let _ = header_writer.flush();
    let header_bytes = bytes::Bytes::from(header_writer.into_inner().unwrap());

    let (tx, rx) = tokio::sync::mpsc::channel::<Result<bytes::Bytes, std::io::Error>>(16);
    let _ = tx.send(Ok(header_bytes)).await;

    let searcher = Arc::clone(&state.searcher);

    tokio::task::spawn(async move {
        let mut record_iter = reader.into_records();
        loop {
            let mut chunk_vec = Vec::with_capacity(256);
            for _ in 0..256 {
                match record_iter.next() {
                    Some(Ok(rec)) => chunk_vec.push(rec),
                    Some(Err(_)) => continue,
                    None => break,
                }
            }
            if chunk_vec.is_empty() {
                break;
            }

            let searcher_cloned = Arc::clone(&searcher);
            let out_fields = output_fields.clone();
            let lt_res = lat_res.clone();
            let ln_res = lon_res.clone();
            let tp_res = type_res.clone();
            let pc_res = postcode_res.clone();
            let cc_res = citycode_res.clone();

            let batch_bytes_res = tokio::task::spawn_blocking(move || {
                let rows: Vec<Vec<String>> = chunk_vec
                    .into_par_iter()
                    .map(|record| {
                        let lat_opt = lt_res
                            .col_idx
                            .and_then(|idx| record.get(idx))
                            .and_then(|s| s.replace(',', ".").trim().parse::<f64>().ok())
                            .or(lt_res.static_val);

                        let lon_opt = ln_res
                            .col_idx
                            .and_then(|idx| record.get(idx))
                            .and_then(|s| s.replace(',', ".").trim().parse::<f64>().ok())
                            .or(ln_res.static_val);

                        let type_opt = tp_res
                            .col_idx
                            .and_then(|idx| record.get(idx))
                            .map(str::trim)
                            .filter(|s| !s.is_empty())
                            .map(ToString::to_string)
                            .or_else(|| tp_res.static_val.clone());

                        let postcode_opt = pc_res
                            .col_idx
                            .and_then(|idx| record.get(idx))
                            .map(str::trim)
                            .filter(|s| !s.is_empty())
                            .map(ToString::to_string)
                            .or_else(|| pc_res.static_val.clone());

                        let citycode_opt = cc_res
                            .col_idx
                            .and_then(|idx| record.get(idx))
                            .map(str::trim)
                            .filter(|s| !s.is_empty())
                            .map(ToString::to_string)
                            .or_else(|| cc_res.static_val.clone());

                        let mut out_row: Vec<String> =
                            record.iter().map(ToString::to_string).collect();

                        let spatial_res = match (lat_opt, lon_opt) {
                            (Some(la), Some(lo)) => {
                                let type_filters: Option<Vec<String>> = type_opt.map(|t| {
                                    t.split(['+', ',', ' '])
                                        .map(|s| s.trim().to_string())
                                        .filter(|s| !s.is_empty())
                                        .collect()
                                });
                                let postcode_filters: Option<Vec<String>> = postcode_opt.map(|p| {
                                    p.split(['+', ',', ' '])
                                        .map(|s| s.trim().to_string())
                                        .filter(|s| !s.is_empty())
                                        .collect()
                                });
                                let citycode_filters: Option<Vec<String>> = citycode_opt.map(|c| {
                                    c.split(['+', ',', ' '])
                                        .map(|s| s.trim().to_string())
                                        .filter(|s| !s.is_empty())
                                        .collect()
                                });

                                searcher_cloned
                                    .reverse_with_filters(
                                        la,
                                        lo,
                                        1,
                                        type_filters.as_deref(),
                                        postcode_filters.as_deref(),
                                        citycode_filters.as_deref(),
                                    )
                                    .into_iter()
                                    .next()
                            }
                            _ => None,
                        };

                        for (_, field) in &out_fields {
                            out_row.push(extract_spatial_result_field(
                                spatial_res.as_ref(),
                                &searcher_cloned.doc_store,
                                *field,
                            ));
                        }
                        out_row
                    })
                    .collect();

                let mut writer = csv::WriterBuilder::new()
                    .delimiter(delimiter)
                    .from_writer(Vec::new());

                for row in rows {
                    writer.write_record(&row).map_err(std::io::Error::other)?;
                }
                writer.flush().map_err(std::io::Error::other)?;
                Ok::<bytes::Bytes, std::io::Error>(bytes::Bytes::from(writer.into_inner().unwrap()))
            })
            .await;

            match batch_bytes_res {
                Ok(Ok(bytes)) => {
                    if tx.send(Ok(bytes)).await.is_err() {
                        break;
                    }
                }
                Ok(Err(e)) => {
                    let _ = tx.send(Err(e)).await;
                    break;
                }
                Err(e) => {
                    let _ = tx.send(Err(std::io::Error::other(e))).await;
                    break;
                }
            }
        }
    });

    let body = Body::from_stream(ReceiverStream::new(rx));
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "text/csv; charset=utf-8")
        .header(
            header::CONTENT_DISPOSITION,
            "attachment; filename=\"geocoded.csv\"",
        )
        .header(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
        .header(
            header::ACCESS_CONTROL_ALLOW_HEADERS,
            "X-Requested-With, Content-Type, Accept",
        )
        .body(body)
        .unwrap()
}

/// `/metrics` Prometheus metrics endpoint
async fn metrics_handler() -> impl IntoResponse {
    let search_total = SEARCH_REQUESTS.load(Ordering::Relaxed);
    let reverse_total = REVERSE_REQUESTS.load(Ordering::Relaxed);
    let csv_search_total = CSV_SEARCH_REQUESTS.load(Ordering::Relaxed);
    let csv_reverse_total = CSV_REVERSE_REQUESTS.load(Ordering::Relaxed);

    let body = format!(
        "# HELP addok_search_requests_total Total number of /search requests\n\
         # TYPE addok_search_requests_total counter\n\
         addok_search_requests_total {}\n\n\
         # HELP addok_reverse_requests_total Total number of /reverse requests\n\
         # TYPE addok_reverse_requests_total counter\n\
         addok_reverse_requests_total {}\n\n\
         # HELP addok_csv_search_requests_total Total number of /search/csv requests\n\
         # TYPE addok_csv_search_requests_total counter\n\
         addok_csv_search_requests_total {}\n\n\
         # HELP addok_csv_reverse_requests_total Total number of /reverse/csv requests\n\
         # TYPE addok_csv_reverse_requests_total counter\n\
         addok_csv_reverse_requests_total {}\n",
        search_total, reverse_total, csv_search_total, csv_reverse_total
    );

    Response::builder()
        .status(StatusCode::OK)
        .header(
            header::CONTENT_TYPE,
            "text/plain; version=0.0.4; charset=utf-8",
        )
        .header(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
        .body(Body::from(body))
        .unwrap()
}

/// `/health` health check loyal to Addok
async fn health_handler() -> impl IntoResponse {
    Json(serde_json::json!({
        "status": "HEALTHY",
        "redis_version": "7.4.2"
    }))
}

/// Launch the HTTP server
pub async fn run_server(searcher: AddokSearcher, addr: SocketAddr) -> Result<()> {
    let state = AppState {
        searcher: Arc::new(searcher),
    };
    let app = create_router(state);

    info!("addok-rs REST API listening on http://{}", addr);
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}
