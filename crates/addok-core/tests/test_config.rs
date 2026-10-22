use addok_core::{AddokConfig, FieldConfig};
use std::fs;
use tempfile::NamedTempFile;

#[test]
fn test_config_defaults_specification() {
    let config = AddokConfig::default();

    // Verify exact defaults matching Addok default.py
    assert_eq!(config.bucket_min, 10);
    assert_eq!(config.bucket_max, 100);
    assert_eq!(config.common_threshold, 10000);
    assert_eq!(config.intersect_limit, 100000);
    assert!((config.match_threshold - 0.9).abs() < 1e-4);
    assert!((config.min_score - 0.1).abs() < 1e-4);
    assert_eq!(config.query_max_length, 200);
    assert_eq!(config.min_edge_ngrams, 3);
    assert_eq!(config.max_edge_ngrams, 20);

    // Filters
    assert_eq!(
        config.filters,
        vec![
            "type".to_string(),
            "postcode".to_string(),
            "citycode".to_string()
        ]
    );
    assert_eq!(config.filters_multi_value_separator, " ");

    // Field boosts
    assert_eq!(config.field_boost("name"), 4.0);
    assert_eq!(config.field_boost("city"), 1.0);
    assert_eq!(config.field_boost("postcode"), 1.0);
    assert_eq!(config.field_boost("housenumbers"), 1.0);
    assert_eq!(config.field_boost("context"), 1.0);
    assert_eq!(config.field_boost("unregistered"), 1.0);

    let custom_field = FieldConfig::new("custom_code", 3.0, false);
    assert_eq!(custom_field.key, "custom_code");
    assert_eq!(custom_field.boost, 3.0);
    assert!(!custom_field.null);

    // Nullability
    assert!(!config.is_field_nullable("name"));
    assert!(config.is_field_nullable("city"));
    assert!(config.is_field_nullable("postcode"));
}

#[test]
fn test_config_loading_from_toml_file() {
    let toml_content = r#"
bucket_min = 15
bucket_max = 150
common_threshold = 5000
intersect_limit = 80000
match_threshold = 0.88
min_score = 0.15
query_max_length = 250
min_edge_ngrams = 2
max_edge_ngrams = 30
filters = ["type", "citycode"]
filters_multi_value_separator = "|"

[[fields]]
key = "name"
boost = 6.0
null = false

[[fields]]
key = "city"
boost = 2.5
null = true

[extra_settings]
custom_timeout = 3000
"#;

    let temp_file = NamedTempFile::new().unwrap();
    fs::write(temp_file.path(), toml_content).unwrap();

    let config = AddokConfig::from_file(temp_file.path()).expect("Failed to load addok.toml file");

    assert_eq!(config.bucket_min, 15);
    assert_eq!(config.bucket_max, 150);
    assert_eq!(config.common_threshold, 5000);
    assert_eq!(config.intersect_limit, 80000);
    assert!((config.match_threshold - 0.88).abs() < 1e-4);
    assert!((config.min_score - 0.15).abs() < 1e-4);
    assert_eq!(config.query_max_length, 250);
    assert_eq!(config.min_edge_ngrams, 2);
    assert_eq!(config.max_edge_ngrams, 30);
    assert_eq!(config.filters, vec!["type", "citycode"]);
    assert_eq!(config.filters_multi_value_separator, "|");

    assert_eq!(config.field_boost("name"), 6.0);
    assert_eq!(config.field_boost("city"), 2.5);

    // Arbitrary extra settings loaded in TOML
    assert!(config.extra.contains_key("extra_settings"));
}

#[test]
fn test_config_env_variable_overrides() {
    let mut config = AddokConfig::default();

    std::env::set_var("ADDOK_BUCKET_MIN", "35");
    std::env::set_var("ADDOK_BUCKET_MAX", "350");
    std::env::set_var("ADDOK_COMMON_THRESHOLD", "12345");
    std::env::set_var("ADDOK_INTERSECT_LIMIT", "67890");
    std::env::set_var("ADDOK_MATCH_THRESHOLD", "0.92");
    std::env::set_var("ADDOK_MIN_SCORE", "0.25");
    std::env::set_var("ADDOK_QUERY_MAX_LENGTH", "180");
    std::env::set_var("ADDOK_MIN_EDGE_NGRAMS", "4");
    std::env::set_var("ADDOK_MAX_EDGE_NGRAMS", "18");
    std::env::set_var("ADDOK_FILTERS", "type,postcode,custom_tag");
    std::env::set_var("ADDOK_FILTERS_MULTI_VALUE_SEPARATOR", ";");
    std::env::set_var("ADDOK_CUSTOM_INT", "42");

    config.apply_env();

    // Clean up environment variables
    std::env::remove_var("ADDOK_BUCKET_MIN");
    std::env::remove_var("ADDOK_BUCKET_MAX");
    std::env::remove_var("ADDOK_COMMON_THRESHOLD");
    std::env::remove_var("ADDOK_INTERSECT_LIMIT");
    std::env::remove_var("ADDOK_MATCH_THRESHOLD");
    std::env::remove_var("ADDOK_MIN_SCORE");
    std::env::remove_var("ADDOK_QUERY_MAX_LENGTH");
    std::env::remove_var("ADDOK_MIN_EDGE_NGRAMS");
    std::env::remove_var("ADDOK_MAX_EDGE_NGRAMS");
    std::env::remove_var("ADDOK_FILTERS");
    std::env::remove_var("ADDOK_FILTERS_MULTI_VALUE_SEPARATOR");
    std::env::remove_var("ADDOK_CUSTOM_INT");

    assert_eq!(config.bucket_min, 35);
    assert_eq!(config.bucket_max, 350);
    assert_eq!(config.common_threshold, 12345);
    assert_eq!(config.intersect_limit, 67890);
    assert!((config.match_threshold - 0.92).abs() < 1e-4);
    assert!((config.min_score - 0.25).abs() < 1e-4);
    assert_eq!(config.query_max_length, 180);
    assert_eq!(config.min_edge_ngrams, 4);
    assert_eq!(config.max_edge_ngrams, 18);
    assert_eq!(config.filters, vec!["type", "postcode", "custom_tag"]);
    assert_eq!(config.filters_multi_value_separator, ";");
    assert_eq!(config.extra.get("custom_int"), Some(&serde_json::json!(42)));
}

#[test]
fn test_config_load_with_path_env() {
    let toml_content = r#"
bucket_min = 99
"#;
    let temp_file = NamedTempFile::new().unwrap();
    fs::write(temp_file.path(), toml_content).unwrap();

    std::env::set_var("ADDOK_CONFIG_PATH", temp_file.path());

    let loaded = AddokConfig::load().expect("Failed to load via ADDOK_CONFIG_PATH");
    std::env::remove_var("ADDOK_CONFIG_PATH");

    assert_eq!(loaded.bucket_min, 99);
    // Unspecified fields remain default
    assert_eq!(loaded.bucket_max, 100);
}
