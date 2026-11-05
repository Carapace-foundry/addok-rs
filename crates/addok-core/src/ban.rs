use serde::{Deserialize, Serialize};

/// Type of address entry in the French BAN (Base Adresse Nationale)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BanType {
    Housenumber,
    Street,
    Locality,
    Municipality,
}

impl std::fmt::Display for BanType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Housenumber => write!(f, "housenumber"),
            Self::Street => write!(f, "street"),
            Self::Locality => write!(f, "locality"),
            Self::Municipality => write!(f, "municipality"),
        }
    }
}

/// Representation of a French BAN (Base Adresse Nationale) address record.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BanRecord {
    pub id: String,

    #[serde(default, deserialize_with = "empty_string_is_none")]
    pub numero: Option<String>,

    #[serde(default, deserialize_with = "empty_string_is_none")]
    pub rep: Option<String>,

    pub nom_voie: String,

    pub code_postal: String,

    #[serde(default, deserialize_with = "empty_string_is_none")]
    pub code_insee: Option<String>,

    pub nom_commune: String,

    #[serde(default, deserialize_with = "empty_string_is_none")]
    pub code_insee_commune_deleguee: Option<String>,

    #[serde(default, deserialize_with = "empty_string_is_none")]
    pub nom_commune_deleguee: Option<String>,

    #[serde(default, deserialize_with = "option_f64_from_lenient_str")]
    pub x: Option<f64>,

    #[serde(default, deserialize_with = "option_f64_from_lenient_str")]
    pub y: Option<f64>,

    #[serde(default, deserialize_with = "option_f64_from_lenient_str")]
    pub lon: Option<f64>,

    #[serde(default, deserialize_with = "option_f64_from_lenient_str")]
    pub lat: Option<f64>,

    #[serde(default, deserialize_with = "empty_string_is_none")]
    pub type_position: Option<String>,

    #[serde(default, deserialize_with = "empty_string_is_none")]
    pub alias: Option<String>,

    #[serde(default, deserialize_with = "empty_string_is_none")]
    pub nom_afnor: Option<String>,

    #[serde(default, deserialize_with = "empty_string_is_none")]
    pub libelle_acheminement: Option<String>,

    #[serde(default, deserialize_with = "empty_string_is_none")]
    pub nom_complementaire: Option<String>,

    #[serde(default, rename = "type", skip_serializing_if = "Option::is_none")]
    pub address_type: Option<String>,
}

impl BanRecord {
    /// Return the address type based on presence of house number, explicit address_type, or street/commune names
    pub fn ban_type(&self) -> BanType {
        if self.numero.as_ref().is_some_and(|n| !n.trim().is_empty()) {
            return BanType::Housenumber;
        }
        if let Some(ref t) = self.address_type {
            match t.to_lowercase().as_str() {
                "municipality" | "commune" => return BanType::Municipality,
                "locality" | "lieu-dit" | "lieudit" => return BanType::Locality,
                "street" | "voie" => return BanType::Street,
                "housenumber" => return BanType::Housenumber,
                _ => {}
            }
        }
        if self.nom_voie.trim().is_empty()
            || self
                .nom_voie
                .trim()
                .eq_ignore_ascii_case(self.nom_commune.trim())
        {
            BanType::Municipality
        } else {
            BanType::Street
        }
    }

    /// Complete house number with repetition (e.g. "8", "8 bis", "12 B")
    pub fn full_housenumber(&self) -> Option<String> {
        let num = self.numero.as_deref()?.trim();
        if num.is_empty() {
            return None;
        }

        match self.rep.as_deref().map(str::trim).filter(|r| !r.is_empty()) {
            Some(rep) => Some(format!("{} {}", num, rep)),
            None => Some(num.to_string()),
        }
    }

    /// Full street and number string (e.g. "8 bis Boulevard Saint-Michel")
    pub fn street_and_number(&self) -> String {
        if let Some(hn) = self.full_housenumber() {
            format!("{} {}", hn, self.nom_voie.trim())
        } else {
            self.nom_voie.trim().to_string()
        }
    }

    /// Standard single-line address label (e.g. "8 Boulevard Saint-Michel 75005 Paris")
    pub fn label(&self) -> String {
        let street_part = self.street_and_number();
        let postal_city = format!("{} {}", self.code_postal.trim(), self.nom_commune.trim());

        if street_part.is_empty() {
            postal_city
        } else {
            format!("{} {}", street_part, postal_city)
        }
    }
}

/// Helper function to deserialize empty or whitespace strings as None
fn empty_string_is_none<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let opt = Option::<String>::deserialize(deserializer)?;
    Ok(opt.and_then(|s| {
        let trimmed = s.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    }))
}

