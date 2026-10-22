use addok_core::document::{Document, HousenumberData};
use addok_index::{AddokIndexer, AddokSearcher, IndexerConfig};
use tempfile::tempdir;

fn build_reverse_test_index(dir: &std::path::Path) -> AddokSearcher {
    let mut doc1 = Document::new("75105_0001", "Boulevard Saint-Michel")
        .with_type("street".to_string())
        .with_postcode("75005".to_string())
        .with_city("Paris".to_string())
        .with_importance(0.6);
    doc1.add_housenumber("8", HousenumberData::new(Some(48.8512), Some(2.3431)));
    doc1.add_housenumber("10", HousenumberData::new(Some(48.8514), Some(2.3433)));

    let mut doc2 = Document::new("75108_0002", "Avenue des Champs-Élysées")
        .with_type("street".to_string())
        .with_postcode("75008".to_string())
        .with_city("Paris".to_string())
        .with_importance(0.8);
    doc2.add_housenumber("1", HousenumberData::new(Some(48.8698), Some(2.3082)));

    let indexer = AddokIndexer::new(IndexerConfig::default());
    let docs = vec![Ok(doc1), Ok(doc2)];
    indexer.index_documents(docs, dir).unwrap();

    AddokSearcher::open(dir).unwrap()
}

#[test]
fn test_reverse_exact_coordinate() {
    // Ported from Addok's test_reverse.py
    let temp = tempdir().unwrap();
    let searcher = build_reverse_test_index(temp.path());

    // Coordinates of 8 Boulevard Saint-Michel: (48.8512, 2.3431)
    let hits = searcher.reverse(48.8512, 2.3431, 1);
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].item.id, "75105_0001_8");
    assert!(hits[0].distance_meters < 1.0);
}

#[test]
fn test_reverse_nearest_neighbor_ordering() {
    let temp = tempdir().unwrap();
    let searcher = build_reverse_test_index(temp.path());

    // Near Saint-Michel (48.8513, 2.3432):
    // Should return Saint-Michel first (< 20m) and Champs-Élysées second (> 3km)
    let hits = searcher.reverse(48.8513, 2.3432, 5);
    assert!(!hits.is_empty());
    assert!(hits[0].item.id.starts_with("75105_0001"));
    assert!(hits[0].distance_meters < 30.0);

    if hits.len() > 1 {
        assert!(hits[1].distance_meters > hits[0].distance_meters);
    }
}

#[test]
fn test_reverse_limit_parameter() {
    let temp = tempdir().unwrap();
    let searcher = build_reverse_test_index(temp.path());

    let hits1 = searcher.reverse(48.8512, 2.3431, 1);
    assert_eq!(hits1.len(), 1);

    let hits2 = searcher.reverse(48.8512, 2.3431, 2);
    assert!(hits2.len() <= 2);
}

#[test]
fn test_reverse_geojson_formatting() {
    let temp = tempdir().unwrap();
    let searcher = build_reverse_test_index(temp.path());

    let hits = searcher.reverse(48.8512, 2.3431, 2);
    let collection = addok_server::api::FeatureCollection::from_spatial_results(hits, 2);
    assert_eq!(collection.collection_type, "FeatureCollection");
    assert!(!collection.features.is_empty());
    let geom = collection.features[0].geometry.as_ref().unwrap();
    assert_eq!(geom.point_type, "Point");
    assert!((geom.coordinates[0] - 2.3431).abs() < 1e-4);
    assert!((geom.coordinates[1] - 48.8512).abs() < 1e-4);
}

#[test]
fn test_reverse_filtered_by_type() {
    let temp = tempdir().unwrap();
    let searcher = build_reverse_test_index(temp.path());

    // Filter by housenumber -> returns 8 bd saint-michel
    let hn_filter = vec!["housenumber".to_string()];
    let hits_hn = searcher.reverse_filtered(48.8512, 2.3431, 5, Some(&hn_filter));
    assert!(!hits_hn.is_empty());
    for hit in &hits_hn {
        assert_eq!(hit.item.ban_type, "housenumber");
    }

    // Filter by non-existing type in this index -> returns empty
    let mun_filter = vec!["municipality".to_string()];
    let hits_mun = searcher.reverse_filtered(48.8512, 2.3431, 5, Some(&mun_filter));
    assert!(hits_mun.is_empty());
}
