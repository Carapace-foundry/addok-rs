use std::collections::HashMap;
use std::path::Path;

use addok_core::config::AddokConfig;
use addok_core::document::Document;
use addok_core::BanRecord;
use tantivy::collector::TopDocs;
use tantivy::query::{BooleanQuery, BoostQuery, Occur, Query, QueryParser, TermQuery};
use tantivy::schema::{IndexRecordOption, Term, Value};
use tantivy::{Index, IndexReader, ReloadPolicy, TantivyDocument};

use crate::collector::BucketCollector;
use crate::doc_store::DocumentStore;
use crate::error::{IndexError, Result};
use crate::fields_index::FieldsInvertedIndex;
use crate::filters::FiltersIndex;
use crate::housenumbers::HousenumbersIndex;
use crate::ngram::EdgeNgramIndex;
use crate::pairs::PairsIndexer;
use crate::query::ParsedQuery;
use crate::schema::AddokSchema;
use crate::scoring::{km_to_score, score_candidate};
use crate::spatial::{haversine_distance_meters, SpatialIndex, SpatialItem, SpatialResult};

/// Query parameters for text search
#[derive(Debug, Clone)]
pub struct SearchParams {
    pub query: String,
    pub limit: usize,
    /// Optional latitude for geographic proximity bias
    pub lat: Option<f64>,
    /// Optional longitude for geographic proximity bias
    pub lon: Option<f64>,
    /// Optional postal code filter
    pub postcode: Option<String>,
    /// Autocomplete mode (enables prefix matching on the last word)
    pub autocomplete: bool,
    /// Multi-value filters: category -> value (e.g. "type" -> "street+city")
    pub filters: HashMap<String, String>,
}

impl SearchParams {
    pub fn new(query: impl Into<String>) -> Self {
        Self {
            query: query.into(),
            limit: 5,
            lat: None,
            lon: None,
            postcode: None,
            autocomplete: true,
            filters: HashMap::new(),
        }
    }

    pub fn with_limit(mut self, limit: usize) -> Self {
        self.limit = limit;
        self
    }

    pub fn with_geo_bias(mut self, lat: f64, lon: f64) -> Self {
        self.lat = Some(lat);
        self.lon = Some(lon);
        self
    }

    pub fn with_filter(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.filters.insert(key.into(), value.into());
        self
    }
}

/// A scored search result item
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SearchHit {
    pub record: BanRecord,
    pub score: f32,
    pub distance_meters: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub document: Option<Document>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub matched_housenumber: Option<String>,
}

/// Diagnostic breakdown of token frequency
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TokenFrequencyInfo {
    pub token: String,
    pub frequency: usize,
    pub is_common: bool,
}

/// Diagnostic of fuzzy variants found in index
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FuzzyVariantInfo {
    pub original_token: String,
    pub matched_variant: String,
    pub variant_frequency: usize,
}

/// Score decomposition for a candidate address
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ExplainCandidate {
    pub id: String,
    pub label: String,
    pub doc_type: Option<String>,
    pub postcode: Option<String>,
    pub city: Option<String>,
    pub string_distance: f32,
    pub token_coverage: f32,
    pub text_score: f32,
    pub importance: f32,
    pub importance_factor: f32,
    pub matched_housenumber: Option<String>,
    pub housenumber_factor: f32,
    pub distance_meters: Option<f64>,
    pub geo_score: Option<f32>,
    pub final_score: f32,
}

