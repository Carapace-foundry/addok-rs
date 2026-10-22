use std::path::{Path, PathBuf};

use addok_core::normalizer::{is_stop_word, normalize_french_address, tokenize_french_address};
use addok_index::query::{is_french_postcode, is_repetition_token};
use addok_index::{
    generate_fuzzy_neighbors, haversine_distance_meters, km_to_score, AddokSearcher,
    EdgeNgramIndex, SearchParams,
};
use anyhow::{Context, Result};
use rustyline::error::ReadlineError;
use rustyline::DefaultEditor;

const BOLD: &str = "\x1b[1m";
const DIM: &str = "\x1b[2m";
const RESET: &str = "\x1b[0m";
const CYAN: &str = "\x1b[1;36m";
const GREEN: &str = "\x1b[1;32m";
const YELLOW: &str = "\x1b[1;33m";
const MAGENTA: &str = "\x1b[1;35m";
const RED: &str = "\x1b[1;31m";

/// Run the interactive Addok REPL shell
pub fn run_shell(index_path: &Path) -> Result<()> {
    println!(
        "\n{CYAN}⚡ Addok-rs Interactive Geocoding Shell{RESET}\n\
         {DIM}Loaded index:{RESET} {BOLD}{:?}{RESET}\n\
         {DIM}Type any query to search, {BOLD}HELP{RESET}{DIM} for command list, {BOLD}EXIT{RESET}{DIM} to quit.{RESET}\n",
        index_path
    );

    let searcher = AddokSearcher::open(index_path)
        .with_context(|| format!("Failed to open index at {:?}", index_path))?;

    let history_file = get_history_file();

    let mut rl = DefaultEditor::new().with_context(|| "Failed to initialize rustyline editor")?;
    if let Some(ref path) = history_file {
        if path.exists() {
            let _ = rl.load_history(path);
        }
    }

    loop {
        let prompt = format!("{CYAN}addok>{RESET} ");
        match rl.readline(&prompt) {
            Ok(line) => {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }

                let _ = rl.add_history_entry(trimmed);

                if is_exit_command(trimmed) {
                    println!("{DIM}Goodbye!{RESET}");
                    break;
                }

                if let Err(e) = execute_command_interactive(&searcher, trimmed) {
                    eprintln!("{RED}Error:{RESET} {}", e);
                }
            }
            Err(ReadlineError::Interrupted) => {
                println!("\n{DIM}(Interrupted - type EXIT to quit){RESET}");
            }
            Err(ReadlineError::Eof) => {
                println!("\n{DIM}Goodbye!{RESET}");
                break;
            }
            Err(err) => {
                eprintln!("{RED}Readline error:{RESET} {:?}", err);
                break;
            }
        }
    }

    if let Some(ref path) = history_file {
        let _ = rl.save_history(path);
    }

    Ok(())
}

fn get_history_file() -> Option<PathBuf> {
    if let Ok(home) = std::env::var("HOME") {
        Some(PathBuf::from(home).join(".addok_shell_history"))
    } else {
        Some(PathBuf::from(".addok_shell_history"))
    }
}

fn is_exit_command(cmd: &str) -> bool {
    let lower = cmd.trim().to_lowercase();
    matches!(lower.as_str(), "exit" | "quit" | ":q" | "q")
}

/// Execute a shell command and print the result interactively
pub fn execute_command_interactive(searcher: &AddokSearcher, line: &str) -> Result<()> {
    let output = handle_command(searcher, line, true)?;
    print!("{}", output);
    Ok(())
}

/// Execute a shell command and return the formatted output (useful for tests and non-interactive calls)
pub fn execute_command(searcher: &AddokSearcher, line: &str) -> Result<String> {
    handle_command(searcher, line, false)
}

