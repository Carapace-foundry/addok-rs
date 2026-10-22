pub mod aggregation;
pub mod ban;
pub mod config;
pub mod document;
pub mod error;
pub mod normalizer;
pub mod parser;

pub use aggregation::{
    aggregate_ban_csv, aggregate_ban_csv_reader, aggregate_ban_records, BanAggregator,
};
pub use ban::{BanRecord, BanType};
pub use config::{default_fields, AddokConfig, FieldConfig};
pub use document::{
    open_ndjson_file, open_ndjson_reader, write_ndjson, Document, DocumentStream, HousenumberData,
};
pub use error::{CoreError, Result};
pub use normalizer::{
    ascii_fold, clean_query, expand_abbreviation, is_stop_word, normalize_address,
    normalize_french_address, tokenize_address, tokenize_french_address, FRENCH_STOP_WORDS,
};
pub use parser::{open_ban_csv, open_ban_csv_reader, BanRecordStream};
