use deunicode::deunicode;

/// List of common French address abbreviations and their canonical full-length expansions
/// matching Addok-France's standard synonyms.txt resource table.
const FRENCH_SYNONYMS: &[(&str, &str)] = &[
    // Numerals & Ordinals
    ("100", "cent"),
    ("10e", "dixieme"),
    ("10eme", "dixieme"),
    ("xe", "dixieme"),
    ("xeme", "dixieme"),
    ("11e", "onzieme"),
    ("11eme", "onzieme"),
    ("xie", "onzieme"),
    ("xieme", "onzieme"),
    ("12e", "douzieme"),
    ("12eme", "douzieme"),
    ("xiie", "douzieme"),
    ("xiieme", "douzieme"),
    ("13e", "treizieme"),
    ("13eme", "treizieme"),
    ("xiiie", "treizieme"),
    ("xiiieme", "treizieme"),
    ("14e", "quatorzieme"),
    ("14eme", "quatorzieme"),
    ("xive", "quatorzieme"),
    ("xiveme", "quatorzieme"),
    ("15e", "quinzieme"),
    ("15eme", "quinzieme"),
    ("xve", "quinzieme"),
    ("xveme", "quinzieme"),
    ("16e", "seizieme"),
    ("16eme", "seizieme"),
    ("xvie", "seizieme"),
    ("xvieme", "seizieme"),
    ("17e", "dix septieme"),
    ("17eme", "dix septieme"),
    ("xviie", "dix septieme"),
    ("xviieme", "dix septieme"),
    ("18e", "dix huitieme"),
    ("18eme", "dix huitieme"),
    ("xviiie", "dix huitieme"),
    ("xviiieme", "dix huitieme"),
    ("19e", "dix neuvieme"),
    ("19eme", "dix neuvieme"),
    ("xixe", "dix neuvieme"),
    ("xixeme", "dix neuvieme"),
    ("1er", "premier"),
    ("1e", "premier"),
    ("ier", "premier"),
    ("1ere", "premiere"),
    ("1re", "premiere"),
    ("iere", "premiere"),
    ("ire", "premiere"),
    ("20e", "vingtieme"),
    ("20eme", "vingtieme"),
    ("xxe", "vingtieme"),
    ("xxeme", "vingtieme"),
    ("2e", "deuxieme"),
    ("2eme", "deuxieme"),
    ("iie", "deuxieme"),
    ("iieme", "deuxieme"),
    ("3e", "troisieme"),
    ("3eme", "troisieme"),
    ("iiie", "troisieme"),
    ("iiieme", "troisieme"),
    ("4e", "quatrieme"),
    ("4eme", "quatrieme"),
    ("ive", "quatrieme"),
    ("iveme", "quatrieme"),
    ("5e", "cinquieme"),
    ("5eme", "cinquieme"),
    ("ve", "cinquieme"),
    ("veme", "cinquieme"),
    ("6e", "sixieme"),
    ("6eme", "sixieme"),
    ("vie", "sixieme"),
    ("vieme", "sixieme"),
    ("7e", "septieme"),
    ("7eme", "septieme"),
    ("viie", "septieme"),
    ("viieme", "septieme"),
    ("8e", "huitieme"),
    ("8eme", "huitieme"),
    ("viiie", "huitieme"),
    ("viiieme", "huitieme"),
    ("9e", "neuvieme"),
    ("9eme", "neuvieme"),
    ("ixe", "neuvieme"),
    ("ixeme", "neuvieme"),
    ("neufieme", "neuvieme"),
    // Thoroughfares (types de voies)
    ("ach", "ancien chemin"),
    ("aer", "aerodrome"),
    ("afn", "afrique du nord"),
    ("agl", "agglomeration"),
    ("all", "allee"),
    ("al", "allee"),
    ("alle", "allee"),
    ("ang", "angle"),
    ("anc", "ancien"),
    ("aut", "autoroute"),
    ("av", "avenue"),
    ("ave", "avenue"),
    ("aven", "avenue"),
    ("bcle", "boucle"),
    ("bd", "boulevard"),
    ("bld", "boulevard"),
    ("bou", "boulevard"),
    ("bould", "boulevard"),
    ("bvd", "boulevard"),
    ("boul", "boulevard"),
    ("ber", "berge"),
    ("brg", "bourg"),
    ("brtl", "bretelle"),
    ("bsn", "bassin"),
    ("car", "carriere"),
    ("caref", "carrefour"),
    ("cd", "chemin departemental"),
    ("cdt", "commandant"),
    ("cf", "chemin forestier"),
    ("ch", "chemin"),
    ("che", "chemin"),
    ("chem", "chemin"),
    ("chev", "chevalier"),
    ("chl", "chalet"),
    ("chp", "champ"),
    ("chps", "champs"),
    ("cha", "chaussee"),
    ("chs", "chaussee"),
    ("chse", "chaussee"),
    ("chss", "chaussee"),
    ("chaus", "chaussee"),
    ("cht", "chateau"),
    ("clef", "cle"),
    ("clefs", "cles"),
    ("clr", "couloir"),
    ("col", "colonel"),
    ("cor", "corniche"),
    ("crnc", "corniche"),
    ("cpg", "camping"),
    ("cpt", "capitaine"),
    ("cr", "chemin rural"),
    ("ci", "cite"),
    ("cit", "cite"),
    ("clo", "clos"),
    ("clos", "clos"),
    ("ctr", "contour"),
    ("cou", "cour"),
    ("crs", "cours"),
    ("crx", "croix"),
    ("ctre", "centre"),
    ("cv", "chemin vicinal"),
    ("dept", "departement"),
    ("devi", "deviation"),
    ("dig", "digue"),
    ("dom", "domaine"),
    ("dr", "docteur"),
    ("doct", "docteur"),
    ("doc", "docteur"),
    ("dra", "draille"),
    ("dre", "dreve"),
    ("dsc", "descente"),
    ("ecl", "ecluse"),
    ("embr", "embranchement"),
    ("enc", "enclos"),
    ("enf", "enfants"),
    ("enfts", "enfants"),
    ("entr", "entree"),
    ("espa", "espace"),
    ("espl", "esplanade"),
    ("etng", "etang"),
    ("fac", "facade"),
    ("fg", "faubourg"),
    ("fbg", "faubourg"),
    ("fbourg", "faubourg"),
    ("fon", "fontaine"),
    ("font", "fontaine"),
    ("for", "foret"),
    ("frm", "ferme"),
    ("gale", "galerie"),
    ("gbd", "grand boulevard"),
    ("gde", "grande"),
    ("gdes", "grandes"),
    ("gds", "grands"),
    ("gen", "general"),
    ("gal", "general"),
    ("gl", "general"),
    ("gir", "giratoire"),
    ("gpl", "grand place"),
    ("gr", "grande rue"),
    ("grp", "groupe"),
    ("grpe", "groupe"),
    ("hab", "habitation"),
    ("habit", "habitation"),
    ("ham", "hameau"),
    ("hle", "halle"),
    ("hlg", "halage"),
    ("hlm", "habitation a loyer modere"),
    ("hop", "hopital"),
    ("hosp", "hopital"),
    ("ht", "haut"),
    ("hte", "haute"),
    ("htes", "hautes"),
    ("hts", "hauts"),
    ("i", "un"),
    ("ii", "deux"),
    ("iii", "trois"),
    ("imp", "impasse"),
    ("impa", "impasse"),
    ("ings", "ingenieurs"),
    ("iv", "quatre"),
    ("ix", "neuf"),
    ("jard", "jardin"),
    ("jar", "jardin"),
    ("jte", "jetee"),
    ("lnt", "lieutenant"),
    ("lnant", "lieutenant"),
    ("lieut", "lieutenant"),
    ("lt", "lieutenant"),
    ("lot", "lotissement"),
    ("lotis", "lotissement"),
    ("lotiss", "lotissement"),
    ("lyc", "lycee"),
    ("mal", "marechal"),
    ("ml", "marechal"),
    ("mar", "marechal"),
    ("mais", "maison"),
    ("march", "marche"),
    ("mln", "moulin"),
    ("mme", "madame"),
    ("mgr", "monseigneur"),
    ("mqs", "marquis"),
    ("mqse", "marquise"),
    ("mrn", "marina"),
    ("mt", "mont"),
    ("mte", "montee"),
    ("mtgne", "montagne"),
    ("nd", "notre dame"),
    ("nouv", "nouveau"),
    ("nte", "nouvelle route"),
    ("nve", "neuve"),
    ("pae", "petite avenue"),
    ("pass", "passage"),
    ("pge", "passage"),
    ("psge", "passage"),
    ("pas", "passage"),
    ("pch", "petit chemin"),
    ("pdt", "president"),
    ("pres", "president"),
    ("periph", "peripherique"),
    ("pkg", "parking"),
    ("pl", "place"),
    ("pla", "place"),
    ("plac", "place"),
    ("plc", "place"),
    ("ple", "passerelle"),
    ("pln", "plaine"),
    ("plt", "plateau"),
    ("pnt", "pointe"),
    ("porq", "portique"),
    ("pr", "pere"),
    ("pref", "prefecture"),
    ("prl", "parc residentiel de loisirs"),
    ("prof", "professeur"),
    ("prom", "promenade"),
    ("prm", "promenade"),
    ("pta", "petite allee"),
    ("pte", "porte"),
    ("ptr", "petite rue"),
    ("ptte", "placette"),
    ("pvr", "parvis"),
    ("pai", "parvis"),
    ("prv", "parvis"),
    ("qrt", "quartier"),
    ("qua", "quartier"),
    ("quart", "quartier"),
    ("qu", "quai"),
    ("qur", "quai"),
    ("q", "quai"),
    ("r", "rue"),
    ("ran", "rangee"),
    ("rav", "ravin"),
    ("rd", "route departementale"),
    ("rdl", "raidillon"),
    ("regt", "regiment"),
    ("renc", "rencontre"),
    ("rep", "republique"),
    ("res", "residence"),
    ("rfg", "refuge"),
    ("rle", "ruelle"),
    ("ruel", "ruelle"),
    ("rul", "ruelle"),
    ("rn", "route nationale"),
    ("rpe", "rampe"),
    ("rpt", "rond point"),
    ("rdpoint", "rond point"),
    ("rop", "rond point"),
    ("rdp", "rond point"),
    ("rondpoint", "rond point"),
    ("rdpt", "rond point"),
    ("rp", "rond point"),
    ("rtd", "rotonde"),
    ("rte", "route"),
    ("rou", "route"),
    ("rt", "route"),
    ("ruet", "ruette"),
    ("ruis", "ruisseau"),
    ("rult", "ruellette"),
    ("rve", "ravine"),
    ("sen", "sentier"),
    ("sent", "sentier"),
    ("sgt", "sergent"),
    ("sq", "square"),
    ("squ", "square"),
    ("ss", "sous"),
    ("st", "saint"),
    ("std", "stade"),
    ("stde", "stade"),
    ("ste", "sainte"),
    ("stes", "saintes"),
    ("stg", "stage"),
    ("sts", "saints"),
    ("tpl", "terre plein"),
    ("trab", "traboule"),
    ("trn", "terrain"),
    ("trt", "tertre"),
    ("tsse", "terrasse"),
    ("tun", "tunnel"),
    ("tunn", "tunnel"),
    ("v", "cinq"),
    ("vall", "vallon"),
    ("valee", "vallee"),
    ("vc", "voie communale"),
    ("vche", "vieux chemin"),
    ("vch", "vieux chemin"),
    ("ven", "venelle"),
    ("vge", "village"),
    ("vi", "six"),
    ("viad", "viaduc"),
    ("vii", "sept"),
    ("viii", "huit"),
    ("vil", "ville"),
    ("vla", "villa"),
    ("vill", "villa"),
    ("vte", "vieille route"),
    ("x", "dix"),
    ("xi", "onze"),
    ("xii", "douze"),
    ("xiii", "treize"),
    ("xiv", "quatorze"),
    ("xix", "dix neuf"),
    ("xv", "quinze"),
    ("xvi", "seize"),
    ("xvii", "dix sept"),
    ("xviii", "dix huit"),
    ("xx", "vingt"),
    ("za", "zone artisanale"),
    ("zac", "zone amenagement concerte"),
    ("zi", "zone industrielle"),
    ("zup", "zone urbaniser en priorite"),
    ("bat", "batiment"),
    ("esc", "escalier"),
    ("etg", "etage"),
    ("apt", "appartement"),
    ("app", "appartement"),
    ("imm", "immeuble"),
];

