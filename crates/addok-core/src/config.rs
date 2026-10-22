use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

use crate::error::Result;

/// Default field boost if unspecified.
fn default_field_boost() -> f32 {
    1.0
}

/// Default field nullability if unspecified.
fn default_field_null() -> bool {
    true
}

/// Configuration for an indexed/searchable document field and its boost weight.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FieldConfig {
    pub key: String,

    #[serde(default = "default_field_boost")]
    pub boost: f32,

    #[serde(default = "default_field_null")]
    pub null: bool,
}

impl FieldConfig {
    pub fn new(key: impl Into<String>, boost: f32, null: bool) -> Self {
        Self {
            key: key.into(),
            boost,
            null,
        }
    }
}

/// Default settings matching Addok's Python `default.py`.
fn default_bucket_min() -> usize {
    10
}
fn default_bucket_max() -> usize {
    100
}
fn default_common_threshold() -> usize {
    10000
}
fn default_intersect_limit() -> usize {
    100000
}
fn default_match_threshold() -> f32 {
    0.9
}
fn default_min_score() -> f32 {
    0.1
}
fn default_query_max_length() -> usize {
    200
}
fn default_min_edge_ngrams() -> usize {
    3
}
fn default_max_edge_ngrams() -> usize {
    20
}
fn default_filters_multi_value_separator() -> String {
    " ".to_string()
}
fn default_filters() -> Vec<String> {
    vec![
        "type".to_string(),
        "postcode".to_string(),
        "citycode".to_string(),
    ]
}

fn default_attribution() -> String {
    "BAN (Base Adresse Nationale)".to_string()
}
fn default_licence() -> String {
    "ODbL 1.0".to_string()
}

pub fn default_plugins() -> Vec<String> {
    vec![
        "addok.shell".to_string(),
        "addok.http".to_string(),
        "addok.batch".to_string(),
        "addok.pairs".to_string(),
        "addok.fuzzy".to_string(),
        "addok.autocomplete".to_string(),
        "addok_france".to_string(),
        "addok_csv".to_string(),
    ]
}

pub fn default_fields() -> Vec<FieldConfig> {
    vec![
        FieldConfig::new("name", 4.0, false),
        FieldConfig::new("street", 1.0, true),
        FieldConfig::new("city", 1.0, true),
        FieldConfig::new("postcode", 1.0, true),
        FieldConfig::new("housenumbers", 1.0, true),
        FieldConfig::new("context", 1.0, true),
    ]
}

/// Addok Configuration Engine matching Addok's specification (`default.py`).
///
/// Supports loading defaults, parsing `addok.toml`, and overriding settings via `ADDOK_*`
/// environment variables.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AddokConfig {
    #[serde(default = "default_bucket_min")]
    pub bucket_min: usize,

    #[serde(default = "default_bucket_max")]
    pub bucket_max: usize,

    #[serde(default = "default_common_threshold")]
    pub common_threshold: usize,

    #[serde(default = "default_intersect_limit")]
    pub intersect_limit: usize,

    #[serde(default = "default_match_threshold")]
    pub match_threshold: f32,

    #[serde(default = "default_min_score")]
    pub min_score: f32,

    #[serde(default = "default_query_max_length")]
    pub query_max_length: usize,

    #[serde(default = "default_min_edge_ngrams")]
    pub min_edge_ngrams: usize,

    #[serde(default = "default_max_edge_ngrams")]
    pub max_edge_ngrams: usize,

    #[serde(default = "default_filters")]
    pub filters: Vec<String>,

    #[serde(default = "default_filters_multi_value_separator")]
    pub filters_multi_value_separator: String,

    #[serde(default = "default_fields")]
    pub fields: Vec<FieldConfig>,

    #[serde(default = "default_plugins")]
    pub plugins: Vec<String>,

    #[serde(default = "default_attribution")]
    pub attribution: String,

    #[serde(default = "default_licence")]
    pub licence: String,

    #[serde(default)]
    pub log_queries: bool,

    #[serde(default)]
    pub slow_queries: Option<f64>,

    #[serde(default)]
    pub log_not_found: bool,

    #[serde(default)]
    pub synonyms_paths: Vec<String>,

    #[serde(flatten, default)]
    pub extra: HashMap<String, serde_json::Value>,
}

