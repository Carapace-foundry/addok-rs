use addok_core::{open_ndjson_file, open_ndjson_reader, write_ndjson, Document, HousenumberData};
use flate2::write::GzEncoder;
use flate2::Compression;
use std::collections::HashMap;
use std::io::Write;
use tempfile::NamedTempFile;

#[test]
fn test_document_serialization_and_fields() {
    let mut housenumbers = HashMap::new();
    housenumbers.insert(
        "8".to_string(),
        HousenumberData {
            lat: Some(48.8512),
            lon: Some(2.3431),
            x: Some(652100.0),
            y: Some(6861500.0),
            extra: HashMap::from([("type_position".to_string(), serde_json::json!("entrée"))]),
        },
    );
    housenumbers.insert(
        "12 bis".to_string(),
        HousenumberData {
            lat: Some(48.8515),
            lon: Some(2.3434),
            x: Some(652120.0),
            y: Some(6861520.0),
            extra: HashMap::new(),
        },
    );

    let mut doc = Document::new("75105_0001", "Boulevard Saint-Michel")
        .with_type("street")
        .with_postcode("75005")
        .with_citycode("75105")
        .with_city("Paris")
        .with_context("75, Paris, Île-de-France")
        .with_importance(0.4)
        .with_housenumbers(housenumbers);

    // Arbitrary custom attributes
    doc.extra
        .insert("osm_id".to_string(), serde_json::json!("way/123456"));
    doc.extra
        .insert("population".to_string(), serde_json::json!(2148271));

    // Serialize to JSON
    let json_str = serde_json::to_string_pretty(&doc).unwrap();

    // Verify Addok JSON specification compliance
    assert!(json_str.contains("\"id\": \"75105_0001\""));
    assert!(json_str.contains("\"name\": \"Boulevard Saint-Michel\""));
    assert!(json_str.contains("\"type\": \"street\""));
    assert!(json_str.contains("\"postcode\": \"75005\""));
    assert!(json_str.contains("\"citycode\": \"75105\""));
    assert!(json_str.contains("\"city\": \"Paris\""));
    assert!(json_str.contains("\"context\": \"75, Paris, Île-de-France\""));
    assert!(json_str.contains("\"importance\": 0.4"));
    assert!(json_str.contains("\"osm_id\": \"way/123456\""));
    assert!(json_str.contains("\"population\": 2148271"));
    assert!(json_str.contains("\"8\": {"));
    assert!(json_str.contains("\"12 bis\": {"));

    // Deserialize back and verify
    let deserialized: Document = serde_json::from_str(&json_str).unwrap();
    assert_eq!(deserialized.id, "75105_0001");
    assert_eq!(deserialized.name, "Boulevard Saint-Michel");
    assert_eq!(deserialized.address_type.as_deref(), Some("street"));
    assert_eq!(deserialized.postcode.as_deref(), Some("75005"));
    assert_eq!(deserialized.citycode.as_deref(), Some("75105"));
    assert_eq!(deserialized.city.as_deref(), Some("Paris"));
    assert_eq!(
        deserialized.context.as_deref(),
        Some("75, Paris, Île-de-France")
    );
    assert!((deserialized.importance - 0.4).abs() < 1e-4);
    assert_eq!(deserialized.housenumbers.len(), 2);

    let hn8 = deserialized.housenumbers.get("8").unwrap();
    assert_eq!(hn8.lat, Some(48.8512));
    assert_eq!(hn8.lon, Some(2.3431));
    assert_eq!(hn8.x, Some(652100.0));
    assert_eq!(hn8.y, Some(6861500.0));
    assert_eq!(
        hn8.extra.get("type_position").unwrap(),
        &serde_json::json!("entrée")
    );

    assert_eq!(
        deserialized.extra.get("osm_id").unwrap(),
        &serde_json::json!("way/123456")
    );
    assert_eq!(
        deserialized.extra.get("population").unwrap(),
        &serde_json::json!(2148271)
    );
}

#[test]
fn test_address_type_alias() {
    let json_with_address_type = r#"{
        "id": "75056",
        "name": "Paris",
        "address_type": "municipality",
        "citycode": "75056",
        "importance": 0.95
    }"#;

    let doc: Document = serde_json::from_str(json_with_address_type).unwrap();
    assert_eq!(doc.address_type.as_deref(), Some("municipality"));
}