/// French stop words common in address strings
pub const FRENCH_STOP_WORDS: &[&str] = &[
    "de", "du", "des", "le", "la", "les", "un", "une", "d", "l", "au", "aux", "sur", "sous", "en",
    "et",
];

/// Expands a single token if it matches a known French address abbreviation or synonym.
#[inline]
pub fn expand_abbreviation(token: &str) -> &str {
    for &(abbr, full) in FRENCH_SYNONYMS {
        if token == abbr {
            return full;
        }
    }
    token
}

/// Checks if a token is a standard French address stop word.
#[inline]
pub fn is_stop_word(token: &str) -> bool {
    FRENCH_STOP_WORDS.contains(&token)
}

/// Folds French ordinals: "3bis" -> "3b", "10ter" -> "10t", "2quater" -> "2q".
pub fn fold_ordinal(token: &str) -> String {
    let lower = token.to_lowercase();
    if let Some(num) = lower.strip_suffix("quinquies") {
        if !num.is_empty() && num.chars().all(|c| c.is_ascii_digit()) {
            return format!("{}c", num);
        }
    }
    if let Some(num) = lower.strip_suffix("quater") {
        if !num.is_empty() && num.chars().all(|c| c.is_ascii_digit()) {
            return format!("{}q", num);
        }
    }
    if let Some(num) = lower.strip_suffix("ter") {
        if !num.is_empty() && num.chars().all(|c| c.is_ascii_digit()) {
            return format!("{}t", num);
        }
    }
    if let Some(num) = lower.strip_suffix("bis") {
        if !num.is_empty() && num.chars().all(|c| c.is_ascii_digit()) {
            return format!("{}b", num);
        }
    }
    token.to_string()
}

