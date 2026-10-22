use std::collections::HashMap;
use tempfile::tempdir;

use addok_core::document::{Document, HousenumberData};
use addok_index::{AddokIndexer, AddokSearcher, IndexerConfig, SearchParams};

fn create_test_index(dir: &std::path::Path) {
    let mut doc1 = Document::new("75105_0001", "Boulevard Saint-Michel")
        .with_type("street")
        .with_postcode("75005")
        .with_citycode("75105")
        .with_city("Paris")
        .with_importance(0.6);

    let mut hn_map1 = HashMap::new();
    hn_map1.insert(
        "8".to_string(),
        HousenumberData::with_coords(48.8512, 2.3431),
    );
    hn_map1.insert(
        "8 bis".to_string(),
        HousenumberData::with_coords(48.8513, 2.3432),
    );
    hn_map1.insert(
        "10".to_string(),
        HousenumberData::with_coords(48.8518, 2.3439),
    );
    doc1 = doc1.with_housenumbers(hn_map1);

    let mut doc2 = Document::new("75108_0002", "Avenue des Champs-Élysées")
        .with_type("street")
        .with_postcode("75008")
        .with_citycode("75108")
        .with_city("Paris")
        .with_importance(0.8);
    doc2.extra
        .insert("lat".to_string(), serde_json::json!(48.8698));
    doc2.extra
        .insert("lon".to_string(), serde_json::json!(2.3082));

    let mut doc3 = Document::new("75110_0003", "Rue de la Gare")
        .with_type("street")
        .with_postcode("75010")
        .with_citycode("75110")
        .with_city("Paris")
        .with_importance(0.4);
    // Gare de l'Est / 10e: (lat: 48.8760, lon: 2.3580)
    doc3.extra
        .insert("lat".to_string(), serde_json::json!(48.8760));
    doc3.extra
        .insert("lon".to_string(), serde_json::json!(2.3580));

    let mut doc4 = Document::new("75115_0004", "Rue de la Gare")
        .with_type("street")
        .with_postcode("75015")
        .with_citycode("75115")
        .with_city("Paris")
        .with_importance(0.4);
    // 15e: (lat: 48.8400, lon: 2.2900)
    doc4.extra
        .insert("lat".to_string(), serde_json::json!(48.8400));
    doc4.extra
        .insert("lon".to_string(), serde_json::json!(2.2900));

    let mut doc5 = Document::new("75056_0000", "Paris")
        .with_type("municipality")
        .with_postcode("75001")
        .with_citycode("75056")
        .with_city("Paris")
        .with_importance(1.0);
    doc5.extra
        .insert("lat".to_string(), serde_json::json!(48.8566));
    doc5.extra
        .insert("lon".to_string(), serde_json::json!(2.3522));

    let docs = vec![Ok(doc1), Ok(doc2), Ok(doc3), Ok(doc4), Ok(doc5)];

    let indexer = AddokIndexer::new(IndexerConfig::default());
    indexer.index_documents(docs, dir).unwrap();
}

#[test]
fn test_exact_phrase_search() {
    let temp = tempdir().unwrap();
    let index_dir = temp.path().join("exact_idx");
    create_test_index(&index_dir);

    let searcher = AddokSearcher::open(&index_dir).unwrap();

    // 1. Exact phrase with accents and title case
    let hits1 = searcher
        .search(&SearchParams::new("Avenue des Champs-Élysées"))
        .unwrap();
    assert!(!hits1.is_empty());
    assert_eq!(hits1[0].record.id, "75108_0002");
    assert!(hits1[0].score >= 0.65);

    let hits_full = searcher
        .search(&SearchParams::new("Avenue des Champs-Élysées 75008 Paris"))
        .unwrap();
    assert!(!hits_full.is_empty());
    assert!(hits_full[0].score >= 0.85);

    // 2. Abbreviated exact phrase: "bd st michel"
    let hits2 = searcher.search(&SearchParams::new("bd st michel")).unwrap();
    assert!(!hits2.is_empty());
    assert_eq!(hits2[0].record.id, "75105_0001");
    assert_eq!(hits2[0].record.nom_voie, "Boulevard Saint-Michel");
}

