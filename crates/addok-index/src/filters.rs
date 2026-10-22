use std::collections::HashMap;
use std::fs::File;
use std::io::{BufReader, BufWriter};
use std::path::Path;

use addok_core::document::Document;
use roaring::RoaringBitmap;
use serde::{Deserialize, Serialize};

use crate::error::Result;

/// Normalize filter key and value for consistent lookups (e.g. ("postcode", " 75005 ") -> "postcode:75005").
pub fn format_filter_key(key: &str, value: &str) -> String {
    format!(
        "{}:{}",
        key.trim().to_lowercase(),
        value.trim().to_lowercase()
    )
}

/// FiltersIndex maintains multi-value filter posting sets backed by Roaring Bitmaps.
///
/// In Addok, queries can be filtered by:
/// - `type`: "street", "housenumber", "municipality", "locality"
/// - `postcode`: 5-digit postal code (e.g. "75005")
/// - `citycode`: INSEE code (e.g. "75105")
/// - custom metadata fields (e.g. department)
///
/// Using Roaring Bitmaps allows instantaneous set intersection (`&`) and difference (`& !`)
/// operations while maintaining a minimal RAM and disk footprint.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FiltersIndex {
    /// Mapping: "category:value" -> RoaringBitmap of internal u32 doc_ids
    pub filters: HashMap<String, RoaringBitmap>,
}

impl FiltersIndex {
    pub fn new() -> Self {
        Self {
            filters: HashMap::new(),
        }
    }

    /// Add an internal document ID to a specific filter key and value.
    pub fn add_filter(&mut self, key: &str, value: &str, doc_id: u32) {
        let v = value.trim();
        if v.is_empty() {
            return;
        }
        let filter_key = format_filter_key(key, v);
        self.filters.entry(filter_key).or_default().insert(doc_id);
    }

    /// Extract and index all filterable fields from a `Document`.
    pub fn index_document(&mut self, doc_id: u32, doc: &Document) {
        if let Some(ref t) = doc.address_type {
            self.add_filter("type", t, doc_id);
        }
        if let Some(ref pc) = doc.postcode {
            self.add_filter("postcode", pc, doc_id);
            // Also index department prefix if 5-digit postcode (e.g. "75" from "75005")
            if pc.len() >= 2 {
                self.add_filter("department", &pc[..2], doc_id);
            }
        }
        if let Some(ref cc) = doc.citycode {
            self.add_filter("citycode", cc, doc_id);
        }
        if let Some(ref city) = doc.city {
            self.add_filter("city", city, doc_id);
        }

        // Custom fields from extra map
        for (k, v) in &doc.extra {
            match v {
                serde_json::Value::String(s) => self.add_filter(k, s, doc_id),
                serde_json::Value::Number(n) => self.add_filter(k, &n.to_string(), doc_id),
                _ => {}
            }
        }
    }

    /// Retrieve the posting bitmap for a given filter key and value.
    pub fn get_filter_bitmap(&self, key: &str, value: &str) -> Option<&RoaringBitmap> {
        let filter_key = format_filter_key(key, value);
        self.filters.get(&filter_key)
    }

    /// Intersect an existing candidate bitmap with a filter criterion.
    /// Returns the filtered bitmap. If the filter does not exist, returns an empty bitmap.
    pub fn filter_doc_ids(
        &self,
        candidates: &RoaringBitmap,
        key: &str,
        value: &str,
    ) -> RoaringBitmap {
        match self.get_filter_bitmap(key, value) {
            Some(filter_set) => candidates & filter_set,
            None => RoaringBitmap::new(),
        }
    }

    /// Total number of unique filter buckets.
    pub fn len(&self) -> usize {
        self.filters.len()
    }

    pub fn is_empty(&self) -> bool {
        self.filters.is_empty()
    }

    /// Merge another FiltersIndex into this one.
    pub fn merge(&mut self, other: FiltersIndex) {
        for (key, bitmap) in other.filters {
            self.filters.entry(key).or_default().extend(bitmap);
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
    fn test_filters_index_basic() {
        let mut index = FiltersIndex::new();

        let doc1 = Document::new("1", "Rue de Rivoli")
            .with_type("street")
            .with_postcode("75004")
            .with_citycode("75104");

        let doc2 = Document::new("2", "Boulevard Saint-Michel")
            .with_type("street")
            .with_postcode("75005")
            .with_citycode("75105");

        let doc3 = Document::new("3", "Paris")
            .with_type("municipality")
            .with_postcode("75000")
            .with_citycode("75056");

        index.index_document(0, &doc1);
        index.index_document(1, &doc2);
        index.index_document(2, &doc3);

        let streets = index.get_filter_bitmap("type", "street").unwrap();
        assert_eq!(streets.len(), 2);
        assert!(streets.contains(0));
        assert!(streets.contains(1));
        assert!(!streets.contains(2));

        let pc_75005 = index.get_filter_bitmap("postcode", "75005").unwrap();
        assert_eq!(pc_75005.len(), 1);
        assert!(pc_75005.contains(1));

        // Test filtering candidates
        let mut candidates = RoaringBitmap::new();
        candidates.insert(0);
        candidates.insert(1);
        candidates.insert(2);

        let filtered = index.filter_doc_ids(&candidates, "postcode", "75005");
        assert_eq!(filtered.len(), 1);
        assert!(filtered.contains(1));
    }

    #[test]
    fn test_filters_index_roundtrip_file() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("filters.bin");

        let mut index = FiltersIndex::new();
        let doc = Document::new("10", "Rue Saint-Denis")
            .with_type("street")
            .with_postcode("75001");
        index.index_document(42, &doc);

        index.save_to_file(&path).unwrap();

        let loaded = FiltersIndex::load_from_file(&path).unwrap();
        let bm = loaded.get_filter_bitmap("postcode", "75001").unwrap();
        assert!(bm.contains(42));
    }
}