/// Strips leading zeros from numeric strings: "0003" -> "3" (limit 1-3 digits to avoid altering postcodes)
pub fn remove_leading_zeros(token: &str) -> String {
    if token.len() >= 2
        && token.len() <= 4
        && token.starts_with('0')
        && token.chars().all(|c| c.is_ascii_digit())
    {
        let trimmed = token.trim_start_matches('0');
        if trimmed.is_empty() {
            "0".to_string()
        } else {
            trimmed.to_string()
        }
    } else {
        token.to_string()
    }
}

/// Glues digit tokens with following French ordinal suffixes:
/// "1 er" -> "1er", "2 eme" -> "2eme", etc.
fn glue_ordinals(tokens: &[&str]) -> Vec<String> {
    let mut result = Vec::with_capacity(tokens.len());
    let mut i = 0;
    while i < tokens.len() {
        let tok = tokens[i];
        if i + 1 < tokens.len() && !tok.is_empty() && tok.chars().all(|c| c.is_ascii_digit()) {
            let next_tok = tokens[i + 1];
            if matches!(next_tok, "er" | "ere" | "eme" | "ers" | "eres" | "emes") {
                result.push(format!("{}{}", tok, next_tok));
                i += 2;
                continue;
            }
        }
        result.push(tok.to_string());
        i += 1;
    }
    result
}