#[test]
fn test_prefix_search_and_autocomplete() {
    let temp = tempdir().unwrap();
    let index_dir = temp.path().join("prefix_idx");
    create_test_index(&index_dir);

    let searcher = AddokSearcher::open(&index_dir).unwrap();

    // 1. Autocomplete enabled: "8 bd st mich" expands to "michel"
    let params_ac = SearchParams::new("8 bd st mich");
    let hits_ac = searcher.search(&params_ac).unwrap();
    assert!(!hits_ac.is_empty());
    assert_eq!(hits_ac[0].record.nom_voie, "Boulevard Saint-Michel");
    assert_eq!(hits_ac[0].record.numero.as_deref(), Some("8"));
    assert_eq!(hits_ac[0].record.lat, Some(48.8512));
    assert_eq!(hits_ac[0].record.lon, Some(2.3431));

    // 2. Autocomplete enabled: "champs ely" expands to "elysees"
    let params_champs = SearchParams::new("champs ely");
    let hits_champs = searcher.search(&params_champs).unwrap();
    assert!(!hits_champs.is_empty());
    assert_eq!(hits_champs[0].record.id, "75108_0002");
}

#[test]
fn test_typo_fuzzy_search() {
    let temp = tempdir().unwrap();
    let index_dir = temp.path().join("typo_idx");
    create_test_index(&index_dir);

    let searcher = AddokSearcher::open(&index_dir).unwrap();

    // Typo: "8 bld st michl" (missing 'e' in michel)
    let params_typo = SearchParams::new("8 bld st michl");
    let hits_typo = searcher.search(&params_typo).unwrap();
    assert!(
        !hits_typo.is_empty(),
        "Fuzzy search should find Boulevard Saint-Michel despite typo"
    );
    assert_eq!(hits_typo[0].record.nom_voie, "Boulevard Saint-Michel");
    assert_eq!(hits_typo[0].record.numero.as_deref(), Some("8"));
}

#[test]
fn test_housenumber_matching_and_coordinates() {
    let temp = tempdir().unwrap();
    let index_dir = temp.path().join("hn_idx");
    create_test_index(&index_dir);

    let searcher = AddokSearcher::open(&index_dir).unwrap();

    // 1. Match #8
    let hits_8 = searcher
        .search(&SearchParams::new("8 bd st michel"))
        .unwrap();
    assert!(!hits_8.is_empty());
    assert_eq!(hits_8[0].record.numero.as_deref(), Some("8"));
    assert_eq!(hits_8[0].record.lat, Some(48.8512));
    assert_eq!(hits_8[0].record.lon, Some(2.3431));
    assert_eq!(hits_8[0].matched_housenumber.as_deref(), Some("8"));

    // 2. Match #8 bis
    let hits_8bis = searcher
        .search(&SearchParams::new("8 bis bd st michel"))
        .unwrap();
    assert!(!hits_8bis.is_empty());
    assert_eq!(hits_8bis[0].record.lat, Some(48.8513));
    assert_eq!(hits_8bis[0].record.lon, Some(2.3432));
    assert_eq!(hits_8bis[0].matched_housenumber.as_deref(), Some("8 bis"));

    // 3. Match #10
    let hits_10 = searcher
        .search(&SearchParams::new("10 bd st michel"))
        .unwrap();
    assert!(!hits_10.is_empty());
    assert_eq!(hits_10[0].record.numero.as_deref(), Some("10"));
    assert_eq!(hits_10[0].record.lat, Some(48.8518));
    assert_eq!(hits_10[0].record.lon, Some(2.3439));

    // 4. Non-existent housenumber: "999 bd st michel" falls back to street
    let hits_999 = searcher
        .search(&SearchParams::new("999 bd st michel"))
        .unwrap();
    assert!(!hits_999.is_empty());
    assert_eq!(hits_999[0].record.nom_voie, "Boulevard Saint-Michel");
    assert_eq!(hits_999[0].matched_housenumber, None);
    // Score should be lower than when housenumber was matched
    assert!(hits_999[0].score < hits_8[0].score);
}

#[test]
fn test_geo_biasing_proximity() {
    let temp = tempdir().unwrap();
    let index_dir = temp.path().join("geo_idx");
    create_test_index(&index_dir);

    let searcher = AddokSearcher::open(&index_dir).unwrap();

    // Query "rue de la gare" exists in both 75010 (lat: 48.8760, lon: 2.3580)
    // and 75015 (lat: 48.8400, lon: 2.2900).

    // 1. Bias near 10e (48.8755, 2.3575) -> 75010 should be ranked #1
    let params_10e = SearchParams::new("rue de la gare")
        .with_geo_bias(48.8755, 2.3575)
        .with_limit(2);
    let hits_10e = searcher.search(&params_10e).unwrap();
    assert_eq!(hits_10e.len(), 2);
    assert_eq!(hits_10e[0].record.id, "75110_0003");
    assert_eq!(hits_10e[1].record.id, "75115_0004");
    assert!(hits_10e[0].score > hits_10e[1].score);

    // 2. Bias near 15e (48.8405, 2.2905) -> 75015 should be ranked #1
    let params_15e = SearchParams::new("rue de la gare")
        .with_geo_bias(48.8405, 2.2905)
        .with_limit(2);
    let hits_15e = searcher.search(&params_15e).unwrap();
    assert_eq!(hits_15e.len(), 2);
    assert_eq!(hits_15e[0].record.id, "75115_0004");
    assert_eq!(hits_15e[1].record.id, "75110_0003");
    assert!(hits_15e[0].score > hits_15e[1].score);
}