fn handle_command(searcher: &AddokSearcher, line: &str, colored: bool) -> Result<String> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return Ok(String::new());
    }

    let (cmd, args) = match trimmed.split_once(char::is_whitespace) {
        Some((c, a)) => (c.to_uppercase(), a.trim()),
        None => (trimmed.to_uppercase(), ""),
    };

    match cmd.as_str() {
        "HELP" | "?" => Ok(format_help(colored)),
        "SEARCH" => cmd_search(searcher, args, true, colored),
        "EXPLAIN" => cmd_explain(searcher, args, colored),
        "REVERSE" => cmd_reverse(searcher, args, colored),
        "TOKENIZE" => cmd_tokenize(args, colored),
        "DOC" | "GET" => cmd_doc(searcher, args, colored),
        "FUZZY" => cmd_fuzzy(searcher, args, colored),
        "PAIR" => cmd_pair(searcher, args, colored),
        "NGRAM" | "AUTOCOMPLETE" => cmd_ngram(searcher, args, colored),
        "FREQUENCY" => cmd_frequency(searcher, args, colored),
        "GEODISTANCE" => cmd_geodistance(args, colored),
        _ => {
            // Default action: typing any string runs a search with colored formatting
            cmd_search(searcher, trimmed, false, colored)
        }
    }
}

fn format_help(c: bool) -> String {
    let (cyan, bold, dim, reset) = if c {
        (CYAN, BOLD, DIM, RESET)
    } else {
        ("", "", "", "")
    };

    format!(
        "\n{cyan}Available Shell Commands:{reset}\n\n\
         {bold}<query>{reset}                   Default action: Run address search with ranking\n\
         {bold}SEARCH <query>{reset}            Full search with detailed score breakdown\n\
         {bold}EXPLAIN <query>{reset}           Step-by-step diagnostic showing tokenization, frequencies, bucket collection, and scoring\n\
         {bold}REVERSE <lat> <lon>{reset}       Reverse geocoding lookup for nearest addresses\n\
         {bold}TOKENIZE <query>{reset}          Display normalization, tokens, stop words, and housenumbers\n\
         {bold}DOC <id>{reset}, {bold}GET <id>{reset}          Fetch raw document JSON by ID from the document store\n\
         {bold}FUZZY <token>{reset}             Generate Damerau-Levenshtein typo variants & match in index\n\
         {bold}PAIR <token>{reset}              Show associated co-occurring tokens and document frequency\n\
         {bold}NGRAM <token>{reset}, {bold}AUTOCOMPLETE{reset} Show edge n-grams and index prefix matches\n\
         {bold}FREQUENCY <token>{reset}         Display occurrence count of token across all fields\n\
         {bold}GEODISTANCE <la1> <lo1> <la2> <lo2>{reset}  Compute Haversine distance and exponential decay score\n\
         {bold}HELP, ?{reset}                   Display this help message\n\
         {bold}EXIT, QUIT{reset}                Exit the shell\n\n\
         {dim}Examples:\n\
         • 8 bd st michel\n\
         • SEARCH 8 bd st michel\n\
         • EXPLAIN 8 bd st michel\n\
         • REVERSE 48.8512 2.3431\n\
         • TOKENIZE 8 bis bd saint-michel 75005 paris\n\
         • DOC 75105_0001_00008\n\
         • FUZZY michel\n\
         • PAIR michel\n\
         • NGRAM champs\n\
         • FREQUENCY michel\n\
         • GEODISTANCE 48.8512 2.3431 48.8698 2.3082{reset}\n\n"
    )
}

fn cmd_search(
    searcher: &AddokSearcher,
    query: &str,
    detailed_breakdown: bool,
    c: bool,
) -> Result<String> {
    if query.trim().is_empty() {
        return Ok("Usage: SEARCH <query>\n".to_string());
    }

    let (green, yellow, magenta, dim, bold, reset) = if c {
        (GREEN, YELLOW, MAGENTA, DIM, BOLD, RESET)
    } else {
        ("", "", "", "", "", "")
    };

    let params = SearchParams::new(query).with_limit(5);
    let hits = searcher.search(&params)?;

    let mut out = String::new();
    out.push_str(&format!(
        "\n🔎 {bold}Search results for:{reset} \"{}\" (limit: 5)\n\n",
        query
    ));

    if hits.is_empty() {
        out.push_str(&format!("   {dim}No matching address found.{reset}\n\n"));
        return Ok(out);
    }

    for (i, hit) in hits.iter().enumerate() {
        let score_color = if hit.score >= 0.8 {
            green
        } else if hit.score >= 0.5 {
            yellow
        } else {
            dim
        };

        let coords_str = match (hit.record.lon, hit.record.lat) {
            (Some(lo), Some(la)) => format!("({:.5}, {:.5})", la, lo),
            _ => "N/A".to_string(),
        };

        let dist_str = match hit.distance_meters {
            Some(d) if d < 1000.0 => format!(" [{:.0}m]", d),
            Some(d) => format!(" [{:.1}km]", d / 1000.0),
            None => String::new(),
        };

        out.push_str(&format!(
            "  {}. [{}{:.2}{}] {}{}{} {}(ID: {}{}{}) - coords: {}{}\n",
            i + 1,
            score_color,
            hit.score,
            reset,
            bold,
            hit.record.label(),
            reset,
            dim,
            magenta,
            hit.record.id,
            reset,
            coords_str,
            dist_str
        ));

        if detailed_breakdown {
            let hn_info = match &hit.matched_housenumber {
                Some(hn) => format!("Matched HN: \"{}\" (+15% boost)", hn),
                None => "HN: Street level".to_string(),
            };
            out.push_str(&format!(
                "     {dim}↳ Type: {} | City: {} ({}) | {}{reset}\n",
                hit.record.ban_type(),
                hit.record.nom_commune,
                hit.record.code_postal,
                hn_info
            ));
        }
    }
    out.push('\n');
    Ok(out)
}

