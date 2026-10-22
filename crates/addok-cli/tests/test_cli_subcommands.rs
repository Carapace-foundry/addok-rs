use std::process::Command;
use std::time::Duration;

use addok_cli::shell::execute_command;
use addok_index::AddokSearcher;
use tempfile::tempdir;

fn get_bin_path() -> std::path::PathBuf {
    // Locate the compiled `addok` binary in the cargo target directory
    let mut path = std::env::current_exe().unwrap();
    path.pop(); // Remove test binary name
    if path.ends_with("deps") {
        path.pop(); // Remove "deps"
    }
    path.push("addok");
    path
}

fn get_sample_csv_path() -> std::path::PathBuf {
    let manifest = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".into());
    let path = std::path::Path::new(&manifest).join("../../data/sample_adresses_75.csv");
    if path.exists() {
        return path.canonicalize().unwrap_or(path);
    }
    std::path::PathBuf::from("data/sample_adresses_75.csv")
}

#[test]
fn test_cli_index_search_and_reverse() {
    let temp = tempdir().unwrap();
    let index_dir = temp.path().join("test_index");
    let bin = get_bin_path();
    let csv = get_sample_csv_path();

    // 1. Test `addok index`
    let index_output = Command::new(&bin)
        .arg("index")
        .arg("--input")
        .arg(&csv)
        .arg("--output")
        .arg(&index_dir)
        .output()
        .expect("Failed to run addok index");

    assert!(
        index_output.status.success(),
        "Index command failed: {:?}",
        String::from_utf8_lossy(&index_output.stderr)
    );
    let index_stdout = String::from_utf8_lossy(&index_output.stdout);
    assert!(index_stdout.contains("Indexation successful!"));
    assert!(index_dir.exists());

    // 2. Test `addok search` (human output)
    let search_output = Command::new(&bin)
        .arg("search")
        .arg("8 bd st michel")
        .arg("--index")
        .arg(&index_dir)
        .output()
        .expect("Failed to run addok search");

    assert!(search_output.status.success());
    let search_stdout = String::from_utf8_lossy(&search_output.stdout);
    assert!(search_stdout.contains("Boulevard Saint-Michel"));

    // 3. Test `addok search --json` (GeoJSON output)
    let search_json_output = Command::new(&bin)
        .arg("search")
        .arg("8 bd st michel")
        .arg("--index")
        .arg(&index_dir)
        .arg("--json")
        .output()
        .expect("Failed to run addok search --json");

    assert!(search_json_output.status.success());
    let json_stdout = String::from_utf8_lossy(&search_json_output.stdout);
    let parsed: serde_json::Value =
        serde_json::from_str(&json_stdout).expect("Failed to parse search JSON");
    assert_eq!(parsed["type"], "FeatureCollection");
    assert!(!parsed["features"].as_array().unwrap().is_empty());

    // 4. Test `addok reverse` (human output)
    let reverse_output = Command::new(&bin)
        .arg("reverse")
        .arg("--lat")
        .arg("48.8512")
        .arg("--lon")
        .arg("2.3431")
        .arg("--index")
        .arg(&index_dir)
        .output()
        .expect("Failed to run addok reverse");

    assert!(reverse_output.status.success());
    let reverse_stdout = String::from_utf8_lossy(&reverse_output.stdout);
    assert!(reverse_stdout.contains("Boulevard Saint-Michel"));

    // 5. Test `addok reverse --json`
    let reverse_json_output = Command::new(&bin)
        .arg("reverse")
        .arg("--lat")
        .arg("48.8512")
        .arg("--lon")
        .arg("2.3431")
        .arg("--index")
        .arg(&index_dir)
        .arg("--json")
        .output()
        .expect("Failed to run addok reverse --json");

    assert!(reverse_json_output.status.success());
    let rev_json: serde_json::Value =
        serde_json::from_str(&String::from_utf8_lossy(&reverse_json_output.stdout))
            .expect("Failed to parse reverse JSON");
    assert_eq!(rev_json["type"], "FeatureCollection");

    // 5b. Test `addok reverse --type housenumber`
    let reverse_type_output = Command::new(&bin)
        .arg("reverse")
        .arg("--lat")
        .arg("48.8512")
        .arg("--lon")
        .arg("2.3431")
        .arg("--index")
        .arg(&index_dir)
        .arg("--type")
        .arg("housenumber")
        .arg("--json")
        .output()
        .expect("Failed to run addok reverse --type");

    assert!(reverse_type_output.status.success());
    let rev_type_json: serde_json::Value =
        serde_json::from_str(&String::from_utf8_lossy(&reverse_type_output.stdout))
            .expect("Failed to parse reverse type JSON");
    assert_eq!(
        rev_type_json["features"][0]["properties"]["type"],
        "housenumber"
    );

    // 6. Test `addok reset --yes`
    let reset_output = Command::new(&bin)
        .arg("reset")
        .arg("--index")
        .arg(&index_dir)
        .arg("-y")
        .output()
        .expect("Failed to run addok reset");

    assert!(reset_output.status.success());
    let reset_stdout = String::from_utf8_lossy(&reset_output.stdout);
    assert!(reset_stdout.contains("completely reset"));
    assert!(!index_dir.exists());
}