#[test]
fn test_ndjson_streaming_reader_and_writer() {
    let doc1 = Document::new("1", "Rue de Rivoli")
        .with_type("street")
        .with_postcode("75004")
        .with_city("Paris");

    let mut doc2 = Document::new("2", "Boulevard Saint-Germain")
        .with_type("street")
        .with_postcode("75005")
        .with_city("Paris");
    doc2.add_housenumber(
        "1",
        HousenumberData {
            lat: Some(48.852),
            lon: Some(2.346),
            x: None,
            y: None,
            extra: HashMap::new(),
        },
    );

    let mut buffer = Vec::new();
    write_ndjson(&mut buffer, vec![doc1.clone(), doc2.clone()]).unwrap();

    let reader = open_ndjson_reader(&buffer[..]);
    let parsed_docs: Vec<Document> = reader.collect::<addok_core::Result<Vec<_>>>().unwrap();

    assert_eq!(parsed_docs.len(), 2);
    assert_eq!(parsed_docs[0].id, "1");
    assert_eq!(parsed_docs[0].name, "Rue de Rivoli");
    assert_eq!(parsed_docs[1].id, "2");
    assert_eq!(parsed_docs[1].housenumbers.len(), 1);
    assert_eq!(
        parsed_docs[1].housenumbers.get("1").unwrap().lat,
        Some(48.852)
    );
}

#[test]
fn test_ndjson_file_gzip_streaming() {
    let doc = Document::new("gz_1", "Avenue Montaigne")
        .with_type("street")
        .with_postcode("75008")
        .with_city("Paris");

    let mut uncompressed = Vec::new();
    write_ndjson(&mut uncompressed, vec![doc]).unwrap();

    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(&uncompressed).unwrap();
    let compressed = encoder.finish().unwrap();

    let mut temp_file = NamedTempFile::new().unwrap();
    temp_file.write_all(&compressed).unwrap();
    let path = temp_file.into_temp_path();

    let stream = open_ndjson_file(&path).unwrap();
    let docs: Vec<Document> = stream.collect::<addok_core::Result<Vec<_>>>().unwrap();

    assert_eq!(docs.len(), 1);
    assert_eq!(docs[0].id, "gz_1");
    assert_eq!(docs[0].name, "Avenue Montaigne");
    assert_eq!(docs[0].city.as_deref(), Some("Paris"));
}

#[test]
fn test_document_lenient_parsing_and_arrays() {
    // 1. Address type as array of strings, HousenumberData with string latitude with comma
    let json_array_type = r#"{
        "id": "doc_arr",
        "name": "Rue de Lyon",
        "type": ["street", "extra"],
        "importance": 0.85,
        "postcode": 75012,
        "housenumbers": {
            "10": {
                "lat": "48,8512",
                "lon": 2.3431,
                "x": null,
                "y": ""
            }
        }
    }"#;

    let doc: Document =
        serde_json::from_str(json_array_type).expect("Failed to deserialize lenient doc");
    assert_eq!(doc.address_type.as_deref(), Some("street"));
    assert!((doc.importance - 0.85).abs() < 1e-4);
    assert_eq!(doc.postcode.as_deref(), Some("75012"));
    let hn10 = doc.housenumbers.get("10").unwrap();
    assert_eq!(hn10.lat, Some(48.8512));
    assert_eq!(hn10.lon, Some(2.3431));
    assert_eq!(hn10.x, None);
    assert_eq!(hn10.y, None);

    // 2. Type as number, null, empty array
    let json_num_type = r#"{
        "id": "doc_num",
        "name": "Rue Neuve",
        "type": 42
    }"#;
    let doc_num: Document = serde_json::from_str(json_num_type).unwrap();
    assert_eq!(doc_num.address_type.as_deref(), Some("42"));

    let json_empty_arr = r#"{
        "id": "doc_empty",
        "name": "Place Blanche",
        "type": []
    }"#;
    let doc_empty: Document = serde_json::from_str(json_empty_arr).unwrap();
    assert_eq!(doc_empty.address_type, None);
}

