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

#[test]
fn test_searcher_explain_postcode_append_and_fuzzy_variants() {
    let temp = tempdir().unwrap();
    let index_dir = temp.path().join("index");
    create_test_index(&index_dir);

    let searcher = AddokSearcher::open(&index_dir).unwrap();

    // 1. Explain with postcode in SearchParams + existing filter (tests lines 495-506 of searcher.rs)
    let mut params = SearchParams::new("8 bd st michel");
    params.postcode = Some("75005".to_string());
    params
        .filters
        .insert("postcode".to_string(), "75006".to_string());

    let diag = searcher.explain(&params).unwrap();
    assert_eq!(diag.raw_query, "8 bd st michel");
    assert!(!diag.candidates.is_empty());

    // 2. Explain with typo query triggering fuzzy variants (tests lines 561-564 of searcher.rs)
    let params_typo = SearchParams::new("8 boulevrad saint michel");
    let diag_typo = searcher.explain(&params_typo).unwrap();
    assert_eq!(diag_typo.raw_query, "8 boulevrad saint michel");
}

#[test]
fn test_searcher_reverse_parent_street_resolution() {
    let temp = tempdir().unwrap();
    let index_dir = temp.path().join("index");
    create_test_index(&index_dir);

    let searcher = AddokSearcher::open(&index_dir).unwrap();

    // Query reverse for type "street" when only housenumber points exist in spatial index
    let rev_street = searcher.reverse_filtered(48.8512, 2.3431, 1, Some(&["street".to_string()]));
    assert!(!rev_street.is_empty());
    assert_eq!(rev_street[0].item.ban_type, "street");
    assert_eq!(rev_street[0].item.id, "75105_0001");
}

#[test]
fn test_searcher_open_invalid_directory() {
    let invalid = std::path::Path::new("/nonexistent/directory/that/does/not/exist");
    assert!(AddokSearcher::open(invalid).is_err());
    assert!(AddokSearcher::open_with_config(invalid, addok_core::AddokConfig::default()).is_err());
}

#[test]
fn test_spatial_index_full_api() {
    use addok_index::spatial::{SpatialIndex, SpatialItem};

    let mut spatial = SpatialIndex::new();
    assert!(spatial.is_empty());
    assert_eq!(spatial.len(), 0);

    let items = vec![
        SpatialItem {
            point: [2.3431, 48.8512],
            id: "pt1".to_string(),
            label: "Point 1".to_string(),
            ban_type: "street".to_string(),
            postcode: "75005".to_string(),
            city: "Paris".to_string(),
            housenumber: None,
            citycode: Some("75105".to_string()),
        },
        SpatialItem {
            point: [2.3082, 48.8698],
            id: "pt2".to_string(),
            label: "Point 2".to_string(),
            ban_type: "street".to_string(),
            postcode: "75008".to_string(),
            city: "Paris".to_string(),
            housenumber: None,
            citycode: Some("75108".to_string()),
        },
    ];

    spatial = SpatialIndex::from_items(items);
    assert_eq!(spatial.len(), 2);
    assert!(!spatial.is_empty());

    // locate_within_distance
    let near = spatial.locate_within_distance(48.8512, 2.3431, 500.0);
    assert_eq!(near.len(), 1);
    assert_eq!(near[0].item.id, "pt1");

    // nearest_neighbors_with_filters
    let nn_postcode = spatial.nearest_neighbors_with_filters(
        48.8512,
        2.3431,
        5,
        None,
        Some(&["75008".to_string()]),
        None,
    );
    assert_eq!(nn_postcode.len(), 1);
    assert_eq!(nn_postcode[0].item.id, "pt2");

    let nn_citycode = spatial.nearest_neighbors_with_filters(
        48.8512,
        2.3431,
        5,
        None,
        None,
        Some(&["75105".to_string()]),
    );
    assert_eq!(nn_citycode.len(), 1);
    assert_eq!(nn_citycode[0].item.id, "pt1");

    // JSON save / load
    let temp = tempdir().unwrap();
    let json_file = temp.path().join("spatial.json");
    spatial.save_to_file(&json_file).unwrap();
    let loaded_json = SpatialIndex::load_from_file(&json_file).unwrap();
    assert_eq!(loaded_json.len(), 2);

    // Binary save / load
    let bin_file = temp.path().join("spatial.bin");
    spatial.save_to_bin(&bin_file).unwrap();
    let loaded_bin = SpatialIndex::load_from_bin(&bin_file).unwrap();
    assert_eq!(loaded_bin.len(), 2);

    // into_items
    let extracted = spatial.into_items();
    assert_eq!(extracted.len(), 2);
}

