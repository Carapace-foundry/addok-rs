use std::collections::HashMap;
use tempfile::tempdir;

use addok_core::document::{Document, HousenumberData};
use addok_index::{
    AddokIndexer, AddokSearcher, EdgeNgramIndex, FiltersIndex, HousenumbersIndex, IndexerConfig,
    PairsIndexer, SearchParams,
};

#[test]
fn test_integration_token_pair_generation() {
    let mut pairs = PairsIndexer::new();

    // Document 1: Boulevard Saint-Michel (Paris 5e)
    pairs.index_tokens(&["boulevard", "saint", "michel", "paris"]);
    // Document 2: Rue Saint-Denis (Paris 1er)
    pairs.index_tokens(&["rue", "saint", "denis", "paris"]);
    // Document 3: Avenue des Champs-Élysées (Paris 8e)
    pairs.index_tokens(&["avenue", "champs", "elysees", "paris"]);

    assert_eq!(pairs.total_docs, 3);
    assert_eq!(pairs.token_doc_frequency("paris"), 3);
    assert_eq!(pairs.token_doc_frequency("saint"), 2);
    assert_eq!(pairs.token_doc_frequency("michel"), 1);

    // Verify token co-occurrences
    assert!(pairs.are_associated("boulevard", "saint"));
    assert!(pairs.are_associated("boulevard", "michel"));
    assert!(pairs.are_associated("saint", "michel"));
    assert!(pairs.are_associated("saint", "denis"));
    assert!(pairs.are_associated("avenue", "elysees"));

    // Tokens from disjoint documents must NOT be associated
    assert!(!pairs.are_associated("boulevard", "denis"));
    assert!(!pairs.are_associated("champs", "saint"));
    assert!(!pairs.are_associated("michel", "elysees"));

    // Selectivity: rare token "michel" (df=1) is more selective than "paris" (df=3)
    let sel_michel = pairs.token_selectivity("michel");
    let sel_paris = pairs.token_selectivity("paris");
    assert!(sel_michel > sel_paris);

    // Autocomplete candidate filtering:
    // Candidate tokens starting with "mich": ["michel", "michelet", "michard"]
    let candidates = vec![
        "michel".to_string(),
        "michelet".to_string(),
        "michard".to_string(),
    ];

    // When query contains "boulevard" + "saint", only "michel" pairs with both!
    let filtered = pairs.filter_candidates(&["boulevard", "saint"], &candidates);
    assert_eq!(filtered, vec!["michel".to_string()]);
}

#[test]
fn test_integration_edge_ngram_generation() {
    let mut ngrams = EdgeNgramIndex::new(2, 10);

    let sample_tokens = vec![
        "boulevard",
        "saint",
        "michel",
        "michelet",
        "champs",
        "elysees",
    ];
    ngrams.index_tokens(&sample_tokens);

    // Direct generation helper verification
    let generated = EdgeNgramIndex::generate_edge_ngrams("champs", 2, 6);
    assert_eq!(generated, vec!["ch", "cha", "cham", "champ", "champs"]);

    // Autocomplete prefix lookups
    let cands_boul = ngrams.get_candidates("boul");
    assert_eq!(cands_boul, vec!["boulevard".to_string()]);

    let cands_mich = ngrams.get_candidates("mich");
    assert_eq!(
        cands_mich,
        vec!["michel".to_string(), "michelet".to_string()]
    );

    let cands_sa = ngrams.get_candidates("sa");
    assert_eq!(cands_sa, vec!["saint".to_string()]);

    let cands_nonexistent = ngrams.get_candidates("xyz");
    assert!(cands_nonexistent.is_empty());
}