#[test]
fn test_multi_value_filters_or_and_logic() {
    let temp = tempdir().unwrap();
    let index_dir = temp.path().join("filters_idx");
    create_test_index(&index_dir);

    let searcher = AddokSearcher::open(&index_dir).unwrap();

    // 1. Filter: type = street+municipality (OR logic)
    let params_or = SearchParams::new("paris")
        .with_filter("type", "street+municipality")
        .with_limit(10);
    let hits_or = searcher.search(&params_or).unwrap();
    assert!(!hits_or.is_empty());
    // Municipality "Paris" (75056_0000) is included
    assert!(hits_or.iter().any(|h| h.record.id == "75056_0000"));
    // Streets in Paris are also included
    assert!(hits_or.iter().any(|h| h.record.id == "75105_0001"));

    // 2. Multi-filter AND logic: type = street AND postcode = 75005
    let params_and = SearchParams::new("paris")
        .with_filter("type", "street")
        .with_filter("postcode", "75005")
        .with_limit(10);
    let hits_and = searcher.search(&params_and).unwrap();
    assert_eq!(hits_and.len(), 1);
    assert_eq!(hits_and[0].record.id, "75105_0001");
}

#[test]
fn test_multi_label_scoring_parity() {
    let temp = tempdir().unwrap();
    let index_dir = temp.path().join("score_parity_idx");
    create_test_index(&index_dir);

    let searcher = AddokSearcher::open(&index_dir).unwrap();

    // Query for street only without city/postcode: "Avenue des Champs-Élysées"
    // Because doc synthesizes labels including "Avenue des Champs-Élysées", this matches with str_score = 1.0!
    // With importance = 0.8: (1.0 + 0.8 * 0.1) / 1.1 = 1.08 / 1.1 ≈ 0.9818
    let hits_street = searcher
        .search(&SearchParams::new("Avenue des Champs-Élysées"))
        .unwrap();
    assert!(!hits_street.is_empty());
    assert_eq!(hits_street[0].record.id, "75108_0002");
    assert!(hits_street[0].score >= 0.95);

    // Query with housenumber and street only: "8 Boulevard Saint-Michel"
    // Matches synthesized label "8 Boulevard Saint-Michel" with str_score = 1.0!
    let hits_hn = searcher
        .search(&SearchParams::new("8 Boulevard Saint-Michel"))
        .unwrap();
    assert!(!hits_hn.is_empty());
    assert_eq!(hits_hn[0].record.id, "75105_0001_8");
    assert!(hits_hn[0].score >= 0.95);
}

#[test]
fn test_autocomplete_scoring_tiers() {
    use addok_core::normalizer::ascii_fold;
    use addok_index::scoring::compute_str_score;

    let labels = vec![
        "8 Boulevard Saint-Michel 75005 Paris".to_string(),
        "Boulevard Saint-Michel 75005 Paris".to_string(),
        "8 Boulevard Saint-Michel".to_string(),
        "Boulevard Saint-Michel".to_string(),
    ];

    // 1. Exact match in autocomplete mode -> 1.0
    let q1 = ascii_fold("8 Boulevard Saint-Michel");
    let score_exact = compute_str_score(&labels, &q1, &[], true, 0.9);
    assert_eq!(score_exact, 1.0);

    // 2. Prefix match in autocomplete mode -> 0.9
    let q2 = ascii_fold("8 Boulevard Saint");
    let score_prefix = compute_str_score(&labels, &q2, &[], true, 0.9);
    assert_eq!(score_prefix, 0.9);

    // 3. Substring containment match in autocomplete mode -> 0.7
    let q3 = ascii_fold("Saint-Michel");
    let score_contains = compute_str_score(&labels, &q3, &[], true, 0.9);
    assert_eq!(score_contains, 0.7);
}