#[test]
fn test_fields_inverted_index_boosts_and_merge() {
    use addok_index::FieldsInvertedIndex;
    use roaring::RoaringBitmap;

    let mut idx1 = FieldsInvertedIndex::default();
    assert!(idx1.is_empty());
    assert_eq!(idx1.len(), 0);

    let mut doc1 = Document::new("d1", "Boulevard Saint-Michel")
        .with_postcode("75005")
        .with_city("Paris")
        .with_importance(0.5);
    doc1.add_housenumber("8", HousenumberData::with_coords(48.85, 2.34));
    doc1.extra
        .insert("context".to_string(), serde_json::json!("Paris IDF"));
    idx1.index_document(1, &doc1);

    let mut idx2 = FieldsInvertedIndex::default();
    let doc2 = Document::new("d2", "Avenue des Champs-Élysées")
        .with_city("Paris")
        .with_importance(0.8);
    idx2.index_document(2, &doc2);

    idx1.merge(idx2);
    assert!(!idx1.is_empty());

    // search_with_filter
    let mut filter_mask = RoaringBitmap::new();
    filter_mask.insert(1);
    let results = idx1.search_with_filter(&["saint", "michel", "paris"], Some(&filter_mask), 5);
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].0, 1);
    assert!(results[0].1 > 0.1);

    // search without filter
    let all_res = idx1.search(&["paris"], 5);
    assert_eq!(all_res.len(), 2);

    // save and load
    let temp = tempdir().unwrap();
    let bin_path = temp.path().join("fields.bin");
    idx1.save_to_file(&bin_path).unwrap();
    let loaded = FieldsInvertedIndex::load_from_file(&bin_path).unwrap();
    assert_eq!(loaded.len(), idx1.len());
}

#[test]
fn test_doc_store_update_existing_and_merge() {
    use addok_index::DocumentStore;

    let mut ds1 = DocumentStore::new();
    assert!(ds1.is_empty());
    assert_eq!(ds1.len(), 0);

    let doc1 = Document::new("doc_1", "Rue Initiale")
        .with_type("street")
        .with_postcode("75001")
        .with_citycode("75101")
        .with_city("Paris")
        .with_importance(0.3);

    let id0 = ds1.insert(doc1.clone());
    assert_eq!(id0, 0);
    assert_eq!(ds1.len(), 1);
    assert_eq!(ds1.doc_id("doc_1"), Some(0));

    // Update existing document in ds1
    let mut doc1_updated = Document::new("doc_1", "Rue Mise à Jour")
        .with_type("street")
        .with_postcode("75002")
        .with_citycode("75102")
        .with_city("Paris 2e")
        .with_context("Context Update")
        .with_importance(0.7);
    doc1_updated.add_housenumber("1", HousenumberData::with_coords(48.86, 2.34));

    let id_updated = ds1.insert(doc1_updated);
    assert_eq!(id_updated, 0); // same doc_id
    assert_eq!(ds1.len(), 1);

    let fetched = ds1.get(0).unwrap();
    assert_eq!(fetched.name, "Rue Mise à Jour");
    assert_eq!(fetched.housenumbers.len(), 1);
    assert_eq!(fetched.postcode.as_deref(), Some("75002"));

    // merge
    let mut ds2 = DocumentStore::new();
    let doc2 = Document::new("doc_2", "Avenue Deux");
    ds2.insert(doc2);

    ds1.merge(ds2);
    assert_eq!(ds1.len(), 2);
    assert!(ds1.get_by_id("doc_2").is_some());

    // save and load
    let temp = tempdir().unwrap();
    let path = temp.path().join("docs.bin");
    ds1.save_to_file(&path).unwrap();
    let loaded = DocumentStore::load_from_file(&path).unwrap();
    assert_eq!(loaded.len(), 2);
}