fn cmd_explain(searcher: &AddokSearcher, query: &str, c: bool) -> Result<String> {
    if query.trim().is_empty() {
        return Ok("Usage: EXPLAIN <query>\n".to_string());
    }

    let (cyan, green, yellow, magenta, dim, bold, reset) = if c {
        (CYAN, GREEN, YELLOW, MAGENTA, DIM, BOLD, RESET)
    } else {
        ("", "", "", "", "", "", "")
    };

    let params = SearchParams::new(query).with_limit(5);
    let diag = searcher.explain(&params)?;

    let mut out = String::new();
    out.push_str(&format!(
        "\n{cyan}═══════════════════════════════════════════════════════════════{reset}\n\
         {cyan}🔬 ADDOK QUERY EXECUTION DIAGNOSTIC & EXPLAIN PLAN{reset}\n\
         {cyan}═══════════════════════════════════════════════════════════════{reset}\n\n"
    ));

    // 1. Query Parsing & Tokenization
    out.push_str(&format!("{bold}1. Query Parsing & Normalization{reset}\n"));
    out.push_str(&format!(
        "   • Raw Query:             \"{}\"\n",
        diag.raw_query
    ));
    out.push_str(&format!(
        "   • Normalized Query:      \"{}\"\n",
        diag.normalized_query
    ));
    out.push_str(&format!(
        "   • Clean Street Query:    \"{}\"\n",
        diag.clean_query
    ));
    out.push_str(&format!(
        "   • Extracted Housenumber: {}\n",
        diag.extracted_housenumber.as_deref().unwrap_or("(none)")
    ));
    out.push_str(&format!(
        "   • Extracted Postcode:    {}\n",
        diag.extracted_postcode.as_deref().unwrap_or("(none)")
    ));
    out.push_str(&format!(
        "   • Autocomplete Prefix:   {}\n",
        diag.autocomplete_prefix.as_deref().unwrap_or("(none)")
    ));
    out.push_str(&format!(
        "   • Street Tokens:         {:?}\n",
        diag.street_tokens
    ));
    out.push_str(&format!(
        "   • Meaningful Tokens:     {:?}\n",
        diag.meaningful_tokens
    ));
    out.push_str(&format!(
        "   • Sorted (Rarest First): {:?}\n\n",
        diag.sorted_tokens
    ));

    // 2. Token Frequencies
    out.push_str(&format!(
        "{bold}2. Inverted Index Token Frequencies{reset}\n"
    ));
    if diag.token_frequencies.is_empty() {
        out.push_str(&format!("   {dim}(no tokens found){reset}\n"));
    } else {
        for tf in &diag.token_frequencies {
            let status = if tf.is_common {
                format!("{yellow}[COMMON, > threshold]{reset}")
            } else {
                format!("{green}[SELECTIVE]{reset}")
            };
            out.push_str(&format!(
                "   • \"{}\": {} document(s) {}\n",
                tf.token, tf.frequency, status
            ));
        }
    }
    out.push('\n');

    // 3. Bucket Collection Phases
    out.push_str(&format!(
        "{bold}3. Candidate Bucket Collection Phases{reset}\n"
    ));
    out.push_str(&format!(
        "   {bold}Phase 1 (Non-Common Token Intersection):{reset}\n\
              - Active tokens: {:?}\n",
        diag.phase1_active_tokens
    ));
    for (tok, cnt) in &diag.phase1_intersections {
        out.push_str(&format!(
            "     - Intersected \"{}\" ➔ {} candidate(s)\n",
            tok, cnt
        ));
    }
    out.push_str(&format!(
        "     ➔ Phase 1 collected: {} document(s)\n\n",
        diag.phase1_count
    ));

    out.push_str(&format!(
        "   {bold}Phase 2 (Edge N-Gram Autocomplete Expansion):{reset}\n\
              - Prefix tested: {}\n\
              - N-Gram Candidates: {:?}\n\
              - Pair-Validated Candidates: {:?}\n\
              ➔ Phase 2 added: {} document(s)\n\n",
        diag.phase2_prefix.as_deref().unwrap_or("(none)"),
        diag.phase2_ngram_candidates,
        diag.phase2_pair_filtered_candidates,
        diag.phase2_added_count
    ));

    out.push_str(&format!(
        "   {bold}Phase 3 (Fuzzy Typo Expansion):{reset}\n\
              - Triggered: {}\n",
        if diag.phase3_triggered {
            format!("{yellow}Yes (bucket < min_bucket){reset}")
        } else {
            format!("{green}No (sufficient exact matches){reset}")
        }
    ));
    if !diag.phase3_fuzzy_variants.is_empty() {
        for fv in &diag.phase3_fuzzy_variants {
            out.push_str(&format!(
                "     - Found variant \"{}\" for \"{}\" ({} docs)\n",
                fv.matched_variant, fv.original_token, fv.variant_frequency
            ));
        }
    }
    out.push_str(&format!(
        "     ➔ Phase 3 added: {} document(s)\n\n",
        diag.phase3_added_count
    ));

    out.push_str(&format!(
        "   {bold}Phase 4 (Filters):{reset}\n\
              - Filters applied: {:?}\n\
              ➔ Total candidate documents ready for scoring: {}\n\n",
        diag.phase4_filters_applied, diag.total_candidates
    ));

    // 4. Candidate Score Decomposition
    out.push_str(&format!(
        "{bold}4. Candidate Score Decomposition (Top Candidates){reset}\n"
    ));
    if diag.candidates.is_empty() {
        out.push_str(&format!(
            "   {dim}No candidate documents met the minimum score threshold.{reset}\n"
        ));
    } else {
        for (i, cand) in diag.candidates.iter().enumerate() {
            let score_color = if cand.final_score >= 0.8 {
                green
            } else if cand.final_score >= 0.5 {
                yellow
            } else {
                dim
            };

            let hn_desc = match (&cand.matched_housenumber, cand.housenumber_factor) {
                (Some(hn), f) if f > 1.0 => {
                    format!("Exact match \"{}\" (+15% boost, factor: {:.2})", hn, f)
                }
                (Some(hn), f) => format!("Exact match \"{}\" (factor: {:.2})", hn, f),
                (None, f) if f < 1.0 => format!("Housenumber mismatch penalty (factor: {:.2})", f),
                _ => "Street level (factor: 1.00)".to_string(),
            };

            let geo_desc = match (cand.distance_meters, cand.geo_score) {
                (Some(d), Some(gs)) => format!("{:.1}m (geo score: {:.3})", d, gs),
                _ => "None (text-only search)".to_string(),
            };

            out.push_str(&format!(
                "  {}. [{}{:.2}{}] {}{}{} (ID: {}{}{})\n\
                     - String Distance Score:  {:.3} (N-gram Sorensen-Dice / Levenshtein)\n\
                     - Token Coverage Ratio:   {:.3}\n\
                     - Base Text Score:        {:.3} (0.7 * string_dist + 0.3 * coverage)\n\
                     - Document Importance:    {:.2} ➔ Factor: {:.3} (1.0 + imp * 0.1)\n\
                     - Housenumber Matching:   {}\n\
                     - Geo-Distance Proximity: {}\n\
                     - Composite Score:        {}{:.2}{}\n\n",
                i + 1,
                score_color,
                cand.final_score,
                reset,
                bold,
                cand.label,
                reset,
                magenta,
                cand.id,
                reset,
                cand.string_distance,
                cand.token_coverage,
                cand.text_score,
                cand.importance,
                cand.importance_factor,
                hn_desc,
                geo_desc,
                score_color,
                cand.final_score,
                reset
            ));
        }
    }

    Ok(out)
}

