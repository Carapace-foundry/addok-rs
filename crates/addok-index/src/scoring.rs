use std::collections::{HashMap, HashSet};

use addok_core::config::AddokConfig;
use addok_core::document::{Document, HousenumberData};
pub use addok_core::normalizer::ascii_fold;

use crate::housenumbers::HousenumbersIndex;
use crate::query::ParsedQuery;
use crate::searcher::SearchHit;
use crate::spatial::haversine_distance_meters;

/// Exponential decay using Addok's exact `km_to_score(distance_in_km)` formula:
/// $S(d) = 0.0 \text{ if } d > 100 \text{ else } \exp(-(d / 50)^2)$
#[inline]
pub fn km_to_score(distance_in_km: f64) -> f64 {
    if distance_in_km > 100.0 {
        return 0.0;
    }
    if distance_in_km <= 0.0 {
        return 1.0;
    }
    (-((distance_in_km / 50.0).powi(2))).exp()
}

/// Extracts sorted list of bigrams from an ASCII string as packed `u16` values.
/// This matches Addok's multiset bigram representation but without any heap hash tables.
#[inline]
pub fn extract_sorted_bigrams(s: &str) -> Vec<u16> {
    let t = s.trim();
    if t.len() < 2 {
        return Vec::new();
    }
    let bytes = t.as_bytes();
    let mut bg = Vec::with_capacity(bytes.len() - 1);
    for w in bytes.windows(2) {
        bg.push(((w[0] as u16) << 8) | (w[1] as u16));
    }
    bg.sort_unstable();
    bg
}

/// Compares two pre-extracted sorted bigram slices using multiset Jaccard similarity.
/// Exactly matches Python Addok's `compare_ngrams` down to IEEE-754 precision.
#[inline]
pub fn compare_sorted_bigrams(bg1: &[u16], bg2: &[u16]) -> f32 {
    if bg1.is_empty() || bg2.is_empty() {
        return 0.0;
    }
    let mut i = 0;
    let mut j = 0;
    let mut intersection = 0usize;
    while i < bg1.len() && j < bg2.len() {
        match bg1[i].cmp(&bg2[j]) {
            std::cmp::Ordering::Equal => {
                intersection += 1;
                i += 1;
                j += 1;
            }
            std::cmp::Ordering::Less => i += 1,
            std::cmp::Ordering::Greater => j += 1,
        }
    }
    let total = bg1.len() + bg2.len();
    let union = total - intersection;
    if union == 0 {
        return 0.0;
    }
    (intersection as f32) / (union as f32)
}

/// N-gram Bigram Jaccard similarity between two strings, matching Addok's `compare_ngrams` / `ngram.NGram.compare(..., N=2, pad_len=0)`.
pub fn compare_ngrams(s1: &str, s2: &str) -> f32 {
    let t1 = s1.trim();
    let t2 = s2.trim();
    if t1.is_empty() || t2.is_empty() {
        return 0.0;
    }
    if t1 == t2 {
        return 1.0;
    }

    if t1.is_ascii() && t2.is_ascii() {
        if t1.len() < 2 || t2.len() < 2 {
            return 0.0;
        }
        let bg1 = extract_sorted_bigrams(t1);
        let bg2 = extract_sorted_bigrams(t2);
        return compare_sorted_bigrams(&bg1, &bg2);
    }

    let chars1: Vec<char> = t1.chars().collect();
    let chars2: Vec<char> = t2.chars().collect();

    if chars1.len() < 2 || chars2.len() < 2 {
        return if t1 == t2 { 1.0 } else { 0.0 };
    }

    let mut ngrams1: HashMap<(char, char), usize> = HashMap::new();
    for w in chars1.windows(2) {
        *ngrams1.entry((w[0], w[1])).or_insert(0) += 1;
    }

    let mut ngrams2: HashMap<(char, char), usize> = HashMap::new();
    for w in chars2.windows(2) {
        *ngrams2.entry((w[0], w[1])).or_insert(0) += 1;
    }

    let mut intersection = 0usize;
    for (bg, c1) in &ngrams1 {
        if let Some(c2) = ngrams2.get(bg) {
            intersection += (*c1).min(*c2);
        }
    }

    let total = (chars1.len() - 1) + (chars2.len() - 1);
    let union = total - intersection;
    if union == 0 {
        return 0.0;
    }
    (intersection as f32) / (union as f32)
}

