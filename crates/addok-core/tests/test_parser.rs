use addok_core::parser::open_ban_csv;
use flate2::write::GzEncoder;
use flate2::Compression;
use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;

#[test]
fn test_create_and_parse_sample_gz() {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let workspace_root = manifest_dir.parent().unwrap().parent().unwrap();
    let csv_path = workspace_root.join("data/sample_adresses_75.csv");
    let gz_path = workspace_root.join("data/sample_adresses_75.csv.gz");

    // If source csv exists, generate .csv.gz
    if csv_path.exists() {
        let mut uncompressed = Vec::new();
        File::open(&csv_path)
            .unwrap()
            .read_to_end(&mut uncompressed)
            .unwrap();

        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(&uncompressed).unwrap();
        let compressed = encoder.finish().unwrap();

        let mut out_file = File::create(&gz_path).unwrap();
        out_file.write_all(&compressed).unwrap();
    }

    if gz_path.exists() {
        let stream = open_ban_csv(&gz_path).expect("Failed to open sample BAN .csv.gz");
        let records: Vec<_> = stream.collect::<addok_core::Result<Vec<_>>>().unwrap();
        assert!(!records.is_empty());
        assert_eq!(records[0].nom_commune, "Paris");
    }
}