fn cmd_reverse(searcher: &AddokSearcher, args: &str, c: bool) -> Result<String> {
    let parts: Vec<&str> = args.split_whitespace().collect();
    if parts.len() < 2 {
        return Ok(
            "Usage: REVERSE <lat> <lon> [limit]\nExample: REVERSE 48.8512 2.3431\n".to_string(),
        );
    }

    let lat: f64 = parts[0]
        .parse()
        .with_context(|| format!("Invalid latitude: {}", parts[0]))?;
    let lon: f64 = parts[1]
        .parse()
        .with_context(|| format!("Invalid longitude: {}", parts[1]))?;
    let limit: usize = parts.get(2).and_then(|s| s.parse().ok()).unwrap_or(5);

    let (green, magenta, dim, bold, reset) = if c {
        (GREEN, MAGENTA, DIM, BOLD, RESET)
    } else {
        ("", "", "", "", "")
    };

    let results = searcher.reverse(lat, lon, limit);

    let mut out = String::new();
    out.push_str(&format!(
        "\n📍 {bold}Nearest addresses to coordinates:{reset} ({:.5}, {:.5})\n\n",
        lat, lon
    ));

    if results.is_empty() {
        out.push_str(&format!(
            "   {dim}No addresses found in spatial index.{reset}\n\n"
        ));
    } else {
        for (i, res) in results.iter().enumerate() {
            out.push_str(&format!(
                "  {}. {}{}{} (ID: {}{}{}) - coords: ({:.5}, {:.5}) ➔ {}{:.1}m away{}\n",
                i + 1,
                bold,
                res.item.label,
                reset,
                magenta,
                res.item.id,
                reset,
                res.item.point[1],
                res.item.point[0],
                green,
                res.distance_meters,
                reset
            ));
        }
        out.push('\n');
    }

    Ok(out)
}

