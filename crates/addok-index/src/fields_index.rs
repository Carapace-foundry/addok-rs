use std::collections::HashMap;
use std::fs::File;
use std::io::{BufReader, BufWriter};
use std::path::Path;

use addok_core::config::AddokConfig;
use addok_core::document::Document;
use addok_core::normalizer::{normalize_address, tokenize_address};
use roaring::RoaringBitmap;
use serde::{Deserialize, Serialize};

use crate::error::Result;

/// Field boosts configuration for query scoring
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldBoostConfig {
    pub name: f32,
    pub city: f32,
    pub postcode: f32,
    pub housenumber: f32,
    pub context: f32,
}

impl Default for FieldBoostConfig {
    fn default() -> Self {
        Self {
            name: 1.0,
            city: 0.8,
            postcode: 0.8,
            housenumber: 0.7,
            context: 0.5,
        }
    }
}

/// Fields Inverted Index: Token -> Document IDs with field boosts and token weight.
///
/// In Addok, each field of a document is tokenized and stored in an inverted index.
/// When scoring query tokens against documents:
/// - Exact matches in the primary `name` field receive the highest boost (1.0).
/// - Secondary matches (`city`, `postcode`) provide disambiguation boosts.
/// - Document importance adds a secondary multiplier.
/// - Roaring Bitmaps provide near-zero overhead set intersections.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldsInvertedIndex {
    pub config: FieldBoostConfig,
    /// Global token postings: token -> RoaringBitmap of doc_ids
    pub postings: HashMap<String, RoaringBitmap>,
    /// Field-specific postings: field_name -> (token -> RoaringBitmap of doc_ids)
    pub field_postings: HashMap<String, HashMap<String, RoaringBitmap>>,
    /// Document importance score indexed by internal u32 doc_id
    pub doc_importances: Vec<f32>,
}

impl Default for FieldsInvertedIndex {
    fn default() -> Self {
        Self::new(FieldBoostConfig::default())
    }
}

impl FieldsInvertedIndex {
    pub fn new(config: FieldBoostConfig) -> Self {
        Self {
            config,
            postings: HashMap::new(),
            field_postings: HashMap::new(),
            doc_importances: Vec::new(),
        }
    }

    /// Index a `Document` with its internal u32 doc_id.
    pub fn index_document(&mut self, doc_id: u32, doc: &Document) {
        let idx = doc_id as usize;
        if idx >= self.doc_importances.len() {
            self.doc_importances.resize(idx + 1, 0.0);
        }
        self.doc_importances[idx] = doc.importance;

        self.index_document_with_config(doc_id, doc, &AddokConfig::default());
    }

    /// Add a document's tokens and boosts into the inverted index respecting plugin configuration
    pub fn index_document_with_config(
        &mut self,
        doc_id: u32,
        doc: &Document,
        config: &AddokConfig,
    ) {
        let idx = doc_id as usize;
        if idx >= self.doc_importances.len() {
            self.doc_importances.resize(idx + 1, 0.0);
        }
        self.doc_importances[idx] = doc.importance;

        let has_france = config.has_france();

        // 1. Index name field
        let norm_name = normalize_address(&doc.name, has_france);
        for token in tokenize_address(&norm_name, false, has_france) {
            self.add_field_token("name", &token, doc_id);
            self.postings.entry(token).or_default().insert(doc_id);
        }

        // 2. Index city field
        if let Some(ref city) = doc.city {
            let norm_city = normalize_address(city, has_france);
            for token in tokenize_address(&norm_city, false, has_france) {
                self.add_field_token("city", &token, doc_id);
                self.postings.entry(token).or_default().insert(doc_id);
            }
        }

        // 3. Index postcode field
        if let Some(ref pc) = doc.postcode {
            let clean_pc = pc.trim().to_lowercase();
            if !clean_pc.is_empty() {
                self.add_field_token("postcode", &clean_pc, doc_id);
                self.postings.entry(clean_pc).or_default().insert(doc_id);
            }
        }

        // 5. Index context field
        if let Some(ref ctx) = doc.context {
            let norm_ctx = normalize_address(ctx, has_france);
            for token in tokenize_address(&norm_ctx, false, has_france) {
                self.add_field_token("context", &token, doc_id);
                self.postings.entry(token).or_default().insert(doc_id);
            }
        }
    }