#[test]
fn test_integration_housenumbers_sub_index() {
    let mut hn_index = HousenumbersIndex::new();

    let mut doc = Document::new("75105_0001", "Boulevard Saint-Michel");
    doc.add_housenumber("8", HousenumberData::with_coords(48.8512, 2.3431));
    doc.add_housenumber("8 bis", HousenumberData::with_coords(48.8513, 2.3432));
    doc.add_housenumber("10", HousenumberData::with_coords(48.8516, 2.3436));

    hn_index.index_document(&doc);

    // Exact match
    assert!(hn_index.has_housenumber("75105_0001", "8"));
    let coords_8 = hn_index.get_coordinates("75105_0001", "8").unwrap();
    assert_eq!(coords_8.lat, Some(48.8512));
    assert_eq!(coords_8.lon, Some(2.3431));

    // Whitespace and case normalization ("8  Bis" -> "8 bis")
    assert!(hn_index.has_housenumber("75105_0001", "8  Bis"));
    assert!(hn_index.has_housenumber("75105_0001", "8bis"));

    // Query non-existent number
    assert!(!hn_index.has_housenumber("75105_0001", "99"));
}

#[test]
fn test_integration_filters_multi_value_postings() {
    let mut filters = FiltersIndex::new();

    let doc1 = Document::new("1", "Rue de Rivoli")
        .with_type("street")
        .with_postcode("75004")
        .with_citycode("75104")
        .with_city("Paris");

    let doc2 = Document::new("2", "Boulevard Saint-Michel")
        .with_type("street")
        .with_postcode("75005")
        .with_citycode("75105")
        .with_city("Paris");

    let doc3 = Document::new("3", "Marseille")
        .with_type("municipality")
        .with_postcode("13001")
        .with_citycode("13055")
        .with_city("Marseille");

    filters.index_document(0, &doc1);
    filters.index_document(1, &doc2);
    filters.index_document(2, &doc3);

    // Multi-value posting sets backed by Roaring Bitmaps
    let streets = filters.get_filter_bitmap("type", "street").unwrap();
    assert_eq!(streets.len(), 2);
    assert!(streets.contains(0));
    assert!(streets.contains(1));
    assert!(!streets.contains(2));

    let pc_75005 = filters.get_filter_bitmap("postcode", "75005").unwrap();
    assert_eq!(pc_75005.len(), 1);
    assert!(pc_75005.contains(1));

    let dep_75 = filters.get_filter_bitmap("department", "75").unwrap();
    assert_eq!(dep_75.len(), 2);
    assert!(dep_75.contains(0));
    assert!(dep_75.contains(1));
    assert!(!dep_75.contains(2));
}

