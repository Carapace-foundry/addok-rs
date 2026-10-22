use addok_core::config::AddokConfig;

/// Recognized French address repetitions and modifiers (e.g. "bis", "ter")
const FRENCH_REPETITIONS: &[&str] = &[
    "bis",
    "ter",
    "quater",
    "quinquies",
    "b",
    "t",
    "q",
    "a",
    "c",
    "d",
    "e",
];

/// Recognized French month names in address dates (e.g. "8 mai", "14 juillet", "11 novembre")
const FRENCH_MONTHS: &[&str] = &[
    "janvier",
    "fevrier",
    "mars",
    "avril",
    "mai",
    "juin",
    "juillet",
    "aout",
    "septembre",
    "octobre",
    "novembre",
    "decembre",
];

/// Checks if a token is a known repetition/ordinal for housenumbers
#[inline]
pub fn is_repetition_token(token: &str) -> bool {
    let lower = token.trim().to_lowercase();
    FRENCH_REPETITIONS.contains(&lower.as_str())
}

/// Checks if a token is a French month name
#[inline]
pub fn is_month_token(token: &str) -> bool {
    let lower = token.trim().to_lowercase();
    FRENCH_MONTHS.contains(&lower.as_str())
}

/// Checks if a token represents a 4-digit historical year (e.g. "1945", "1940", "1789")
#[inline]
pub fn is_four_digit_year(token: &str) -> bool {
    if token.len() == 4 && token.chars().all(|c| c.is_ascii_digit()) {
        if let Ok(year) = token.parse::<u32>() {
            return (1500..=2099).contains(&year);
        }
    }
    false
}

/// Checks if a token represents a 5-digit French postal code
#[inline]
pub fn is_french_postcode(token: &str) -> bool {
    token.len() == 5 && token.chars().all(|c| c.is_ascii_digit())
}

/// Normalized and preprocessed query components according to Addok's pipeline
#[derive(Debug, Clone, PartialEq)]
pub struct ParsedQuery {
    /// Raw query as provided by the user
    pub raw: String,
    /// Normalized query with abbreviations expanded and diacritics removed
    pub normalized: String,
    /// Clean street query without extracted housenumber or postcode
    pub clean_query: String,
    /// Street name tokens (including stop words)
    pub street_tokens: Vec<String>,
    /// Meaningful street tokens (stop words filtered out)
    pub meaningful_tokens: Vec<String>,
    /// Extracted candidate housenumber (e.g. "8", "8 bis")
    pub extracted_housenumber: Option<String>,
    /// Extracted 5-digit postal code (e.g. "75005")
    pub extracted_postcode: Option<String>,
    /// Last typed token in original query (for autocomplete prefix matching)
    pub raw_last_token: Option<String>,
    /// Prefix token to expand via Edge N-Grams (None if last token was housenumber)
    pub autocomplete_prefix: Option<String>,
    /// Tokens sorted by ascending document frequency (rarest first)
    pub sorted_tokens: Vec<String>,
    /// ASCII-folded raw query string for fast ngram distance scoring
    pub ascii_query: String,
    /// Pre-extracted sorted bigrams of the ascii-folded raw query
    pub query_bigrams: Vec<u16>,
}