fn cmd_tokenize(query: &str, c: bool) -> Result<String> {
    if query.trim().is_empty() {
        return Ok(
            "Usage: TOKENIZE <query>\nExample: TOKENIZE 8 bis bd st michel 75005 paris\n"
                .to_string(),
        );
    }

    let (cyan, bold, dim, reset) = if c {
        (CYAN, BOLD, DIM, RESET)
    } else {
        ("", "", "", "")
    };

    let normalized = normalize_french_address(query);
    let tokens_all = tokenize_french_address(&normalized, false);
    let tokens_meaningful = tokenize_french_address(&normalized, true);

    let mut out = String::new();
    out.push_str(&format!(
        "\n🔤 {cyan}Tokenization Diagnostic:{reset}\n\n\
         • Raw Query:             \"{}\"\n\
         • Normalized String:     \"{}\"\n\
         • All Tokens:            {:?}\n\
         • Meaningful Tokens:     {:?} (stop words filtered)\n\n",
        query, normalized, tokens_all, tokens_meaningful
    ));

    out.push_str(&format!("{bold}Token Classifications:{reset}\n"));
    for tok in &tokens_all {
        let mut tags = Vec::new();
        if is_stop_word(tok) {
            tags.push("Stop Word");
        }
        if is_french_postcode(tok) {
            tags.push("Postal Code (5-digit)");
        }
        if is_repetition_token(tok) {
            tags.push("Housenumber Modifier (bis/ter)");
        }
        if tok.chars().all(|ch| ch.is_ascii_digit()) {
            tags.push("Number");
        }
        if tags.is_empty() {
            tags.push("Word / Street Name Token");
        }

        out.push_str(&format!("  • \"{}\" ➔ {}\n", tok, tags.join(", ")));
    }
    out.push_str(&format!("\n{dim}Note: Diacritics removed, abbreviations expanded, apostrophes normalized.{reset}\n\n"));

    Ok(out)
}