#[test]
fn test_integration_document_retrieval_and_autocomplete() {
    let temp = tempdir().unwrap();
    let index_dir = temp.path().join("multi_idx");

    let mut doc1 = Document::new("75105_0001", "Boulevard Saint-Michel")
        .with_type("street")
        .with_postcode("75005")
        .with_citycode("75105")
        .with_city("Paris")
        .with_importance(0.4);

    let mut hn_map = HashMap::new();
    hn_map.insert(
        "8".to_string(),
        HousenumberData::with_coords(48.8512, 2.3431),
    );
    hn_map.insert(
        "10".to_string(),
        HousenumberData::with_coords(48.8518, 2.3439),
    );
    doc1 = doc1.with_housenumbers(hn_map);

    let mut doc2 = Document::new("75108_0002", "Avenue des Champs-Élysées")
        .with_type("street")
        .with_postcode("75008")
        .with_citycode("75108")
        .with_city("Paris")
        .with_importance(0.5);
    doc2.extra
        .insert("lat".to_string(), serde_json::json!(48.8698));
    doc2.extra
        .insert("lon".to_string(), serde_json::json!(2.3082));

    let mut doc3 = Document::new("75115_0003", "Rue de l'Église")
        .with_type("street")
        .with_postcode("75015")
        .with_citycode("75115")
        .with_city("Paris")
        .with_importance(0.3);
    doc3.extra
        .insert("lat".to_string(), serde_json::json!(48.8451));
    doc3.extra
        .insert("lon".to_string(), serde_json::json!(2.2885));

    let docs = vec![Ok(doc1), Ok(doc2), Ok(doc3)];

    let indexer = AddokIndexer::new(IndexerConfig::default());
    let stats = indexer.index_documents(docs, &index_dir).unwrap();
    assert_eq!(stats.indexed_records, 3);
    assert_eq!(stats.spatial_records, 5); // 1 centroid + 2 hn on doc1 + 1 doc2 + 1 doc3

    let searcher = AddokSearcher::open(&index_dir).unwrap();

    // 1. Autocomplete test: "8 bd st mich"
    // "mich" prefix expands to "michel" via Edge N-Gram + Pairs filtering with "boulevard" + "saint"
    // and exact housenumber "8" coordinates are resolved
    let params_ac = SearchParams::new("8 bd st mich");
    let hits_ac = searcher.search(&params_ac).unwrap();
    assert!(!hits_ac.is_empty(), "Autocomplete should return results");
    assert_eq!(hits_ac[0].record.nom_voie, "Boulevard Saint-Michel");
    assert_eq!(hits_ac[0].record.numero.as_deref(), Some("8"));
    assert_eq!(hits_ac[0].record.lat, Some(48.8512));
    assert_eq!(hits_ac[0].record.lon, Some(2.3431));

    // 2. Exact address test: "avenue des champs elysees"
    let params_champs = SearchParams::new("champs elysees");
    let hits_champs = searcher.search(&params_champs).unwrap();
    assert!(!hits_champs.is_empty());
    assert_eq!(hits_champs[0].record.id, "75108_0002");

    // 3. Postcode filter test
    let mut params_filtered = SearchParams::new("paris");
    params_filtered.postcode = Some("75015".to_string());
    let hits_filtered = searcher.search(&params_filtered).unwrap();
    assert_eq!(hits_filtered.len(), 1);
    assert_eq!(hits_filtered[0].record.id, "75115_0003");

    // 4. Reverse geocoding near Boulevard Saint-Michel (48.8512, 2.3431)
    let rev_results = searcher.reverse(48.8512, 2.3431, 1);
    assert_eq!(rev_results.len(), 1);
    assert_eq!(rev_results[0].item.city, "Paris");
    assert!(rev_results[0].distance_meters < 10.0);
}

#[test]
fn test_integration_incremental_batch_commit() {
    let temp = tempdir().unwrap();
    let index_dir = temp.path().join("incremental_idx");

    let indexer = AddokIndexer::new(IndexerConfig::default());

    // Batch 1: Index Paris 5e street
    let batch1 = vec![Ok(Document::new("75105_0001", "Boulevard Saint-Michel")
        .with_type("street")
        .with_postcode("75005")
        .with_city("Paris"))];
    indexer.index_documents(batch1, &index_dir).unwrap();

    // Verify Batch 1 is indexed
    {
        let searcher1 = AddokSearcher::open(&index_dir).unwrap();
        let hits = searcher1
            .search(&SearchParams::new("saint michel"))
            .unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].record.id, "75105_0001");
    }

    // Batch 2: Append Paris 8e street to the existing index
    let batch2 = vec![Ok(Document::new("75108_0002", "Avenue des Champs-Élysées")
        .with_type("street")
        .with_postcode("75008")
        .with_city("Paris"))];
    indexer.index_documents(batch2, &index_dir).unwrap();

    // Verify BOTH Batch 1 and Batch 2 are retrievable from the committed index
    {
        let searcher2 = AddokSearcher::open(&index_dir).unwrap();
        let hits1 = searcher2
            .search(&SearchParams::new("saint michel"))
            .unwrap();
        assert_eq!(hits1.len(), 1);
        assert_eq!(hits1[0].record.id, "75105_0001");

        let hits2 = searcher2
            .search(&SearchParams::new("champs elysees"))
            .unwrap();
        assert_eq!(hits2.len(), 1);
        assert_eq!(hits2[0].record.id, "75108_0002");
    }
}
