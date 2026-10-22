pub mod collector;
pub mod doc_store;
pub mod error;
pub mod fields_index;
pub mod filters;
pub mod housenumbers;
pub mod indexer;
pub mod ngram;
pub mod pairs;
pub mod query;
pub mod schema;
pub mod scoring;
pub mod searcher;
pub mod spatial;

pub use collector::{generate_fuzzy_neighbors, BucketCollector, CollectionTrace};
pub use doc_store::DocumentStore;
pub use error::{IndexError, Result};
pub use fields_index::{FieldBoostConfig, FieldsInvertedIndex};
pub use filters::FiltersIndex;
pub use housenumbers::{normalize_housenumber, HousenumbersIndex};
pub use indexer::{AddokIndexer, IndexStats, IndexerConfig};
pub use ngram::EdgeNgramIndex;
pub use pairs::PairsIndexer;
pub use query::ParsedQuery;
pub use schema::AddokSchema;
pub use scoring::{
    compare_ngrams, km_to_score, levenshtein_distance, levenshtein_ratio, score_candidate,
};
pub use searcher::{
    AddokSearcher, ExplainCandidate, ExplainResult, FuzzyVariantInfo, SearchHit, SearchParams,
    TokenFrequencyInfo,
};
pub use spatial::{haversine_distance_meters, SpatialIndex, SpatialItem, SpatialResult};

#[cfg(test)]
mod tests {
    use super::*;
    use addok_core::BanRecord;
    use tempfile::tempdir;

    #[test]
    fn test_index_and_search_roundtrip() {
        let temp_dir = tempdir().unwrap();
        let index_dir = temp_dir.path().join("test_index");

        let records = vec![
            Ok(BanRecord {
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
                lon: Some(2.3431),
                lat: Some(48.8512),
                type_position: Some("entrée".to_string()),
                alias: None,
                nom_afnor: Some("BD SAINT MICHEL".to_string()),
                libelle_acheminement: Some("PARIS".to_string()),
                nom_complementaire: None,
            }),
            Ok(BanRecord {
                id: "75108_0002_00010".to_string(),
                address_type: None,
                numero: Some("10".to_string()),
                rep: None,
                nom_voie: "Avenue des Champs-Élysées".to_string(),
                code_postal: "75008".to_string(),
                code_insee: Some("75108".to_string()),
                nom_commune: "Paris".to_string(),
                code_insee_commune_deleguee: None,
                nom_commune_deleguee: None,
                x: Some(649800.0),
                y: Some(6863200.0),
                lon: Some(2.3082),
                lat: Some(48.8698),
                type_position: Some("entrée".to_string()),
                alias: None,
                nom_afnor: Some("AV DES CHAMPS ELYSEES".to_string()),
                libelle_acheminement: Some("PARIS".to_string()),
                nom_complementaire: None,
            }),
            Ok(BanRecord {
                id: "75115_0003_00003".to_string(),
                address_type: None,
                numero: Some("3".to_string()),
                rep: None,
                nom_voie: "Rue de l'Église".to_string(),
                code_postal: "75015".to_string(),
                code_insee: Some("75115".to_string()),
                nom_commune: "Paris".to_string(),
                code_insee_commune_deleguee: None,
                nom_commune_deleguee: None,
                x: Some(648200.0),
                y: Some(6860100.0),
                lon: Some(2.2885),
                lat: Some(48.8451),
                type_position: Some("entrée".to_string()),
                alias: None,
                nom_afnor: Some("R DE L EGLISE".to_string()),
                libelle_acheminement: Some("PARIS".to_string()),
                nom_complementaire: None,
            }),
        ];

        let indexer = AddokIndexer::new(IndexerConfig::default());
        let stats = indexer.index_records(records, &index_dir).unwrap();
        assert_eq!(stats.indexed_records, 3);
        assert_eq!(stats.spatial_records, 6);

        let searcher = AddokSearcher::open(&index_dir).unwrap();

        // 1. Search with abbreviated query "8 bd st michel"
        let params = SearchParams::new("8 bd st michel");
        let hits = searcher.search(&params).unwrap();
        assert!(!hits.is_empty());
        assert_eq!(hits[0].record.id, "75105_0001_00008");
        assert_eq!(hits[0].record.nom_voie, "Boulevard Saint-Michel");

        // 2. Search "avenue des champs elysees"
        let params2 = SearchParams::new("champs elysees");
        let hits2 = searcher.search(&params2).unwrap();
        assert!(!hits2.is_empty());
        assert_eq!(hits2[0].record.id, "75108_0002");

        // 3. Search "rue de l'eglise"
        let params3 = SearchParams::new("3 rue eglise");
        let hits3 = searcher.search(&params3).unwrap();
        assert!(!hits3.is_empty());
        assert_eq!(hits3[0].record.id, "75115_0003_00003");

        // 4. Reverse geocoding test near Champs-Élysées
        let rev_hits = searcher.reverse(48.8697, 2.3081, 1);
        assert_eq!(rev_hits.len(), 1);
        assert_eq!(rev_hits[0].item.id, "75108_0002_00010");
        assert!(rev_hits[0].distance_meters < 50.0);

        // 5. Explain diagnostic test
        let explain = searcher.explain(&params).unwrap();
        assert_eq!(explain.raw_query, "8 bd st michel");
        assert_eq!(explain.extracted_housenumber.as_deref(), Some("8"));
        assert!(!explain.token_frequencies.is_empty());
        assert!(!explain.candidates.is_empty());
        assert_eq!(explain.candidates[0].id, "75105_0001");
        assert_eq!(
            explain.candidates[0].matched_housenumber.as_deref(),
            Some("8")
        );
    }
}