/// Cleans a query string matching `addok-france`'s `clean_query` query processor:
/// - Expands `s/` to ` sur ` and `s/s` to ` sous `
/// - Strips `cedex` and following postal numbers
/// - Strips postal box codes: `bp`, `cs`, `tsa`, `cidex`
/// - Strips floor indicators: `1er etage`, `2eme etage`, etc.
/// - Strips leading `lieu-dit` / `lieudit`
pub fn clean_query(input: &str) -> String {
    if input.is_empty() {
        return String::new();
    }

    // 1. Expand s/ and s/s
    let mut text = input.to_string();
    let lower = text.to_lowercase();
    if lower.contains("s/") {
        text = text
            .replace(" s/s ", " sous ")
            .replace(" S/s ", " sous ")
            .replace(" S/S ", " sous ")
            .replace("-s/s-", "-sous-")
            .replace("-S/s-", "-sous-")
            .replace("-S/S-", "-sous-")
            .replace(" s/ ", " sur ")
            .replace(" S/ ", " sur ")
            .replace("-s/-", "-sur-")
            .replace("-S/-", "-sur-");
    }

    // 2. Strip leading lieu-dit / lieudit
    let trimmed = text.trim();
    let lower_trimmed = trimmed.to_lowercase();
    let after_lieudit = if let Some(rest) = lower_trimmed
        .strip_prefix("lieux-dits")
        .or_else(|| lower_trimmed.strip_prefix("lieux dits"))
        .or_else(|| lower_trimmed.strip_prefix("lieu-dit"))
        .or_else(|| lower_trimmed.strip_prefix("lieu dit"))
        .or_else(|| lower_trimmed.strip_prefix("lieudit"))
    {
        let offset = trimmed.len() - rest.len();
        trimmed[offset..].trim_start_matches(|c: char| c.is_whitespace() || c == '-' || c == ':')
    } else {
        trimmed
    };

    // 3. Tokenize for contextual noise filtering (cedex, bp, etage)
    let words: Vec<&str> = after_lieudit.split_whitespace().collect();
    let mut cleaned_words = Vec::with_capacity(words.len());
    let mut i = 0;
    while i < words.len() {
        let w = words[i];
        let w_clean = w
            .trim_matches(|c: char| !c.is_alphanumeric())
            .to_lowercase();

        // Strip "cedex" or "cedex XX"
        if w_clean == "cedex" {
            i += 1;
            // Also skip following number token if any (e.g. "05", "15")
            if i < words.len() && words[i].chars().all(|c| c.is_ascii_digit()) {
                i += 1;
            }
            continue;
        }

        // Strip BP / CS / TSA / CIDEX + numbers (e.g. "BP 123", "CS 40001")
        if matches!(w_clean.as_str(), "bp" | "cs" | "tsa" | "cidex") {
            i += 1;
            // Skip optional "no" or "n°"
            if i < words.len()
                && matches!(words[i].to_lowercase().trim_matches('.'), "no" | "n" | "n°")
            {
                i += 1;
            }
            // Skip following digits
            if i < words.len() && words[i].chars().all(|c| c.is_ascii_digit()) {
                i += 1;
            }
            continue;
        }

        // Strip ordinal + etage (e.g. "1er etage", "2eme etage")
        if i + 1 < words.len() {
            let next_clean = words[i + 1]
                .trim_matches(|c: char| !c.is_alphanumeric())
                .to_lowercase();
            if matches!(next_clean.as_str(), "etage" | "étage")
                && (w_clean.ends_with("er") || w_clean.ends_with("eme") || w_clean.ends_with("ere"))
            {
                i += 2;
                continue;
            }
        }

        cleaned_words.push(w);
        i += 1;
    }

    cleaned_words.join(" ")
}