pub fn levenshtein_distance(s1: &str, s2: &str) -> usize {
    let mut c1: Vec<char> = s1.chars().collect();
    let mut c2: Vec<char> = s2.chars().collect();
    if c1.len() < c2.len() {
        std::mem::swap(&mut c1, &mut c2);
    }
    let n = c1.len();
    let m = c2.len();
    if m == 0 {
        return n;
    }

    let mut prev_row: Vec<usize> = (0..=m).collect();
    let mut curr_row = vec![0usize; m + 1];

    for i in 1..=n {
        curr_row[0] = i;
        for j in 1..=m {
            let cost = if c1[i - 1] == c2[j - 1] { 0 } else { 1 };
            curr_row[j] = (curr_row[j - 1] + 1)
                .min(prev_row[j] + 1)
                .min(prev_row[j - 1] + cost);
        }
        std::mem::swap(&mut prev_row, &mut curr_row);
    }

    prev_row[m]
}

/// Levenshtein similarity ratio in [0.0, 1.0]
pub fn levenshtein_ratio(s1: &str, s2: &str) -> f32 {
    let max_len = s1.chars().count().max(s2.chars().count());
    if max_len == 0 {
        return 1.0;
    }
    let dist = levenshtein_distance(s1, s2);
    (1.0 - (dist as f32 / max_len as f32)).max(0.0)
}

/// Computes token overlap ratio between query tokens and document tokens
pub fn token_coverage_ratio(query_tokens: &[String], doc_tokens_set: &HashSet<String>) -> f32 {
    if query_tokens.is_empty() {
        return 1.0;
    }
    let mut matched = 0;
    for q in query_tokens {
        if doc_tokens_set.contains(q) {
            matched += 1;
        }
    }
    matched as f32 / query_tokens.len() as f32
}

/// Computes string similarity score loyal to Addok's scoring pipeline:
/// - If `autocomplete == true`: executes `score_by_autocomplete_distance`:
///   - Exact match: 1.0
///   - Prefix match (`label.starts_with(query)`): 0.9
///   - Substring containment (`label.contains(query)`): 0.7
///   - Fallback: Character Bigram similarity * 0.9
/// - If `autocomplete == false`: executes `score_by_ngram_distance`:
///   - Exact match: 1.0
///   - Character Bigram similarity * 1.0
///
/// Evaluates across all generated label variants and takes the maximum score,
/// breaking early if `score >= match_threshold` (0.9).
pub fn compute_str_score(
    labels: &[String],
    ascii_query: &str,
    query_bigrams: &[u16],
    autocomplete: bool,
    match_threshold: f32,
) -> f32 {
    if labels.is_empty() || ascii_query.trim().is_empty() {
        return 0.0;
    }

    let q = ascii_query.trim();

    if autocomplete {
        let mut best_score: f32 = 0.0;
        for label in labels {
            let label_folded = ascii_fold(label);
            let s = if label_folded == q {
                1.0
            } else if label_folded.starts_with(q) {
                0.9
            } else if label_folded.contains(q) {
                0.7
            } else {
                0.0
            };
            if s > best_score {
                best_score = s;
                if best_score >= 1.0 {
                    return 1.0;
                }
            }
        }
        if best_score > 0.0 {
            return best_score;
        }

        // Fallback to ngram distance scaled by 0.9 loyal to score_by_autocomplete_distance
        let mut best_ngram: f32 = 0.0;
        for label in labels {
            let label_folded = ascii_fold(label);
            let score = if label_folded == q {
                0.9
            } else {
                let label_bigrams = extract_sorted_bigrams(&label_folded);
                compare_sorted_bigrams(&label_bigrams, query_bigrams) * 0.9
            };
            if score > best_ngram {
                best_ngram = score;
                if best_ngram >= match_threshold {
                    break;
                }
            }
        }
        best_ngram
    } else {
        let mut best_score: f32 = 0.0;
        for label in labels {
            let label_folded = ascii_fold(label);
            let score = if label_folded == q {
                1.0
            } else {
                let label_bigrams = extract_sorted_bigrams(&label_folded);
                compare_sorted_bigrams(&label_bigrams, query_bigrams)
            };
            if score > best_score {
                best_score = score;
                if best_score >= match_threshold {
                    break;
                }
            }
        }
        best_score
    }
}

