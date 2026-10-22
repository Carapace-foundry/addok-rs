use std::collections::HashMap;

use addok_core::config::AddokConfig;
use roaring::RoaringBitmap;

use crate::fields_index::FieldsInvertedIndex;
use crate::filters::FiltersIndex;
use crate::ngram::EdgeNgramIndex;
use crate::pairs::PairsIndexer;
use crate::query::ParsedQuery;

/// Surrounding letters on standard keyboard (default French AZERTY) from Addok's FUZZY_KEY_MAP
fn azerty_fuzzy_substitutions(c: char) -> Option<&'static str> {
    match c {
        'a' => Some("ezqop"),
        'z' => Some("aqse"),
        'e' => Some("azsdryu"),
        'r' => Some("edft"),
        't' => Some("rfgy"),
        'y' => Some("teghu"),
        'u' => Some("yehji"),
        'i' => Some("ujko"),
        'o' => Some("iaklp"),
        'p' => Some("oalm"),
        'q' => Some("azsw"),
        's' => Some("qzedxw"),
        'd' => Some("serfcx"),
        'f' => Some("drtgvc"),
        'g' => Some("ftyhbv"),
        'h' => Some("gyujnb"),
        'j' => Some("huikn"),
        'k' => Some("jil"),
        'l' => Some("kom"),
        'm' => Some("lpu"),
        'w' => Some("qsx"),
        'x' => Some("wsdc"),
        'c' => Some("xdfvio"),
        'v' => Some("cfgb"),
        'b' => Some("vghn"),
        'n' => Some("bhj"),
        _ => None,
    }
}

/// Generates Damerau-Levenshtein distance-1 neighbors for a token loyal to Addok's make_fuzzy:
/// 1. Inversions of adjacent characters
/// 2. Substitutions limited to keyboard mapping (or ascii letters)
/// 3. Insertions of ascii letters
/// 4. Deletions (if length > 3)
/// 5. Order-preserving deduplication
pub fn generate_fuzzy_neighbors(token: &str) -> Vec<String> {
    let clean = token.trim().to_lowercase();
    let chars: Vec<char> = clean.chars().collect();
    let n = chars.len();
    if n < 3 {
        return Vec::new();
    }

    let mut neighbors: Vec<String> = Vec::new();

    // 1. Inversions of adjacent characters
    for i in 0..(n - 1) {
        let mut modified = chars.clone();
        modified.swap(i, i + 1);
        neighbors.push(modified.into_iter().collect());
    }

    // 2. Substitutions (keyboard map first, followed by general alphabet)
    for i in 0..n {
        if let Some(subs) = azerty_fuzzy_substitutions(chars[i]) {
            for sub_c in subs.chars() {
                if sub_c != chars[i] {
                    let mut modified = chars.clone();
                    modified[i] = sub_c;
                    neighbors.push(modified.into_iter().collect());
                }
            }
        }
    }
    for i in 0..n {
        for sub_c in "abcdefghijklmnopqrstuvwxyz".chars() {
            if sub_c != chars[i] {
                let mut modified = chars.clone();
                modified[i] = sub_c;
                neighbors.push(modified.into_iter().collect());
            }
        }
    }

    // 3. Insertions
    let alphabet = "abcdefghijklmnopqrstuvwxyz";
    for ins_c in alphabet.chars() {
        for i in 0..=n {
            let mut modified = chars.clone();
            modified.insert(i, ins_c);
            neighbors.push(modified.into_iter().collect());
        }
    }

    // 4. Deletions (if length > 3)
    if n > 3 {
        for i in 0..n {
            let mut modified = chars.clone();
            modified.remove(i);
            neighbors.push(modified.into_iter().collect());
        }
    }

    // 5. Order-preserving deduplication
    let mut seen = std::collections::HashSet::new();
    neighbors.retain(|s| seen.insert(s.clone()));

    neighbors
}