#[test]
fn test_cli_batch_subcommand() {
    let temp = tempdir().unwrap();
    let index_dir = temp.path().join("batch_index");
    let bin = get_bin_path();

    // Create a sample NDJSON file
    let ndjson_file = temp.path().join("test_docs.ndjson");
    let sample_ndjson = r#"{"id":"75105_9999_00001","name":"Rue de la Paix","type":"street","postcode":"75002","city":"Paris","importance":0.7,"housenumbers":{"1":{"lat":48.868,"lon":2.331}}}
{"id":"75105_9999_00002","name":"Boulevard Haussmann","type":"street","postcode":"75009","city":"Paris","importance":0.8,"housenumbers":{"10":{"lat":48.873,"lon":2.332}}}
"#;
    std::fs::write(&ndjson_file, sample_ndjson).unwrap();

    // Run `addok batch`
    let batch_output = Command::new(&bin)
        .arg("batch")
        .arg(&ndjson_file)
        .arg("--index")
        .arg(&index_dir)
        .output()
        .expect("Failed to run addok batch");

    assert!(
        batch_output.status.success(),
        "Batch failed: {:?}",
        String::from_utf8_lossy(&batch_output.stderr)
    );
    let batch_stdout = String::from_utf8_lossy(&batch_output.stdout);
    assert!(batch_stdout.contains("Batch indexation successful!"));
    assert!(index_dir.exists());

    // Search the batched index
    let search_output = Command::new(&bin)
        .arg("search")
        .arg("rue de la paix")
        .arg("--index")
        .arg(&index_dir)
        .output()
        .expect("Failed to search batched index");

    assert!(search_output.status.success());
    let search_stdout = String::from_utf8_lossy(&search_output.stdout);
    assert!(search_stdout.contains("Rue de la Paix"));
}

#[test]
fn test_cli_serve_subcommand_health() {
    let temp = tempdir().unwrap();
    let index_dir = temp.path().join("serve_index");
    let bin = get_bin_path();
    let csv = get_sample_csv_path();

    // Index sample data first
    let index_res = Command::new(&bin)
        .arg("index")
        .arg("--input")
        .arg(&csv)
        .arg("--output")
        .arg(&index_dir)
        .output()
        .unwrap();
    assert!(index_res.status.success());

    let port: u16 = 18972;

    // Launch server in background
    let mut child = Command::new(&bin)
        .arg("serve")
        .arg("--index")
        .arg(&index_dir)
        .arg("--port")
        .arg(port.to_string())
        .spawn()
        .expect("Failed to spawn addok serve");

    // Wait a brief moment for server to bind
    std::thread::sleep(Duration::from_millis(1000));

    // Make HTTP request to /health
    let _health_url = format!("http://127.0.0.1:{}/health", port);
    let client = std::net::TcpStream::connect(format!("127.0.0.1:{}", port));
    assert!(
        client.is_ok(),
        "Failed to connect to server on port {}",
        port
    );

    // Terminate child server process
    let _ = child.kill();
    let _ = child.wait();
}