/// Alphanumerize and ascii-fold a string, matching Addok's `ascii(s) = alphanumerize(unidecode(s.lower()))`.
pub fn ascii_fold(s: &str) -> String {
    let transliterated = deunicode(s);
    let mut cleaned = String::with_capacity(transliterated.len());
    let mut last_was_space = false;
    for ch in transliterated.chars() {
        if ch.is_alphanumeric() {
            for lower_ch in ch.to_lowercase() {
                cleaned.push(lower_ch);
            }
            last_was_space = false;
        } else if !last_was_space {
            cleaned.push(' ');
            last_was_space = true;
        }
    }
    cleaned.trim().to_string()
}

/// Normalizes an address string for indexing and matching.
///
/// If `has_france` is enabled (i.e. `addok_france` plugin is active):
/// - Expands `s/` -> `sur` and `s/s` -> `sous`.
/// - Strips diacritics / accents using `deunicode`.
/// - Converts to lowercase.
/// - Replaces non-alphanumeric punctuation with spaces.
/// - Expands French abbreviations (`bd` -> `boulevard`, `av` -> `avenue`, etc.).
/// - Glues ordinal suffixes (`1 er` -> `1er`, etc.).
///
/// If `has_france` is disabled (generic Addok core):
/// - Strips diacritics / accents using `deunicode`.
/// - Converts to lowercase.
/// - Replaces non-alphanumeric punctuation with spaces.
/// - Tokenizes by whitespace without French-specific expansions or ordinal gluing.
pub fn normalize_address(input: &str, has_france: bool) -> String {
    if input.is_empty() {
        return String::new();
    }

    let mut text = input.to_string();
    if has_france {
        let lower = text.to_lowercase();
        if lower.contains("s/") {
            text = text
                .replace(" s/s ", " sous ")
                .replace(" S/s ", " sous ")
                .replace(" S/S ", " sous ")
                .replace("-s/s-", "-sous-")
                .replace("-S/s-", "-sous-")
                .replace("-S/S-", "-sous-")
                .replace(" s/ ", " sur ")
                .replace(" S/ ", " sur ")
                .replace("-s/-", "-sur-")
                .replace("-S/-", "-sur-");
        }
    }

    // Step 1: Remove accents and diacritics
    let transliterated = deunicode(&text);

    // Step 2 & 3: Lowercase and replace non-alphanumeric punctuation with spaces
    let mut cleaned = String::with_capacity(transliterated.len());
    for ch in transliterated.chars() {
        if ch.is_alphanumeric() {
            for lower_ch in ch.to_lowercase() {
                cleaned.push(lower_ch);
            }
        } else {
            cleaned.push(' ');
        }
    }

    if has_france {
        // Step 4 & 5: Tokenize and expand abbreviations
        let tokens = cleaned
            .split_whitespace()
            .map(expand_abbreviation)
            .collect::<Vec<_>>();

        // Step 6: Glue ordinals
        let glued = glue_ordinals(&tokens);

        // Step 7: Join
        glued.join(" ")
    } else {
        cleaned.split_whitespace().collect::<Vec<_>>().join(" ")
    }
}

