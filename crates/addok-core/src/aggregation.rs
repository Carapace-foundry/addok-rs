use std::collections::HashMap;
use std::io::Read;
use std::path::Path;

use crate::ban::{BanRecord, BanType};
use crate::document::{Document, HousenumberData};
use crate::error::Result;
use crate::parser::{open_ban_csv, open_ban_csv_reader};

/// Aggregates individual BAN records into hierarchical street/municipality documents.
///
/// In the French Base Adresse Nationale (BAN), tens of millions of rows represent individual house
/// numbers on a street. Hierarchical aggregation groups all house numbers under their parent street
/// document, drastically reducing memory usage, Tantivy index size, and redundant tokens.
#[derive(Debug, Default)]
pub struct BanAggregator {
    documents: HashMap<String, Document>,
}

impl BanAggregator {
    pub fn new() -> Self {
        Self {
            documents: HashMap::new(),
        }
    }

    /// Add a single `BanRecord` into the aggregator, grouping under its parent street.
    pub fn add_record(&mut self, record: BanRecord) {
        let is_housenumber = record.numero.as_ref().is_some_and(|n| !n.trim().is_empty());

        if is_housenumber {
            let parent_id = extract_parent_street_id(&record);
            let full_hn = record
                .full_housenumber()
                .unwrap_or_else(|| record.numero.clone().unwrap_or_default());

            let mut hn_extra = HashMap::new();
            if let Some(tp) = record.type_position {
                hn_extra.insert("type_position".to_string(), serde_json::Value::String(tp));
            }
            hn_extra.insert(
                "id".to_string(),
                serde_json::Value::String(record.id.clone()),
            );

            let hn_data = HousenumberData {
                lat: record.lat,
                lon: record.lon,
                x: record.x,
                y: record.y,
                extra: hn_extra,
            };

            let entry = self.documents.entry(parent_id.clone()).or_insert_with(|| {
                let dep = if record.code_postal.len() >= 2 {
                    &record.code_postal[..2]
                } else {
                    ""
                };
                let context = if dep.is_empty() {
                    record.nom_commune.clone()
                } else {
                    format!("{}, {}", dep, record.nom_commune)
                };

                let mut doc = Document::new(parent_id, record.nom_voie.clone())
                    .with_type("street")
                    .with_postcode(record.code_postal.clone())
                    .with_city(record.nom_commune.clone())
                    .with_context(context)
                    .with_importance(0.4);

                if let Some(insee) = record.code_insee {
                    doc = doc.with_citycode(insee);
                }

                doc
            });

            entry.housenumbers.insert(full_hn, hn_data);
        } else {
            // Record is a Street, Locality, or Municipality
            let doc_id = record.id.clone();
            let btype = record.ban_type();
            let name = if !record.nom_voie.trim().is_empty() {
                record.nom_voie.clone()
            } else {
                record.nom_commune.clone()
            };

            let dep = if record.code_postal.len() >= 2 {
                &record.code_postal[..2]
            } else {
                ""
            };
            let context = if dep.is_empty() {
                record.nom_commune.clone()
            } else {
                format!("{}, {}", dep, record.nom_commune)
            };

            let importance = match btype {
                BanType::Municipality => 0.5,
                BanType::Street => 0.4,
                BanType::Locality => 0.3,
                BanType::Housenumber => 0.1,
            };

            if let Some(existing) = self.documents.get_mut(&doc_id) {
                // Update existing record if it was already created from a housenumber
                existing.name = name;
                existing.address_type = Some(btype.to_string());
                existing.postcode = Some(record.code_postal);
                existing.city = Some(record.nom_commune);
                if let Some(insee) = record.code_insee {
                    existing.citycode = Some(insee);
                }
            } else {
                let mut doc = Document::new(doc_id.clone(), name)
                    .with_type(btype.to_string())
                    .with_postcode(record.code_postal)
                    .with_city(record.nom_commune)
                    .with_context(context)
                    .with_importance(importance);

                if let Some(insee) = record.code_insee {
                    doc = doc.with_citycode(insee);
                }

                if let (Some(lat), Some(lon)) = (record.lat, record.lon) {
                    doc.extra.insert("lat".to_string(), serde_json::json!(lat));
                    doc.extra.insert("lon".to_string(), serde_json::json!(lon));
                }

                self.documents.insert(doc_id, doc);
            }
        }
    }

    /// Number of aggregated documents.
    pub fn len(&self) -> usize {
        self.documents.len()
    }

    /// Check if the aggregator contains no documents.
    pub fn is_empty(&self) -> bool {
        self.documents.is_empty()
    }