/// Diagnostic trace collected during candidate retrieval
#[derive(Debug, Clone, Default)]
pub struct CollectionTrace {
    pub token_frequencies: Vec<(String, usize, bool)>,
    pub phase1_active_tokens: Vec<String>,
    pub phase1_intersections: Vec<(String, usize)>,
    pub phase1_count: usize,
    pub phase2_prefix: Option<String>,
    pub phase2_ngram_candidates: Vec<String>,
    pub phase2_pair_filtered_candidates: Vec<String>,
    pub phase2_added_count: usize,
    pub phase3_triggered: bool,
    pub phase3_fuzzy_variants: Vec<(String, String, usize)>,
    pub phase3_added_count: usize,
    pub phase4_filters_applied: HashMap<String, String>,
}

/// Candidate Bucket Collector executing Addok's 4-step candidate retrieval:
/// - Step 1: Intersect non-common tokens until `bucket_min` candidates are collected.
/// - Step 2: Autocomplete Collector (Edge N-Grams + Token Pairs).
/// - Step 3: Fuzzy Collector (Levenshtein / transposition neighbors if bucket is dry).
/// - Step 4: Multi-value filters evaluation (support `type=street+city` OR logic and multi-filter AND logic).
pub struct BucketCollector<'a> {
    pub fields_index: &'a FieldsInvertedIndex,
    pub ngram_index: &'a EdgeNgramIndex,
    pub pairs_index: &'a PairsIndexer,
    pub filters_index: &'a FiltersIndex,
    pub config: &'a AddokConfig,
}

impl<'a> BucketCollector<'a> {
    pub fn new(
        fields_index: &'a FieldsInvertedIndex,
        ngram_index: &'a EdgeNgramIndex,
        pairs_index: &'a PairsIndexer,
        filters_index: &'a FiltersIndex,
        config: &'a AddokConfig,
    ) -> Self {
        Self {
            fields_index,
            ngram_index,
            pairs_index,
            filters_index,
            config,
        }
    }

    /// Execute candidate collection for a parsed query and optional filters.
    pub fn collect(
        &self,
        parsed_query: &ParsedQuery,
        autocomplete: bool,
        filters: &HashMap<String, String>,
    ) -> RoaringBitmap {
        self.collect_with_trace(parsed_query, autocomplete, filters)
            .0
    }