impl ParsedQuery {
    /// Parse and preprocess a query according to Addok's pipeline:
    /// 1. Query string cleaning, diacritics removal, stop word handling, abbreviation expansion.
    /// 2. Housenumber extraction (leading/trailing numbers and modifiers).
    /// 3. Token frequency sorting (using `get_frequency`).
    pub fn parse<F>(raw_query: &str, config: &AddokConfig, get_frequency: F) -> Self
    where
        F: Fn(&str) -> usize,
    {
        let raw = raw_query.trim().to_string();
        let ascii_query = addok_core::normalizer::ascii_fold(&raw);
        let query_bigrams = crate::scoring::extract_sorted_bigrams(&ascii_query);

        if raw.is_empty() {
            return Self {
                raw,
                normalized: String::new(),
                clean_query: String::new(),
                street_tokens: Vec::new(),
                meaningful_tokens: Vec::new(),
                extracted_housenumber: None,
                extracted_postcode: None,
                raw_last_token: None,
                autocomplete_prefix: None,
                sorted_tokens: Vec::new(),
                ascii_query,
                query_bigrams,
            };
        }

        let has_france = config.has_france();

        // 1. Text normalization: clean French address query noise, diacritics removal, lowercase, abbreviation expansion
        let cleaned_raw = if has_france {
            addok_core::normalizer::clean_query(&raw)
        } else {
            raw.clone()
        };
        let normalized = addok_core::normalizer::normalize_address(&cleaned_raw, has_france);
        let raw_tokens: Vec<String> = normalized
            .split_whitespace()
            .map(|s| s.to_string())
            .collect();

        if raw_tokens.is_empty() {
            return Self {
                raw,
                normalized,
                clean_query: String::new(),
                street_tokens: Vec::new(),
                meaningful_tokens: Vec::new(),
                extracted_housenumber: None,
                extracted_postcode: None,
                raw_last_token: None,
                autocomplete_prefix: None,
                sorted_tokens: Vec::new(),
                ascii_query,
                query_bigrams,
            };
        }

        let raw_last_token = raw_tokens.last().cloned();

        // 2. Housenumber & Postcode extraction
        let (extracted_hn, extracted_pc, street_tokens, autocomplete_prefix) =
            extract_housenumber_and_street(&raw_tokens, config);

        // In Addok, stop words are not removed statically; all street tokens are kept
        // and categorized dynamically based on inverted index frequency thresholds.
        let effective_tokens = street_tokens.clone();

        // 4. Token frequency sorting: sort by ascending frequency (rarest first)
        let mut sorted = effective_tokens.clone();
        sorted.sort_by_key(|t| get_frequency(t));

        let clean_query = street_tokens.join(" ");

        Self {
            raw,
            normalized,
            clean_query,
            street_tokens,
            meaningful_tokens: effective_tokens,
            extracted_housenumber: extracted_hn,
            extracted_postcode: extracted_pc,
            raw_last_token,
            autocomplete_prefix,
            sorted_tokens: sorted,
            ascii_query,
            query_bigrams,
        }
    }
}

