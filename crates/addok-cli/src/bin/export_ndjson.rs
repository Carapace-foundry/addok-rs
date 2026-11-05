#![deny(unsafe_code)]

use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;

use addok_core::aggregation::StreamingBanAggregator;
use addok_core::parser::open_ban_csv;

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let input_path = args
        .get(1)
        .map(String::as_str)
        .unwrap_or("data/adresses-75.csv.gz");
    let output_path = args
        .get(2)
        .map(String::as_str)
        .unwrap_or("/tmp/paris.ndjson");
    let input = Path::new(input_path);
    let output = Path::new(output_path);
    println!("Streaming BAN records from {}...", input.display());
    let records = open_ban_csv(input)?;
    let aggregator = StreamingBanAggregator::new(records, 10_000);
    let mut writer = BufWriter::new(File::create(output)?);
    let mut count = 0;
    for doc_res in aggregator {
        let mut doc = doc_res?;
        if let Some((lat, lon)) = doc.coordinates(None) {
            doc.extra.insert("lat".to_string(), serde_json::json!(lat));
            doc.extra.insert("lon".to_string(), serde_json::json!(lon));
        }
        serde_json::to_writer(&mut writer, &doc)?;
        writer.write_all(b"\n")?;
        count += 1;
    }
    writer.flush()?;
    println!(
        "✅ Successfully exported {} street documents to {}",
        count,
        output.display()
    );
    Ok(())
}