#[test]
fn test_filters_and_housenumbers_merge_and_edge_cases() {
    use addok_core::document::Document;
    use addok_index::{FiltersIndex, HousenumbersIndex};

    // 1. FiltersIndex
    let mut fi1 = FiltersIndex::new();
    assert!(fi1.is_empty());
    assert_eq!(fi1.len(), 0);

    // empty filter value ignored
    fi1.add_filter("type", "   ", 1);
    assert!(fi1.is_empty());

    let mut doc = Document::new("d1", "Test")
        .with_type("street")
        .with_postcode("75001")
        .with_citycode("75101")
        .with_city("Paris");
    doc.extra
        .insert("custom_num".to_string(), serde_json::json!(42));
    fi1.index_document(1, &doc);
    assert_eq!(fi1.len(), 6); // type, postcode, department (75), citycode, city, custom_num

    // short postcode (< 2 chars, no department)
    let short_doc = Document::new("d2", "Short").with_postcode("7");
    fi1.index_document(2, &short_doc);

    // non-existent filter returns empty
    let candidates = roaring::RoaringBitmap::from_sorted_iter(vec![1, 2]).unwrap();
    let empty_bm = fi1.filter_doc_ids(&candidates, "nonexistent", "val");
    assert!(empty_bm.is_empty());

    // merge
    let mut fi2 = FiltersIndex::new();
    fi2.add_filter("type", "locality", 3);
    fi1.merge(fi2);
    assert_eq!(fi1.len(), 8);

    // 2. HousenumbersIndex
    let mut hi1 = HousenumbersIndex::new();
    assert!(hi1.is_empty());
    assert_eq!(hi1.len(), 0);

    let mut doc_hn = Document::new("st_1", "Voie");
    doc_hn.add_housenumber("8 bis", HousenumberData::with_coords(48.85, 2.34));
    hi1.index_document(&doc_hn);
    assert_eq!(hi1.len(), 1);
    assert_eq!(hi1.get_street_housenumbers("st_1").unwrap(), &["8 bis"]);

    // Collapsed variant lookup ("8bis" matching "8 bis")
    assert!(hi1.has_housenumber("st_1", "8bis"));
    assert!(hi1.has_housenumber("st_1", "8 bis"));

    // merge
    let mut hi2 = HousenumbersIndex::new();
    let mut doc_hn2 = Document::new("st_2", "Autre Voie");
    doc_hn2.add_housenumber("10", HousenumberData::with_coords(48.86, 2.35));
    hi2.index_document(&doc_hn2);

    hi1.merge(hi2);
    assert_eq!(hi1.len(), 2);
}

#[test]
fn test_ngram_and_pairs_merge_and_edge_cases() {
    use addok_index::{EdgeNgramIndex, PairsIndexer};

    // 1. EdgeNgramIndex
    let mut ng1 = EdgeNgramIndex::new(2, 6);
    assert!(ng1.is_empty());
    assert_eq!(ng1.len(), 0);

    // empty prefix
    assert!(ng1.get_candidates("").is_empty());

    ng1.index_token("michel");
    assert_eq!(ng1.get_candidates("mi"), vec!["michel"]);

    // prefix longer than max_size (6)
    ng1.index_token("boulevardier");
    let cands_long = ng1.get_candidates("boulevard");
    assert_eq!(cands_long, vec!["boulevardier"]);

    // merge
    let mut ng2 = EdgeNgramIndex::new(2, 6);
    ng2.index_token("champs");
    ng1.merge(ng2);
    assert!(ng1.len() > 5);

    // 2. PairsIndexer
    let mut p1 = PairsIndexer::new();
    p1.index_tokens(&["boulevard", "saint", "michel"]);
    let mut p2 = PairsIndexer::new();
    p2.index_tokens(&["rue", "de", "rivoli"]);
    p1.merge(p2);
    assert_eq!(p1.total_docs, 2);

    // filter_candidates when none co-occur (retains candidates)
    let cands = vec!["inconnu".to_string()];
    let res = p1.filter_candidates(&["boulevard"], &cands);
    assert_eq!(res, cands);
}