/// Addok Document Scorer implementing the exact Addok scoring formula:
/// 1. Housenumber Matching: Exact coordinate resolution and label integration
/// 2. Label Synthesis: Synthesize full address and street labels with matched housenumber
/// 3. String Distance Score: Multi-label evaluation with tiered autocomplete scoring
/// 4. Importance Score: Document importance * IMPORTANCE_WEIGHT (0.1, ceiling 0.1)
/// 5. Geo-Distance Score: Exponential Gaussian decay km_to_score * GEO_DISTANCE_WEIGHT (0.1, ceiling 0.1)
/// 6. Score Normalization: Ceiling-normalized sum(scores) / sum(ceilings)
/// 7. Score filtering against `min_score`
pub fn score_candidate(
    doc: &Document,
    parsed_query: &ParsedQuery,
    housenumbers_index: &HousenumbersIndex,
    config: &AddokConfig,
    lat: Option<f64>,
    lon: Option<f64>,
    autocomplete: bool,
) -> Option<SearchHit> {
    let has_france = config.has_france();

    // -----------------------------------------------------------------
    // 1. Housenumber Matching
    // -----------------------------------------------------------------
    let mut matched_hn_data: Option<HousenumberData> = None;
    let mut matched_hn_str: Option<String> = None;

    if let Some(ref req_hn) = parsed_query.extracted_housenumber {
        if let Some(hn_data) = housenumbers_index.get_coordinates(&doc.id, req_hn) {
            matched_hn_data = Some(hn_data.clone());
            matched_hn_str = Some(req_hn.clone());
        } else if let Some(doc_hn) = doc.housenumbers.get(req_hn) {
            matched_hn_data = Some(doc_hn.clone());
            matched_hn_str = Some(req_hn.clone());
        } else if req_hn.contains(' ') {
            if let Some((base_hn, _)) = req_hn.split_once(' ') {
                if let Some(hn_data) = housenumbers_index.get_coordinates(&doc.id, base_hn) {
                    matched_hn_data = Some(hn_data.clone());
                    matched_hn_str = Some(base_hn.to_string());
                } else if let Some(doc_hn) = doc.housenumbers.get(base_hn) {
                    matched_hn_data = Some(doc_hn.clone());
                    matched_hn_str = Some(base_hn.to_string());
                }
            }
        }
    }

    // -----------------------------------------------------------------
    // 2. Synthesize Labels loyal to Addok's make_labels
    // -----------------------------------------------------------------
    let labels = doc.make_labels(matched_hn_str.as_deref(), has_france);

    // -----------------------------------------------------------------
    // 3. String Distance Score: multi-label evaluation loyal to Addok
    // -----------------------------------------------------------------
    let str_score_raw = compute_str_score(
        &labels,
        &parsed_query.ascii_query,
        &parsed_query.query_bigrams,
        autocomplete,
        config.match_threshold,
    );

    let str_score = if parsed_query.normalized != parsed_query.raw && str_score_raw < 1.0 {
        let norm_ascii = ascii_fold(&parsed_query.normalized);
        let norm_bigrams = extract_sorted_bigrams(&norm_ascii);
        let str_score_norm = compute_str_score(
            &labels,
            &norm_ascii,
            &norm_bigrams,
            autocomplete,
            config.match_threshold,
        );
        str_score_raw.max(str_score_norm)
    } else {
        str_score_raw
    };

    // -----------------------------------------------------------------
    // 4. Importance Score (ceiling 0.1)
    // -----------------------------------------------------------------
    let importance_score = doc.importance.clamp(0.0, 1.0) * 0.1;

    // -----------------------------------------------------------------
    // 5. Geo-Distance Score (ceiling 0.1)
    // -----------------------------------------------------------------
    let mut distance_meters: Option<f64> = None;
    let mut geo_score: f32 = 0.0;

    if let (Some(plat), Some(plon)) = (lat, lon) {
        let coords = matched_hn_data
            .as_ref()
            .and_then(|hn| match (hn.lat, hn.lon) {
                (Some(la), Some(lo)) => Some((la, lo)),
                _ => None,
            })
            .or_else(|| doc.coordinates(matched_hn_str.as_deref()));

        if let Some((rlat, rlon)) = coords {
            let dist_m = haversine_distance_meters(plat, plon, rlat, rlon);
            distance_meters = Some(dist_m);

            let dist_km = dist_m / 1000.0;
            geo_score = (km_to_score(dist_km) as f32) * 0.1;
        }
    }

    // -----------------------------------------------------------------
    // 6. Ceiling-Normalized Score
    // Without geo: sum(scores) / sum(ceilings) = (str + imp) / (1.0 + 0.1) = (str + imp) / 1.1
    // With geo: (str*0.9 + imp*0.1 + geo) / (1.0 + 0.1 + 0.1) = (str*0.9 + imp*0.1 + geo) / 1.2
    // -----------------------------------------------------------------
    let score = if distance_meters.is_some() {
        (str_score * 0.9 + importance_score * 0.1 + geo_score) / 1.2
    } else {
        (str_score + importance_score) / 1.1
    };

    if score < config.min_score {
        return None;
    }

    // Build the final BanRecord
    let mut rec = doc.to_ban_record(matched_hn_str.as_deref());
    if let Some(ref hn_data) = matched_hn_data {
        if let (Some(la), Some(lo)) = (hn_data.lat, hn_data.lon) {
            rec.lat = Some(la);
            rec.lon = Some(lo);
        }
        if hn_data.x.is_some() {
            rec.x = hn_data.x;
        }
        if hn_data.y.is_some() {
            rec.y = hn_data.y;
        }
    }

    Some(SearchHit {
        record: rec,
        score,
        distance_meters,
        document: Some(doc.clone_metadata()),
        matched_housenumber: matched_hn_str,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_km_to_score_exponential_decay() {
        // Distance 0 km -> 1.0
        assert!((km_to_score(0.0) - 1.0).abs() < 1e-6);

        // Distance 50 km -> exp(-1) ≈ 0.367879
        assert!((km_to_score(50.0) - (-1.0_f64).exp()).abs() < 1e-4);

        // Distance 100 km -> exp(-4) ≈ 0.0183156
        assert!((km_to_score(100.0) - (-4.0_f64).exp()).abs() < 1e-4);

        // Distance > 100 km -> 0.0
        assert_eq!(km_to_score(101.0), 0.0);

        // Negative distance clamped to 1.0
        assert_eq!(km_to_score(-10.0), 1.0);

        // Monotonic decreasing
        assert!(km_to_score(10.0) > km_to_score(30.0));
        assert!(km_to_score(30.0) > km_to_score(80.0));
    }

    #[test]
    fn test_compare_ngrams_similarity() {
        // Exact match
        assert_eq!(compare_ngrams("paris", "paris"), 1.0);

        // Near match
        let sim1 = compare_ngrams("champs elysees", "avenue des champs elysees");
        assert!(sim1 > 0.5);

        // Completely different
        let sim2 = compare_ngrams("paris", "marseille");
        assert!(sim2 < 0.2);
    }

    #[test]
    fn test_levenshtein_ratio() {
        assert_eq!(levenshtein_ratio("paris", "paris"), 1.0);
        let ratio = levenshtein_ratio("michel", "michl");
        assert!(ratio >= 0.8);
    }
}