fn cmd_doc(searcher: &AddokSearcher, id: &str, c: bool) -> Result<String> {
    let clean_id = id.trim();
    if clean_id.is_empty() {
        return Ok("Usage: DOC <id>\nExample: DOC 75105_0001_00008\n".to_string());
    }

    let (magenta, red, reset) = if c {
        (MAGENTA, RED, RESET)
    } else {
        ("", "", "")
    };

    if let Some(doc) = searcher.doc_store.get_by_id(clean_id) {
        let json_str = serde_json::to_string_pretty(doc)?;
        Ok(format!(
            "\n📄 {magenta}Document Record for ID:{reset} \"{}\"\n{}\n\n",
            clean_id, json_str
        ))
    } else {
        Ok(format!(
            "\n{red}❌ Document ID not found in DocumentStore:{reset} \"{}\"\n\n",
            clean_id
        ))
    }
}

fn cmd_fuzzy(searcher: &AddokSearcher, token: &str, c: bool) -> Result<String> {
    let clean_token = token.trim().to_lowercase();
    if clean_token.is_empty() {
        return Ok("Usage: FUZZY <token>\nExample: FUZZY michel\n".to_string());
    }

    let (green, yellow, dim, bold, reset) = if c {
        (GREEN, YELLOW, DIM, BOLD, RESET)
    } else {
        ("", "", "", "", "")
    };

    let variants = generate_fuzzy_neighbors(&clean_token);

    let mut indexed_variants = Vec::new();
    for v in &variants {
        if let Some(bm) = searcher.fields_index.get_postings(v) {
            indexed_variants.push((v.clone(), bm.len() as usize));
        }
    }

    let mut out = String::new();
    out.push_str(&format!(
        "\n🔀 {bold}Damerau-Levenshtein Fuzzy Neighbors for:{reset} \"{}\"\n\
         {dim}Generated {} single-edit variants (deletions, transpositions, substitutions, insertions).{reset}\n\n",
        clean_token,
        variants.len()
    ));

    if indexed_variants.is_empty() {
        out.push_str(&format!(
            "   {yellow}None of the generated typo variants currently exist in the index.{reset}\n\n"
        ));
    } else {
        out.push_str(&format!(
            "   {green}Matching variants present in index ({}):{reset}\n",
            indexed_variants.len()
        ));
        for (v, count) in &indexed_variants {
            out.push_str(&format!("   • \"{}\" ➔ {} document(s)\n", v, count));
        }
        out.push('\n');
    }

    // Show a sample of generated variants
    let sample: Vec<&String> = variants.iter().take(12).collect();
    out.push_str(&format!(
        "{dim}Sample generated typo neighbors: {:?}{reset}\n\n",
        sample
    ));

    Ok(out)
}

fn cmd_pair(searcher: &AddokSearcher, token: &str, c: bool) -> Result<String> {
    let clean_token = token.trim().to_lowercase();
    if clean_token.is_empty() {
        return Ok("Usage: PAIR <token>\nExample: PAIR michel\n".to_string());
    }

    let (green, magenta, dim, bold, reset) = if c {
        (GREEN, MAGENTA, DIM, BOLD, RESET)
    } else {
        ("", "", "", "", "")
    };

    let doc_freq = searcher.pairs_index.token_doc_frequency(&clean_token);
    let associated = searcher.pairs_index.get_associated_tokens(&clean_token);

    let mut out = String::new();
    out.push_str(&format!(
        "\n🔗 {bold}Token Co-occurrence Pair Analysis for:{reset} \"{}\"\n\
         • Document Frequency: {}{}{} documents\n",
        clean_token, green, doc_freq, reset
    ));

    match associated {
        Some(set) if !set.is_empty() => {
            let mut sorted: Vec<&String> = set.iter().collect();
            sorted.sort();
            out.push_str(&format!(
                "• Associated Tokens ({}): {}{:?}{}\n\n",
                set.len(),
                magenta,
                sorted,
                reset
            ));
        }
        _ => {
            out.push_str(&format!(
                "• Associated Tokens: {dim}None (no co-occurring tokens recorded){reset}\n\n"
            ));
        }
    }

    Ok(out)
}

