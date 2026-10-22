use std::sync::Arc;

use addok_core::BanRecord;
use addok_index::{AddokIndexer, AddokSearcher, IndexerConfig};
use addok_server::{create_router, AppState, FeatureCollection};
use axum::{
    body::Body,
    http::{header, Request, StatusCode},
};
use http_body_util::BodyExt;
use tempfile::tempdir;
use tower::ServiceExt;

/// Helper to initialize an Addok test index with sample Paris BAN records
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
        Ok(BanRecord {
            id: "75115_0003_00003".to_string(),
            address_type: None,
            numero: Some("3".to_string()),
            rep: None,
            nom_voie: "Rue de l'Église".to_string(),
            code_postal: "75015".to_string(),
            code_insee: Some("75115".to_string()),
            nom_commune: "Paris".to_string(),
            code_insee_commune_deleguee: None,
            nom_commune_deleguee: None,
            x: Some(648200.0),
            y: Some(6860100.0),
            lon: Some(2.2885),
            lat: Some(48.8451),
            type_position: Some("entrée".to_string()),
            alias: None,
            nom_afnor: Some("R DE L EGLISE".to_string()),
            libelle_acheminement: Some("PARIS".to_string()),
            nom_complementaire: None,
        }),
    ];

    let indexer = AddokIndexer::new(IndexerConfig::default());
    indexer.index_records(records, &index_dir).unwrap();

    let searcher = AddokSearcher::open(&index_dir).unwrap();
    (temp_dir, searcher)
}

/// Helper function to build multipart/form-data bodies for testing CSV endpoints
fn build_multipart_body(
    boundary: &str,
    fields: &[(&str, &str)],
    file_field: Option<(&str, &str, &[u8])>,
) -> Vec<u8> {
    let mut body = Vec::new();
    for (k, v) in fields {
        body.extend_from_slice(format!("--{}\r\n", boundary).as_bytes());
        body.extend_from_slice(
            format!("Content-Disposition: form-data; name=\"{}\"\r\n\r\n", k).as_bytes(),
        );
        body.extend_from_slice(v.as_bytes());
        body.extend_from_slice(b"\r\n");
    }
    if let Some((field_name, filename, data)) = file_field {
        body.extend_from_slice(format!("--{}\r\n", boundary).as_bytes());
        body.extend_from_slice(
            format!(
                "Content-Disposition: form-data; name=\"{}\"; filename=\"{}\"\r\nContent-Type: text/csv\r\n\r\n",
                field_name, filename
            )
            .as_bytes(),
        );
        body.extend_from_slice(data);
        body.extend_from_slice(b"\r\n");
    }
    body.extend_from_slice(format!("--{}--\r\n", boundary).as_bytes());
    body
}