#[test]
fn test_document_coordinates_fallback_and_centroid() {
    // 1. Centroid calculation from multiple housenumbers
    let mut doc = Document::new("street_1", "Rue de Paris");
    doc.add_housenumber(
        "1",
        HousenumberData {
            lat: Some(48.80),
            lon: Some(2.30),
            x: None,
            y: None,
            extra: HashMap::new(),
        },
    );
    doc.add_housenumber(
        "3",
        HousenumberData {
            lat: Some(48.90),
            lon: Some(2.40),
            x: None,
            y: None,
            extra: HashMap::new(),
        },
    );

    let (c_lat, c_lon) = doc.coordinates(None).expect("Expected centroid");
    assert!((c_lat - 48.85).abs() < 1e-4);
    assert!((c_lon - 2.35).abs() < 1e-4);

    // Specific house number coordinates
    let (h1_lat, h1_lon) = doc.coordinates(Some("1")).expect("Expected hn 1 coords");
    assert_eq!(h1_lat, 48.80);
    assert_eq!(h1_lon, 2.30);

    // 2. Fallback to extra lat/lon when no housenumber coords
    let mut doc_extra = Document::new("mun_1", "Marseille");
    doc_extra
        .extra
        .insert("lat".to_string(), serde_json::json!(43.2965));
    doc_extra
        .extra
        .insert("lon".to_string(), serde_json::json!(5.3698));
    let (ex_lat, ex_lon) = doc_extra.coordinates(None).expect("Expected extra coords");
    assert_eq!(ex_lat, 43.2965);
    assert_eq!(ex_lon, 5.3698);

    // 3. None when completely empty
    let empty_doc = Document::new("empty_1", "Chemin sans nom");
    assert_eq!(empty_doc.coordinates(None), None);
}

#[test]
fn test_document_make_labels_without_france() {
    let doc = Document::new("us_1", "123 Main Street")
        .with_city("Springfield")
        .with_postcode("62701");

    let labels = doc.make_labels(Some("123"), false);
    assert_eq!(labels, vec!["123 123 Main Street 62701 Springfield"]);

    let doc_no_pc = Document::new("us_2", "Broadway").with_city("New York");
    let labels2 = doc_no_pc.make_labels(None, false);
    assert_eq!(labels2, vec!["Broadway New York"]);
}

#[test]
fn test_document_to_ban_record_full_coverage() {
    let mut doc = Document::new("75101_0001", "Rue de Rivoli")
        .with_type("street")
        .with_postcode("75001")
        .with_citycode("75101")
        .with_city("Paris");

    let mut hn_extra = HashMap::new();
    hn_extra.insert("id".to_string(), serde_json::json!("BAN_CUSTOM_HN_8"));

    let hn_data = HousenumberData {
        lat: Some(48.856),
        lon: Some(2.352),
        x: Some(652000.0),
        y: Some(6861000.0),
        extra: hn_extra,
    };
    doc.add_housenumber("8 bis", hn_data);

    // Compact matching "8bis" resolving "8 bis"
    let rec1 = doc.to_ban_record(Some("8bis"));
    assert_eq!(rec1.id, "BAN_CUSTOM_HN_8");
    assert_eq!(rec1.numero.as_deref(), Some("8bis"));
    assert_eq!(rec1.x, Some(652000.0));
    assert_eq!(rec1.y, Some(6861000.0));

    // Project coordinates on document extra
    let mut doc_proj = Document::new("proj_1", "Voie");
    doc_proj
        .extra
        .insert("x".to_string(), serde_json::json!(100.0));
    doc_proj
        .extra
        .insert("y".to_string(), serde_json::json!(200.0));
    let ban_proj = doc_proj.to_ban_record(None);
    assert_eq!(ban_proj.x, Some(100.0));
    assert_eq!(ban_proj.y, Some(200.0));

    let hn_proj = HousenumberData::new(Some(48.0), Some(2.0)).with_projected(300.0, 400.0);
    assert_eq!(hn_proj.x, Some(300.0));
    assert_eq!(hn_proj.y, Some(400.0));
}

#[test]
fn test_ndjson_empty_lines_and_error() {
    let content = "\n\n{\"id\":\"1\",\"name\":\"Rue A\"}\n\n{invalid json}\n";
    let stream = open_ndjson_reader(content.as_bytes());
    let results: Vec<_> = stream.collect();
    assert_eq!(results.len(), 2);
    assert!(results[0].is_ok());
    assert!(results[1].is_err());
}
