use std::collections::HashMap;
use std::fs::File;
use std::io::{BufReader, BufWriter};
use std::path::Path;

use addok_core::document::{Document, HousenumberData};
use serde::{Deserialize, Serialize};

use crate::error::Result;

/// Normalize a housenumber string for consistent lookups (e.g. "8  Bis" -> "8 bis", "8bis" -> "8 bis", "08" -> "8").
pub fn normalize_housenumber(raw: &str) -> String {
    let lower = raw.trim().to_lowercase();
    let digit_end = lower.chars().take_while(|c| c.is_ascii_digit()).count();
    if digit_end > 0 {
        let (num, rep) = lower.split_at(digit_end);
        let clean_num = num.trim_start_matches('0');
        let final_num = if clean_num.is_empty() { "0" } else { clean_num };
        let rep_clean = rep.trim();
        if !rep_clean.is_empty() {
            return format!("{} {}", final_num, rep_clean);
        } else {
            return final_num.to_string();
        }
    }
    let parts: Vec<&str> = lower.split_whitespace().collect();
    parts.join(" ")
}

/// HousenumbersIndex maps (street_doc_id, normalized_housenumber) -> exact coordinates and metadata.
///
/// In Addok, house numbers are grouped under parent street documents.
/// This sub-index provides $O(1)$ coordinate resolution for a specific house number once
/// the street document has been retrieved.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct HousenumbersIndex {
    /// Composite key: "{street_doc_id}:{normalized_housenumber}" -> HousenumberData
    pub numbers: HashMap<String, HousenumberData>,
    /// Street document ID -> list of available normalized house numbers
    pub street_numbers: HashMap<String, Vec<String>>,
}

impl HousenumbersIndex {
    pub fn new() -> Self {
        Self {
            numbers: HashMap::new(),
            street_numbers: HashMap::new(),
        }
    }

    /// Index all housenumbers from a `Document`.
    pub fn index_document(&mut self, doc: &Document) {
        if doc.housenumbers.is_empty() {
            return;
        }

        let entry = self.street_numbers.entry(doc.id.clone()).or_default();

        for (hn_str, data) in &doc.housenumbers {
            let norm_hn = normalize_housenumber(hn_str);
            if norm_hn.is_empty() {
                continue;
            }

            let key = format!("{}:{}", doc.id, norm_hn);
            self.numbers.insert(key, data.clone());

            if !entry.contains(&norm_hn) {
                entry.push(norm_hn);
            }
        }
    }

    /// Look up coordinates and metadata for a specific house number on a street.
    pub fn get_coordinates(
        &self,
        street_doc_id: &str,
        housenumber: &str,
    ) -> Option<&HousenumberData> {
        let norm_hn = normalize_housenumber(housenumber);
        let key = format!("{}:{}", street_doc_id, norm_hn);
        if let Some(data) = self.numbers.get(&key) {
            return Some(data);
        }

        // Try collapsed variant (e.g. "8bis")
        let collapsed: String = norm_hn.chars().filter(|c| !c.is_whitespace()).collect();
        let compact_key = format!("{}:{}", street_doc_id, collapsed);
        self.numbers.get(&compact_key)
    }

    /// Check if a street has a specific house number.
    pub fn has_housenumber(&self, street_doc_id: &str, housenumber: &str) -> bool {
        self.get_coordinates(street_doc_id, housenumber).is_some()
    }

    /// Retrieve all house numbers registered for a street document.
    pub fn get_street_housenumbers(&self, street_doc_id: &str) -> Option<&[String]> {
        self.street_numbers.get(street_doc_id).map(Vec::as_slice)
    }

    /// Total number of indexed house numbers across all streets.
    pub fn len(&self) -> usize {
        self.numbers.len()
    }

    pub fn is_empty(&self) -> bool {
        self.numbers.is_empty()
    }

    /// Merge another HousenumbersIndex into this one.
    pub fn merge(&mut self, other: HousenumbersIndex) {
        self.numbers.extend(other.numbers);
        for (street_id, numbers) in other.street_numbers {
            self.street_numbers
                .entry(street_id)
                .or_default()
                .extend(numbers);
        }
    }

    /// Save index to file on disk.
    pub fn save_to_file<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        let file = File::create(path)?;
        let writer = BufWriter::new(file);
        serde_json::to_writer(writer, self)?;
        Ok(())
    }

    /// Load index from file on disk using memory mapping and fast deserialization
    /// with fallback to serde_json for backward compatibility.
    pub fn load_from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let file = File::open(&path)?;
        if let Ok(mmap) = unsafe { memmap2::Mmap::map(&file) } {
            if let Ok(index) = serde_json::from_slice::<Self>(&mmap) {
                return Ok(index);
            }
            if let Ok(index) = bincode::deserialize::<Self>(&mmap) {
                return Ok(index);
            }
        }
        let reader = BufReader::new(File::open(path)?);
        let index: Self = serde_json::from_reader(reader)?;
        Ok(index)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_housenumbers_index_basic() {
        let mut index = HousenumbersIndex::new();

        let mut doc = Document::new("75105_0001", "Boulevard Saint-Michel");
        doc.add_housenumber("8", HousenumberData::with_coords(48.8512, 2.3431));
        doc.add_housenumber("10 B", HousenumberData::with_coords(48.8515, 2.3435));

        index.index_document(&doc);

        assert!(index.has_housenumber("75105_0001", "8"));
        let coords_8 = index.get_coordinates("75105_0001", "8").unwrap();
        assert_eq!(coords_8.lat, Some(48.8512));
        assert_eq!(coords_8.lon, Some(2.3431));

        // Test with varying casing / spacing
        assert!(index.has_housenumber("75105_0001", "10 b"));
        assert!(index.has_housenumber("75105_0001", "10b"));

        // Non-existent housenumber
        assert!(!index.has_housenumber("75105_0001", "99"));
    }

    #[test]
    fn test_housenumbers_index_roundtrip_file() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("housenumbers.bin");

        let mut index = HousenumbersIndex::new();
        let mut doc = Document::new("75108_0002", "Avenue des Champs-Élysées");
        doc.add_housenumber("10", HousenumberData::with_coords(48.8698, 2.3082));
        index.index_document(&doc);

        index.save_to_file(&path).unwrap();

        let loaded = HousenumbersIndex::load_from_file(&path).unwrap();
        assert!(loaded.has_housenumber("75108_0002", "10"));
    }
}
