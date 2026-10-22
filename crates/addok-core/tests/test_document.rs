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
