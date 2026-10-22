use std::io::Write;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};

use addok_core::config::AddokConfig;
use addok_core::document::{open_ndjson_file, open_ndjson_reader};
use addok_core::parser::open_ban_csv;
use addok_index::{AddokIndexer, AddokSearcher, IndexerConfig, SearchParams};
use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use tracing::info;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

use addok_cli::shell;

#[derive(Parser, Debug)]
#[command(
    name = "addok",
    about = "⚡ Fast, lightweight French address geocoder in Rust (Addok rewrite)",
    version
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Index a BAN CSV or CSV.GZ file into a Tantivy + Spatial memory-mapped index
    Index {
        /// Input file path (.csv or .csv.gz)
        #[arg(short, long)]
        input: PathBuf,

        /// Output directory for the Tantivy & Spatial index
        #[arg(short, long)]
        output: PathBuf,

        /// Batch size for multi-threaded processing (default: 10,000)
        #[arg(long, default_value_t = 10_000)]
        batch_size: usize,

        /// Memory budget for indexing in Megabytes (default: 50 MB)
        #[arg(long, default_value_t = 50)]
        memory_budget_mb: usize,
    },

    /// Stream and index documents in NDJSON format (from file or standard input)
    Batch {
        /// Input NDJSON file path (if omitted or '-', reads from standard input)
        filepath: Option<PathBuf>,

        /// Path to the index directory
        #[arg(short, long, default_value = "./index_data")]
        index: PathBuf,

        /// Batch size for periodic indexing (default: 10,000)
        #[arg(long, default_value_t = 10_000)]
        batch_size: usize,

        /// Memory budget for indexing in Megabytes (default: 50 MB)
        #[arg(long, default_value_t = 50)]
        memory_budget_mb: usize,
    },

    /// Reset (wipe) an existing index directory with confirmation prompt
    Reset {
        /// Path to the index directory
        #[arg(short, long, default_value = "./index_data")]
        index: PathBuf,

        /// Automatically confirm index deletion without interactive prompt
        #[arg(short = 'y', long)]
        yes: bool,
    },

    /// Search an address against an existing index
    Search {
        /// Free-form address query (e.g. "8 bd st michel paris")
        query: String,

        /// Path to the index directory
        #[arg(short, long)]
        index: PathBuf,

        /// Maximum number of results (default: 5)
        #[arg(short, long, default_value_t = 5)]
        limit: usize,

        /// Optional center latitude for geographic proximity biasing
        #[arg(long)]
        lat: Option<f64>,

        /// Optional center longitude for geographic proximity biasing
        #[arg(long)]
        lon: Option<f64>,

        /// Optional postcode filter (e.g. "75005")
        #[arg(long)]
        postcode: Option<String>,

        /// Optional type filter (e.g. "housenumber", "street", "municipality")
        #[arg(long = "type")]
        address_type: Option<String>,

        /// Optional citycode / INSEE code filter (e.g. "75105")
        #[arg(long)]
        citycode: Option<String>,

        /// Output results as JSON (GeoJSON FeatureCollection)
        #[arg(long)]
        json: bool,
    },

    /// Reverse geocode coordinates to find nearest addresses
    Reverse {
        /// Latitude in WGS84 (e.g. 48.8512)
        #[arg(long)]
        lat: f64,

        /// Longitude in WGS84 (e.g. 2.3431)
        #[arg(long)]
        lon: f64,

        /// Path to the index directory
        #[arg(short, long)]
        index: PathBuf,

        /// Maximum number of results (default: 5)
        #[arg(short, long, default_value_t = 5)]
        limit: usize,

        /// Optional type filter (e.g. "housenumber", "street", "municipality")
        #[arg(long = "type")]
        address_type: Option<String>,

        /// Optional postcode filter (e.g. "75005")
        #[arg(long)]
        postcode: Option<String>,

        /// Optional citycode / INSEE code filter (e.g. "75105")
        #[arg(long)]
        citycode: Option<String>,

        /// Output results as JSON (GeoJSON FeatureCollection)
        #[arg(long)]
        json: bool,
    },

    /// Start the Addok-compatible REST API HTTP server
    Serve {
        /// Path to the index directory
        #[arg(short, long)]
        index: PathBuf,

        /// HTTP port to listen on (default: 7878)
        #[arg(short, long, default_value_t = 7878)]
        port: u16,

        /// Host address to bind to (default: 127.0.0.1)
        #[arg(long, default_value = "127.0.0.1")]
        host: String,
    },

    /// Launch the interactive Addok REPL shell
    Shell {
        /// Path to the index directory
        #[arg(short, long, default_value = "./index_data")]
        index: PathBuf,
    },

    /// Index or verify edge n-grams for autocomplete (Addok CLI compatibility)
    Ngrams {
        /// Path to the index directory
        #[arg(short, long, default_value = "./index_data")]
        index: PathBuf,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::registry()
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .with(tracing_subscriber::fmt::layer().with_writer(std::io::stderr))
        .init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Index {
            input,
            output,
            batch_size,
            memory_budget_mb,
        } => {
            info!("Starting indexing from {:?}", input);
            info!("Output index directory: {:?}", output);

            let records_stream = open_ban_csv(&input)
                .with_context(|| format!("Failed to open BAN input file: {:?}", input))?;

            let config = IndexerConfig {
                writer_memory_budget: memory_budget_mb * 1024 * 1024,
                batch_size,
                addok_config: AddokConfig::load().unwrap_or_default(),
            };

            let indexer = AddokIndexer::new(config);
            let stats = indexer
                .index_records(records_stream, &output)
                .with_context(|| "Failed during index build")?;

            println!("\n✅ Indexation successful!");
            println!("   Total records read:   {}", stats.total_records);
            println!("   Indexed records:      {}", stats.indexed_records);
            println!("   Spatial records:      {}", stats.spatial_records);
            println!("   Duration:             {:.2}s", stats.duration_secs);
            if stats.duration_secs > 0.0 {
                let rps = stats.indexed_records as f64 / stats.duration_secs;
                println!("   Throughput:           {:.0} records/sec", rps);
            }
        }

        Commands::Batch {
            filepath,
            index,
            batch_size,
            memory_budget_mb,
        } => {
            let config = IndexerConfig {
                writer_memory_budget: memory_budget_mb * 1024 * 1024,
                batch_size,
                addok_config: AddokConfig::load().unwrap_or_default(),
            };
            let indexer = AddokIndexer::new(config);

            let is_stdin = match &filepath {
                None => true,
                Some(p) if p == Path::new("-") => true,
                _ => false,
            };

            let stats = if is_stdin {
                info!("Reading NDJSON documents from standard input...");
                let stdin = std::io::stdin();
                let stream = open_ndjson_reader(stdin.lock());
                indexer
                    .index_documents(stream, &index)
                    .with_context(|| "Failed during batch indexing from stdin")?
            } else {
                let path = filepath.unwrap();
                info!("Reading NDJSON documents from {:?}", path);
                let stream = open_ndjson_file(&path)
                    .with_context(|| format!("Failed to open NDJSON file: {:?}", path))?;
                indexer
                    .index_documents(stream, &index)
                    .with_context(|| format!("Failed during batch indexing from {:?}", path))?
            };

            println!("\n✅ Batch indexation successful!");
            println!("   Total records read:   {}", stats.total_records);
            println!("   Indexed records:      {}", stats.indexed_records);
            println!("   Spatial records:      {}", stats.spatial_records);
            println!("   Duration:             {:.2}s", stats.duration_secs);
            if stats.duration_secs > 0.0 {
                let rps = stats.indexed_records as f64 / stats.duration_secs;
                println!("   Throughput:           {:.0} records/sec", rps);
            }
        }

        Commands::Reset { index, yes } => {
            if !index.exists() {
                println!(
                    "Index directory {:?} does not exist. Nothing to reset.",
                    index
                );
                return Ok(());
            }

            let proceed = if yes {
                true
            } else {
                print!(
                    "⚠️  Are you sure you want to completely reset index at {:?}? [y/N]: ",
                    index
                );
                std::io::stdout().flush()?;

                let mut input = String::new();
                std::io::stdin().read_line(&mut input)?;
                let trimmed = input.trim().to_lowercase();
                trimmed == "y" || trimmed == "yes"
            };

            if proceed {
                std::fs::remove_dir_all(&index)
                    .with_context(|| format!("Failed to remove index directory {:?}", index))?;
                println!("✅ Index directory {:?} has been completely reset.", index);
            } else {
                println!("Reset cancelled.");
            }
        }

        Commands::Search {
            query,
            index,
            limit,
            lat,
            lon,
            postcode,
            address_type,
            citycode,
            json,
        } => {
            let searcher = AddokSearcher::open(&index)
                .with_context(|| format!("Failed to open index at {:?}", index))?;

            let mut params = SearchParams::new(&query).with_limit(limit);
            let center = match (lat, lon) {
                (Some(la), Some(lo)) => {
                    params = params.with_geo_bias(la, lo);
                    Some([lo, la])
                }
                _ => None,
            };
            params.postcode = postcode;
            if let Some(t) = address_type {
                params.filters.insert("type".to_string(), t);
            }
            if let Some(c) = citycode {
                params.filters.insert("citycode".to_string(), c);
            }

            let hits = searcher
                .search(&params)
                .with_context(|| "Search query failed")?;

            if json {
                let collection = addok_server::api::FeatureCollection::from_search_hits_with_center(
                    hits, &query, limit, center,
                );
                println!("{}", serde_json::to_string_pretty(&collection)?);
            } else {
                println!(
                    "\n🔎 Search results for: \"{}\" (limit: {})\n",
                    query, limit
                );
                if hits.is_empty() {
                    println!("   No matching address found.");
                } else {
                    for (i, hit) in hits.iter().enumerate() {
                        let coords = match (hit.record.lon, hit.record.lat) {
                            (Some(lo), Some(la)) => format!("({:.5}, {:.5})", la, lo),
                            _ => "N/A".to_string(),
                        };

                        let dist_str = match hit.distance_meters {
                            Some(d) if d < 1000.0 => format!(" [{:.0}m]", d),
                            Some(d) => format!(" [{:.1}km]", d / 1000.0),
                            None => String::new(),
                        };

                        println!(
                            "  {}. [{:.2}] {} (ID: {}) - coords: {}{}",
                            i + 1,
                            hit.score,
                            hit.record.label(),
                            hit.record.id,
                            coords,
                            dist_str
                        );
                    }
                }
                println!();
            }
        }

        Commands::Reverse {
            lat,
            lon,
            index,
            limit,
            address_type,
            postcode,
            citycode,
            json,
        } => {
            let searcher = AddokSearcher::open(&index)
                .with_context(|| format!("Failed to open index at {:?}", index))?;

            let type_filters: Option<Vec<String>> = address_type.map(|t| {
                t.split(['+', ',', ' '])
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect()
            });
            let postcode_filters: Option<Vec<String>> = postcode.map(|p| {
                p.split(['+', ',', ' '])
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect()
            });
            let citycode_filters: Option<Vec<String>> = citycode.map(|c| {
                c.split(['+', ',', ' '])
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect()
            });

            let results = searcher.reverse_with_filters(
                lat,
                lon,
                limit,
                type_filters.as_deref(),
                postcode_filters.as_deref(),
                citycode_filters.as_deref(),
            );

            if json {
                let collection =
                    addok_server::api::FeatureCollection::from_spatial_results_with_details(
                        results,
                        limit,
                        Some([lon, lat]),
                        Some(&searcher.doc_store),
                    )
                    .with_attribution_and_licence(
                        &searcher.config.attribution,
                        &searcher.config.licence,
                    );
                println!("{}", serde_json::to_string_pretty(&collection)?);
            } else {
                println!(
                    "\n📍 Reverse geocoding for coordinates: ({:.5}, {:.5})\n",
                    lat, lon
                );
                if results.is_empty() {
                    println!("   No addresses found in spatial index.");
                } else {
                    for (i, res) in results.iter().enumerate() {
                        println!(
                            "  {}. {} [{}] (ID: {}) - {:.1}m away",
                            i + 1,
                            res.item.label,
                            res.item.ban_type,
                            res.item.id,
                            res.distance_meters
                        );
                    }
                }
                println!();
            }
        }

        Commands::Serve { index, port, host } => {
            let searcher = AddokSearcher::open(&index)
                .with_context(|| format!("Failed to open index at {:?}", index))?;

            let addr: SocketAddr = format!("{}:{}", host, port)
                .parse()
                .with_context(|| format!("Invalid host/port combination: {}:{}", host, port))?;

            addok_server::server::run_server(searcher, addr).await?;
        }

        Commands::Shell { index } => {
            shell::run_shell(&index)?;
        }

        Commands::Ngrams { index } => {
            let ngram_path = index.join("ngram.bin");
            if ngram_path.exists() {
                info!(
                    "Edge n-grams index at {:?} is up-to-date and ready for autocomplete.",
                    ngram_path
                );
                println!("Edge n-grams index verified at {:?}", ngram_path);
            } else {
                eprintln!(
                    "Notice: Index directory {:?} does not exist or has not been populated yet. Run 'addok batch' or 'addok index' first.",
                    index
                );
            }
        }
    }

    Ok(())
}
