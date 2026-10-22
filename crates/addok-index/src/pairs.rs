use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{BufReader, BufWriter};
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::Result;

/// PairsIndexer tracks token co-occurrences within indexed documents.
///
/// In Addok, every address document has an associated set of normalized tokens.
/// For each token, the PairsIndexer stores:
/// 1. The set of associated tokens that co-occur in the same documents (`p|token -> set(associated_tokens)`).
/// 2. Document frequencies to measure token selectivity.
///
/// During autocomplete, when the user is typing the last word of a query (e.g. "mich" in "8 bd st mich"),
/// edge n-grams produce candidate tokens ("michel", "michelet", "michard", ...).
/// The PairsIndexer filters these candidates so that ONLY tokens that actually co-occur with the
/// query's other tokens ("boulevard", "saint") are retained.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PairsIndexer {
    /// Mapping: token -> set of associated tokens that appear in the same document.
    pub pairs: HashMap<String, HashSet<String>>,
    /// Number of documents containing each token.
    pub doc_frequencies: HashMap<String, u32>,
    /// Total number of documents indexed.
    pub total_docs: usize,
}

impl PairsIndexer {
    pub fn new() -> Self {
        Self {
            pairs: HashMap::new(),
            doc_frequencies: HashMap::new(),
            total_docs: 0,
        }
    }

    /// Index tokens from a document. Extracts unique tokens, increments document counts,
    /// and records pairwise associations.
    #[allow(clippy::needless_range_loop)]
    pub fn index_tokens(&mut self, tokens: &[&str]) {
        let mut unique_tokens: Vec<&str> = tokens
            .iter()
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .collect();
        unique_tokens.sort_unstable();
        unique_tokens.dedup();

        if unique_tokens.is_empty() {
            return;
        }

        self.total_docs += 1;

        for &tok in &unique_tokens {
            *self.doc_frequencies.entry(tok.to_string()).or_insert(0) += 1;
        }

        let n = unique_tokens.len();
        for i in 0..n {
            let tok_a = unique_tokens[i];
            for j in (i + 1)..n {
                let tok_b = unique_tokens[j];
                self.pairs
                    .entry(tok_a.to_string())
                    .or_default()
                    .insert(tok_b.to_string());
                self.pairs
                    .entry(tok_b.to_string())
                    .or_default()
                    .insert(tok_a.to_string());
            }
        }
    }

    /// Index tokens provided as a slice of owned Strings.
    pub fn index_document_tokens(&mut self, tokens: &[String]) {
        let token_refs: Vec<&str> = tokens.iter().map(String::as_str).collect();
        self.index_tokens(&token_refs);
    }

    /// Retrieve the set of tokens that co-occur with `token`.
    pub fn get_associated_tokens(&self, token: &str) -> Option<&HashSet<String>> {
        self.pairs.get(token)
    }

    /// Check if two tokens co-occur in any indexed document.
    pub fn are_associated(&self, token_a: &str, token_b: &str) -> bool {
        if token_a == token_b {
            return true;
        }
        self.pairs
            .get(token_a)
            .is_some_and(|set| set.contains(token_b))
    }

    /// Returns the document frequency of a token (0 if token was never indexed).
    pub fn token_doc_frequency(&self, token: &str) -> u32 {
        self.doc_frequencies.get(token).copied().unwrap_or(0)
    }

    /// Computes token selectivity score in [0.0, 1.0].
    /// Rare tokens have high selectivity (~1.0); common stop words have low selectivity (~0.0).
    pub fn token_selectivity(&self, token: &str) -> f64 {
        if self.total_docs == 0 {
            return 1.0;
        }
        let freq = self.token_doc_frequency(token) as f64;
        if freq == 0.0 {
            return 1.0;
        }
        (1.0 - (freq / (self.total_docs as f64))).clamp(0.0, 1.0)
    }