impl Default for AddokConfig {
    fn default() -> Self {
        Self {
            bucket_min: default_bucket_min(),
            bucket_max: default_bucket_max(),
            common_threshold: default_common_threshold(),
            intersect_limit: default_intersect_limit(),
            match_threshold: default_match_threshold(),
            min_score: default_min_score(),
            query_max_length: default_query_max_length(),
            min_edge_ngrams: default_min_edge_ngrams(),
            max_edge_ngrams: default_max_edge_ngrams(),
            filters: default_filters(),
            filters_multi_value_separator: default_filters_multi_value_separator(),
            fields: default_fields(),
            plugins: default_plugins(),
            attribution: default_attribution(),
            licence: default_licence(),
            synonyms_paths: Vec::new(),
            log_queries: false,
            slow_queries: None,
            log_not_found: false,
            extra: HashMap::new(),
        }
    }
}

impl AddokConfig {
    /// Load configuration following priority:
    /// 1. Embedded defaults (matching Addok's `default.py`)
    /// 2. `addok.toml` (either path in `ADDOK_CONFIG_PATH` / `ADDOK_CONF`, or `./addok.toml`)
    /// 3. Environment variable overrides starting with `ADDOK_*`
    pub fn load() -> Result<Self> {
        let mut config = if let Some(path) =
            std::env::var_os("ADDOK_CONFIG_PATH").or_else(|| std::env::var_os("ADDOK_CONF"))
        {
            Self::from_file(path)?
        } else if Path::new("addok.toml").exists() {
            Self::from_file("addok.toml")?
        } else {
            Self::default()
        };

        config.apply_env();
        Ok(config)
    }

