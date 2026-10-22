use std::sync::Arc;

use addok_core::BanRecord;
use addok_index::{AddokIndexer, AddokSearcher, IndexerConfig};
use addok_server::{create_router, AppState, FeatureCollection};
use axum::{
    body::Body,
    http::{header, Request, StatusCode},
};
use tempfile::tempdir;
use tower::ServiceExt;

fn setup_test_index() -> (tempfile::TempDir, AddokSearcher) {
    let temp_dir = tempdir().unwrap();
    let index_dir = temp_dir.path().join("index");

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
    ];

    let indexer = AddokIndexer::new(IndexerConfig::default());
    indexer.index_records(records, &index_dir).unwrap();

    let searcher = AddokSearcher::open(&index_dir).unwrap();
    (temp_dir, searcher)
}

#[tokio::test]
async fn test_http_health_endpoint() {
    let (_dir, searcher) = setup_test_index();
    let app = create_router(AppState {
        searcher: Arc::new(searcher),
    });

    let req = Request::builder()
        .uri("/health")
        .body(Body::empty())
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_http_search_endpoint_and_geojson_format() {
    // Ported from Addok's test_http.py: test_search
    let (_dir, searcher) = setup_test_index();
    let app = create_router(AppState {
        searcher: Arc::new(searcher),
    });

    let req = Request::builder()
        .uri("/search/?q=8+bd+st+michel")
        .body(Body::empty())
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(
        res.headers()
            .get(header::CONTENT_TYPE)
            .unwrap()
            .to_str()
            .unwrap(),
        "application/json"
    );

    let bytes = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .unwrap();
    let collection: FeatureCollection = serde_json::from_slice(&bytes).unwrap();

    assert_eq!(collection.collection_type, "FeatureCollection");
    assert!(!collection.features.is_empty());
    assert_eq!(collection.features[0].properties.id, "75105_0001_00008");
    assert_eq!(
        collection.features[0].geometry.as_ref().unwrap().point_type,
        "Point"
    );
    assert_eq!(collection.query, Some("8 bd st michel".to_string()));
}

#[tokio::test]
async fn test_http_reverse_endpoint() {
    // Ported from Addok's test_http.py: test_reverse
    let (_dir, searcher) = setup_test_index();
    let app = create_router(AppState {
        searcher: Arc::new(searcher),
    });

    let req = Request::builder()
        .uri("/reverse/?lat=48.8512&lon=2.3431")
        .body(Body::empty())
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let bytes = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .unwrap();
    let collection: FeatureCollection = serde_json::from_slice(&bytes).unwrap();

    assert_eq!(collection.collection_type, "FeatureCollection");
    assert!(!collection.features.is_empty());
    assert_eq!(collection.features[0].properties.id, "75105_0001_00008");
}

#[tokio::test]
async fn test_http_missing_params_400_error() {
    // Ported from Addok's test_http.py: test_400_on_missing_query
    let (_dir, searcher) = setup_test_index();
    let app = create_router(AppState {
        searcher: Arc::new(searcher),
    });

    // Missing 'q'
    let req = Request::builder()
        .uri("/search/")
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);

    // Missing lat/lon on reverse
    let req_rev = Request::builder()
        .uri("/reverse/")
        .body(Body::empty())
        .unwrap();
    let res_rev = app.oneshot(req_rev).await.unwrap();
    assert_eq!(res_rev.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_http_cors_headers() {
    // Ported from Addok's test_http.py: test_cors
    let (_dir, searcher) = setup_test_index();
    let app = create_router(AppState {
        searcher: Arc::new(searcher),
    });

    let req = Request::builder()
        .method("OPTIONS")
        .uri("/search/?q=paris")
        .header("Origin", "https://example.com")
        .header("Access-Control-Request-Method", "GET")
        .body(Body::empty())
        .unwrap();

    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(
        res.headers()
            .get(header::ACCESS_CONTROL_ALLOW_ORIGIN)
            .unwrap()
            .to_str()
            .unwrap(),
        "*"
    );
}