    /// Access the internal document map.
    pub fn documents(&self) -> &HashMap<String, Document> {
        &self.documents
    }

    /// Consume the aggregator and return the list of aggregated documents.
    pub fn into_documents(self) -> Vec<Document> {
        self.documents.into_values().collect()
    }
}

fn extract_parent_street_id(record: &BanRecord) -> String {
    let parts: Vec<&str> = record.id.split('_').collect();
    if parts.len() >= 3 {
        return format!("{}_{}", parts[0], parts[1]);
    }
    if parts.len() == 2 {
        return record.id.clone();
    }

    // Fallback if ID doesn't follow the insee_fantoir pattern
    format!(
        "{}_{}",
        record.code_insee.as_deref().unwrap_or(&record.code_postal),
        record.nom_voie.trim().replace(' ', "-").to_lowercase()
    )
}

/// Aggregate an iterator of `BanRecord` entries into a list of hierarchical `Document`s.
pub fn aggregate_ban_records<I: IntoIterator<Item = BanRecord>>(records: I) -> Vec<Document> {
    let mut aggregator = BanAggregator::new();
    for record in records {
        aggregator.add_record(record);
    }
    aggregator.into_documents()
}

/// Read a BAN CSV or CSV.GZ file and aggregate all records into hierarchical `Document`s.
pub fn aggregate_ban_csv<P: AsRef<Path>>(path: P) -> Result<Vec<Document>> {
    let stream = open_ban_csv(path)?;
    let mut aggregator = BanAggregator::new();
    for record_res in stream {
        let record = record_res?;
        aggregator.add_record(record);
    }
    Ok(aggregator.into_documents())
}

/// Read a BAN CSV stream and aggregate all records into hierarchical `Document`s.
pub fn aggregate_ban_csv_reader(reader: Box<dyn Read + Send>) -> Result<Vec<Document>> {
    let stream = open_ban_csv_reader(reader)?;
    let mut aggregator = BanAggregator::new();
    for record_res in stream {
        let record = record_res?;
        aggregator.add_record(record);
    }
    Ok(aggregator.into_documents())
}

/// Streaming aggregator that processes BAN records with a bounded window of active streets,
/// drastically reducing memory usage when indexing large datasets.
pub struct StreamingBanAggregator<I> {
    records: I,
    aggregator: BanAggregator,
    order: std::collections::VecDeque<String>,
    max_active_streets: usize,
    exhausted: bool,
}

impl<I> StreamingBanAggregator<I>
where
    I: Iterator<Item = Result<BanRecord>>,
{
    pub fn new(records: I, max_active_streets: usize) -> Self {
        Self {
            records,
            aggregator: BanAggregator::new(),
            order: std::collections::VecDeque::new(),
            max_active_streets: max_active_streets.max(100),
            exhausted: false,
        }
    }
}

