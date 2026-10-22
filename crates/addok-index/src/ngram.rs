use std::collections::{BTreeSet, HashMap};
use std::fs::File;
use std::io::{BufReader, BufWriter};
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::Result;

/// Edge N-Gram index mapping prefixes to matching full vocabulary tokens.
///
/// In Addok, autocomplete relies on an edge n-gram table (`n|prefix -> set(tokens)`).
/// For each token in the vocabulary (e.g. "michel"), prefixes of lengths between `min_size` (e.g. 2)
/// and `max_size` (e.g. 15) are indexed:
/// - "mi" -> "michel"
/// - "mic" -> "michel"
/// - "mich" -> "michel"
/// - "miche" -> "michel"
/// - "michel" -> "michel"
///
/// When the user is typing, prefix lookups execute in $O(1)$ time without scanning the whole dictionary.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EdgeNgramIndex {
    pub min_size: usize,
    pub max_size: usize,
    /// Mapping: prefix -> sorted set of matching full tokens.
    pub prefixes: HashMap<String, BTreeSet<String>>,
}

impl Default for EdgeNgramIndex {
    fn default() -> Self {
        Self::new(2, 15)
    }
}

impl EdgeNgramIndex {
    pub fn new(min_size: usize, max_size: usize) -> Self {
        Self {
            min_size,
            max_size,
            prefixes: HashMap::new(),
        }
    }

    /// Static generator function: generates edge n-grams for a token.
    ///
    /// For "champs" with min_size=2, max_size=5:
    /// returns ["ch", "cha", "cham", "champ", "champs"]
    #[allow(clippy::needless_range_loop)]
    pub fn generate_edge_ngrams(token: &str, min_size: usize, max_size: usize) -> Vec<String> {
        let clean = token.trim().to_lowercase();
        let char_indices: Vec<usize> = clean.char_indices().map(|(idx, _)| idx).collect();
        let char_count = char_indices.len();

        if char_count < min_size {
            return Vec::new();
        }

        let mut ngrams = Vec::new();
        let upper = char_count.min(max_size);

        for len in min_size..=upper {
            let byte_end = if len < char_count {
                char_indices[len]
            } else {
                clean.len()
            };
            ngrams.push(clean[..byte_end].to_string());
        }

        ngrams
    }

    /// Index a single token into the edge n-gram table.
    pub fn index_token(&mut self, token: &str) {
        let clean = token.trim().to_lowercase();
        if clean.len() < self.min_size {
            return;
        }

        let ngrams = Self::generate_edge_ngrams(&clean, self.min_size, self.max_size);
        for ngram in ngrams {
            self.prefixes
                .entry(ngram)
                .or_default()
                .insert(clean.clone());
        }
    }

    /// Index multiple tokens.
    pub fn index_tokens<I, S>(&mut self, tokens: I)
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        for token in tokens {
            self.index_token(token.as_ref());
        }
    }

    /// Retrieve full vocabulary tokens starting with `prefix`.
    pub fn get_candidates(&self, prefix: &str) -> Vec<String> {
        let clean = prefix.trim().to_lowercase();
        if clean.is_empty() {
            return Vec::new();
        }

        if let Some(set) = self.prefixes.get(&clean) {
            return set.iter().cloned().collect();
        }

        // If prefix is longer than max_size, try to match via known prefixes
        if clean.len() > self.max_size {
            let sub = &clean[..self.max_size];
            if let Some(set) = self.prefixes.get(sub) {
                return set
                    .iter()
                    .filter(|cand| cand.starts_with(&clean))
                    .cloned()
                    .collect();
            }
        }

        Vec::new()
    }

    /// Total number of unique prefixes indexed.
    pub fn len(&self) -> usize {
        self.prefixes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.prefixes.is_empty()
    }

    /// Merge another EdgeNgramIndex into this one.
    pub fn merge(&mut self, other: EdgeNgramIndex) {
        for (prefix, set) in other.prefixes {
            self.prefixes.entry(prefix).or_default().extend(set);
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
    fn test_edge_ngram_generation() {
        let ngrams = EdgeNgramIndex::generate_edge_ngrams("michel", 2, 6);
        assert_eq!(ngrams, vec!["mi", "mic", "mich", "miche", "michel"]);

        let ngrams_short = EdgeNgramIndex::generate_edge_ngrams("a", 2, 6);
        assert!(ngrams_short.is_empty());
    }

    #[test]
    fn test_edge_ngram_index_lookup() {
        let mut index = EdgeNgramIndex::new(2, 10);
        index.index_token("michel");
        index.index_token("michelet");
        index.index_token("saint");

        let cands_mich = index.get_candidates("mich");
        assert_eq!(
            cands_mich,
            vec!["michel".to_string(), "michelet".to_string()]
        );

        let cands_sa = index.get_candidates("sa");
        assert_eq!(cands_sa, vec!["saint".to_string()]);

        let cands_unknown = index.get_candidates("xyz");
        assert!(cands_unknown.is_empty());
    }

    #[test]
    fn test_edge_ngram_roundtrip_file() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("ngram.bin");

        let mut index = EdgeNgramIndex::default();
        index.index_token("boulevard");
        index.save_to_file(&path).unwrap();

        let loaded = EdgeNgramIndex::load_from_file(&path).unwrap();
        assert_eq!(loaded.get_candidates("boul"), vec!["boulevard".to_string()]);
    }
}