    /// Load configuration from a TOML formatted file.
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let content = fs::read_to_string(path)?;
        Self::from_toml_str(&content)
    }

    /// Parse configuration from a TOML string.
    pub fn from_toml_str(content: &str) -> Result<Self> {
        let config: Self = toml::from_str(content)?;
        Ok(config)
    }

    /// Serialize configuration to a TOML formatted string.
    pub fn to_toml_string(&self) -> Result<String> {
        let toml_str = toml::to_string_pretty(self)?;
        Ok(toml_str)
    }

    /// Save configuration to a file.
    pub fn save<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        let toml_str = self.to_toml_string()?;
        fs::write(path, toml_str)?;
        Ok(())
    }

    /// Inspect environment variables and apply any `ADDOK_*` overrides.
    pub fn apply_env(&mut self) {
        for (key, val) in std::env::vars() {
            if let Some(suffix) = key.strip_prefix("ADDOK_") {
                let lower_suffix = suffix.to_ascii_lowercase();
                match lower_suffix.as_str() {
                    "bucket_min" => {
                        if let Ok(v) = val.parse::<usize>() {
                            self.bucket_min = v;
                        }
                    }
                    "bucket_max" => {
                        if let Ok(v) = val.parse::<usize>() {
                            self.bucket_max = v;
                        }
                    }
                    "common_threshold" => {
                        if let Ok(v) = val.parse::<usize>() {
                            self.common_threshold = v;
                        }
                    }
                    "intersect_limit" => {
                        if let Ok(v) = val.parse::<usize>() {
                            self.intersect_limit = v;
                        }
                    }
                    "match_threshold" => {
                        if let Ok(v) = val.parse::<f32>() {
                            self.match_threshold = v;
                        }
                    }
                    "min_score" => {
                        if let Ok(v) = val.parse::<f32>() {
                            self.min_score = v;
                        }
                    }
                    "query_max_length" => {
                        if let Ok(v) = val.parse::<usize>() {
                            self.query_max_length = v;
                        }
                    }
                    "min_edge_ngrams" => {
                        if let Ok(v) = val.parse::<usize>() {
                            self.min_edge_ngrams = v;
                        }
                    }
                    "max_edge_ngrams" => {
                        if let Ok(v) = val.parse::<usize>() {
                            self.max_edge_ngrams = v;
                        }
                    }
                    "filters_multi_value_separator" => {
                        self.filters_multi_value_separator = val;
                    }
                    "filters" => {
                        // Split by comma or whitespace
                        let parts: Vec<String> = val
                            .split([',', ' '])
                            .map(str::trim)
                            .filter(|s| !s.is_empty())
                            .map(ToString::to_string)
                            .collect();
                        if !parts.is_empty() {
                            self.filters = parts;
                        }
                    }
                    "plugins" => {
                        let parts: Vec<String> = val
                            .split([',', ' '])
                            .map(str::trim)
                            .filter(|s| !s.is_empty())
                            .map(ToString::to_string)
                            .collect();
                        if !parts.is_empty() {
                            self.plugins = parts;
                        }
                    }
                    "log_queries" => {
                        self.log_queries = val == "1"
                            || val.eq_ignore_ascii_case("true")
                            || val.eq_ignore_ascii_case("yes");
                    }
                    "slow_queries" => {
                        if let Ok(ms) = val.parse::<f64>() {
                            self.slow_queries = Some(ms);
                        }
                    }
                    "log_not_found" => {
                        self.log_not_found = val == "1"
                            || val.eq_ignore_ascii_case("true")
                            || val.eq_ignore_ascii_case("yes");
                    }
                    "attribution" => {
                        self.attribution = val;
                    }
                    "licence" => {
                        self.licence = val;
                    }
                    _ => {
                        // Custom attributes
                        if let Ok(num) = val.parse::<i64>() {
                            self.extra.insert(lower_suffix, serde_json::json!(num));
                        } else if let Ok(flt) = val.parse::<f64>() {
                            self.extra.insert(lower_suffix, serde_json::json!(flt));
                        } else if let Ok(b) = val.parse::<bool>() {
                            self.extra.insert(lower_suffix, serde_json::json!(b));
                        } else {
                            self.extra.insert(lower_suffix, serde_json::json!(val));
                        }
                    }
                }
            }
        }

        if let Ok(val) = std::env::var("PLUGINS") {
            let parts: Vec<String> = val
                .split([',', ' '])
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(ToString::to_string)
                .collect();
            if !parts.is_empty() {
                self.plugins = parts;
            }
        }
        if let Ok(val) = std::env::var("LOG_QUERIES") {
            self.log_queries =
                val == "1" || val.eq_ignore_ascii_case("true") || val.eq_ignore_ascii_case("yes");
        }
        if let Ok(val) = std::env::var("SLOW_QUERIES") {
            if let Ok(ms) = val.parse::<f64>() {
                self.slow_queries = Some(ms);
            }
        }
        if let Ok(val) = std::env::var("LOG_NOT_FOUND") {
            self.log_not_found =
                val == "1" || val.eq_ignore_ascii_case("true") || val.eq_ignore_ascii_case("yes");
        }
    }

    /// Check whether a plugin is enabled.
    ///
    /// Matches exact name, name with hyphens replaced by underscores,
    /// or suffix (e.g. `has_plugin("france")` matches `"addok_france"` or `"addok-france"`).
    pub fn has_plugin(&self, name: &str) -> bool {
        let norm_target = name.trim().replace('-', "_").to_ascii_lowercase();
        self.plugins.iter().any(|p| {
            let p_norm = p.trim().replace('-', "_").to_ascii_lowercase();
            p_norm == norm_target
                || p_norm.ends_with(&format!(".{}", norm_target))
                || p_norm.ends_with(&format!("_{}", norm_target))
        })
    }

    /// Check if French address plugin (`addok_france` / `addok-france` / `addok_fr`) is active.
    #[inline]
    pub fn has_france(&self) -> bool {
        self.has_plugin("addok_france") || self.has_plugin("france") || self.has_plugin("addok_fr")
    }

    /// Check if CSV batch plugin (`addok_csv` / `addok-csv`) is active.
    #[inline]
    pub fn has_csv(&self) -> bool {
        self.has_plugin("addok_csv") || self.has_plugin("csv")
    }

    /// Check if fuzzy search plugin (`addok.fuzzy`) is active.
    #[inline]
    pub fn has_fuzzy(&self) -> bool {
        self.has_plugin("addok.fuzzy") || self.has_plugin("fuzzy")
    }

    /// Check if autocomplete plugin (`addok.autocomplete`) is active.
    #[inline]
    pub fn has_autocomplete(&self) -> bool {
        self.has_plugin("addok.autocomplete") || self.has_plugin("autocomplete")
    }

    /// Check if token pairs indexer plugin (`addok.pairs`) is active.
    #[inline]
    pub fn has_pairs(&self) -> bool {
        self.has_plugin("addok.pairs") || self.has_plugin("pairs")
    }

    /// Retrieve configured boost weight for a given field key.
    ///
    /// Defaults to 1.0 if field is not explicitly registered in `fields`.
    pub fn field_boost(&self, key: &str) -> f32 {
        self.fields
            .iter()
            .find(|f| f.key == key)
            .map(|f| f.boost)
            .unwrap_or(1.0)
    }

    /// Check if a field key is configured as a query filter.
    pub fn is_filter(&self, key: &str) -> bool {
        self.filters.iter().any(|f| f == key)
    }

    /// Check if a field is configured as nullable.
    pub fn is_field_nullable(&self, key: &str) -> bool {
        self.fields
            .iter()
            .find(|f| f.key == key)
            .map(|f| f.null)
            .unwrap_or(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config_matches_addok() {
        let config = AddokConfig::default();
        assert_eq!(config.bucket_min, 10);
        assert_eq!(config.bucket_max, 100);
        assert_eq!(config.common_threshold, 10000);
        assert_eq!(config.intersect_limit, 100000);
        assert!((config.match_threshold - 0.9).abs() < 1e-4);
        assert!((config.min_score - 0.1).abs() < 1e-4);
        assert_eq!(config.query_max_length, 200);
        assert_eq!(config.min_edge_ngrams, 3);
        assert_eq!(config.max_edge_ngrams, 20);
        assert_eq!(config.filters, vec!["type", "postcode", "citycode"]);
        assert_eq!(config.filters_multi_value_separator, " ");

        assert_eq!(config.field_boost("name"), 4.0);
        assert_eq!(config.field_boost("city"), 1.0);
        assert_eq!(config.field_boost("unknown_field"), 1.0);
        assert!(!config.is_field_nullable("name"));
        assert!(config.is_field_nullable("city"));
        assert!(config.is_filter("postcode"));
        assert!(!config.is_filter("random_filter"));
    }

    #[test]
    fn test_toml_serialization_deserialization() {
        let toml_str = r#"
bucket_min = 25
bucket_max = 250
common_threshold = 20000
intersect_limit = 50000
match_threshold = 0.85
min_score = 0.2
query_max_length = 150
min_edge_ngrams = 2
max_edge_ngrams = 25
filters = ["type", "postcode"]
filters_multi_value_separator = ","

[[fields]]
key = "name"
boost = 5.0
null = false

[[fields]]
key = "city"
boost = 2.0
null = true
"#;

        let config = AddokConfig::from_toml_str(toml_str).unwrap();
        assert_eq!(config.bucket_min, 25);
        assert_eq!(config.bucket_max, 250);
        assert_eq!(config.common_threshold, 20000);
        assert_eq!(config.intersect_limit, 50000);
        assert!((config.match_threshold - 0.85).abs() < 1e-4);
        assert!((config.min_score - 0.2).abs() < 1e-4);
        assert_eq!(config.query_max_length, 150);
        assert_eq!(config.min_edge_ngrams, 2);
        assert_eq!(config.max_edge_ngrams, 25);
        assert_eq!(config.filters, vec!["type", "postcode"]);
        assert_eq!(config.filters_multi_value_separator, ",");
        assert_eq!(config.field_boost("name"), 5.0);
        assert_eq!(config.field_boost("city"), 2.0);

        // Test roundtrip
        let serialized = config.to_toml_string().unwrap();
        let config2 = AddokConfig::from_toml_str(&serialized).unwrap();
        assert_eq!(config, config2);
    }

    #[test]
    fn test_env_override() {
        let mut config = AddokConfig::default();

        std::env::set_var("ADDOK_BUCKET_MIN", "42");
        std::env::set_var("ADDOK_MATCH_THRESHOLD", "0.75");
        std::env::set_var("ADDOK_FILTERS", "type,department");

        config.apply_env();

        std::env::remove_var("ADDOK_BUCKET_MIN");
        std::env::remove_var("ADDOK_MATCH_THRESHOLD");
        std::env::remove_var("ADDOK_FILTERS");

        assert_eq!(config.bucket_min, 42);
        assert!((config.match_threshold - 0.75).abs() < 1e-4);
        assert_eq!(config.filters, vec!["type", "department"]);
    }

    #[test]
    fn test_plugins_configuration() {
        let config = AddokConfig::default();
        assert!(config.has_plugin("addok_france"));
        assert!(config.has_plugin("addok-france"));
        assert!(config.has_france());
        assert!(config.has_csv());
        assert!(config.has_fuzzy());
        assert!(config.has_autocomplete());
        assert!(config.has_pairs());

        // Test custom plugins without France and CSV
        let toml_str = r#"
plugins = ["addok.shell", "addok.http", "addok.fuzzy"]
"#;
        let custom: AddokConfig = AddokConfig::from_toml_str(toml_str).unwrap();
        assert!(!custom.has_france());
        assert!(!custom.has_csv());
        assert!(custom.has_fuzzy());
        assert!(!custom.has_autocomplete());
    }
}