    /// Execute candidate collection and record step-by-step diagnostic trace.
    pub fn collect_with_trace(
        &self,
        parsed_query: &ParsedQuery,
        autocomplete: bool,
        filters: &HashMap<String, String>,
    ) -> (RoaringBitmap, CollectionTrace) {
        let mut bucket = RoaringBitmap::new();
        let mut trace = CollectionTrace::default();

        // 0. Compute token frequencies for street tokens
        for tok in &parsed_query.street_tokens {
            let freq = self
                .fields_index
                .get_postings(tok)
                .map_or(0, |bm| bm.len() as usize);
            let is_common = freq >= self.config.common_threshold;
            trace.token_frequencies.push((tok.clone(), freq, is_common));
        }

        // -------------------------------------------------------------
        // Step 1: Intersect non-common tokens until `bucket_min` candidates
        // -------------------------------------------------------------
        let tokens = if !parsed_query.sorted_tokens.is_empty() {
            &parsed_query.sorted_tokens
        } else if !parsed_query.meaningful_tokens.is_empty() {
            &parsed_query.meaningful_tokens
        } else {
            &parsed_query.street_tokens
        };

        if !tokens.is_empty() {
            let non_common: Vec<&String> = tokens
                .iter()
                .filter(|t| {
                    self.fields_index
                        .get_postings(t)
                        .map_or(0, |bm| bm.len() as usize)
                        < self.config.common_threshold
                })
                .collect();

            let active_tokens: Vec<&String> = if !non_common.is_empty() {
                non_common
            } else {
                tokens.iter().collect()
            };

            trace.phase1_active_tokens = active_tokens.iter().map(|s| (*s).clone()).collect();

            let mut matched_first = false;
            for tok in &active_tokens {
                if let Some(bm) = self.fields_index.get_postings(tok) {
                    if !matched_first {
                        bucket = bm.clone();
                        matched_first = true;
                        trace
                            .phase1_intersections
                            .push(((*tok).clone(), bucket.len() as usize));
                    } else {
                        let inter = &bucket & bm;
                        if !inter.is_empty() {
                            bucket = inter;
                            trace
                                .phase1_intersections
                                .push(((*tok).clone(), bucket.len() as usize));
                        }
                    }
                }
            }

            // Step 1b: If bucket is dry (< bucket_min) and we have > 1 active tokens,
            // try reducing 1 token (Addok's extend_results_reducing_tokens)
            if (bucket.len() as usize) < self.config.bucket_min && active_tokens.len() > 1 {
                for skip_idx in 0..active_tokens.len() {
                    let subset: Vec<&String> = active_tokens
                        .iter()
                        .enumerate()
                        .filter(|&(i, _)| i != skip_idx)
                        .map(|(_, &t)| t)
                        .collect();

                    let mut sub_bucket = RoaringBitmap::new();
                    let mut first = true;
                    for tok in subset {
                        if let Some(bm) = self.fields_index.get_postings(tok) {
                            if first {
                                sub_bucket = bm.clone();
                                first = false;
                            } else {
                                sub_bucket &= bm;
                            }
                        }
                    }
                    if !sub_bucket.is_empty() {
                        bucket |= sub_bucket;
                        if (bucket.len() as usize) >= self.config.bucket_min {
                            break;
                        }
                    }
                }
            }
        }
        trace.phase1_count = bucket.len() as usize;

        // -------------------------------------------------------------
        // Step 2: Autocomplete Collector
        // If autocomplete plugin is enabled, autocomplete=true, and last token
        // has length >= min_edge_ngrams, expand last token via Edge N-Grams
        // filtered by Token Pairs (if pairs plugin enabled).
        // -------------------------------------------------------------
        trace.phase2_prefix = parsed_query.autocomplete_prefix.clone();
        if autocomplete && self.config.has_autocomplete() {
            if let Some(ref prefix) = parsed_query.autocomplete_prefix {
                if prefix.len() >= self.config.min_edge_ngrams {
                    let candidates = self.ngram_index.get_candidates(prefix);
                    trace.phase2_ngram_candidates = candidates.clone();

                    if !candidates.is_empty() {
                        let other_tokens: Vec<&str> = parsed_query
                            .street_tokens
                            .iter()
                            .filter(|t| *t != prefix)
                            .map(String::as_str)
                            .collect();

                        let filtered = if self.config.has_pairs() {
                            self.pairs_index
                                .filter_candidates(&other_tokens, &candidates)
                        } else {
                            candidates.clone()
                        };
                        trace.phase2_pair_filtered_candidates = filtered.clone();

                        let other_postings: Option<RoaringBitmap> = if other_tokens.is_empty() {
                            None
                        } else {
                            let mut iter = other_tokens
                                .iter()
                                .filter_map(|&ot| self.fields_index.get_postings(ot));
                            if let Some(first) = iter.next() {
                                let mut acc = first.clone();
                                for bm in iter {
                                    acc &= bm;
                                }
                                Some(acc)
                            } else {
                                None
                            }
                        };

                        let mut ac_postings = RoaringBitmap::new();
                        for match_tok in &filtered {
                            if let Some(bm) = self.fields_index.get_postings(match_tok) {
                                if let Some(ref others) = other_postings {
                                    let narrowed = bm & others;
                                    if !narrowed.is_empty() {
                                        ac_postings |= narrowed;
                                    } else {
                                        ac_postings |= bm;
                                    }
                                } else {
                                    ac_postings |= bm;
                                }
                            }
                        }

                        let before_len = bucket.len();
                        if !ac_postings.is_empty() {
                            if bucket.is_empty() {
                                bucket = ac_postings;
                            } else {
                                bucket |= ac_postings;
                            }
                        }
                        trace.phase2_added_count =
                            (bucket.len().saturating_sub(before_len)) as usize;
                    }
                }
            }
        }

        // -------------------------------------------------------------
        // Step 3: Fuzzy Collector
        // If fuzzy plugin is enabled and bucket is empty or dry, generate
        // Levenshtein / transposition neighbors for rare / unmatched tokens
        // and collect candidates.
        // -------------------------------------------------------------
        if (bucket.is_empty() || (bucket.len() as usize) < self.config.bucket_min)
            && self.config.has_fuzzy()
        {
            trace.phase3_triggered = true;
            let before_fuzzy_len = bucket.len();

            for tok in &parsed_query.meaningful_tokens {
                let freq = self
                    .fields_index
                    .get_postings(tok)
                    .map_or(0, |bm| bm.len() as usize);

                // Check rare or zero-hit tokens
                if freq < 10 {
                    let other_tokens: Vec<&str> = parsed_query
                        .meaningful_tokens
                        .iter()
                        .filter(|t| *t != tok)
                        .map(String::as_str)
                        .collect();

                    let other_postings: Option<RoaringBitmap> = if other_tokens.is_empty() {
                        None
                    } else {
                        let mut iter = other_tokens
                            .iter()
                            .filter_map(|&ot| self.fields_index.get_postings(ot));
                        if let Some(first) = iter.next() {
                            let mut acc = first.clone();
                            for bm in iter {
                                acc &= bm;
                            }
                            Some(acc)
                        } else {
                            None
                        }
                    };

                    let neighbors = generate_fuzzy_neighbors(tok);
                    for neighbor in neighbors {
                        if let Some(bm) = self.fields_index.get_postings(&neighbor) {
                            trace.phase3_fuzzy_variants.push((
                                tok.clone(),
                                neighbor.clone(),
                                bm.len() as usize,
                            ));

                            if let Some(ref others) = other_postings {
                                let narrowed = bm & others;
                                if !narrowed.is_empty() {
                                    bucket |= narrowed;
                                } else {
                                    bucket |= bm;
                                }
                            } else {
                                bucket |= bm;
                            }

                            if (bucket.len() as usize) >= self.config.bucket_min {
                                break;
                            }
                        }
                    }
                }

                if (bucket.len() as usize) >= self.config.bucket_min {
                    break;
                }
            }
            trace.phase3_added_count = (bucket.len().saturating_sub(before_fuzzy_len)) as usize;
        }

        // -------------------------------------------------------------
        // Step 4: Multi-value filters evaluation
        // Support `type=street+city` OR logic and multi-filter AND logic.
        // -------------------------------------------------------------
        let mut total_filter_mask: Option<RoaringBitmap> = None;
        trace.phase4_filters_applied = filters.clone();

        for (key, val_str) in filters {
            let clean_key = key.trim().to_lowercase();
            // Split by '+' (standard in Addok for OR logic) or ',' or configured separator
            let values: Vec<&str> = val_str
                .split(['+', ',', ' '])
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .collect();

            if values.is_empty() {
                continue;
            }

            // OR logic between values for this key
            let mut key_mask = RoaringBitmap::new();
            for v in values {
                if let Some(bm) = self.filters_index.get_filter_bitmap(&clean_key, v) {
                    key_mask |= bm;
                }
            }

            // AND logic across distinct filter keys
            match total_filter_mask {
                None => total_filter_mask = Some(key_mask),
                Some(ref mut curr) => *curr &= key_mask,
            }
        }

        // Apply detected query postcode if not already explicitly in filters
        if let Some(ref pc) = parsed_query.extracted_postcode {
            if !filters.contains_key("postcode") {
                trace
                    .phase4_filters_applied
                    .insert("postcode".to_string(), pc.clone());
                if let Some(bm) = self.filters_index.get_filter_bitmap("postcode", pc) {
                    match total_filter_mask {
                        None => total_filter_mask = Some(bm.clone()),
                        Some(ref mut curr) => *curr &= bm,
                    }
                }
            }
        }

        if let Some(mask) = total_filter_mask {
            bucket &= mask;
        }

        (bucket, trace)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fields_index::FieldBoostConfig;
    use addok_core::document::Document;

    #[test]
    fn test_fuzzy_neighbor_generation() {
        let neighbors = generate_fuzzy_neighbors("chemim");
        // "chemin" must be generated via substitution of 'm' -> 'n'
        assert!(neighbors.contains(&"chemin".to_string()));

        let neighbors_boulvard = generate_fuzzy_neighbors("boulvard");
        // "boulevard" must be generated via insertion of 'e'
        assert!(neighbors_boulvard.contains(&"boulevard".to_string()));

        let neighbors_swap = generate_fuzzy_neighbors("parsi");
        // "paris" must be generated via transposition of 's' and 'i'
        assert!(neighbors_swap.contains(&"paris".to_string()));
    }

    #[test]
    fn test_collector_pipeline() {
        let config = AddokConfig::default();
        let mut fields_index = FieldsInvertedIndex::new(FieldBoostConfig::default());
        let mut ngram_index = EdgeNgramIndex::default();
        let mut pairs_index = PairsIndexer::new();
        let mut filters_index = FiltersIndex::new();

        // Index doc1: Boulevard Saint-Michel (street, 75005)
        let doc1 = Document::new("1", "Boulevard Saint-Michel")
            .with_type("street")
            .with_postcode("75005")
            .with_city("Paris");
        fields_index.index_document(0, &doc1);
        filters_index.index_document(0, &doc1);
        ngram_index.index_tokens(["boulevard", "saint", "michel", "paris"]);
        pairs_index.index_tokens(&["boulevard", "saint", "michel", "paris"]);

        // Index doc2: Avenue des Champs-Élysées (street, 75008)
        let doc2 = Document::new("2", "Avenue des Champs-Élysées")
            .with_type("street")
            .with_postcode("75008")
            .with_city("Paris");
        fields_index.index_document(1, &doc2);
        filters_index.index_document(1, &doc2);
        ngram_index.index_tokens(["avenue", "champs", "elysees", "paris"]);
        pairs_index.index_tokens(&["avenue", "champs", "elysees", "paris"]);

        // Index doc3: Paris (municipality, 75056)
        let doc3 = Document::new("3", "Paris")
            .with_type("municipality")
            .with_postcode("75001")
            .with_city("Paris");
        fields_index.index_document(2, &doc3);
        filters_index.index_document(2, &doc3);
        ngram_index.index_tokens(["paris"]);
        pairs_index.index_tokens(&["paris"]);

        let collector = BucketCollector::new(
            &fields_index,
            &ngram_index,
            &pairs_index,
            &filters_index,
            &config,
        );

        // 1. Step 1: exact tokens
        let q1 = ParsedQuery::parse("saint michel", &config, |t| {
            fields_index.get_postings(t).map_or(0, |b| b.len() as usize)
        });
        let cands1 = collector.collect(&q1, false, &HashMap::new());
        assert!(cands1.contains(0));
        assert!(!cands1.contains(1));

        // 2. Step 2: Autocomplete "saint mich" -> expands to "michel"
        let q2 = ParsedQuery::parse("saint mich", &config, |t| {
            fields_index.get_postings(t).map_or(0, |b| b.len() as usize)
        });
        let cands2 = collector.collect(&q2, true, &HashMap::new());
        assert!(cands2.contains(0));

        // 3. Step 3: Fuzzy search typo "saint michl" -> finds "michel"
        let q3 = ParsedQuery::parse("saint michl", &config, |t| {
            fields_index.get_postings(t).map_or(0, |b| b.len() as usize)
        });
        let cands3 = collector.collect(&q3, false, &HashMap::new());
        assert!(cands3.contains(0));

        // 4. Step 4: Multi-value filter OR logic: type=street+municipality
        let mut filters = HashMap::new();
        filters.insert("type".to_string(), "street+municipality".to_string());
        let q_paris = ParsedQuery::parse("paris", &config, |t| {
            fields_index.get_postings(t).map_or(0, |b| b.len() as usize)
        });
        let cands_or = collector.collect(&q_paris, false, &filters);
        // All 3 docs have "paris", and their types are "street" and "municipality", so all 3 match!
        assert_eq!(cands_or.len(), 3);

        // Multi-value filter AND logic: type=street AND postcode=75005
        filters.insert("postcode".to_string(), "75005".to_string());
        let cands_and = collector.collect(&q_paris, false, &filters);
        assert_eq!(cands_and.len(), 1);
        assert!(cands_and.contains(0)); // Only doc1 matches both
    }
}