/// Helper to extract housenumber, postcode, street tokens, and autocomplete prefix
fn extract_housenumber_and_street(
    tokens: &[String],
    config: &AddokConfig,
) -> (Option<String>, Option<String>, Vec<String>, Option<String>) {
    let n = tokens.len();
    if n == 0 {
        return (None, None, Vec::new(), None);
    }

    let has_france = config.has_france();
    let mut housenumber: Option<String> = None;
    let mut postcode: Option<String> = None;
    let mut street_tokens: Vec<String> = Vec::new();
    let mut last_token_is_hn = false;

    // A. Check for leading housenumber
    // e.g. "8 bd st michel" or "8 bis bd st michel" or "8bis bd st michel"
    let mut start_idx = 0;
    if starts_with_digit(&tokens[0]) {
        if has_france && is_french_postcode(&tokens[0]) && n > 1 {
            postcode = Some(tokens[0].clone());
            start_idx = 1;
        } else if has_france && n > 1 && is_month_token(&tokens[1]) {
            // e.g. "8 mai 1945", "14 juillet" -> "8" is part of the date, not a housenumber!
        } else {
            // Check if followed by repetition (e.g. "8 bis")
            if has_france && n > 1 && is_repetition_token(&tokens[1]) {
                housenumber = Some(format!("{} {}", tokens[0], tokens[1]));
                start_idx = 2;
            } else {
                housenumber = Some(normalize_hn_token(&tokens[0]));
                start_idx = 1;
            }
        }
    }

    // B. Check for trailing housenumber and/or postcode
    let mut end_idx = n;

    // Check if last token is a 5-digit postcode (when French plugin enabled)
    if has_france && end_idx > start_idx && is_french_postcode(&tokens[end_idx - 1]) {
        if postcode.is_none() {
            postcode = Some(tokens[end_idx - 1].clone());
        }
        end_idx -= 1;
    }

    // If housenumber was not found leading, check trailing
    if housenumber.is_none() && end_idx > start_idx {
        if has_france
            && is_four_digit_year(&tokens[end_idx - 1])
            && end_idx >= start_idx + 2
            && is_month_token(&tokens[end_idx - 2])
        {
            // e.g. "... mai 1945" -> 1945 is a year, keep it in street tokens!
        } else if has_france
            && end_idx >= start_idx + 2
            && is_repetition_token(&tokens[end_idx - 1])
            && starts_with_digit(&tokens[end_idx - 2])
        {
            housenumber = Some(format!("{} {}", tokens[end_idx - 2], tokens[end_idx - 1]));
            end_idx -= 2;
            last_token_is_hn = true;
        } else if starts_with_digit(&tokens[end_idx - 1])
            && (!has_france || !is_four_digit_year(&tokens[end_idx - 1]))
        {
            housenumber = Some(normalize_hn_token(&tokens[end_idx - 1]));
            end_idx -= 1;
            last_token_is_hn = true;
        }
    }

    // Collect remaining tokens as street tokens
    for token in &tokens[start_idx..end_idx] {
        if has_france && is_french_postcode(token) && postcode.is_none() {
            postcode = Some(token.clone());
        } else {
            street_tokens.push(token.clone());
        }
    }

    // Determine autocomplete prefix:
    // If autocomplete plugin enabled and the last token was a street token (and not a housenumber)
    let autocomplete_prefix =
        if config.has_autocomplete() && !last_token_is_hn && !street_tokens.is_empty() {
            let last = street_tokens.last().unwrap();
            if last.len() >= config.min_edge_ngrams {
                Some(last.clone())
            } else {
                None
            }
        } else {
            None
        };

    (housenumber, postcode, street_tokens, autocomplete_prefix)
}

#[inline]
fn starts_with_digit(s: &str) -> bool {
    s.chars().next().is_some_and(|c| c.is_ascii_digit())
}