impl<I> Iterator for StreamingBanAggregator<I>
where
    I: Iterator<Item = Result<BanRecord>>,
{
    type Item = Result<Document>;

    fn next(&mut self) -> Option<Self::Item> {
        while !self.exhausted {
            match self.records.next() {
                Some(Ok(rec)) => {
                    let parent_id = if rec.numero.as_ref().is_some_and(|n| !n.trim().is_empty()) {
                        extract_parent_street_id(&rec)
                    } else {
                        rec.id.clone()
                    };

                    if !self.aggregator.documents.contains_key(&parent_id) {
                        self.order.push_back(parent_id);
                    }
                    self.aggregator.add_record(rec);

                    if self.aggregator.documents.len() > self.max_active_streets {
                        if let Some(oldest_id) = self.order.pop_front() {
                            if let Some(doc) = self.aggregator.documents.remove(&oldest_id) {
                                return Some(Ok(doc));
                            }
                        }
                    }
                }
                Some(Err(e)) => return Some(Err(e)),
                None => {
                    self.exhausted = true;
                    break;
                }
            }
        }

        // Flush remaining buffered documents
        while let Some(oldest_id) = self.order.pop_front() {
            if let Some(doc) = self.aggregator.documents.remove(&oldest_id) {
                return Some(Ok(doc));
            }
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_aggregate_ban_records() {
        let r1 = BanRecord {
            id: "75105_0001_00008".to_string(),
            address_type: None,
            numero: Some("8".to_string()),
            rep: None,
            nom_voie: "Boulevard Saint-Michel".to_string(),
            code_postal: "75005".to_string(),
            code_insee: Some("75105".to_string()),
            nom_commune: "Paris".to_string(),
            code_insee_commune_deleguee: None,
            nom_commune_deleguee: None,
            x: Some(652100.0),
            y: Some(6861500.0),
            lon: Some(2.34310),
            lat: Some(48.85120),
            type_position: Some("entrée".to_string()),
            alias: None,
            nom_afnor: None,
            libelle_acheminement: None,
            nom_complementaire: None,
        };

        let r2 = BanRecord {
            id: "75105_0001_00010".to_string(),
            address_type: None,
            numero: Some("10".to_string()),
            rep: None,
            nom_voie: "Boulevard Saint-Michel".to_string(),
            code_postal: "75005".to_string(),
            code_insee: Some("75105".to_string()),
            nom_commune: "Paris".to_string(),
            code_insee_commune_deleguee: None,
            nom_commune_deleguee: None,
            x: Some(652110.0),
            y: Some(6861510.0),
            lon: Some(2.34325),
            lat: Some(48.85135),
            type_position: Some("entrée".to_string()),
            alias: None,
            nom_afnor: None,
            libelle_acheminement: None,
            nom_complementaire: None,
        };

        let r3 = BanRecord {
            id: "75105_0001_00012".to_string(),
            address_type: None,
            numero: Some("12".to_string()),
            rep: Some("bis".to_string()),
            nom_voie: "Boulevard Saint-Michel".to_string(),
            code_postal: "75005".to_string(),
            code_insee: Some("75105".to_string()),
            nom_commune: "Paris".to_string(),
            code_insee_commune_deleguee: None,
            nom_commune_deleguee: None,
            x: Some(652120.0),
            y: Some(6861520.0),
            lon: Some(2.34340),
            lat: Some(48.85150),
            type_position: Some("entrée".to_string()),
            alias: None,
            nom_afnor: None,
            libelle_acheminement: None,
            nom_complementaire: None,
        };

        let docs = aggregate_ban_records(vec![r1, r2, r3]);
        assert_eq!(docs.len(), 1);
        let doc = &docs[0];
        assert_eq!(doc.id, "75105_0001");
        assert_eq!(doc.name, "Boulevard Saint-Michel");
        assert_eq!(doc.address_type.as_deref(), Some("street"));
        assert_eq!(doc.postcode.as_deref(), Some("75005"));
        assert_eq!(doc.city.as_deref(), Some("Paris"));
        assert_eq!(doc.housenumbers.len(), 3);
        assert!(doc.housenumbers.contains_key("8"));
        assert!(doc.housenumbers.contains_key("10"));
        assert!(doc.housenumbers.contains_key("12 bis"));
    }

    #[test]
    fn test_aggregate_ban_records_with_repetition_suffixes() {
        let r1 = BanRecord {
            id: "75106_8705_00080".to_string(),
            address_type: None,
            numero: Some("80".to_string()),
            rep: None,
            nom_voie: "Boulevard Saint-Michel".to_string(),
            code_postal: "75006".to_string(),
            code_insee: Some("75106".to_string()),
            nom_commune: "Paris".to_string(),
            code_insee_commune_deleguee: None,
            nom_commune_deleguee: None,
            x: Some(651413.0),
            y: Some(6860519.0),
            lon: Some(2.33798),
            lat: Some(48.84289),
            type_position: Some("parcelle".to_string()),
            alias: None,
            nom_afnor: None,
            libelle_acheminement: None,
            nom_complementaire: None,
        };

        let r2 = BanRecord {
            id: "75106_8705_00080_a".to_string(),
            address_type: None,
            numero: Some("80".to_string()),
            rep: Some("A".to_string()),
            nom_voie: "Boulevard Saint-Michel".to_string(),
            code_postal: "75006".to_string(),
            code_insee: Some("75106".to_string()),
            nom_commune: "Paris".to_string(),
            code_insee_commune_deleguee: None,
            nom_commune_deleguee: None,
            x: Some(651415.0),
            y: Some(6860522.0),
            lon: Some(2.33801),
            lat: Some(48.84292),
            type_position: Some("parcelle".to_string()),
            alias: None,
            nom_afnor: None,
            libelle_acheminement: None,
            nom_complementaire: None,
        };

        let docs = aggregate_ban_records(vec![r1, r2]);
        assert_eq!(
            docs.len(),
            1,
            "Both 80 and 80 A must group under the single street 75106_8705"
        );
        let doc = &docs[0];
        assert_eq!(doc.id, "75106_8705");
        assert_eq!(doc.housenumbers.len(), 2);
        assert!(doc.housenumbers.contains_key("80"));
        assert!(doc.housenumbers.contains_key("80 A"));
    }
}