#[tokio::test]
async fn test_search_missing_query_error_400() {
    let (_temp, searcher) = setup_test_index();
    let app = create_router(AppState {
        searcher: Arc::new(searcher),
    });

    // 1. Missing `q` parameter entirely
    let req = Request::builder()
        .method("GET")
        .uri("/search/")
        .body(Body::empty())
        .unwrap();

    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(json["type"], "invalid_query");
    assert!(json["detail"]
        .as_str()
        .unwrap()
        .contains("Missing query parameter"));

    // 2. Empty `q` parameter
    let req_empty = Request::builder()
        .method("GET")
        .uri("/search/?q=")
        .body(Body::empty())
        .unwrap();

    let resp_empty = app.oneshot(req_empty).await.unwrap();
    assert_eq!(resp_empty.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_search_geojson_featurecollection_format() {
    let (_temp, searcher) = setup_test_index();
    let app = create_router(AppState {
        searcher: Arc::new(searcher),
    });

    let req = Request::builder()
        .method("GET")
        .uri("/search/?q=8+boulevard+saint-michel&limit=5")
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let collection: FeatureCollection = serde_json::from_slice(&bytes).unwrap();

    // Verify FeatureCollection top-level format
    assert_eq!(collection.collection_type, "FeatureCollection");
    assert_eq!(collection.version, "draft");
    assert_eq!(collection.attribution, "BAN (Base Adresse Nationale)");
    assert_eq!(collection.licence, "ODbL 1.0");
    assert_eq!(
        collection.query.as_deref(),
        Some("8 boulevard saint-michel")
    );
    assert_eq!(collection.limit, 5);
    assert!(!collection.features.is_empty());

    // Verify feature structure and properties
    let feat = &collection.features[0];
    assert_eq!(feat.feature_type, "Feature");
    let geom = feat.geometry.as_ref().unwrap();
    assert_eq!(geom.point_type, "Point");
    assert!((geom.coordinates[0] - 2.3431).abs() < 1e-3);
    assert!((geom.coordinates[1] - 48.8512).abs() < 1e-3);

    let props = &feat.properties;
    assert_eq!(props.id, "75105_0001_00008");
    assert!(props.label.contains("Boulevard Saint-Michel"));
    assert_eq!(props.housenumber.as_deref(), Some("8"));
    assert_eq!(props.address_type, "housenumber");
    assert_eq!(props.postcode, "75005");
    assert_eq!(props.citycode.as_deref(), Some("75105"));
    assert_eq!(props.city, "Paris");
    assert!(props.context.as_ref().unwrap().contains("75"));
    assert!(props.score > 0.5);
    assert!(props.x.is_some());
    assert!(props.y.is_some());
}

#[tokio::test]
async fn test_search_query_parameters_and_filters() {
    let (_temp, searcher) = setup_test_index();
    let app = create_router(AppState {
        searcher: Arc::new(searcher),
    });

    // 1. Geo bias with center coordinates output
    let req = Request::builder()
        .method("GET")
        .uri("/search/?q=saint-michel&lat=48.8512&lon=2.3431")
        .body(Body::empty())
        .unwrap();

    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let col: FeatureCollection = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(col.center, Some([2.3431, 48.8512]));
    assert!(col.features[0].properties.distance.is_some());

    // 2. Postcode filter
    let req_pc = Request::builder()
        .method("GET")
        .uri("/search/?q=paris&postcode=75008")
        .body(Body::empty())
        .unwrap();

    let resp_pc = app.clone().oneshot(req_pc).await.unwrap();
    let bytes_pc = resp_pc.into_body().collect().await.unwrap().to_bytes();
    let col_pc: FeatureCollection = serde_json::from_slice(&bytes_pc).unwrap();
    for f in &col_pc.features {
        assert_eq!(f.properties.postcode, "75008");
    }

    // 3. Multi-value filters with repeated keys (?type=housenumber&type=street)
    let req_mv = Request::builder()
        .method("GET")
        .uri("/search/?q=champs&type=housenumber&type=street")
        .body(Body::empty())
        .unwrap();

    let resp_mv = app.clone().oneshot(req_mv).await.unwrap();
    assert_eq!(resp_mv.status(), StatusCode::OK);

    // 4. Autocomplete mode toggle
    let req_ac = Request::builder()
        .method("GET")
        .uri("/search/?q=saint-m&autocomplete=0")
        .body(Body::empty())
        .unwrap();

    let resp_ac = app.oneshot(req_ac).await.unwrap();
    assert_eq!(resp_ac.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_reverse_error_handling_and_results() {
    let (_temp, searcher) = setup_test_index();
    let app = create_router(AppState {
        searcher: Arc::new(searcher),
    });

    // 1. Missing lat/lon -> 400 Bad Request
    let req_err = Request::builder()
        .method("GET")
        .uri("/reverse/")
        .body(Body::empty())
        .unwrap();
    let resp_err = app.clone().oneshot(req_err).await.unwrap();
    assert_eq!(resp_err.status(), StatusCode::BAD_REQUEST);

    // 2. Valid reverse coordinates near Champs-Élysées
    let req_ok = Request::builder()
        .method("GET")
        .uri("/reverse/?lat=48.8698&lon=2.3082")
        .body(Body::empty())
        .unwrap();
    let resp_ok = app.oneshot(req_ok).await.unwrap();
    assert_eq!(resp_ok.status(), StatusCode::OK);

    let bytes = resp_ok.into_body().collect().await.unwrap().to_bytes();
    let col: FeatureCollection = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(col.center, Some([2.3082, 48.8698]));
    assert_eq!(col.query, None);
    assert!(!col.features.is_empty());
    assert_eq!(col.features[0].properties.id, "75108_0002_00010");
    assert!(col.features[0].properties.distance.unwrap() < 20);
}

#[tokio::test]
async fn test_cors_middleware_headers_and_preflight() {
    let (_temp, searcher) = setup_test_index();
    let app = create_router(AppState {
        searcher: Arc::new(searcher),
    });

    // 1. Standard GET request has CORS headers
    let req_get = Request::builder()
        .method("GET")
        .uri("/search/?q=test")
        .body(Body::empty())
        .unwrap();
    let resp_get = app.clone().oneshot(req_get).await.unwrap();
    assert_eq!(
        resp_get
            .headers()
            .get(header::ACCESS_CONTROL_ALLOW_ORIGIN)
            .unwrap(),
        "*"
    );
    assert!(resp_get
        .headers()
        .get(header::ACCESS_CONTROL_ALLOW_HEADERS)
        .unwrap()
        .to_str()
        .unwrap()
        .contains("X-Requested-With"));

    // 2. OPTIONS preflight request
    let req_options = Request::builder()
        .method("OPTIONS")
        .uri("/search/")
        .body(Body::empty())
        .unwrap();
    let resp_options = app.oneshot(req_options).await.unwrap();
    assert_eq!(resp_options.status(), StatusCode::OK);
    assert_eq!(
        resp_options
            .headers()
            .get(header::ACCESS_CONTROL_ALLOW_ORIGIN)
            .unwrap(),
        "*"
    );
    assert!(resp_options
        .headers()
        .get(header::ACCESS_CONTROL_ALLOW_HEADERS)
        .unwrap()
        .to_str()
        .unwrap()
        .contains("X-Requested-With"));
}

#[tokio::test]
async fn test_search_csv_batch_streaming_with_delimiters() {
    let (_temp, searcher) = setup_test_index();
    let app = create_router(AppState {
        searcher: Arc::new(searcher),
    });

    let boundary = "---------------------------974767299852498929531610575";

    // 1. Semicolon delimited CSV with columns parameter
    let csv_content = b"client_id;adresse;code_postal\n\
1;8 boulevard saint-michel;75005\n\
2;10 avenue des champs elysees;75008\n\
3;3 rue de l eglise;75015\n";

    let multipart_body = build_multipart_body(
        boundary,
        &[("columns", "adresse,code_postal")],
        Some(("data", "input.csv", csv_content)),
    );

    let req = Request::builder()
        .method("POST")
        .uri("/search/csv/")
        .header(
            "Content-Type",
            format!("multipart/form-data; boundary={}", boundary),
        )
        .body(Body::from(multipart_body))
        .unwrap();

    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert!(resp
        .headers()
        .get(header::CONTENT_TYPE)
        .unwrap()
        .to_str()
        .unwrap()
        .contains("text/csv"));

    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let output_csv = String::from_utf8(bytes.to_vec()).unwrap();

    // Verify delimiter preservation (semicolon)
    assert!(output_csv.contains(';'));

    // Verify extra output columns present in header
    let first_line = output_csv.lines().next().unwrap();
    assert!(first_line.contains("result_label"));
    assert!(first_line.contains("result_score"));
    assert!(first_line.contains("result_type"));
    assert!(first_line.contains("result_id"));
    assert!(first_line.contains("result_housenumber"));
    assert!(first_line.contains("result_postcode"));
    assert!(first_line.contains("result_city"));
    assert!(first_line.contains("result_citycode"));
    assert!(first_line.contains("result_distance"));
    assert!(first_line.contains("latitude"));
    assert!(first_line.contains("longitude"));

    // Verify geocoded address matching
    assert!(output_csv.contains("75105_0001_00008"));
    assert!(output_csv.contains("Boulevard Saint-Michel"));
    assert!(output_csv.contains("Avenue des Champs-Élysées"));

    // 2. Comma delimited CSV auto-detection without explicit columns (auto-detects 'adresse')
    let csv_comma = b"id,nom,adresse\n\
100,User1,8 bd saint michel\n";

    let multipart_comma =
        build_multipart_body(boundary, &[], Some(("data", "comma.csv", csv_comma)));

    let req_comma = Request::builder()
        .method("POST")
        .uri("/search/csv/")
        .header(
            "Content-Type",
            format!("multipart/form-data; boundary={}", boundary),
        )
        .body(Body::from(multipart_comma))
        .unwrap();

    let resp_comma = app.clone().oneshot(req_comma).await.unwrap();
    assert_eq!(resp_comma.status(), StatusCode::OK);
    let bytes_comma = resp_comma.into_body().collect().await.unwrap().to_bytes();
    let output_comma = String::from_utf8(bytes_comma.to_vec()).unwrap();

    assert!(output_comma.contains(','));
    assert!(output_comma.contains("75105_0001_00008"));

    // 3. Tab delimited CSV auto-detection
    let csv_tab = b"id\tq\n\
200\t10 champs elysees\n\
201\tchamps elysees\n";

    let multipart_tab = build_multipart_body(boundary, &[], Some(("data", "tab.csv", csv_tab)));

    let req_tab = Request::builder()
        .method("POST")
        .uri("/search/csv/")
        .header(
            "Content-Type",
            format!("multipart/form-data; boundary={}", boundary),
        )
        .body(Body::from(multipart_tab))
        .unwrap();

    let resp_tab = app.oneshot(req_tab).await.unwrap();
    assert_eq!(resp_tab.status(), StatusCode::OK);
    let bytes_tab = resp_tab.into_body().collect().await.unwrap().to_bytes();
    let output_tab = String::from_utf8(bytes_tab.to_vec()).unwrap();

    assert!(output_tab.contains('\t'));
    assert!(output_tab.contains("75108_0002_00010"));
    assert!(output_tab.contains("75108_0002"));
}

#[tokio::test]
async fn test_reverse_csv_batch_streaming() {
    let (_temp, searcher) = setup_test_index();
    let app = create_router(AppState {
        searcher: Arc::new(searcher),
    });

    let boundary = "---------------------------974767299852498929531610575";

    let csv_content = b"point_id,lat,lon\n\
P1,48.8512,2.3431\n\
P2,48.8698,2.3082\n";

    let multipart_body =
        build_multipart_body(boundary, &[], Some(("data", "points.csv", csv_content)));

    let req = Request::builder()
        .method("POST")
        .uri("/reverse/csv/")
        .header(
            "Content-Type",
            format!("multipart/form-data; boundary={}", boundary),
        )
        .body(Body::from(multipart_body))
        .unwrap();

    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let output_csv = String::from_utf8(bytes.to_vec()).unwrap();

    // Check extra output columns present
    assert!(output_csv.contains("result_label"));
    assert!(output_csv.contains("result_distance"));
    assert!(output_csv.contains("latitude"));
    assert!(output_csv.contains("longitude"));

    // Check nearest addresses found for coordinates
    assert!(output_csv.contains("75105_0001_00008"));
    assert!(output_csv.contains("75108_0002_00010"));

    // Error test: Missing lat/lon columns returns 400 Bad Request
    let csv_bad = b"point_id,name\nP1,test\n";
    let multipart_bad = build_multipart_body(boundary, &[], Some(("data", "bad.csv", csv_bad)));
    let req_bad = Request::builder()
        .method("POST")
        .uri("/reverse/csv/")
        .header(
            "Content-Type",
            format!("multipart/form-data; boundary={}", boundary),
        )
        .body(Body::from(multipart_bad))
        .unwrap();

    let resp_bad = app.oneshot(req_bad).await.unwrap();
    assert_eq!(resp_bad.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_query_logging_environment_variables() {
    std::env::set_var("LOG_QUERIES", "1");
    std::env::set_var("SLOW_QUERIES", "0.1");
    std::env::set_var("LOG_NOT_FOUND", "1");

    let (_temp, searcher) = setup_test_index();
    let app = create_router(AppState {
        searcher: Arc::new(searcher),
    });

    let req = Request::builder()
        .method("GET")
        .uri("/search/?q=nonexistent+street+nowhere")
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    std::env::remove_var("LOG_QUERIES");
    std::env::remove_var("SLOW_QUERIES");
    std::env::remove_var("LOG_NOT_FOUND");
}

#[tokio::test]
async fn test_prometheus_metrics_endpoint() {
    let (_temp, searcher) = setup_test_index();
    let app = create_router(AppState {
        searcher: Arc::new(searcher),
    });

    let req = Request::builder()
        .method("GET")
        .uri("/metrics")
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(
        resp.headers().get("content-type").unwrap(),
        "text/plain; version=0.0.4; charset=utf-8"
    );

    let body_bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let body_str = String::from_utf8(body_bytes.to_vec()).unwrap();
    assert!(body_str.contains("addok_search_requests_total"));
    assert!(body_str.contains("addok_reverse_requests_total"));
    assert!(body_str.contains("addok_csv_search_requests_total"));
    assert!(body_str.contains("addok_csv_reverse_requests_total"));
}

#[tokio::test]
async fn test_search_csv_with_result_columns() {
    let (_temp, searcher) = setup_test_index();
    let app = create_router(AppState {
        searcher: Arc::new(searcher),
    });

    let boundary = "---------------------------974767299852498929531610575";
    let csv_content = b"id,q\n1,8 boulevard saint-michel\n";

    let multipart_body = build_multipart_body(
        boundary,
        &[("result_columns", "latitude,longitude,result_score")],
        Some(("data", "input.csv", csv_content)),
    );

    let req = Request::builder()
        .method("POST")
        .uri("/search/csv/")
        .header(
            "Content-Type",
            format!("multipart/form-data; boundary={}", boundary),
        )
        .body(Body::from(multipart_body))
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let output_csv = String::from_utf8(bytes.to_vec()).unwrap();
    let mut lines = output_csv.lines();
    let header_line = lines.next().unwrap();

    // Verify only the requested extra columns are present
    assert_eq!(header_line, "id,q,latitude,longitude,result_score");
    assert!(!header_line.contains("result_postcode"));
    assert!(!header_line.contains("result_city"));

    // Verify row output
    let row_line = lines.next().unwrap();
    assert!(row_line.contains("48.8512"));
    assert!(row_line.contains("2.3431"));
}

#[tokio::test]
async fn test_filter_brackets_and_reverse_type_filter() {
    let (_temp, searcher) = setup_test_index();
    let app = create_router(AppState {
        searcher: Arc::new(searcher),
    });

    // 1. Search with filter[postcode] syntax
    let req = Request::builder()
        .method("GET")
        .uri("/search/?q=paris&filter[postcode]=75008")
        .body(Body::empty())
        .unwrap();

    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let col: FeatureCollection = serde_json::from_slice(&bytes).unwrap();
    for f in &col.features {
        assert_eq!(f.properties.postcode, "75008");
    }

    // 2. Reverse with filter[type]=housenumber
    let req_rev = Request::builder()
        .method("GET")
        .uri("/reverse/?lat=48.8512&lon=2.3431&filter[type]=housenumber")
        .body(Body::empty())
        .unwrap();

    let resp_rev = app.clone().oneshot(req_rev).await.unwrap();
    assert_eq!(resp_rev.status(), StatusCode::OK);
    let bytes_rev = resp_rev.into_body().collect().await.unwrap().to_bytes();
    let col_rev: FeatureCollection = serde_json::from_slice(&bytes_rev).unwrap();
    assert!(!col_rev.features.is_empty());
    assert_eq!(col_rev.features[0].properties.address_type, "housenumber");

    // 3. Reverse with type not matching -> empty results
    let req_none = Request::builder()
        .method("GET")
        .uri("/reverse/?lat=48.8512&lon=2.3431&type=municipality")
        .body(Body::empty())
        .unwrap();

    let resp_none = app.oneshot(req_none).await.unwrap();
    assert_eq!(resp_none.status(), StatusCode::OK);
    let bytes_none = resp_none.into_body().collect().await.unwrap().to_bytes();
    let col_none: FeatureCollection = serde_json::from_slice(&bytes_none).unwrap();
    assert!(col_none.features.is_empty());
}

#[tokio::test]
async fn test_search_clean_query_cedex_and_noise() {
    let (_temp, searcher) = setup_test_index();
    let app = create_router(AppState {
        searcher: Arc::new(searcher),
    });

    // Query with "cedex 15" and "bp 402" noise stripped by clean_query
    let req = Request::builder()
        .method("GET")
        .uri("/search/?q=8+boulevard+saint-michel+cedex+15+bp+402")
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let col: FeatureCollection = serde_json::from_slice(&bytes).unwrap();
    assert!(!col.features.is_empty());
    assert_eq!(col.features[0].properties.housenumber.as_deref(), Some("8"));
    assert_eq!(
        col.features[0].properties.street.as_deref(),
        Some("Boulevard Saint-Michel")
    );
}

#[tokio::test]
async fn test_reverse_multi_filter_postcode_citycode_and_k_alias() {
    let (_temp, searcher) = setup_test_index();
    let app = create_router(AppState {
        searcher: Arc::new(searcher),
    });

    // Reverse with postcode filter matching
    let req_pc = Request::builder()
        .method("GET")
        .uri("/reverse/?lat=48.8512&lon=2.3431&postcode=75005&k=2")
        .body(Body::empty())
        .unwrap();

    let resp_pc = app.clone().oneshot(req_pc).await.unwrap();
    assert_eq!(resp_pc.status(), StatusCode::OK);
    let bytes_pc = resp_pc.into_body().collect().await.unwrap().to_bytes();
    let col_pc: FeatureCollection = serde_json::from_slice(&bytes_pc).unwrap();
    assert!(!col_pc.features.is_empty());
    assert_eq!(col_pc.features[0].properties.postcode, "75005");

    // Reverse with postcode=75008: filters out 75005 (at 0m) and returns 75008
    let req_75008 = Request::builder()
        .method("GET")
        .uri("/reverse/?lat=48.8512&lon=2.3431&postcode=75008")
        .body(Body::empty())
        .unwrap();

    let resp_75008 = app.clone().oneshot(req_75008).await.unwrap();
    assert_eq!(resp_75008.status(), StatusCode::OK);
    let bytes_75008 = resp_75008.into_body().collect().await.unwrap().to_bytes();
    let col_75008: FeatureCollection = serde_json::from_slice(&bytes_75008).unwrap();
    assert!(!col_75008.features.is_empty());
    assert_eq!(col_75008.features[0].properties.postcode, "75008");
    assert_eq!(
        col_75008.features[0].properties.housenumber.as_deref(),
        Some("10")
    );

    // Reverse with postcode=99999 (non-existent) -> empty
    let req_nomatch = Request::builder()
        .method("GET")
        .uri("/reverse/?lat=48.8512&lon=2.3431&postcode=99999")
        .body(Body::empty())
        .unwrap();

    let resp_nomatch = app.clone().oneshot(req_nomatch).await.unwrap();
    assert_eq!(resp_nomatch.status(), StatusCode::OK);
    let bytes_nomatch = resp_nomatch.into_body().collect().await.unwrap().to_bytes();
    let col_nomatch: FeatureCollection = serde_json::from_slice(&bytes_nomatch).unwrap();
    assert!(col_nomatch.features.is_empty());

    // Reverse with citycode filter matching
    let req_cc = Request::builder()
        .method("GET")
        .uri("/reverse/?lat=48.8512&lon=2.3431&citycode=75105")
        .body(Body::empty())
        .unwrap();

    let resp_cc = app.oneshot(req_cc).await.unwrap();
    assert_eq!(resp_cc.status(), StatusCode::OK);
    let bytes_cc = resp_cc.into_body().collect().await.unwrap().to_bytes();
    let col_cc: FeatureCollection = serde_json::from_slice(&bytes_cc).unwrap();
    assert!(!col_cc.features.is_empty());
    assert_eq!(
        col_cc.features[0].properties.citycode.as_deref(),
        Some("75105")
    );
}

#[tokio::test]
async fn test_search_csv_with_name_and_street_columns() {
    let (_temp, searcher) = setup_test_index();
    let app = create_router(AppState {
        searcher: Arc::new(searcher),
    });

    let boundary = "X-BOUNDARY-123";
    let csv_content = b"address,city\n8 Boulevard Saint-Michel,Paris\n";
    let body = build_multipart_body(
        boundary,
        &[("result_columns", "result_name,result_street,result_score")],
        Some(("data", "input.csv", csv_content)),
    );

    let req = Request::builder()
        .method("POST")
        .uri("/search/csv/?columns=address&columns=city")
        .header(
            header::CONTENT_TYPE,
            format!("multipart/form-data; boundary={}", boundary),
        )
        .body(Body::from(body))
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let text = String::from_utf8(bytes.to_vec()).unwrap();
    let mut lines = text.lines();

    let header_line = lines.next().unwrap();
    assert!(header_line.contains("result_name"));
    assert!(header_line.contains("result_street"));

    let row_line = lines.next().unwrap();
    assert!(row_line.contains("Boulevard Saint-Michel"));
}

#[tokio::test]
async fn test_custom_extra_properties_in_geojson() {
    let temp_dir = tempdir().unwrap();
    let index_dir = temp_dir.path().join("extra_index");

    let mut doc = addok_core::document::Document::new("75105_9999", "Rue de Sorbonne")
        .with_type("street")
        .with_postcode("75005")
        .with_city("Paris");
    doc.extra.insert(
        "custom_source".to_string(),
        serde_json::json!("cadastre_dgfip"),
    );
    doc.extra
        .insert("insee_dep".to_string(), serde_json::json!("75"));

    let indexer = addok_index::AddokIndexer::new(addok_index::IndexerConfig::default());
    indexer.index_documents(vec![Ok(doc)], &index_dir).unwrap();

    let searcher = addok_index::AddokSearcher::open(&index_dir).unwrap();
    let app = create_router(AppState {
        searcher: Arc::new(searcher),
    });

    let req = Request::builder()
        .method("GET")
        .uri("/search/?q=Rue+de+Sorbonne")
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let col: FeatureCollection = serde_json::from_slice(&bytes).unwrap();
    assert!(!col.features.is_empty());
    let props = &col.features[0].properties;
    assert_eq!(
        props.extra.get("custom_source"),
        Some(&serde_json::json!("cadastre_dgfip"))
    );
    assert_eq!(props.extra.get("insee_dep"), Some(&serde_json::json!("75")));
}

#[tokio::test]
async fn test_plugin_csv_disabled_returns_404() {
    let (_dir, searcher) = setup_test_index();

    // Create searcher with custom config that does NOT include addok_csv
    let custom_config = addok_core::AddokConfig {
        plugins: vec![
            "addok.shell".to_string(),
            "addok.http".to_string(),
            "addok.fuzzy".to_string(),
            "addok.autocomplete".to_string(),
        ],
        ..Default::default()
    };
    let searcher_without_csv = searcher.with_config(custom_config);
    let app = create_router(AppState {
        searcher: Arc::new(searcher_without_csv),
    });

    let csv_payload = "q\n8 bd st michel\n";
    let req = Request::builder()
        .method("POST")
        .uri("/search/csv/")
        .header(header::CONTENT_TYPE, "multipart/form-data; boundary=X")
        .body(Body::from(format!(
            "--X\r\nContent-Disposition: form-data; name=\"data\"; filename=\"test.csv\"\r\nContent-Type: text/csv\r\n\r\n{}\r\n--X--\r\n",
            csv_payload
        )))
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}
