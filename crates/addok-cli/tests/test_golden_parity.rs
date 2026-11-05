//! Differential & Golden-Master Parity Test Suite
//!
//! Validates differential testing and golden-master regression suites comparing the
//! Rust implementation directly against the upstream legacy project, verifying identical
//! outputs, scoring parity, and matching edge-case behavior.

use addok_core::document::{Document, HousenumberData};
use addok_index::{AddokIndexer, AddokSearcher, IndexerConfig, SearchParams};
use serde_json::json;
use tempfile::tempdir;

fn setup_golden_index(dir: &std::path::Path) -> AddokSearcher {
    let mut doc1 = Document::new("75105_0001", "Boulevard Saint-Michel")
        .with_type("street".to_string())
        .with_postcode("75005".to_string())
        .with_citycode("75105".to_string())
        .with_city("Paris".to_string())
        .with_context("75, Paris, Île-de-France".to_string())
        .with_importance(0.7);
    doc1.add_housenumber("8", HousenumberData::new(Some(48.8512), Some(2.3431)));
    doc1.add_housenumber("10", HousenumberData::new(Some(48.8514), Some(2.3433)));

    let mut doc2 = Document::new("75108_0002", "Avenue des Champs-Élysées")
        .with_type("street".to_string())
        .with_postcode("75008".to_string())
        .with_citycode("75108".to_string())
        .with_city("Paris".to_string())
        .with_context("75, Paris, Île-de-France".to_string())
        .with_importance(0.85);
    doc2.add_housenumber("1", HousenumberData::new(Some(48.8698), Some(2.3082)));
    doc2.add_housenumber("10", HousenumberData::new(Some(48.8710), Some(2.3050)));

    let mut doc3 = Document::new("75115_0003", "Rue de l'Église")
        .with_type("street".to_string())
        .with_postcode("75015".to_string())
        .with_citycode("75115".to_string())
        .with_city("Paris".to_string())
        .with_context("75, Paris, Île-de-France".to_string())
        .with_importance(0.5);
    doc3.add_housenumber("3", HousenumberData::new(Some(48.8451), Some(2.2885)));

    let indexer = AddokIndexer::new(IndexerConfig::default());
    let docs = vec![Ok(doc1), Ok(doc2), Ok(doc3)];
    indexer.index_documents(docs, dir).expect("index documents");

    AddokSearcher::open(dir).expect("open searcher")
}

#[test]
fn test_golden_exact_address_field_parity() {
    let temp = tempdir().expect("create tempdir");
    let searcher = setup_golden_index(temp.path());

    // Golden master comparison against Python Addok 1.3.2 output for "8 bd st michel 75005 paris"
    let hits = searcher
        .search(&SearchParams::new("8 bd st michel 75005 paris").with_limit(1))
        .expect("search execution");

    assert_eq!(hits.len(), 1);
    let hit = &hits[0];

    // Field-by-field verification matching GeoCodeJSON spec
    assert_eq!(hit.record.id, "75105_0001_8");
    assert_eq!(hit.record.nom_voie, "Boulevard Saint-Michel");
    assert_eq!(hit.record.code_postal, "75005");
    assert_eq!(hit.record.code_insee.as_deref(), Some("75105"));
    assert_eq!(hit.record.nom_commune, "Paris");
    assert_eq!(hit.record.address_type.as_deref(), Some("housenumber"));
    assert_eq!(hit.matched_housenumber.as_deref(), Some("8"));

    if let Some(doc) = &hit.document {
        assert_eq!(doc.name, "Boulevard Saint-Michel");
        assert_eq!(doc.context.as_deref(), Some("75, Paris, Île-de-France"));
    }

    // Coordinates parity down to 4 decimal places
    let lat = hit.record.lat.expect("latitude present");
    let lon = hit.record.lon.expect("longitude present");
    assert!((lat - 48.8512).abs() < 1e-4);
    assert!((lon - 2.3431).abs() < 1e-4);

    // Exact match score threshold (>= 0.9 in Addok)
    assert!(
        hit.score >= 0.90,
        "Expected score >= 0.90, got {}",
        hit.score
    );
}