/// Helper function to parse f64 from string or numeric, handling commas and empty values
fn option_f64_from_lenient_str<'de, D>(deserializer: D) -> Result<Option<f64>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de::Error;
    let value = Option::<serde_json::Value>::deserialize(deserializer)?;
    match value {
        None | Some(serde_json::Value::Null) => Ok(None),
        Some(serde_json::Value::Number(n)) => Ok(n.as_f64()),
        Some(serde_json::Value::String(s)) => {
            let s = s.trim().replace(',', ".");
            if s.is_empty() {
                Ok(None)
            } else {
                s.parse::<f64>().map(Some).map_err(D::Error::custom)
            }
        }
        _ => Err(D::Error::custom("expected float or string")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ban_record_formatting() {
        let record = BanRecord {
            id: "75105_0001_00008".to_string(),
            address_type: None,
            numero: Some("8".to_string()),
            rep: Some("bis".to_string()),
            nom_voie: "Boulevard Saint-Michel".to_string(),
            code_postal: "75005".to_string(),
            code_insee: Some("75105".to_string()),
            nom_commune: "Paris".to_string(),
            code_insee_commune_deleguee: None,
            nom_commune_deleguee: None,
            x: Some(652100.0),
            y: Some(6861500.0),
            lon: Some(2.3431),
            lat: Some(48.8512),
            type_position: Some("entrée".to_string()),
            alias: None,
            nom_afnor: Some("BD SAINT MICHEL".to_string()),
            libelle_acheminement: Some("PARIS".to_string()),
            nom_complementaire: None,
        };

        assert_eq!(record.ban_type(), BanType::Housenumber);
        assert_eq!(record.full_housenumber().as_deref(), Some("8 bis"));
        assert_eq!(record.street_and_number(), "8 bis Boulevard Saint-Michel");
        assert_eq!(record.label(), "8 bis Boulevard Saint-Michel 75005 Paris");
    }

    #[test]
    fn test_ban_type_and_defaults_derivation() {
        // 1. Explicit address types
        let mut rec = BanRecord {
            id: "mun_1".to_string(),
            address_type: Some("municipality".to_string()),
            numero: None,
            rep: None,
            nom_voie: String::new(),
            code_postal: "75001".to_string(),
            code_insee: Some("75101".to_string()),
            nom_commune: "Paris".to_string(),
            code_insee_commune_deleguee: None,
            nom_commune_deleguee: None,
            x: None,
            y: None,
            lon: None,
            lat: None,
            type_position: None,
            alias: None,
            nom_afnor: None,
            libelle_acheminement: None,
            nom_complementaire: None,
        };
        assert_eq!(rec.ban_type(), BanType::Municipality);

        rec.address_type = Some("locality".to_string());
        assert_eq!(rec.ban_type(), BanType::Locality);

        rec.address_type = Some("street".to_string());
        assert_eq!(rec.ban_type(), BanType::Street);

        rec.address_type = Some("housenumber".to_string());
        assert_eq!(rec.ban_type(), BanType::Housenumber);

        // 2. Infer Municipality when nom_voie is empty or equals nom_commune
        rec.address_type = None;
        rec.nom_voie = "  ".to_string();
        assert_eq!(rec.ban_type(), BanType::Municipality);

        rec.nom_voie = "Paris".to_string();
        assert_eq!(rec.ban_type(), BanType::Municipality);

        rec.nom_voie = "Rue de la Paix".to_string();
        assert_eq!(rec.ban_type(), BanType::Street);

        // 3. full_housenumber with blank numero or whitespace
        rec.numero = Some("   ".to_string());
        assert_eq!(rec.full_housenumber(), None);

        rec.numero = Some("10".to_string());
        rec.rep = None;
        assert_eq!(rec.full_housenumber().as_deref(), Some("10"));

        rec.rep = Some("ter".to_string());
        assert_eq!(rec.full_housenumber().as_deref(), Some("10 ter"));

        // 4. street_and_number & label when street part is empty
        rec.numero = None;
        rec.rep = None;
        rec.nom_voie = String::new();
        assert_eq!(rec.street_and_number(), "");
        assert_eq!(rec.label(), "75001 Paris");
    }

    #[test]
    fn test_lenient_json_parsing_and_deserialization() {
        // String with comma, null, empty string
        let raw_json = r#"{
            "id": "75001_01",
            "nom_voie": "Rue de Rivoli",
            "code_postal": "75001",
            "nom_commune": "Paris",
            "lat": "48,8566",
            "lon": 2.3522,
            "x": null,
            "y": "",
            "numero": "  "
        }"#;

        let record: BanRecord =
            serde_json::from_str(raw_json).expect("Failed to parse lenient JSON");
        assert_eq!(record.lat, Some(48.8566));
        assert_eq!(record.lon, Some(2.3522));
        assert_eq!(record.x, None);
        assert_eq!(record.y, None);
        assert_eq!(record.numero, None); // empty string parsed as None
    }
}
