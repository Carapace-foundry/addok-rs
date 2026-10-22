use thiserror::Error;

#[derive(Error, Debug)]
pub enum IndexError {
    #[error("Core error: {0}")]
    Core(#[from] addok_core::CoreError),

    #[error("Tantivy error: {0}")]
    Tantivy(#[from] tantivy::TantivyError),

    #[error("Tantivy query parse error: {0}")]
    QueryParse(#[from] tantivy::query::QueryParserError),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Bincode serialization error: {0}")]
    Bincode(#[from] bincode::Error),

    #[error("Index directory not found or invalid: {0}")]
    InvalidDirectory(String),
}

pub type Result<T> = std::result::Result<T, IndexError>;
