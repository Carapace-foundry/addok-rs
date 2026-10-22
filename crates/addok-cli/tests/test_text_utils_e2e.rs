use addok_core::normalizer::{is_stop_word, normalize_french_address, tokenize_french_address};
use addok_index::query::{is_french_postcode, is_repetition_token};
use addok_index::{compare_ngrams, km_to_score, levenshtein_distance, levenshtein_ratio};

#[test]
fn test_accents_and_diacritics_normalization() {
    // Ported from Addok's test_text_utils.py: test_fold_to_ascii
    assert_eq!(normalize_french_address("Élysées"), "elysees");
    assert_eq!(normalize_french_address("Saint-Étienne"), "saint etienne");
    assert_eq!(normalize_french_address("façade"), "facade");
    assert_eq!(normalize_french_address("cœur"), "coeur");
    assert_eq!(normalize_french_address("Nîmes"), "nimes");
    assert_eq!(normalize_french_address("août"), "aout");
    assert_eq!(normalize_french_address("Hélène"), "helene");
    assert_eq!(normalize_french_address("Châteauneuf"), "chateauneuf");
}

#[test]
fn test_apostrophes_and_elisions() {
    // Ported from Addok's test_text_utils.py: test_replace_apostrophes
    assert_eq!(normalize_french_address("l'église"), "l eglise");
    assert_eq!(
        normalize_french_address("d'aubervilliers"),
        "d aubervilliers"
    );
    assert_eq!(normalize_french_address("presqu'île"), "presqu ile");
    // Curly right single quotation mark
    assert_eq!(normalize_french_address("l’église"), "l eglise");
    assert_eq!(normalize_french_address("d’armagnac"), "d armagnac");
}

#[test]
fn test_thoroughfare_abbreviations_expansion() {
    // Ported from Addok's test_text_utils.py: test_expand_abbreviations
    assert_eq!(
        normalize_french_address("8 bd st michel"),
        "8 boulevard saint michel"
    );
    assert_eq!(
        normalize_french_address("10 av champs elysees"),
        "10 avenue champs elysees"
    );
    assert_eq!(
        normalize_french_address("3 r de la paix"),
        "3 rue de la paix"
    );
    assert_eq!(
        normalize_french_address("5 pl de la concorde"),
        "5 place de la concorde"
    );
    assert_eq!(
        normalize_french_address("12 all des cerisiers"),
        "12 allee des cerisiers"
    );
    assert_eq!(
        normalize_french_address("2 imp des lilas"),
        "2 impasse des lilas"
    );
    assert_eq!(
        normalize_french_address("18 crs vitor hugo"),
        "18 cours vitor hugo"
    );
    assert_eq!(
        normalize_french_address("4 fg st antoine"),
        "4 faubourg saint antoine"
    );
    assert_eq!(normalize_french_address("7 qu branly"), "7 quai branly");
    assert_eq!(normalize_french_address("1 sq monceau"), "1 square monceau");
}

#[test]
fn test_word_boundary_safety_no_false_positive_expansions() {
    // Ported from Addok's test_text_utils.py: test_abbreviations_word_boundaries
    // "st" inside "brest", "station", "austin" must NOT be expanded!
    let norm = normalize_french_address("rue de brest station austin rabat allier");
    assert!(norm.contains("brest"));
    assert!(norm.contains("station"));
    assert!(norm.contains("austin"));
    assert!(norm.contains("rabat"));
    assert!(norm.contains("allier"));
    assert!(!norm.contains("saintation"));
}

#[test]
fn test_stop_words_identification_and_filtering() {
    // Ported from Addok's test_text_utils.py: test_remove_stop_words
    assert!(is_stop_word("de"));
    assert!(is_stop_word("du"));
    assert!(is_stop_word("des"));
    assert!(is_stop_word("le"));
    assert!(is_stop_word("la"));
    assert!(is_stop_word("les"));
    assert!(is_stop_word("d"));
    assert!(is_stop_word("l"));
    assert!(!is_stop_word("rue"));
    assert!(!is_stop_word("paris"));
    assert!(!is_stop_word("michel"));

    let tokens_with_stop = tokenize_french_address("rue de la paix", false);
    assert_eq!(tokens_with_stop, vec!["rue", "de", "la", "paix"]);

    let tokens_without_stop = tokenize_french_address("rue de la paix", true);
    assert_eq!(tokens_without_stop, vec!["rue", "paix"]);
}