/// Full step-by-step diagnostic of search execution
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ExplainResult {
    pub raw_query: String,
    pub normalized_query: String,
    pub clean_query: String,
    pub extracted_housenumber: Option<String>,
    pub extracted_postcode: Option<String>,
    pub autocomplete_prefix: Option<String>,
    pub street_tokens: Vec<String>,
    pub meaningful_tokens: Vec<String>,
    pub sorted_tokens: Vec<String>,
    pub token_frequencies: Vec<TokenFrequencyInfo>,
    pub phase1_active_tokens: Vec<String>,
    pub phase1_intersections: Vec<(String, usize)>,
    pub phase1_count: usize,
    pub phase2_prefix: Option<String>,
    pub phase2_ngram_candidates: Vec<String>,
    pub phase2_pair_filtered_candidates: Vec<String>,
    pub phase2_added_count: usize,
    pub phase3_triggered: bool,
    pub phase3_fuzzy_variants: Vec<FuzzyVariantInfo>,
    pub phase3_added_count: usize,
    pub phase4_filters_applied: HashMap<String, String>,
    pub total_candidates: usize,
    pub candidates: Vec<ExplainCandidate>,
}

/// Zero-copy memory-mapped searcher for Addok's multi-index architecture:
/// - Fields Inverted Index (Tokens -> Doc IDs with field boosts)
/// - Edge N-Gram Index (Prefixes -> Full tokens for instant autocompletion)
/// - Pairs Index (`PairsIndexer` for candidate pruning and co-occurrence filtering)
/// - Housenumbers Index (Fast coordinate lookup for exact house numbers)
/// - Filters Index (Roaring Bitmaps for multi-value filtering)
/// - Spatial Index (R-Tree for proximity re-ranking and reverse geocoding)
/// - Tantivy Index (full-text index fallback & verification)
pub struct AddokSearcher {
    pub schema: AddokSchema,
    pub reader: Option<IndexReader>,
    pub query_parser: Option<QueryParser>,
    pub fields_index: FieldsInvertedIndex,
    pub ngram_index: EdgeNgramIndex,
    pub pairs_index: PairsIndexer,
    pub housenumbers_index: HousenumbersIndex,
    pub filters_index: FiltersIndex,
    pub doc_store: DocumentStore,
    pub spatial_index: Option<SpatialIndex>,
    pub config: AddokConfig,
}

impl AddokSearcher {
    /// Open an index from directory
    pub fn open<P: AsRef<Path>>(index_dir: P) -> Result<Self> {
        let path = index_dir.as_ref();
        if !path.exists() {
            return Err(IndexError::InvalidDirectory(format!(
                "Path does not exist: {}",
                path.display()
            )));
        }

        let config = AddokConfig::load().unwrap_or_default();
        Self::open_with_config(path, config)
    }