    /// Filter autocomplete candidate tokens using co-occurrence with the query's existing tokens.
    ///
    /// If candidates matching a prefix are [c1, c2, c3], this prunes any candidate that does NOT
    /// co-occur with the already confirmed query tokens.
    pub fn filter_candidates(&self, query_tokens: &[&str], candidates: &[String]) -> Vec<String> {
        let clean_query: Vec<&str> = query_tokens
            .iter()
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .collect();

        if clean_query.is_empty() || candidates.is_empty() {
            return candidates.to_vec();
        }

        // 1. Try to find candidates that pair with ALL query tokens (strongest constraint)
        let matched_all: Vec<String> = candidates
            .iter()
            .filter(|cand| clean_query.iter().all(|q| self.are_associated(q, cand)))
            .cloned()
            .collect();

        if !matched_all.is_empty() {
            return matched_all;
        }

        // 2. Fall back to candidates that pair with AT LEAST ONE query token
        let matched_any: Vec<String> = candidates
            .iter()
            .filter(|cand| clean_query.iter().any(|q| self.are_associated(q, cand)))
            .cloned()
            .collect();

        if !matched_any.is_empty() {
            matched_any
        } else {
            // If none co-occur (e.g. unknown new combination), keep original candidates
            candidates.to_vec()
        }
    }

    /// Merge another PairsIndexer into this one (supports incremental batch commits).
    pub fn merge(&mut self, other: PairsIndexer) {
        self.total_docs += other.total_docs;
        for (tok, count) in other.doc_frequencies {
            *self.doc_frequencies.entry(tok).or_insert(0) += count;
        }
        for (tok, set) in other.pairs {
            self.pairs.entry(tok).or_default().extend(set);
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
            if let Ok(indexer) = bincode::deserialize::<Self>(&mmap) {
                return Ok(indexer);
            }
        }
        let reader = BufReader::new(File::open(path)?);
        let indexer: Self = bincode::deserialize_from(reader)?;
        Ok(indexer)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_pairs_indexer_basic() {
        let mut indexer = PairsIndexer::new();

        indexer.index_tokens(&["boulevard", "saint", "michel"]);
        indexer.index_tokens(&["rue", "saint", "denis"]);
        indexer.index_tokens(&["avenue", "des", "champs", "elysees"]);

        assert_eq!(indexer.total_docs, 3);
        assert_eq!(indexer.token_doc_frequency("saint"), 2);
        assert_eq!(indexer.token_doc_frequency("michel"), 1);

        assert!(indexer.are_associated("boulevard", "saint"));
        assert!(indexer.are_associated("boulevard", "michel"));
        assert!(indexer.are_associated("saint", "michel"));
        assert!(indexer.are_associated("saint", "denis"));

        // Boulevard and Denis do not co-occur
        assert!(!indexer.are_associated("boulevard", "denis"));
        assert!(!indexer.are_associated("champs", "saint"));
    }

    #[test]
    fn test_pairs_indexer_candidate_filtering() {
        let mut indexer = PairsIndexer::new();
        indexer.index_tokens(&["boulevard", "saint", "michel"]);
        indexer.index_tokens(&["rue", "saint", "denis"]);
        indexer.index_tokens(&["rue", "de", "la", "paix"]);

        let candidates = vec![
            "michel".to_string(),
            "denis".to_string(),
            "paix".to_string(),
            "inconnu".to_string(),
        ];

        // If user typed "saint", candidates matching "saint" are "michel" and "denis"
        let filtered = indexer.filter_candidates(&["saint"], &candidates);
        assert_eq!(filtered, vec!["michel".to_string(), "denis".to_string()]);

        // If user typed "boulevard" and "saint", only "michel" pairs with both!
        let filtered_both = indexer.filter_candidates(&["boulevard", "saint"], &candidates);
        assert_eq!(filtered_both, vec!["michel".to_string()]);
    }

    #[test]
    fn test_pairs_indexer_roundtrip_file() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("pairs.bin");

        let mut indexer = PairsIndexer::new();
        indexer.index_tokens(&["rue", "de", "rivoli"]);
        indexer.save_to_file(&path).unwrap();

        let loaded = PairsIndexer::load_from_file(&path).unwrap();
        assert_eq!(loaded.total_docs, 1);
        assert!(loaded.are_associated("rue", "rivoli"));
    }
}