#[test]
fn test_repetition_modifiers_and_postcodes() {
    // Housenumber repetitions (bis, ter, etc.)
    assert!(is_repetition_token("bis"));
    assert!(is_repetition_token("ter"));
    assert!(is_repetition_token("quater"));
    assert!(is_repetition_token("b"));
    assert!(is_repetition_token("t"));
    assert!(!is_repetition_token("paris"));

    // French 5-digit postcodes
    assert!(is_french_postcode("75005"));
    assert!(is_french_postcode("13001"));
    assert!(is_french_postcode("69002"));
    assert!(!is_french_postcode("7500"));
    assert!(!is_french_postcode("750001"));
    assert!(!is_french_postcode("abcde"));
}

#[test]
fn test_scoring_distance_and_dice_similarity() {
    // Exact match = 1.0
    assert_eq!(
        compare_ngrams("boulevard saint michel", "boulevard saint michel"),
        1.0
    );
    assert_eq!(levenshtein_ratio("boulevard", "boulevard"), 1.0);

    // Empty string
    assert_eq!(compare_ngrams("", "test"), 0.0);
    assert_eq!(compare_ngrams("test", ""), 0.0);

    // Minor typo (Levenshtein distance 1)
    let dist = levenshtein_distance("michel", "mcihel");
    assert_eq!(dist, 2); // Levenshtein substitution/swap
    let ratio = levenshtein_ratio("michel", "mchel");
    assert!(ratio > 0.8);

    // Addok exponential Gaussian decay geo-score: exp(-((km/50)^2))
    assert!((km_to_score(0.0) - 1.0).abs() < 1e-6);
    assert!((km_to_score(50.0) - (-1.0_f64).exp()).abs() < 1e-4);
    assert!((km_to_score(100.0) - (-4.0_f64).exp()).abs() < 1e-4);
    assert_eq!(km_to_score(101.0), 0.0);
}

#[test]
fn test_plugin_france_enabled_vs_disabled() {
    use addok_core::normalizer::{normalize_address, tokenize_address};

    // When addok_france is enabled:
    assert_eq!(
        normalize_address("8 bd st michel", true),
        "8 boulevard saint michel"
    );
    assert_eq!(
        normalize_address("Neuilly s/ Seine", true),
        "neuilly sur seine"
    );
    assert_eq!(
        tokenize_address("rue de l eglise", true, true),
        vec!["rue", "eglise"]
    );

    // When addok_france is disabled (generic international core):
    assert_eq!(normalize_address("8 bd st michel", false), "8 bd st michel");
    assert_eq!(
        normalize_address("Neuilly s/ Seine", false),
        "neuilly s seine"
    );
    assert_eq!(
        tokenize_address("rue de l eglise", true, false),
        vec!["rue", "de", "l", "eglise"]
    );
}

#[test]
fn test_parsed_query_with_and_without_france_plugin() {
    use addok_core::config::AddokConfig;
    use addok_index::query::ParsedQuery;

    // Config WITH addok_france
    let config_with_france = AddokConfig::default();
    assert!(config_with_france.has_france());

    let pq_fr = ParsedQuery::parse("8 bd st michel cedex 05", &config_with_france, |_| 100);
    assert_eq!(pq_fr.extracted_housenumber, Some("8".to_string()));
    assert_eq!(pq_fr.clean_query, "boulevard saint michel");
    assert_eq!(
        pq_fr.meaningful_tokens,
        vec!["boulevard", "saint", "michel"]
    );

    // Config WITHOUT addok_france
    let config_without_france = AddokConfig {
        plugins: vec![
            "addok.shell".to_string(),
            "addok.http".to_string(),
            "addok.fuzzy".to_string(),
            "addok.autocomplete".to_string(),
        ],
        ..Default::default()
    };
    assert!(!config_without_france.has_france());

    let pq_no_fr = ParsedQuery::parse("8 bd st michel cedex 05", &config_without_france, |_| 100);
    assert_eq!(pq_no_fr.extracted_housenumber, Some("8".to_string()));
    // "cedex" and "05" are NOT stripped as French noise! "bd" and "st" are NOT expanded!
    assert_eq!(pq_no_fr.clean_query, "bd st michel cedex 05");
    assert_eq!(
        pq_no_fr.meaningful_tokens,
        vec!["bd", "st", "michel", "cedex", "05"]
    );
}