    /// Open an index from directory with explicit Addok configuration
    pub fn open_with_config<P: AsRef<Path>>(index_dir: P, config: AddokConfig) -> Result<Self> {
        let path = index_dir.as_ref();
        if !path.exists() {
            return Err(IndexError::InvalidDirectory(format!(
                "Path does not exist: {}",
                path.display()
            )));
        }

        let schema = AddokSchema::new();

        // 1. Try opening Tantivy index
        let (reader, query_parser) = if let Ok(index) = Index::open_in_dir(path) {
            let r: std::result::Result<IndexReader, _> = index
                .reader_builder()
                .reload_policy(ReloadPolicy::OnCommitWithDelay)
                .try_into();

            if let Ok(r) = r {
                let mut qp = QueryParser::for_index(
                    &index,
                    vec![
                        schema.text,
                        schema.street,
                        schema.city,
                        schema.postcode,
                        schema.housenumber,
                    ],
                );
                qp.set_field_boost(schema.text, 2.0);
                qp.set_field_boost(schema.street, 1.5);
                qp.set_field_boost(schema.city, 1.2);
                qp.set_field_boost(schema.housenumber, 1.8);
                qp.set_field_boost(schema.postcode, 1.5);
                (Some(r), Some(qp))
            } else {
                (None, None)
            }
        } else {
            (None, None)
        };

        // 2. Load multi-index binary tables
        let fields_path = path.join("fields.bin");
        let fields_index = if fields_path.exists() {
            FieldsInvertedIndex::load_from_file(&fields_path).unwrap_or_default()
        } else {
            FieldsInvertedIndex::default()
        };

        let ngram_path = path.join("ngram.bin");
        let ngram_index = if ngram_path.exists() {
            EdgeNgramIndex::load_from_file(&ngram_path).unwrap_or_default()
        } else {
            EdgeNgramIndex::default()
        };

        let pairs_path = path.join("pairs.bin");
        let pairs_index = if pairs_path.exists() {
            PairsIndexer::load_from_file(&pairs_path).unwrap_or_default()
        } else {
            PairsIndexer::new()
        };

        let hn_path = path.join("housenumbers.bin");
        let housenumbers_index = if hn_path.exists() {
            HousenumbersIndex::load_from_file(&hn_path).unwrap_or_default()
        } else {
            HousenumbersIndex::new()
        };

        let filters_path = path.join("filters.bin");
        let filters_index = if filters_path.exists() {
            FiltersIndex::load_from_file(&filters_path).unwrap_or_default()
        } else {
            FiltersIndex::new()
        };

        let docs_path = path.join("docs.bin");
        let doc_store = if docs_path.exists() {
            DocumentStore::load_from_file(&docs_path).unwrap_or_default()
        } else {
            DocumentStore::new()
        };

        // 3. Load spatial index if it exists (try binary first, then json)
        let spatial_bin_path = path.join("spatial.bin");
        let spatial_json_path = path.join("spatial.json");
        let spatial_index = if spatial_bin_path.exists() {
            SpatialIndex::load_from_bin(&spatial_bin_path).ok()
        } else {
            None
        }
        .or_else(|| {
            if spatial_json_path.exists() {
                SpatialIndex::load_from_file(&spatial_json_path).ok()
            } else {
                None
            }
        });

        Ok(Self {
            schema,
            reader,
            query_parser,
            fields_index,
            ngram_index,
            pairs_index,
            housenumbers_index,
            filters_index,
            doc_store,
            spatial_index,
            config,
        })
    }

    /// Provide a customized configuration
    pub fn with_config(mut self, config: AddokConfig) -> Self {
        self.config = config;
        self
    }

    /// Perform text geocoding search using Addok's candidate collection & scoring pipeline
    pub fn search(&self, params: &SearchParams) -> Result<Vec<SearchHit>> {
        // 1. Query parsing & preprocessing pipeline
        let parsed_query = ParsedQuery::parse(&params.query, &self.config, |t| {
            self.fields_index
                .get_postings(t)
                .map_or(0, |bm| bm.len() as usize)
        });

        if parsed_query.normalized.trim().is_empty() {
            return Ok(Vec::new());
        }

        // 2. Prepare filters: combine params.filters with params.postcode
        let mut filters = params.filters.clone();
        if let Some(ref pc) = params.postcode {
            filters
                .entry("postcode".to_string())
                .and_modify(|existing| {
                    if !existing
                        .split(['+', ',', ' '])
                        .any(|v| v.trim() == pc.trim())
                    {
                        existing.push('+');
                        existing.push_str(pc.trim());
                    }
                })
                .or_insert_with(|| pc.clone());
        }

        // 3. Candidate Bucket Collection (Steps 1 to 4)
        let collector = BucketCollector::new(
            &self.fields_index,
            &self.ngram_index,
            &self.pairs_index,
            &self.filters_index,
            &self.config,
        );

        let candidate_doc_ids = collector.collect(&parsed_query, params.autocomplete, &filters);

        // 4. Scoring using the exact Addok scoring formula
        let mut hits = Vec::new();

        if !candidate_doc_ids.is_empty() && !self.doc_store.is_empty() {
            for doc_id in candidate_doc_ids.iter() {
                if let Some(doc) = self.doc_store.get(doc_id) {
                    if let Some(hit) = score_candidate(
                        doc,
                        &parsed_query,
                        &self.housenumbers_index,
                        &self.config,
                        params.lat,
                        params.lon,
                        params.autocomplete,
                    ) {
                        hits.push(hit);
                    }
                }
            }
        }

        // 5. Tantivy fallback if multi-index returned 0 hits or is empty
        if hits.is_empty() {
            if let (Some(ref reader), Some(ref _parser)) = (&self.reader, &self.query_parser) {
                hits = self.search_tantivy(
                    reader,
                    &parsed_query.normalized,
                    params,
                    parsed_query.extracted_housenumber.as_deref(),
                )?;
            }
        }

        // 6. Rank descending by score and truncate to limit
        hits.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        hits.truncate(params.limit);

        Ok(hits)
    }