#[test]
fn test_schema_and_scoring_unicode_and_housenumber_matching() {
    use addok_core::ban::BanRecord;
    use addok_core::config::AddokConfig;
    use addok_core::document::{Document, HousenumberData};
    use addok_index::query::ParsedQuery;
    use addok_index::schema::AddokSchema;
    use addok_index::scoring::{compare_ngrams, compare_sorted_bigrams, score_candidate};
    use addok_index::HousenumbersIndex;

    // 1. AddokSchema default and create_document
    let schema = AddokSchema::default();
    let ban = BanRecord {
        id: "b1".to_string(),
        address_type: None,
        numero: Some("1".to_string()),
        rep: None,
        nom_voie: "Rue".to_string(),
        code_postal: "75001".to_string(),
        code_insee: Some("75101".to_string()),
        nom_commune: "Paris".to_string(),
        code_insee_commune_deleguee: None,
        nom_commune_deleguee: None,
        x: None,
        y: None,
        lon: Some(2.35),
        lat: Some(48.85),
        type_position: None,
        alias: None,
        nom_afnor: None,
        libelle_acheminement: None,
        nom_complementaire: None,
    };
    let tdoc = schema.create_document(&ban, "1 rue paris", "rue", "paris");
    assert!(tdoc.field_values().count() > 0);

    let doc = Document::new("d1", "Rue de Paris")
        .with_postcode("75001")
        .with_city("Paris");
    let tdoc2 = schema.create_document_from_doc(&doc);
    assert!(tdoc2.field_values().count() > 0);

    // 2. compare_sorted_bigrams
    assert_eq!(compare_sorted_bigrams(&[], &[]), 0.0);
    assert_eq!(compare_sorted_bigrams(&[1, 2], &[1, 2]), 1.0);

    // 3. compare_ngrams Unicode & non-ASCII
    assert_eq!(compare_ngrams("café", "café"), 1.0);
    assert!(compare_ngrams("naïve", "naive") > 0.0);
    assert_eq!(compare_ngrams("", "test"), 0.0);
    assert_eq!(compare_ngrams("a", "b"), 0.0);

    // 4. score_candidate with hn in doc.housenumbers but not in housenumbers_index
    let mut doc_hn = Document::new("75101_0001", "Rue de la Paix")
        .with_type("street")
        .with_postcode("75001")
        .with_city("Paris");
    doc_hn.add_housenumber("8", HousenumberData::with_coords(48.868, 2.330));

    let hi_empty = HousenumbersIndex::new();
    let config = AddokConfig::default();

    let pq = ParsedQuery::parse("8 rue de la paix", &config, |_| 10);
    let hit = score_candidate(
        &doc_hn,
        &pq,
        &hi_empty,
        &config,
        Some(48.868),
        Some(2.330),
        false,
    );
    assert!(hit.is_some());
    let hit = hit.unwrap();
    assert_eq!(hit.matched_housenumber.as_deref(), Some("8"));
    assert!(hit.distance_meters.is_some());
}

#[test]
fn test_searcher_tantivy_fallback() {
    use addok_core::ban::BanRecord;
    use addok_index::schema::AddokSchema;
    use tantivy::Index;

    let temp = tempdir().unwrap();
    let index_dir = temp.path().join("tantivy_index");
    std::fs::create_dir_all(&index_dir).unwrap();

    let schema = AddokSchema::new();
    let tantivy_idx = Index::create_in_dir(&index_dir, schema.schema.clone()).unwrap();
    let mut writer = tantivy_idx
        .writer::<tantivy::TantivyDocument>(15_000_000)
        .unwrap();

    let ban = BanRecord {
        id: "tantivy_doc_1".to_string(),
        address_type: Some("street".to_string()),
        numero: Some("15".to_string()),
        rep: None,
        nom_voie: "Rue de Tantivy".to_string(),
        code_postal: "75001".to_string(),
        code_insee: Some("75101".to_string()),
        nom_commune: "Paris".to_string(),
        code_insee_commune_deleguee: None,
        nom_commune_deleguee: None,
        x: None,
        y: None,
        lon: Some(2.35),
        lat: Some(48.85),
        type_position: None,
        alias: None,
        nom_afnor: None,
        libelle_acheminement: None,
        nom_complementaire: None,
    };
    let tdoc = schema.create_document(&ban, "15 rue de tantivy paris", "rue de tantivy", "paris");
    writer.add_document(tdoc).unwrap();

    let doc_tantivy = Document::new("tantivy_doc_2", "Avenue de Tantivy")
        .with_type("street")
        .with_postcode("75001")
        .with_city("Paris");
    let tdoc2 = schema.create_document_from_doc(&doc_tantivy);
    writer.add_document(tdoc2).unwrap();

    writer.commit().unwrap();

    let searcher = AddokSearcher::open(&index_dir).unwrap();
    assert!(searcher.reader.is_some());

    // Search query: multi-index is empty so hits.is_empty() triggers Tantivy fallback!
    let mut params = SearchParams::new("rue tantivy");
    params = params.with_geo_bias(48.85, 2.35);
    params.postcode = Some("75001".to_string());
    let hits = searcher.search(&params).unwrap();
    assert!(!hits.is_empty());
    assert_eq!(hits[0].record.id, "tantivy_doc_1");
    assert!(hits[0].distance_meters.is_some());

    let params2 = SearchParams::new("avenue tantivy");
    let hits2 = searcher.search(&params2).unwrap();
    assert!(!hits2.is_empty());
    assert_eq!(hits2[0].record.id, "tantivy_doc_2");

    let rev_simple = searcher.reverse(48.85, 2.35, 1);
    assert_eq!(rev_simple.len(), 0);
}
