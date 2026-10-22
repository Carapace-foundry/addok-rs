use std::collections::HashMap;
use std::fs::File;
use std::io::{BufReader, BufWriter};
use std::path::Path;

use addok_core::document::Document;
use serde::{Deserialize, Serialize};

use crate::error::Result;

/// DocumentStore maintains mapping between external string IDs (e.g. "75105_0001")
/// and internal zero-based u32 IDs, and stores `Document` records.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DocumentStore {
    /// Internal doc_id -> Document
    pub documents: Vec<Document>,
    /// External string ID -> internal u32 doc_id
    pub id_to_doc_id: HashMap<String, u32>,
}

impl DocumentStore {
    pub fn new() -> Self {
        Self {
            documents: Vec::new(),
            id_to_doc_id: HashMap::new(),
        }
    }

    /// Insert or update a `Document`. Returns its internal u32 doc_id.
    pub fn insert(&mut self, doc: Document) -> u32 {
        if let Some(&existing_id) = self.id_to_doc_id.get(&doc.id) {
            let existing = &mut self.documents[existing_id as usize];
            if !doc.housenumbers.is_empty() {
                existing.housenumbers.extend(doc.housenumbers);
            }
            if !doc.name.is_empty() {
                existing.name = doc.name;
            }
            if doc.address_type.is_some() {
                existing.address_type = doc.address_type;
            }
            if doc.postcode.is_some() {
                existing.postcode = doc.postcode;
            }
            if doc.citycode.is_some() {
                existing.citycode = doc.citycode;
            }
            if doc.city.is_some() {
                existing.city = doc.city;
            }
            if doc.context.is_some() {
                existing.context = doc.context;
            }
            if doc.importance > 0.0 {
                existing.importance = doc.importance;
            }
            existing.extra.extend(doc.extra);
            existing_id
        } else {
            let doc_id = self.documents.len() as u32;
            self.id_to_doc_id.insert(doc.id.clone(), doc_id);
            self.documents.push(doc);
            doc_id
        }
    }

    /// Retrieve a `Document` by its internal u32 doc_id.
    pub fn get(&self, doc_id: u32) -> Option<&Document> {
        self.documents.get(doc_id as usize)
    }

    /// Retrieve a `Document` by its external string ID.
    pub fn get_by_id(&self, id: &str) -> Option<&Document> {
        self.id_to_doc_id.get(id).and_then(|&idx| self.get(idx))
    }

    /// Look up the internal u32 doc_id for an external string ID.
    pub fn doc_id(&self, id: &str) -> Option<u32> {
        self.id_to_doc_id.get(id).copied()
    }

    /// Number of stored documents.
    pub fn len(&self) -> usize {
        self.documents.len()
    }

    pub fn is_empty(&self) -> bool {
        self.documents.is_empty()
    }

    /// Merge another DocumentStore into this one.
    pub fn merge(&mut self, other: DocumentStore) {
        for doc in other.documents {
            self.insert(doc);
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
            if let Ok(store) = serde_json::from_slice::<Self>(&mmap) {
                return Ok(store);
            }
            if let Ok(store) = bincode::deserialize::<Self>(&mmap) {
                return Ok(store);
            }
        }
        let reader = BufReader::new(File::open(path)?);
        let store: Self = serde_json::from_reader(reader)?;
        Ok(store)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_doc_store_basic() {
        let mut store = DocumentStore::new();

        let doc1 = Document::new("75105_0001", "Boulevard Saint-Michel");
        let doc2 = Document::new("75108_0002", "Avenue des Champs-Élysées");

        let id1 = store.insert(doc1);
        let id2 = store.insert(doc2);

        assert_eq!(id1, 0);
        assert_eq!(id2, 1);
        assert_eq!(store.len(), 2);

        assert_eq!(store.get(id1).unwrap().name, "Boulevard Saint-Michel");
        assert_eq!(
            store.get_by_id("75108_0002").unwrap().name,
            "Avenue des Champs-Élysées"
        );

        // Update existing document
        let doc1_updated = Document::new("75105_0001", "Boulevard Saint-Michel (Updated)");
        let id1_again = store.insert(doc1_updated);
        assert_eq!(id1_again, id1);
        assert_eq!(store.len(), 2);
        assert_eq!(
            store.get(id1).unwrap().name,
            "Boulevard Saint-Michel (Updated)"
        );
    }

    #[test]
    fn test_doc_store_roundtrip_file() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("docs.bin");

        let mut store = DocumentStore::new();
        store.insert(Document::new("1", "Rue de Paris"));
        store.save_to_file(&path).unwrap();

        let loaded = DocumentStore::load_from_file(&path).unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded.get(0).unwrap().name, "Rue de Paris");
    }
}
