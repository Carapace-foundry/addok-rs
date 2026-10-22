use std::fs;
use std::path::Path;
use std::time::Instant;

use addok_core::config::AddokConfig;
use addok_core::document::Document;
use addok_core::normalizer::{normalize_address, tokenize_address};
use addok_core::BanRecord;
use tantivy::{Index, IndexWriter};
use tracing::info;

use crate::doc_store::DocumentStore;
use crate::error::Result;
use crate::fields_index::FieldsInvertedIndex;
use crate::filters::FiltersIndex;
use crate::housenumbers::HousenumbersIndex;
use crate::ngram::EdgeNgramIndex;
use crate::pairs::PairsIndexer;
use crate::schema::AddokSchema;
use crate::spatial::{SpatialIndex, SpatialItem};

/// Indexing configuration
#[derive(Debug, Clone)]
pub struct IndexerConfig {
    /// Memory budget allocated for Tantivy IndexWriter buffer (in bytes, default: 50MB)
    pub writer_memory_budget: usize,
    /// Batch size for periodic indexing (default: 10,000)
    pub batch_size: usize,
    /// Addok system configuration (determines active plugins)
    pub addok_config: AddokConfig,
}

impl Default for IndexerConfig {
    fn default() -> Self {
        Self {
            writer_memory_budget: 50_000_000, // 50 MB
            batch_size: 10_000,
            addok_config: AddokConfig::default(),
        }
    }
}

/// Statistics returned after indexing completion
#[derive(Debug, Clone, Default)]
pub struct IndexStats {
    pub total_records: usize,
    pub indexed_records: usize,
    pub spatial_records: usize,
    pub duration_secs: f64,
}

/// Main indexer for building Addok's multi-index architecture:
/// - Fields Inverted Index
/// - Edge N-Gram Index
/// - Pairs Index (`PairsIndexer`)
/// - Housenumbers Index
/// - Filters Index
/// - Spatial Index
/// - Document Store
/// - Tantivy Index
pub struct AddokIndexer {
    schema: AddokSchema,
    config: IndexerConfig,
}

impl AddokIndexer {
    pub fn new(config: IndexerConfig) -> Self {
        Self {
            schema: AddokSchema::new(),
            config,
        }
    }

    pub fn with_addok_config(mut self, config: AddokConfig) -> Self {
        self.config.addok_config = config;
        self
    }

    /// Run full streaming indexing from an iterator of `Document` (NDJSON) into `output_dir`.
    /// Supports incremental batch commits if the index directory already exists.
    pub fn index_documents<I, P>(&self, documents: I, output_dir: P) -> Result<IndexStats>
    where
        I: IntoIterator<Item = addok_core::Result<Document>>,
        P: AsRef<Path>,
    {
        let start_time = Instant::now();
        let output_path = output_dir.as_ref();
        fs::create_dir_all(output_path)?;

        // 1. Initialize or load existing multi-index structures
        let mut doc_store = if output_path.join("docs.bin").exists() {
            DocumentStore::load_from_file(output_path.join("docs.bin")).unwrap_or_default()
        } else {
            DocumentStore::new()
        };

        let mut fields_index = if output_path.join("fields.bin").exists() {
            FieldsInvertedIndex::load_from_file(output_path.join("fields.bin")).unwrap_or_default()
        } else {
            FieldsInvertedIndex::default()
        };

        let mut filters_index = if output_path.join("filters.bin").exists() {
            FiltersIndex::load_from_file(output_path.join("filters.bin")).unwrap_or_default()
        } else {
            FiltersIndex::new()
        };

        let mut housenumbers_index = if output_path.join("housenumbers.bin").exists() {
            HousenumbersIndex::load_from_file(output_path.join("housenumbers.bin"))
                .unwrap_or_default()
        } else {
            HousenumbersIndex::new()
        };

        let mut pairs_index = if output_path.join("pairs.bin").exists() {
            PairsIndexer::load_from_file(output_path.join("pairs.bin")).unwrap_or_default()
        } else {
            PairsIndexer::new()
        };

        let mut ngram_index = if output_path.join("ngram.bin").exists() {
            EdgeNgramIndex::load_from_file(output_path.join("ngram.bin")).unwrap_or_default()
        } else {
            EdgeNgramIndex::default()
        };

        let mut spatial_items: Vec<SpatialItem> = if output_path.join("spatial.bin").exists() {
            SpatialIndex::load_from_bin(output_path.join("spatial.bin"))
                .map(|idx| idx.into_items())
                .unwrap_or_default()
        } else if output_path.join("spatial.json").exists() {
            SpatialIndex::load_from_file(output_path.join("spatial.json"))
                .map(|idx| idx.into_items())
                .unwrap_or_default()
        } else {
            Vec::new()
        };

        // Tantivy index
        let index = if let Ok(existing) = Index::open_in_dir(output_path) {
            existing
        } else {
            Index::create_in_dir(output_path, self.schema.schema.clone())?
        };
        let mut writer: IndexWriter = index.writer(self.config.writer_memory_budget)?;

        let mut total_records = 0;
        let mut indexed_records = 0;
        let mut batch_count = 0;

        for doc_res in documents {
            total_records += 1;
            match doc_res {
                Ok(doc) => {
                    self.process_document(
                        &doc,
                        &mut doc_store,
                        &mut fields_index,
                        &mut filters_index,
                        &mut housenumbers_index,
                        &mut pairs_index,
                        &mut ngram_index,
                        &mut spatial_items,
                        &mut writer,
                    )?;
                    indexed_records += 1;
                    batch_count += 1;

                    if batch_count >= self.config.batch_size {
                        writer.commit()?;
                        batch_count = 0;
                    }
                }
                Err(e) => {
                    tracing::warn!("Skipping invalid Document {}: {}", total_records, e);
                }
            }
        }

        info!("Committing Tantivy index and saving multi-index binary tables...");
        writer.commit()?;

        // Persist all multi-index structures to disk
        doc_store.save_to_file(output_path.join("docs.bin"))?;
        fields_index.save_to_file(output_path.join("fields.bin"))?;
        filters_index.save_to_file(output_path.join("filters.bin"))?;
        housenumbers_index.save_to_file(output_path.join("housenumbers.bin"))?;
        pairs_index.save_to_file(output_path.join("pairs.bin"))?;
        ngram_index.save_to_file(output_path.join("ngram.bin"))?;

        let spatial_records = spatial_items.len();
        let spatial_index = SpatialIndex::from_items(spatial_items);
        spatial_index.save_to_file(output_path.join("spatial.json"))?;
        spatial_index.save_to_bin(output_path.join("spatial.bin"))?;

        let duration = start_time.elapsed().as_secs_f64();
        info!(
            "Documents indexing completed: {} processed, {} indexed in {:.2}s",
            total_records, indexed_records, duration
        );

        Ok(IndexStats {
            total_records,
            indexed_records,
            spatial_records,
            duration_secs: duration,
        })
    }