    fn add_field_token(&mut self, field: &str, token: &str, doc_id: u32) {
        self.field_postings
            .entry(field.to_string())
            .or_default()
            .entry(token.to_string())
            .or_default()
            .insert(doc_id);
    }

    /// Retrieve the global posting bitmap for a token.
    pub fn get_postings(&self, token: &str) -> Option<&RoaringBitmap> {
        self.postings.get(token)
    }

    /// Search matching documents for a list of tokens, optionally filtered by a candidate bitmap.
    /// Returns a list of (doc_id, score) sorted by score descending.
    pub fn search_with_filter(
        &self,
        tokens: &[&str],
        filter_mask: Option<&RoaringBitmap>,
        limit: usize,
    ) -> Vec<(u32, f32)> {
        let clean_tokens: Vec<&str> = tokens
            .iter()
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .collect();

        if clean_tokens.is_empty() {
            return Vec::new();
        }

        // 1. Gather posting bitmaps for each token
        let mut token_postings = Vec::new();
        for &tok in &clean_tokens {
            if let Some(bm) = self.postings.get(tok) {
                token_postings.push((tok, bm));
            }
        }

        if token_postings.is_empty() {
            return Vec::new();
        }

        // 2. Compute candidate document IDs:
        // Try strict intersection (all tokens) first
        let mut candidates = token_postings[0].1.clone();
        for (_, bm) in &token_postings[1..] {
            candidates &= *bm;
        }

        // Apply external filter mask if provided
        if let Some(mask) = filter_mask {
            candidates &= mask;
        }

        // If intersection yields fewer than limit results, expand with union of all tokens
        if (candidates.len() as usize) < limit {
            let mut union_candidates = RoaringBitmap::new();
            for (_, bm) in &token_postings {
                union_candidates |= *bm;
            }
            if let Some(mask) = filter_mask {
                union_candidates &= mask;
            }
            candidates = union_candidates;
        }

        if candidates.is_empty() {
            return Vec::new();
        }

        // 3. Score each candidate document
        let total_tokens_count = clean_tokens.len() as f32;
        let mut scored_docs = Vec::with_capacity(candidates.len() as usize);

        for doc_id in candidates.iter() {
            let mut matched_token_count = 0;
            let mut total_token_boost = 0.0f32;

            for &tok in &clean_tokens {
                let mut token_boost = 0.0f32;

                if let Some(map) = self.field_postings.get("name") {
                    if let Some(bm) = map.get(tok) {
                        if bm.contains(doc_id) {
                            token_boost += self.config.name;
                        }
                    }
                }
                if let Some(map) = self.field_postings.get("city") {
                    if let Some(bm) = map.get(tok) {
                        if bm.contains(doc_id) {
                            token_boost += self.config.city;
                        }
                    }
                }
                if let Some(map) = self.field_postings.get("postcode") {
                    if let Some(bm) = map.get(tok) {
                        if bm.contains(doc_id) {
                            token_boost += self.config.postcode;
                        }
                    }
                }
                if let Some(map) = self.field_postings.get("housenumber") {
                    if let Some(bm) = map.get(tok) {
                        if bm.contains(doc_id) {
                            token_boost += self.config.housenumber;
                        }
                    }
                }
                if let Some(map) = self.field_postings.get("context") {
                    if let Some(bm) = map.get(tok) {
                        if bm.contains(doc_id) {
                            token_boost += self.config.context;
                        }
                    }
                }

                if token_boost > 0.0 {
                    matched_token_count += 1;
                    total_token_boost += token_boost;
                }
            }

            if matched_token_count == 0 {
                continue;
            }

            let match_ratio = matched_token_count as f32 / total_tokens_count;
            let avg_boost = total_token_boost / total_tokens_count;

            let importance = self
                .doc_importances
                .get(doc_id as usize)
                .copied()
                .unwrap_or(0.0);

            let raw_score = (avg_boost * 0.7 + match_ratio * 0.3) * (1.0 + importance * 0.4);
            let normalized_score = raw_score.clamp(0.05, 1.0);

            scored_docs.push((doc_id, normalized_score));
        }

        // Sort descending by score
        scored_docs.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        scored_docs.truncate(limit);
        scored_docs
    }