    /// Perform reverse geocoding to find nearest addresses from coordinates
    pub fn reverse(&self, lat: f64, lon: f64, limit: usize) -> Vec<SpatialResult> {
        self.reverse_filtered(lat, lon, limit, None)
    }

    /// Perform reverse geocoding to find nearest addresses from coordinates with optional type filtering
    pub fn reverse_filtered(
        &self,
        lat: f64,
        lon: f64,
        limit: usize,
        type_filter: Option<&[String]>,
    ) -> Vec<SpatialResult> {
        self.reverse_with_filters(lat, lon, limit, type_filter, None, None)
    }

    /// Perform reverse geocoding to find nearest addresses from coordinates with optional filters (type, postcode, citycode)
    pub fn reverse_with_filters(
        &self,
        lat: f64,
        lon: f64,
        limit: usize,
        type_filter: Option<&[String]>,
        postcode_filter: Option<&[String]>,
        citycode_filter: Option<&[String]>,
    ) -> Vec<SpatialResult> {
        let mut results = match &self.spatial_index {
            Some(spatial) => spatial.nearest_neighbors_with_filters(
                lat,
                lon,
                limit,
                type_filter,
                postcode_filter,
                citycode_filter,
            ),
            None => Vec::new(),
        };

        // If filtering for street and no street items were directly found in the spatial R-tree
        // (e.g. index contains only housenumber points), resolve parent streets from nearby housenumbers
        if results.is_empty() {
            if let Some(types) = type_filter {
                if types.iter().any(|t| t.eq_ignore_ascii_case("street")) {
                    if let Some(spatial) = &self.spatial_index {
                        let raw_items = spatial.nearest_neighbors_with_filters(
                            lat,
                            lon,
                            limit * 10,
                            None,
                            postcode_filter,
                            citycode_filter,
                        );
                        let mut seen_street_ids = std::collections::HashSet::new();
                        for item in raw_items {
                            let street_id = if let Some(idx) = item.item.id.rfind('_') {
                                &item.item.id[..idx]
                            } else {
                                &item.item.id
                            };
                            if seen_street_ids.insert(street_id.to_string()) {
                                if let Some(street_doc) = self.doc_store.get_by_id(street_id) {
                                    results.push(SpatialResult {
                                        item: SpatialItem {
                                            point: item.item.point,
                                            id: street_doc.id.clone(),
                                            label: street_doc.label(None),
                                            ban_type: "street".to_string(),
                                            postcode: street_doc
                                                .postcode
                                                .clone()
                                                .unwrap_or_default(),
                                            city: street_doc.city.clone().unwrap_or_default(),
                                            housenumber: None,
                                            citycode: street_doc.citycode.clone(),
                                        },
                                        distance_meters: item.distance_meters,
                                    });
                                    if results.len() >= limit {
                                        break;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        results
    }

    /// Explain the search process step-by-step with complete diagnostic details
    pub fn explain(&self, params: &SearchParams) -> Result<ExplainResult> {
        let parsed_query = ParsedQuery::parse(&params.query, &self.config, |t| {
            self.fields_index
                .get_postings(t)
                .map_or(0, |bm| bm.len() as usize)
        });

        // 2. Prepare filters: combine params.filters with params.postcode
        let mut filters = params.filters.clone();
        if let Some(ref pc) = params.postcode {
            filters
                .entry("postcode".to_string())
                .and_modify(|existing| {
                    if !existing
                        .split(['+', ',', ' '])
                        .any(|v| v.trim() == pc.trim())
                    {
                        existing.push('+');
                        existing.push_str(pc.trim());
                    }
                })
                .or_insert_with(|| pc.clone());
        }

        let collector = BucketCollector::new(
            &self.fields_index,
            &self.ngram_index,
            &self.pairs_index,
            &self.filters_index,
            &self.config,
        );

        let (bucket, trace) =
            collector.collect_with_trace(&parsed_query, params.autocomplete, &filters);
        let total_candidates = bucket.len() as usize;

        // 3. Score candidate docs
        let mut candidates = Vec::new();
        if !bucket.is_empty() && !self.doc_store.is_empty() {
            for doc_id in bucket.iter() {
                if let Some(doc) = self.doc_store.get(doc_id) {
                    if let Some(candidate_detail) = self.decompose_score(
                        doc,
                        &parsed_query,
                        params.lat,
                        params.lon,
                        params.autocomplete,
                    ) {
                        candidates.push(candidate_detail);
                    }
                }
            }
        }

        candidates.sort_by(|a, b| {
            b.final_score
                .partial_cmp(&a.final_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        candidates.truncate(params.limit);

        let token_frequencies = trace
            .token_frequencies
            .into_iter()
            .map(|(token, frequency, is_common)| TokenFrequencyInfo {
                token,
                frequency,
                is_common,
            })
            .collect();

        let phase3_fuzzy_variants = trace
            .phase3_fuzzy_variants
            .into_iter()
            .map(
                |(original_token, matched_variant, variant_frequency)| FuzzyVariantInfo {
                    original_token,
                    matched_variant,
                    variant_frequency,
                },
            )
            .collect();

        Ok(ExplainResult {
            raw_query: parsed_query.raw.clone(),
            normalized_query: parsed_query.normalized.clone(),
            clean_query: parsed_query.clean_query.clone(),
            extracted_housenumber: parsed_query.extracted_housenumber.clone(),
            extracted_postcode: parsed_query.extracted_postcode.clone(),
            autocomplete_prefix: parsed_query.autocomplete_prefix.clone(),
            street_tokens: parsed_query.street_tokens.clone(),
            meaningful_tokens: parsed_query.meaningful_tokens.clone(),
            sorted_tokens: parsed_query.sorted_tokens.clone(),
            token_frequencies,
            phase1_active_tokens: trace.phase1_active_tokens,
            phase1_intersections: trace.phase1_intersections,
            phase1_count: trace.phase1_count,
            phase2_prefix: trace.phase2_prefix,
            phase2_ngram_candidates: trace.phase2_ngram_candidates,
            phase2_pair_filtered_candidates: trace.phase2_pair_filtered_candidates,
            phase2_added_count: trace.phase2_added_count,
            phase3_triggered: trace.phase3_triggered,
            phase3_fuzzy_variants,
            phase3_added_count: trace.phase3_added_count,
            phase4_filters_applied: trace.phase4_filters_applied,
            total_candidates,
            candidates,
        })
    }

    fn decompose_score(
        &self,
        doc: &Document,
        parsed_query: &ParsedQuery,
        lat: Option<f64>,
        lon: Option<f64>,
        autocomplete: bool,
    ) -> Option<ExplainCandidate> {
        use crate::scoring::{compute_str_score, extract_sorted_bigrams, token_coverage_ratio};
        use addok_core::normalizer::{ascii_fold, normalize_address, tokenize_address};

        let hit = score_candidate(
            doc,
            parsed_query,
            &self.housenumbers_index,
            &self.config,
            lat,
            lon,
            autocomplete,
        )?;

        let has_france = self.config.has_france();
        let labels = doc.make_labels(hit.matched_housenumber.as_deref(), has_france);

        let str_score_raw = compute_str_score(
            &labels,
            &parsed_query.ascii_query,
            &parsed_query.query_bigrams,
            autocomplete,
            self.config.match_threshold,
        );

        let string_distance = if parsed_query.normalized != parsed_query.raw && str_score_raw < 1.0
        {
            let norm_ascii = ascii_fold(&parsed_query.normalized);
            let norm_bigrams = extract_sorted_bigrams(&norm_ascii);
            let str_score_norm = compute_str_score(
                &labels,
                &norm_ascii,
                &norm_bigrams,
                autocomplete,
                self.config.match_threshold,
            );
            str_score_raw.max(str_score_norm)
        } else {
            str_score_raw
        };

        let norm_doc_name = normalize_address(&doc.name, has_france);
        let mut doc_tokens: std::collections::HashSet<String> =
            tokenize_address(&norm_doc_name, false, has_france)
                .into_iter()
                .collect();
        if let Some(ref c) = doc.city {
            doc_tokens.extend(tokenize_address(
                &normalize_address(c, has_france),
                false,
                has_france,
            ));
        }
        if let Some(ref pc) = doc.postcode {
            doc_tokens.insert(pc.trim().to_lowercase());
        }
        let token_cov = token_coverage_ratio(&parsed_query.meaningful_tokens, &doc_tokens);

        let geo_score = hit.distance_meters.map(|d| {
            let dist_km = d / 1000.0;
            (km_to_score(dist_km) as f32) * 0.1
        });

        Some(ExplainCandidate {
            id: doc.id.clone(),
            label: labels
                .first()
                .cloned()
                .unwrap_or_else(|| doc.label(hit.matched_housenumber.as_deref())),
            doc_type: doc.address_type.clone(),
            postcode: doc.postcode.clone(),
            city: doc.city.clone(),
            string_distance,
            token_coverage: token_cov,
            text_score: string_distance,
            importance: doc.importance,
            importance_factor: doc.importance.clamp(0.0, 1.0) * 0.1,
            matched_housenumber: hit.matched_housenumber,
            housenumber_factor: 1.0,
            distance_meters: hit.distance_meters,
            geo_score,
            final_score: hit.score,
        })
    }

    fn search_tantivy(
        &self,
        reader: &IndexReader,
        normalized_query: &str,
        params: &SearchParams,
        detected_hn: Option<&str>,
    ) -> Result<Vec<SearchHit>> {
        let searcher = reader.searcher();
        let query = self.build_query(normalized_query, params)?;

        let fetch_limit = if params.lat.is_some() && params.lon.is_some() {
            params.limit.max(20)
        } else {
            params.limit
        };

        let top_docs = searcher.search(&query, &TopDocs::with_limit(fetch_limit).order_by_score())?;
        if top_docs.is_empty() {
            return Ok(Vec::new());
        }

        let max_raw_score = top_docs
            .first()
            .map(|(score, _)| *score)
            .unwrap_or(1.0)
            .max(1.0);

        let mut hits = Vec::with_capacity(top_docs.len());

        for (score, doc_address) in top_docs {
            let doc: TantivyDocument = searcher.doc(doc_address)?;
            if let Some(mut record) = self.extract_record(&doc) {
                let mut matched_hn_str: Option<String> = None;

                if let Some(hn) = detected_hn {
                    if let Some(hn_data) = self.housenumbers_index.get_coordinates(&record.id, hn) {
                        record.numero = Some(hn.to_string());
                        if let (Some(la), Some(lo)) = (hn_data.lat, hn_data.lon) {
                            record.lat = Some(la);
                            record.lon = Some(lo);
                        }
                        matched_hn_str = Some(hn.to_string());
                    }
                }

                let dist = match (params.lat, params.lon, record.lat, record.lon) {
                    (Some(plat), Some(plon), Some(rlat), Some(rlon)) => {
                        Some(haversine_distance_meters(plat, plon, rlat, rlon))
                    }
                    _ => None,
                };

                let mut normalized_score = (score / max_raw_score).clamp(0.1, 1.0);
                if let Some(distance) = dist {
                    let geo_factor = km_to_score(distance / 1000.0) as f32;
                    normalized_score = normalized_score * 0.9 + geo_factor * 0.1;
                }

                if normalized_score >= self.config.min_score {
                    hits.push(SearchHit {
                        record,
                        score: (normalized_score * 100.0).round() / 100.0,
                        distance_meters: dist,
                        document: None,
                        matched_housenumber: matched_hn_str,
                    });
                }
            }
        }

        Ok(hits)
    }

    fn build_query(&self, normalized_query: &str, params: &SearchParams) -> Result<Box<dyn Query>> {
        let qp = match &self.query_parser {
            Some(qp) => qp,
            None => {
                return Err(IndexError::InvalidDirectory(
                    "Query parser not available".to_string(),
                ))
            }
        };

        let tokens: Vec<&str> = normalized_query.split_whitespace().collect();
        if tokens.is_empty() {
            return Ok(qp.parse_query(normalized_query)?);
        }

        let mut clauses: Vec<(Occur, Box<dyn Query>)> = Vec::new();

        // 1. Full text clause
        let standard_query = qp.parse_query(normalized_query)?;
        clauses.push((Occur::Should, standard_query));

        // 2. Exact match boosts on specific fields
        for &token in &tokens {
            if token.len() == 5 && token.chars().all(|c| c.is_ascii_digit()) {
                let term = Term::from_field_text(self.schema.postcode, token);
                let term_query = TermQuery::new(term, IndexRecordOption::Basic);
                clauses.push((
                    Occur::Should,
                    Box::new(BoostQuery::new(Box::new(term_query), 2.5)),
                ));
            }

            if token.chars().next().is_some_and(|c| c.is_ascii_digit()) {
                let term = Term::from_field_text(self.schema.housenumber, token);
                let term_query = TermQuery::new(term, IndexRecordOption::Basic);
                clauses.push((
                    Occur::Should,
                    Box::new(BoostQuery::new(Box::new(term_query), 2.0)),
                ));
            }
        }

        // 3. Postcode filter if explicitly provided
        if let Some(ref pc) = params.postcode {
            let term = Term::from_field_text(self.schema.postcode, pc.trim());
            let term_query = TermQuery::new(term, IndexRecordOption::Basic);
            clauses.push((Occur::Must, Box::new(term_query)));
        }

        Ok(Box::new(BooleanQuery::new(clauses)))
    }

    fn extract_record(&self, doc: &TantivyDocument) -> Option<BanRecord> {
        if let Some(payload_str) = doc.get_first(self.schema.payload).and_then(|v| v.as_str()) {
            if let Ok(record) = serde_json::from_str::<BanRecord>(payload_str) {
                return Some(record);
            }
            if let Ok(document) = serde_json::from_str::<Document>(payload_str) {
                return Some(document.to_ban_record(None));
            }
        }

        let id = self.get_str(doc, self.schema.id)?;
        let street = self.get_str(doc, self.schema.street)?;
        let postcode = self.get_str(doc, self.schema.postcode)?;
        let city = self.get_str(doc, self.schema.city)?;
        let housenumber = self.get_str(doc, self.schema.housenumber);
        let lon = self.get_f64(doc, self.schema.lon);
        let lat = self.get_f64(doc, self.schema.lat);

        Some(BanRecord {
            id: id.to_string(),
            address_type: None,
            numero: housenumber.map(|h| h.to_string()),
            rep: None,
            nom_voie: street.to_string(),
            code_postal: postcode.to_string(),
            code_insee: None,
            nom_commune: city.to_string(),
            code_insee_commune_deleguee: None,
            nom_commune_deleguee: None,
            x: None,
            y: None,
            lon,
            lat,
            type_position: None,
            alias: None,
            nom_afnor: None,
            libelle_acheminement: None,
            nom_complementaire: None,
        })
    }

    fn get_str<'a>(
        &self,
        doc: &'a TantivyDocument,
        field: tantivy::schema::Field,
    ) -> Option<&'a str> {
        doc.get_first(field).and_then(|val| val.as_str())
    }

    fn get_f64(&self, doc: &TantivyDocument, field: tantivy::schema::Field) -> Option<f64> {
        doc.get_first(field).and_then(|val| val.as_f64())
    }
}