/// Normalizes token like "8bis" into "8 bis", or returns "8", stripping leading zeros
fn normalize_hn_token(token: &str) -> String {
    crate::housenumbers::normalize_housenumber(token)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_leading_housenumber_simple() {
        let config = AddokConfig::default();
        let query = "8 bd st michel";
        let parsed = ParsedQuery::parse(query, &config, |_| 1);

        assert_eq!(parsed.extracted_housenumber.as_deref(), Some("8"));
        assert_eq!(parsed.street_tokens, vec!["boulevard", "saint", "michel"]);
        assert_eq!(parsed.clean_query, "boulevard saint michel");
        assert_eq!(parsed.raw_last_token.as_deref(), Some("michel"));
        assert_eq!(parsed.autocomplete_prefix.as_deref(), Some("michel"));
    }

    #[test]
    fn test_leading_housenumber_repetition() {
        let config = AddokConfig::default();

        // Separate repetition: "8 bis"
        let parsed1 = ParsedQuery::parse("8 bis bd st michel", &config, |_| 1);
        assert_eq!(parsed1.extracted_housenumber.as_deref(), Some("8 bis"));
        assert_eq!(parsed1.street_tokens, vec!["boulevard", "saint", "michel"]);

        // Attached repetition: "8bis"
        let parsed2 = ParsedQuery::parse("8bis bd st michel", &config, |_| 1);
        assert_eq!(parsed2.extracted_housenumber.as_deref(), Some("8 bis"));
        assert_eq!(parsed2.street_tokens, vec!["boulevard", "saint", "michel"]);
    }

    #[test]
    fn test_trailing_housenumber() {
        let config = AddokConfig::default();

        // Simple trailing: "bd st michel 8"
        let parsed1 = ParsedQuery::parse("bd st michel 8", &config, |_| 1);
        assert_eq!(parsed1.extracted_housenumber.as_deref(), Some("8"));
        assert_eq!(parsed1.street_tokens, vec!["boulevard", "saint", "michel"]);
        assert_eq!(parsed1.autocomplete_prefix, None); // last token was housenumber

        // Trailing with repetition: "bd st michel 8 bis"
        let parsed2 = ParsedQuery::parse("bd st michel 8 bis", &config, |_| 1);
        assert_eq!(parsed2.extracted_housenumber.as_deref(), Some("8 bis"));
        assert_eq!(parsed2.street_tokens, vec!["boulevard", "saint", "michel"]);

        // Trailing housenumber before postcode: "bd st michel 8 75005"
        let parsed3 = ParsedQuery::parse("bd st michel 8 75005", &config, |_| 1);
        assert_eq!(parsed3.extracted_housenumber.as_deref(), Some("8"));
        assert_eq!(parsed3.extracted_postcode.as_deref(), Some("75005"));
        assert_eq!(parsed3.street_tokens, vec!["boulevard", "saint", "michel"]);
    }

    #[test]
    fn test_stop_words_and_frequency_sorting() {
        let config = AddokConfig::default();
        let query = "rue de l'eglise";

        // Mock document frequencies: "rue" -> 5000, "eglise" -> 100
        let parsed = ParsedQuery::parse(query, &config, |t| match t {
            "rue" => 5000,
            "eglise" => 100,
            _ => 1000,
        });

        assert_eq!(parsed.extracted_housenumber, None);
        assert_eq!(parsed.street_tokens, vec!["rue", "de", "l", "eglise"]);
        // In Addok, stop words are not removed statically; all street tokens are kept
        assert_eq!(parsed.meaningful_tokens, vec!["rue", "de", "l", "eglise"]);
        // Frequency sorting: rarest first ("eglise" (100) before "de"/"l" (1000) before "rue" (5000))
        assert_eq!(parsed.sorted_tokens[0], "eglise");
        assert_eq!(*parsed.sorted_tokens.last().unwrap(), "rue");
    }

    #[test]
    fn test_prefix_for_autocomplete() {
        let config = AddokConfig::default();
        let query = "8 bd st mich";
        let parsed = ParsedQuery::parse(query, &config, |_| 1);

        assert_eq!(parsed.extracted_housenumber.as_deref(), Some("8"));
        assert_eq!(parsed.autocomplete_prefix.as_deref(), Some("mich"));
    }

    #[test]
    fn test_french_date_street_names_not_confused_with_housenumbers() {
        let config = AddokConfig::default();

        // 1. "rue du 8 mai 1945" -> neither 8 nor 1945 should be extracted as housenumber
        let parsed1 = ParsedQuery::parse("rue du 8 mai 1945", &config, |_| 1);
        assert_eq!(parsed1.extracted_housenumber, None);
        assert_eq!(parsed1.street_tokens, vec!["rue", "du", "8", "mai", "1945"]);

        // 2. "12 rue du 8 mai 1945" -> 12 is extracted, 8 and 1945 stay in street tokens
        let parsed2 = ParsedQuery::parse("12 rue du 8 mai 1945", &config, |_| 1);
        assert_eq!(parsed2.extracted_housenumber.as_deref(), Some("12"));
        assert_eq!(parsed2.street_tokens, vec!["rue", "du", "8", "mai", "1945"]);

        // 3. "8 mai 1945" -> 8 is followed by mai, 1945 preceded by mai -> no housenumber
        let parsed3 = ParsedQuery::parse("8 mai 1945", &config, |_| 1);
        assert_eq!(parsed3.extracted_housenumber, None);
        assert_eq!(parsed3.street_tokens, vec!["8", "mai", "1945"]);

        // 4. "14 juillet" -> 14 followed by juillet -> no housenumber
        let parsed4 = ParsedQuery::parse("14 juillet", &config, |_| 1);
        assert_eq!(parsed4.extracted_housenumber, None);
        assert_eq!(parsed4.street_tokens, vec!["14", "juillet"]);
    }
}