    /// Search matching documents without external filter.
    pub fn search(&self, tokens: &[&str], limit: usize) -> Vec<(u32, f32)> {
        self.search_with_filter(tokens, None, limit)
    }

    /// Total number of unique tokens in vocabulary.
    pub fn len(&self) -> usize {
        self.postings.len()
    }

    pub fn is_empty(&self) -> bool {
        self.postings.is_empty()
    }

    /// Merge another FieldsInvertedIndex into this one.
    pub fn merge(&mut self, other: FieldsInvertedIndex) {
        if other.doc_importances.len() > self.doc_importances.len() {
            self.doc_importances
                .resize(other.doc_importances.len(), 0.0);
        }
        for (i, &imp) in other.doc_importances.iter().enumerate() {
            if imp > 0.0 {
                self.doc_importances[i] = imp;
            }
        }
        for (tok, bm) in other.postings {
            self.postings.entry(tok).or_default().extend(bm);
        }
        for (field, map) in other.field_postings {
            let target_field = self.field_postings.entry(field).or_default();
            for (tok, bm) in map {
                target_field.entry(tok).or_default().extend(bm);
            }
        }
    }

    /// Save index to binary file on disk.
    pub fn save_to_file<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        let file = File::create(path)?;
        let writer = BufWriter::new(file);
        bincode::serialize_into(writer, self)?;
        Ok(())
    }

    /// Load index from binary file on disk using memory mapping.
    pub fn load_from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let file = File::open(&path)?;
        if let Ok(mmap) = unsafe { memmap2::Mmap::map(&file) } {
            if let Ok(index) = bincode::deserialize::<Self>(&mmap) {
                return Ok(index);
            }
        }
        let reader = BufReader::new(File::open(path)?);
        let index: Self = bincode::deserialize_from(reader)?;
        Ok(index)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_fields_inverted_index_basic() {
        let mut index = FieldsInvertedIndex::default();

        let doc1 = Document::new("1", "Boulevard Saint-Michel")
            .with_city("Paris")
            .with_postcode("75005")
            .with_importance(0.4);

        let doc2 = Document::new("2", "Rue Saint-Denis")
            .with_city("Paris")
            .with_postcode("75001")
            .with_importance(0.3);

        index.index_document(0, &doc1);
        index.index_document(1, &doc2);

        // Search "saint"
        let results = index.search(&["saint"], 5);
        assert_eq!(results.len(), 2);

        // Search "saint michel"
        let results_michel = index.search(&["saint", "michel"], 5);
        assert_eq!(results_michel.len(), 2);
        assert_eq!(results_michel[0].0, 0); // doc1 is top result
    }

    #[test]
    fn test_fields_inverted_index_with_filter() {
        let mut index = FieldsInvertedIndex::default();
        let doc1 = Document::new("1", "Rue de Paris").with_postcode("75001");
        let doc2 = Document::new("2", "Rue de Paris").with_postcode("75002");
        index.index_document(0, &doc1);
        index.index_document(1, &doc2);

        let mut filter_75002 = RoaringBitmap::new();
        filter_75002.insert(1);

        let res = index.search_with_filter(&["rue", "paris"], Some(&filter_75002), 5);
        assert_eq!(res.len(), 1);
        assert_eq!(res[0].0, 1);
    }

    #[test]
    fn test_fields_inverted_index_roundtrip_file() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("fields.bin");

        let mut index = FieldsInvertedIndex::default();
        let doc = Document::new("1", "Avenue de l'Opera");
        index.index_document(0, &doc);
        index.save_to_file(&path).unwrap();

        let loaded = FieldsInvertedIndex::load_from_file(&path).unwrap();
        let res = loaded.search(&["opera"], 5);
        assert_eq!(res.len(), 1);
        assert_eq!(res[0].0, 0);
    }
}
