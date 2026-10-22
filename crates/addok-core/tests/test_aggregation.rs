use addok_core::{aggregate_ban_csv, aggregate_ban_records, BanRecord};
use std::path::Path;

#[test]
fn test_ban_hierarchical_grouping_from_sample_csv() {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let workspace_root = manifest_dir.parent().unwrap().parent().unwrap();
    let csv_path = workspace_root.join("data/sample_adresses_75.csv");

    if !csv_path.exists() {
        return;
    }

    let documents = aggregate_ban_csv(&csv_path).expect("Failed to aggregate sample BAN CSV");

    // Sample file contains 18 lines (all house numbers), with several sharing streets:
    // - 3 house numbers for Boulevard Saint-Michel (8, 10, 12 bis)
    // - 2 house numbers for Avenue des Champs-Élysées (10, 50)
    // - and single numbers for other streets.
    // 18 BAN rows must be aggregated into 14 distinct street documents.
    assert_eq!(documents.len(), 14);

    // Find Boulevard Saint-Michel parent document
    let st_michel = documents
        .iter()
        .find(|d| d.name == "Boulevard Saint-Michel")
        .expect("Boulevard Saint-Michel document not found");

    assert_eq!(st_michel.id, "75105_0001");
    assert_eq!(st_michel.address_type.as_deref(), Some("street"));
    assert_eq!(st_michel.postcode.as_deref(), Some("75005"));
    assert_eq!(st_michel.citycode.as_deref(), Some("75105"));
    assert_eq!(st_michel.city.as_deref(), Some("Paris"));

    // Verify all 3 housenumbers are grouped hierarchically
    assert_eq!(st_michel.housenumbers.len(), 3);
    assert!(st_michel.housenumbers.contains_key("8"));
    assert!(st_michel.housenumbers.contains_key("10"));
    assert!(st_michel.housenumbers.contains_key("12 bis"));

    let hn8 = st_michel.housenumbers.get("8").unwrap();
    assert_eq!(hn8.lat, Some(48.85120));
    assert_eq!(hn8.lon, Some(2.34310));
    assert_eq!(hn8.x, Some(652100.0));
    assert_eq!(hn8.y, Some(6861500.0));
    assert_eq!(
        hn8.extra.get("type_position").unwrap(),
        &serde_json::json!("entrée")
    );

    let hn12_bis = st_michel.housenumbers.get("12 bis").unwrap();
    assert_eq!(hn12_bis.lat, Some(48.85150));
    assert_eq!(hn12_bis.lon, Some(2.34340));

    // Find Avenue des Champs-Élysées parent document
    let champs_elysees = documents
        .iter()
        .find(|d| d.name == "Avenue des Champs-Élysées")
        .expect("Avenue des Champs-Élysées document not found");

    assert_eq!(champs_elysees.id, "75108_0002");
    assert_eq!(champs_elysees.housenumbers.len(), 2);
    assert!(champs_elysees.housenumbers.contains_key("10"));
    assert!(champs_elysees.housenumbers.contains_key("50"));
}

#[test]
fn test_hierarchical_reduction_metrics() {
    // Generate 100 housenumbers belonging to 5 distinct streets
    let mut records = Vec::new();
    for street_idx in 1..=5 {
        for num in 1..=20 {
            records.push(BanRecord {
                id: format!("75101_{:04}_{:05}", street_idx, num),
                address_type: None,
                numero: Some(num.to_string()),
                rep: None,
                nom_voie: format!("Rue Numéro {}", street_idx),
                code_postal: "75001".to_string(),
                code_insee: Some("75101".to_string()),
                nom_commune: "Paris".to_string(),
                code_insee_commune_deleguee: None,
                nom_commune_deleguee: None,
                x: Some(650000.0 + num as f64 * 10.0),
                y: Some(6860000.0 + num as f64 * 10.0),
                lon: Some(2.34 + (num as f64 * 0.0001)),
                lat: Some(48.85 + (num as f64 * 0.0001)),
                type_position: Some("entrée".to_string()),
                alias: None,
                nom_afnor: None,
                libelle_acheminement: None,
                nom_complementaire: None,
            });
        }
    }

    assert_eq!(records.len(), 100);

    let documents = aggregate_ban_records(records);

    // 100 individual records reduced to 5 hierarchical street documents
    assert_eq!(documents.len(), 5);
    for doc in documents {
        assert_eq!(doc.housenumbers.len(), 20);
        assert_eq!(doc.address_type.as_deref(), Some("street"));
        assert_eq!(doc.postcode.as_deref(), Some("75001"));
    }
}