/// Extracts clean tokens, optionally filtering out stop words.
///
/// If `has_france` is enabled, French stop words are filtered out when requested.
/// If `has_france` is disabled, stop words are retained.
pub fn tokenize_address(input: &str, filter_stop_words: bool, has_france: bool) -> Vec<String> {
    let normalized = normalize_address(input, has_france);
    normalized
        .split_whitespace()
        .filter(|token| !(filter_stop_words && has_france && is_stop_word(token)))
        .map(|s| s.to_string())
        .collect()
}

/// Normalizes a French address string for robust indexing and matching (Addok + `addok-france`).
#[inline]
pub fn normalize_french_address(input: &str) -> String {
    normalize_address(input, true)
}

/// Extracts clean tokens for French addresses, optionally filtering out French stop words.
#[inline]
pub fn tokenize_french_address(input: &str, filter_stop_words: bool) -> Vec<String> {
    tokenize_address(input, filter_stop_words, true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bd_st_michel() {
        let input = "8 bd st michel";
        let expected = "8 boulevard saint michel";
        assert_eq!(normalize_french_address(input), expected);
    }

    #[test]
    fn test_avenue_des_champs_elysees() {
        let input = "avenue des champs elysees";
        let expected = "avenue des champs elysees";
        assert_eq!(normalize_french_address(input), expected);

        let input_with_accents = "Avenue des Champs-Élysées";
        assert_eq!(normalize_french_address(input_with_accents), expected);
    }

    #[test]
    fn test_rue_de_leglise() {
        let input = "rue de l'eglise";
        let expected = "rue de l eglise";
        assert_eq!(normalize_french_address(input), expected);

        let input_with_accents = "Rue de l'Église";
        assert_eq!(normalize_french_address(input_with_accents), expected);

        // Typographic apostrophe
        let input_typo = "Rue de l’Église";
        assert_eq!(normalize_french_address(input_typo), expected);
    }

    #[test]
    fn test_abbreviations_expansion() {
        assert_eq!(
            normalize_french_address("12 av victor hugo"),
            "12 avenue victor hugo"
        );
        assert_eq!(
            normalize_french_address("5 all des lilas"),
            "5 allee des lilas"
        );
        assert_eq!(
            normalize_french_address("3 imp. du moulin"),
            "3 impasse du moulin"
        );
        assert_eq!(normalize_french_address("r de la paix"), "rue de la paix");
        assert_eq!(
            normalize_french_address("ste genevieve"),
            "sainte genevieve"
        );
        assert_eq!(
            normalize_french_address("pl de la republique"),
            "place de la republique"
        );
        assert_eq!(
            normalize_french_address("14 crs lafayette"),
            "14 cours lafayette"
        );
        assert_eq!(
            normalize_french_address("quai st bernard"),
            "quai saint bernard"
        );
    }

    #[test]
    fn test_no_false_positive_abbreviations_in_words() {
        // "brest" must not be modified into "broulevardest"
        assert_eq!(normalize_french_address("brest"), "brest");
        // "station" must not become "sainttion"
        assert_eq!(normalize_french_address("station"), "station");
        // "poste" must not become "posaintte"
        assert_eq!(normalize_french_address("poste"), "poste");
    }

    #[test]
    fn test_complex_punctuations_and_casing() {
        let input = "  12,   BOULEVARD   SAINT-GERMAIN  --  75005 PARIS !! ";
        let expected = "12 boulevard saint germain 75005 paris";
        assert_eq!(normalize_french_address(input), expected);
    }

    #[test]
    fn test_tokenize_with_and_without_stop_words() {
        let address = "Rue de l'Église";
        let tokens_with_stop = tokenize_french_address(address, false);
        assert_eq!(tokens_with_stop, vec!["rue", "de", "l", "eglise"]);

        let tokens_without_stop = tokenize_french_address(address, true);
        assert_eq!(tokens_without_stop, vec!["rue", "eglise"]);
    }

    #[test]
    fn test_glue_ordinals() {
        assert_eq!(
            normalize_french_address("rue du 1 er mai"),
            "rue du 1er mai"
        );
        assert_eq!(normalize_french_address("2 eme etage"), "2eme etage");
        assert_eq!(normalize_french_address("18 eme siecle"), "18eme siecle");
    }

    #[test]
    fn test_clean_query() {
        assert_eq!(clean_query("8 bd st michel cedex 05"), "8 bd st michel");
        assert_eq!(clean_query("Neuilly s/ Seine"), "Neuilly sur Seine");
        assert_eq!(clean_query("Boulogne s/s Bois"), "Boulogne sous Bois");
        assert_eq!(clean_query("BP 42 10 rue de la paix"), "10 rue de la paix");
        assert_eq!(
            clean_query("lieu-dit les grands champs"),
            "les grands champs"
        );
        assert_eq!(
            clean_query("12 rue victor hugo 2eme etage"),
            "12 rue victor hugo"
        );
    }
}