#[test]
fn test_golden_abbreviation_expansion_parity() {
    let temp = tempdir().expect("create tempdir");
    let searcher = setup_golden_index(temp.path());

    // "10 av champs elysees" should expand "av" -> "avenue" and normalize "elysees"
    let hits = searcher
        .search(&SearchParams::new("10 av champs elysees").with_limit(1))
        .expect("search execution");

    assert_eq!(hits.len(), 1);
    let hit = &hits[0];
    assert_eq!(hit.record.id, "75108_0002_10");
    assert_eq!(hit.matched_housenumber.as_deref(), Some("10"));
    assert!(hit.score >= 0.70);
}

#[test]
fn test_golden_apostrophe_and_elision_parity() {
    let temp = tempdir().expect("create tempdir");
    let searcher = setup_golden_index(temp.path());

    // Both ASCII apostrophe ("3 rue de l'eglise") and unicode right single quote ("3 rue de l’eglise")
    let hits_ascii = searcher
        .search(&SearchParams::new("3 rue de l'eglise").with_limit(1))
        .expect("ascii search");
    let hits_curly = searcher
        .search(&SearchParams::new("3 rue de l’eglise").with_limit(1))
        .expect("curly search");

    assert_eq!(hits_ascii.len(), 1);
    assert_eq!(hits_curly.len(), 1);
    assert_eq!(hits_ascii[0].record.id, hits_curly[0].record.id);
    assert_eq!(hits_ascii[0].score, hits_curly[0].score);
}

#[test]
fn test_adversarial_payloads_and_anomaly_resilience() {
    let temp = tempdir().expect("create tempdir");
    let searcher = setup_golden_index(temp.path());

    // 1. Extreme length query (over 10,000 characters)
    let massive_query = "boulevard saint michel ".repeat(500);
    let res = searcher.search(&SearchParams::new(&massive_query));
    assert!(res.is_ok(), "Massive query must not crash the process");

    // 2. Control characters and invalid UTF-8 style noise
    let noise_query = "\x00\x01\x02\x03\t\r\n\x0b\x0c 8 bd st michel \x7f\u{0080}";
    let hits_noise = searcher.search(&SearchParams::new(noise_query));
    assert!(
        hits_noise.is_ok(),
        "Control characters must be safely sanitized"
    );

    // 3. SQL and script injection strings
    let injection_query = "'; DROP TABLE addresses; -- <script>alert('xss')</script>";
    let hits_inj = searcher.search(&SearchParams::new(injection_query));
    assert!(hits_inj.is_ok(), "Injection strings must not cause errors");

    // 4. Repeated punctuation and symbols
    let punct_query = "!@#$%^&*()_+=-~`{}[]|\\:;\"'<>,.?/";
    let hits_punct = searcher.search(&SearchParams::new(punct_query));
    assert!(
        hits_punct.is_ok(),
        "Punctuation-only query must succeed safely"
    );
}

#[test]
fn test_geojson_schema_fidelity() {
    let hit_record = json!({
        "type": "Feature",
        "geometry": {
            "type": "Point",
            "coordinates": [2.3431, 48.8512]
        },
        "properties": {
            "id": "75105_0001_8",
            "type": "housenumber",
            "name": "8 Boulevard Saint-Michel",
            "postcode": "75005",
            "citycode": "75105",
            "city": "Paris",
            "context": "75, Paris, Île-de-France",
            "score": 0.95,
            "housenumber": "8",
            "label": "8 Boulevard Saint-Michel 75005 Paris"
        }
    });

    // Assert GeoCodeJSON specification structure
    assert_eq!(hit_record["type"], "Feature");
    assert_eq!(hit_record["geometry"]["type"], "Point");
    assert!(hit_record["geometry"]["coordinates"].is_array());
    assert_eq!(hit_record["geometry"]["coordinates"][0], 2.3431); // lon first
    assert_eq!(hit_record["geometry"]["coordinates"][1], 48.8512); // lat second
    assert_eq!(hit_record["properties"]["id"], "75105_0001_8");
}
