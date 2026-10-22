use addok_core::document::{Document, HousenumberData};
use addok_index::{AddokIndexer, AddokSearcher, IndexerConfig, SearchParams};
use tempfile::tempdir;

fn build_test_index(dir: &std::path::Path) -> AddokSearcher {
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

    let mut doc3 = Document::new("75115_0003", "Rue de l'Église")
        .with_type("street".to_string())
        .with_postcode("75015".to_string())
        .with_city("Paris".to_string())
        .with_importance(0.5);
    doc3.add_housenumber("3", HousenumberData::new(Some(48.8451), Some(2.2885)));

    // Duplicate street name in Lyon to test geo proximity biasing
    let mut doc4 = Document::new("69002_0004", "Rue de l'Église")
        .with_type("street".to_string())
        .with_postcode("69002".to_string())
        .with_city("Lyon".to_string())
        .with_importance(0.5);
    doc4.add_housenumber("3", HousenumberData::new(Some(45.7578), Some(4.8320)));

    let indexer = AddokIndexer::new(IndexerConfig::default());
    let docs = vec![Ok(doc1), Ok(doc2), Ok(doc3), Ok(doc4)];
    indexer.index_documents(docs, dir).unwrap();

    AddokSearcher::open(dir).unwrap()
}

#[test]
fn test_exact_and_abbreviated_search() {
    let temp = tempdir().unwrap();
    let searcher = build_test_index(temp.path());

    // 1. Exact street + hn query (without postcode/city)
    let hits1 = searcher
        .search(&SearchParams::new("8 Boulevard Saint-Michel"))
        .unwrap();
    assert!(!hits1.is_empty());
    assert_eq!(hits1[0].record.id, "75105_0001_8");
    assert_eq!(hits1[0].matched_housenumber.as_deref(), Some("8"));
    assert!(hits1[0].score >= 0.65);

    // Full address query (with postcode and city) yields >= 0.95
    let hits_full = searcher
        .search(&SearchParams::new("8 Boulevard Saint-Michel 75005 Paris"))
        .unwrap();
    assert!(!hits_full.is_empty());
    assert!(hits_full[0].score >= 0.95);

    // 2. Abbreviated query
    let hits2 = searcher
        .search(&SearchParams::new("8 bd st michel"))
        .unwrap();
    assert!(!hits2.is_empty());
    assert_eq!(hits2[0].record.id, "75105_0001_8");
    assert_eq!(hits2[0].matched_housenumber.as_deref(), Some("8"));

    // 3. Trailing housenumber: "bd st michel 8"
    let hits3 = searcher
        .search(&SearchParams::new("bd st michel 8"))
        .unwrap();
    assert!(!hits3.is_empty());
    assert_eq!(hits3[0].record.id, "75105_0001_8");
    assert_eq!(hits3[0].matched_housenumber.as_deref(), Some("8"));
}

#[test]
fn test_apostrophe_and_stop_words_search() {
    let temp = tempdir().unwrap();
    let searcher = build_test_index(temp.path());

    // "rue de l'eglise" with stop words
    let hits = searcher
        .search(&SearchParams::new("rue de l'eglise"))
        .unwrap();
    assert!(!hits.is_empty());
    assert!(hits[0].record.nom_voie.contains("Église"));

    // without stop words "rue eglise"
    let hits2 = searcher.search(&SearchParams::new("rue eglise")).unwrap();
    assert!(!hits2.is_empty());
    assert!(hits2[0].record.nom_voie.contains("Église"));
}

#[test]
fn test_autocomplete_prefix_expansion() {
    let temp = tempdir().unwrap();
    let searcher = build_test_index(temp.path());

    // Prefix "mich" should expand via Edge N-Grams and Pairs to "michel"
    let params = SearchParams::new("bd st mich");
    let hits = searcher.search(&params).unwrap();
    assert!(!hits.is_empty());
    assert_eq!(hits[0].record.id, "75105_0001");
}

#[test]
fn test_geo_proximity_biasing() {
    let temp = tempdir().unwrap();
    let searcher = build_test_index(temp.path());

    // Both Paris (75015) and Lyon (69002) have "Rue de l'Église".
    // 1. Biasing near Paris (lat: 48.85, lon: 2.30)
    let params_paris = SearchParams::new("3 rue de l'eglise").with_geo_bias(48.85, 2.30);
    let hits_paris = searcher.search(&params_paris).unwrap();
    assert!(!hits_paris.is_empty());
    assert_eq!(hits_paris[0].record.code_postal, "75015");

    // 2. Biasing near Lyon (lat: 45.75, lon: 4.83)
    let params_lyon = SearchParams::new("3 rue de l'eglise").with_geo_bias(45.75, 4.83);
    let hits_lyon = searcher.search(&params_lyon).unwrap();
    assert!(!hits_lyon.is_empty());
    assert_eq!(hits_lyon[0].record.code_postal, "69002");
}

#[test]
fn test_postcode_and_category_filters() {
    let temp = tempdir().unwrap();
    let searcher = build_test_index(temp.path());

    let mut params = SearchParams::new("rue de l'eglise");
    params.postcode = Some("69002".to_string());
    let hits = searcher.search(&params).unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].record.code_postal, "69002");
    assert_eq!(hits[0].record.nom_commune, "Lyon");
}

#[test]
fn test_fuzzy_search_typo_tolerance() {
    let temp = tempdir().unwrap();
    let searcher = build_test_index(temp.path());

    // "champs elysees" with typo: "chams elysees"
    let hits = searcher
        .search(&SearchParams::new("chams elysees"))
        .unwrap();
    assert!(!hits.is_empty());
    assert_eq!(hits[0].record.id, "75108_0002");
}

#[test]
fn test_empty_query_returns_empty() {
    let temp = tempdir().unwrap();
    let searcher = build_test_index(temp.path());

    let hits = searcher.search(&SearchParams::new("   ")).unwrap();
    assert!(hits.is_empty());
}