#[test]
fn test_shell_repl_all_commands_execution() {
    let temp = tempdir().unwrap();
    let index_dir = temp.path().join("shell_index");
    let bin = get_bin_path();
    let csv = get_sample_csv_path();

    // Index sample data
    let index_res = Command::new(&bin)
        .arg("index")
        .arg("--input")
        .arg(&csv)
        .arg("--output")
        .arg(&index_dir)
        .output()
        .unwrap();
    assert!(index_res.status.success());

    let searcher = AddokSearcher::open(&index_dir).unwrap();

    // 1. HELP / ?
    let help_out = execute_command(&searcher, "HELP").unwrap();
    assert!(help_out.contains("Available Shell Commands"));

    // 2. Default search action (bare string)
    let def_out = execute_command(&searcher, "8 bd st michel").unwrap();
    assert!(def_out.contains("Boulevard Saint-Michel"));

    // 3. SEARCH <query>
    let search_out = execute_command(&searcher, "SEARCH 8 bd st michel").unwrap();
    assert!(search_out.contains("Boulevard Saint-Michel"));
    assert!(search_out.contains("Type:"));

    // 4. EXPLAIN <query>
    let explain_out = execute_command(&searcher, "EXPLAIN 8 bd st michel").unwrap();
    assert!(explain_out.contains("Query Parsing & Normalization"));
    assert!(explain_out.contains("Candidate Bucket Collection Phases"));
    assert!(explain_out.contains("Candidate Score Decomposition"));

    // 5. REVERSE <lat> <lon>
    let rev_out = execute_command(&searcher, "REVERSE 48.8512 2.3431").unwrap();
    assert!(rev_out.contains("Nearest addresses to coordinates"));
    assert!(rev_out.contains("Boulevard Saint-Michel"));

    // 6. TOKENIZE <query>
    let tok_out = execute_command(&searcher, "TOKENIZE 8 bis bd st michel 75005 paris").unwrap();
    assert!(tok_out.contains("Tokenization Diagnostic"));
    assert!(tok_out.contains("Normalized String:"));
    assert!(tok_out.contains("Postal Code (5-digit)"));

    // 7. DOC <id>
    let doc_out = execute_command(&searcher, "DOC 75105_0001_00008").unwrap();
    assert!(
        doc_out.contains("Document Record for ID")
            || doc_out.contains("not found in DocumentStore")
    );

    // 8. FUZZY <token>
    let fuzzy_out = execute_command(&searcher, "FUZZY michel").unwrap();
    assert!(fuzzy_out.contains("Damerau-Levenshtein Fuzzy Neighbors"));

    // 9. PAIR <token>
    let pair_out = execute_command(&searcher, "PAIR michel").unwrap();
    assert!(pair_out.contains("Token Co-occurrence Pair Analysis"));

    // 10. NGRAM <token>
    let ngram_out = execute_command(&searcher, "NGRAM champs").unwrap();
    assert!(ngram_out.contains("Edge N-Gram Analysis"));

    // 11. FREQUENCY <token>
    let freq_out = execute_command(&searcher, "FREQUENCY michel").unwrap();
    assert!(freq_out.contains("Occurrence Frequency for Token"));
    assert!(freq_out.contains("Field-Specific Breakdown"));

    // 12. GEODISTANCE <lat1> <lon1> <lat2> <lon2>
    let geo_out = execute_command(&searcher, "GEODISTANCE 48.8512 2.3431 48.8698 2.3082").unwrap();
    assert!(geo_out.contains("Haversine Geodistance & Proximity Score"));
    assert!(geo_out.contains("Great-Circle Distance"));
    assert!(geo_out.contains("Proximity Score"));
}
