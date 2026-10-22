use thiserror::Error;

#[derive(Error, Debug)]
pub enum CoreError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("CSV parsing error: {0}")]
    Csv(#[from] csv::Error),

    #[error("JSON serialization/deserialization error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("TOML deserialization error: {0}")]
    TomlDe(#[from] toml::de::Error),

    #[error("TOML serialization error: {0}")]
    TomlSer(#[from] toml::ser::Error),

    #[error("Invalid BAN record: {0}")]
    InvalidRecord(String),

    #[error("Invalid document at line {line}: {reason}")]
    InvalidDocument { line: usize, reason: String },

    #[error("Configuration error: {0}")]
    Config(String),

    #[error("Unsupported file format or extension: {0}")]
    UnsupportedFormat(String),
}

pub type Result<T> = std::result::Result<T, CoreError>;