    /// Run streaming indexing from an iterator of `BanRecord` into `output_dir`.
    /// Aggregates individual BAN records hierarchically with a bounded memory window.
    pub fn index_records<I, P>(&self, records: I, output_dir: P) -> Result<IndexStats>
    where
        I: IntoIterator<Item = addok_core::Result<BanRecord>>,
        P: AsRef<Path>,
    {
        let mut total_records = 0;
        let counted_records = records.into_iter().inspect(|_| total_records += 1);
        let streaming_docs =
            addok_core::aggregation::StreamingBanAggregator::new(counted_records, 10_000);
        let mut stats = self.index_documents(streaming_docs, output_dir)?;
        stats.total_records = total_records;
        Ok(stats)
    }

    #[allow(clippy::too_many_arguments)]
    fn process_document(
        &self,
        doc: &Document,
        doc_store: &mut DocumentStore,
        fields_index: &mut FieldsInvertedIndex,
        filters_index: &mut FiltersIndex,
        housenumbers_index: &mut HousenumbersIndex,
        pairs_index: &mut PairsIndexer,
        ngram_index: &mut EdgeNgramIndex,
        spatial_items: &mut Vec<SpatialItem>,
        writer: &mut IndexWriter,
    ) -> Result<()> {
        let doc_id = doc_store.insert(doc.clone());

        let has_france = self.config.addok_config.has_france();
        fields_index.index_document_with_config(doc_id, doc, &self.config.addok_config);
        filters_index.index_document(doc_id, doc);
        housenumbers_index.index_document(doc);

        // Collect tokens for Pairs and Edge N-Gram indexing
        let mut tokens = Vec::new();
        let norm_name = normalize_address(&doc.name, has_france);
        tokens.extend(tokenize_address(&norm_name, false, has_france));

        if let Some(ref c) = doc.city {
            let norm_c = normalize_address(c, has_france);
            tokens.extend(tokenize_address(&norm_c, false, has_france));
        }
        if let Some(ref pc) = doc.postcode {
            let clean = pc.trim().to_lowercase();
            if !clean.is_empty() {
                tokens.push(clean);
            }
        }

        if self.config.addok_config.has_pairs() {
            pairs_index.index_document_tokens(&tokens);
        }
        if self.config.addok_config.has_autocomplete() {
            for tok in &tokens {
                ngram_index.index_token(tok);
            }
        }

        // Spatial items: index doc centroid when coordinates exist
        if let Some((lat, lon)) = doc.coordinates(None) {
            spatial_items.push(SpatialItem {
                point: [lon, lat],
                id: doc.id.clone(),
                label: doc.label(None),
                ban_type: doc
                    .address_type
                    .clone()
                    .unwrap_or_else(|| "street".to_string()),
                postcode: doc.postcode.clone().unwrap_or_default(),
                city: doc.city.clone().unwrap_or_default(),
                housenumber: None,
                citycode: doc.citycode.clone(),
            });
        }

        // Also index each housenumber
        for (hn, data) in &doc.housenumbers {
            if let (Some(lat), Some(lon)) = (data.lat, data.lon) {
                let item_id = if let Some(serde_json::Value::String(orig_id)) = data.extra.get("id")
                {
                    orig_id.clone()
                } else if !hn.is_empty() {
                    format!("{}_{}", doc.id, hn.trim().replace(' ', "_"))
                } else {
                    doc.id.clone()
                };
                spatial_items.push(SpatialItem {
                    point: [lon, lat],
                    id: item_id,
                    label: doc.label(Some(hn)),
                    ban_type: "housenumber".to_string(),
                    postcode: doc.postcode.clone().unwrap_or_default(),
                    city: doc.city.clone().unwrap_or_default(),
                    housenumber: Some(hn.clone()),
                    citycode: doc.citycode.clone(),
                });
            }
        }

        // Tantivy document
        let tdoc = self
            .schema
            .create_document_from_doc_with_config(doc, &self.config.addok_config);
        writer.add_document(tdoc)?;

        Ok(())
    }
}