fn cmd_ngram(searcher: &AddokSearcher, token: &str, c: bool) -> Result<String> {
    let clean_token = token.trim().to_lowercase();
    if clean_token.is_empty() {
        return Ok("Usage: NGRAM <token>\nExample: NGRAM michel\n".to_string());
    }

    let (cyan, green, dim, bold, reset) = if c {
        (CYAN, GREEN, DIM, BOLD, RESET)
    } else {
        ("", "", "", "", "")
    };

    let ngrams = EdgeNgramIndex::generate_edge_ngrams(&clean_token, 2, 15);
    let candidates = searcher.ngram_index.get_candidates(&clean_token);

    let mut out = String::new();
    out.push_str(&format!(
        "\n📐 {bold}Edge N-Gram Analysis for:{reset} \"{}\"\n\
         • Generated Edge N-Grams (min: 2, max: 15): {}{:?}{}\n",
        clean_token, cyan, ngrams, reset
    ));

    if candidates.is_empty() {
        out.push_str(&format!(
            "• Index Prefix Matches: {dim}None (no vocabulary terms match prefix \"{}\"){reset}\n\n",
            clean_token
        ));
    } else {
        out.push_str(&format!(
            "• Index Prefix Matches ({}): {}{:?}{}\n\n",
            candidates.len(),
            green,
            candidates,
            reset
        ));
    }

    Ok(out)
}

fn cmd_frequency(searcher: &AddokSearcher, token: &str, c: bool) -> Result<String> {
    let clean_token = token.trim().to_lowercase();
    if clean_token.is_empty() {
        return Ok("Usage: FREQUENCY <token>\nExample: FREQUENCY michel\n".to_string());
    }

    let (green, bold, reset) = if c {
        (GREEN, BOLD, RESET)
    } else {
        ("", "", "")
    };

    let postings_count = searcher
        .fields_index
        .get_postings(&clean_token)
        .map_or(0, |bm| bm.len() as usize);
    let pairs_doc_freq = searcher.pairs_index.token_doc_frequency(&clean_token);

    let mut out = String::new();
    out.push_str(&format!(
        "\n📈 {bold}Occurrence Frequency for Token:{reset} \"{}\"\n\
         • Total Inverted Postings: {}{}{} document(s)\n\
         • Pairs Document Frequency: {}{}{} document(s)\n\n\
         {bold}Field-Specific Breakdown:{reset}\n",
        clean_token, green, postings_count, reset, green, pairs_doc_freq, reset
    ));

    let fields = ["name", "city", "postcode", "housenumber", "context"];
    for f in &fields {
        let count = searcher
            .fields_index
            .field_postings
            .get(*f)
            .and_then(|map| map.get(&clean_token))
            .map_or(0, |bm| bm.len() as usize);
        out.push_str(&format!("  • Field \"{:12}\": {} document(s)\n", f, count));
    }
    out.push('\n');

    Ok(out)
}

fn cmd_geodistance(args: &str, c: bool) -> Result<String> {
    let parts: Vec<&str> = args.split_whitespace().collect();
    if parts.len() < 4 {
        return Ok(
            "Usage: GEODISTANCE <lat1> <lon1> <lat2> <lon2>\nExample: GEODISTANCE 48.8512 2.3431 48.8698 2.3082\n"
                .to_string(),
        );
    }

    let lat1: f64 = parts[0]
        .parse()
        .with_context(|| format!("Invalid lat1: {}", parts[0]))?;
    let lon1: f64 = parts[1]
        .parse()
        .with_context(|| format!("Invalid lon1: {}", parts[1]))?;
    let lat2: f64 = parts[2]
        .parse()
        .with_context(|| format!("Invalid lat2: {}", parts[2]))?;
    let lon2: f64 = parts[3]
        .parse()
        .with_context(|| format!("Invalid lon2: {}", parts[3]))?;

    let (green, yellow, bold, dim, reset) = if c {
        (GREEN, YELLOW, BOLD, DIM, RESET)
    } else {
        ("", "", "", "", "")
    };

    let dist_m = haversine_distance_meters(lat1, lon1, lat2, lon2);
    let dist_km = dist_m / 1000.0;
    let score = km_to_score(dist_km);

    Ok(format!(
        "\n🌐 {bold}Haversine Geodistance & Proximity Score:{reset}\n\
         • Point 1: ({:.5}, {:.5})\n\
         • Point 2: ({:.5}, {:.5})\n\
         • Great-Circle Distance: {}{:.2} meters{reset} ({:.3} km)\n\
         • Proximity Score:       {}{:.4}{reset}\n\
         {dim}Decay formula: S(d) = 2^(-d / 50 km) | S(0km) = 1.0, S(50km) = 0.5, S(100km) = 0.25{reset}\n\n",
        lat1, lon1, lat2, lon2, green, dist_m, dist_km, yellow, score
    ))
}
