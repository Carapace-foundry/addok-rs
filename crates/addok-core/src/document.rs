use flate2::read::GzDecoder;
use serde::{Deserialize, Deserializer, Serialize};
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::Path;

use crate::ban::BanRecord;
use crate::error::{CoreError, Result};

/// Geographic coordinates and metadata for a specific house number.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct HousenumberData {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "option_f64_from_lenient_value"
    )]
    pub lat: Option<f64>,

    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "option_f64_from_lenient_value"
    )]
    pub lon: Option<f64>,

    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "option_f64_from_lenient_value"
    )]
    pub x: Option<f64>,

    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "option_f64_from_lenient_value"
    )]
    pub y: Option<f64>,

    #[serde(flatten, default)]
    pub extra: HashMap<String, serde_json::Value>,
}

impl HousenumberData {
    pub fn new(lat: Option<f64>, lon: Option<f64>) -> Self {
        Self {
            lat,
            lon,
            x: None,
            y: None,
            extra: HashMap::new(),
        }
    }

    pub fn with_coords(lat: f64, lon: f64) -> Self {
        Self {
            lat: Some(lat),
            lon: Some(lon),
            x: None,
            y: None,
            extra: HashMap::new(),
        }
    }

    pub fn with_projected(mut self, x: f64, y: f64) -> Self {
        self.x = Some(x);
        self.y = Some(y);
        self
    }
}

/// Generic address document representing a searchable entity (street, locality, municipality).
///
/// In Addok, house numbers are grouped under their parent street document in the `housenumbers` map,
/// drastically reducing redundancy and index size.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Document {
    pub id: String,

    pub name: String,

    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub housenumbers: HashMap<String, HousenumberData>,

    #[serde(
        default,
        rename = "type",
        alias = "address_type",
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient_option_string"
    )]
    pub address_type: Option<String>,

    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient_option_string"
    )]
    pub postcode: Option<String>,

    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient_option_string"
    )]
    pub citycode: Option<String>,

    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient_option_string"
    )]
    pub city: Option<String>,

    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient_option_string"
    )]
    pub context: Option<String>,

    #[serde(default)]
    pub importance: f32,

    #[serde(flatten, default)]
    pub extra: HashMap<String, serde_json::Value>,
}

impl Document {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            housenumbers: HashMap::new(),
            address_type: None,
            postcode: None,
            citycode: None,
            city: None,
            context: None,
            importance: 0.0,
            extra: HashMap::new(),
        }
    }

    pub fn with_type(mut self, address_type: impl Into<String>) -> Self {
        self.address_type = Some(address_type.into());
        self
    }

    pub fn with_postcode(mut self, postcode: impl Into<String>) -> Self {
        self.postcode = Some(postcode.into());
        self
    }

    pub fn with_citycode(mut self, citycode: impl Into<String>) -> Self {
        self.citycode = Some(citycode.into());
        self
    }

    pub fn with_city(mut self, city: impl Into<String>) -> Self {
        self.city = Some(city.into());
        self
    }

    pub fn with_context(mut self, context: impl Into<String>) -> Self {
        self.context = Some(context.into());
        self
    }

    pub fn with_importance(mut self, importance: f32) -> Self {
        self.importance = importance;
        self
    }

    pub fn with_housenumbers(mut self, housenumbers: HashMap<String, HousenumberData>) -> Self {
        self.housenumbers = housenumbers;
        self
    }

    /// Add a house number to this document.
    pub fn add_housenumber(&mut self, number: impl Into<String>, data: HousenumberData) {
        self.housenumbers.insert(number.into(), data);
    }

    /// Retrieve house number coordinates if available.
    pub fn get_housenumber(&self, number: &str) -> Option<&HousenumberData> {
        self.housenumbers.get(number)
    }

    /// Best effort coordinates for this document (e.g. from a specific house number, or top-level extra lat/lon, or centroid).
    pub fn coordinates(&self, housenumber: Option<&str>) -> Option<(f64, f64)> {
        if let Some(hn) = housenumber {
            if let Some(data) = self.housenumbers.get(hn) {
                if let (Some(lat), Some(lon)) = (data.lat, data.lon) {
                    return Some((lat, lon));
                }
            }
        }

        // Try top-level extra lat/lon
        if let (Some(lat_val), Some(lon_val)) = (self.extra.get("lat"), self.extra.get("lon")) {
            if let (Some(lat), Some(lon)) = (lat_val.as_f64(), lon_val.as_f64()) {
                return Some((lat, lon));
            }
        }

        // Compute centroid from housenumbers if any
        if !self.housenumbers.is_empty() {
            let mut sum_lat = 0.0;
            let mut sum_lon = 0.0;
            let mut count = 0;
            for data in self.housenumbers.values() {
                if let (Some(lat), Some(lon)) = (data.lat, data.lon) {
                    sum_lat += lat;
                    sum_lon += lon;
                    count += 1;
                }
            }
            if count > 0 {
                return Some((sum_lat / count as f64, sum_lon / count as f64));
            }
        }

        None
    }

    /// Build a single-line label for this document with an optional house number.
    pub fn label(&self, housenumber: Option<&str>) -> String {
        let street = match housenumber {
            Some(hn) if !hn.trim().is_empty() => format!("{} {}", hn.trim(), self.name.trim()),
            _ => self.name.trim().to_string(),
        };

        let mut parts = Vec::new();
        if !street.is_empty() {
            parts.push(street);
        }
        if let Some(pc) = self.postcode.as_deref().filter(|s| !s.trim().is_empty()) {
            parts.push(pc.trim().to_string());
        }
        if let Some(c) = self
            .city
            .as_deref()
            .filter(|s| !s.trim().is_empty() && *s != self.name)
        {
            parts.push(c.trim().to_string());
        }

        parts.join(" ")
    }

    /// Synthesizes address search labels loyal to Addok and Addok-France's `make_labels`.
    /// The primary label is guaranteed to be `labels[0]`.
    pub fn make_labels(&self, housenumber: Option<&str>, has_france: bool) -> Vec<String> {
        let hn = housenumber.map(str::trim).filter(|s| !s.is_empty());

        if !has_france {
            let mut base_label = self.name.trim().to_string();
            if let Some(ref c) = self.city {
                let c_trimmed = c.trim();
                if c_trimmed != self.name.trim() {
                    if let Some(ref pc) = self.postcode {
                        base_label = format!("{} {} {}", base_label, pc.trim(), c_trimmed);
                    } else {
                        base_label = format!("{} {}", base_label, c_trimmed);
                    }
                }
            }
            let primary_label = if let Some(h) = hn {
                format!("{} {}", h, base_label)
            } else {
                base_label
            };
            return vec![primary_label];
        }

        let mut labels = Vec::new();
        let add = |labels_vec: &mut Vec<String>, lbl: &str| {
            labels_vec.insert(0, lbl.to_string());
            if let Some(h) = hn {
                labels_vec.insert(0, format!("{} {}", h, lbl));
            }
        };

        let is_municipality = self.address_type.as_deref().is_some_and(|t| {
            t.eq_ignore_ascii_case("municipality") || t.eq_ignore_ascii_case("commune")
        }) || (self.city.as_deref().map(str::trim) == Some(self.name.trim())
            && !self.name.trim().is_empty());

        let name = self.name.trim();
        let city = self.city.as_deref().map(str::trim);
        let postcode = self.postcode.as_deref().map(str::trim);

        if let Some(pc) = postcode {
            if is_municipality {
                add(&mut labels, &format!("{} {}", name, pc));
                add(&mut labels, &format!("{} {}", pc, name));
            }
        }

        add(&mut labels, name);

        if let Some(c) = city {
            if c != name {
                add(&mut labels, &format!("{} {}", name, c));
                if let Some(pc) = postcode {
                    let mut current = format!("{} {}", name, pc);
                    add(&mut labels, &current);
                    current = format!("{} {}", current, c);
                    add(&mut labels, &current);
                }
            }
        }

        labels
    }

    /// Create a clone of the document metadata without copying the large `housenumbers` map.
    pub fn clone_metadata(&self) -> Self {
        Self {
            id: self.id.clone(),
            name: self.name.clone(),
            housenumbers: HashMap::new(),
            address_type: self.address_type.clone(),
            postcode: self.postcode.clone(),
            citycode: self.citycode.clone(),
            city: self.city.clone(),
            context: self.context.clone(),
            importance: self.importance,
            extra: self.extra.clone(),
        }
    }

    /// Convert this document (with optional matched house number) into a BanRecord for GeoJSON output and backward-compatibility.
    pub fn to_ban_record(&self, housenumber: Option<&str>) -> BanRecord {
        let (num, rep) = match housenumber {
            Some(hn) => {
                let trimmed = hn.trim();
                let mut parts = trimmed.splitn(2, ' ');
                let n = parts.next().map(|s| s.to_string());
                let r = parts.next().map(|s| s.to_string());
                (n, r)
            }
            None => (None, None),
        };

        let coords = self.coordinates(housenumber);

        let hn_data_opt = housenumber.and_then(|hn| {
            let trimmed = hn.trim();
            self.housenumbers.get(trimmed).or_else(|| {
                let compact: String = trimmed.chars().filter(|c| !c.is_whitespace()).collect();
                self.housenumbers.get(&compact).or_else(|| {
                    let digit_end = trimmed.chars().take_while(|c| c.is_ascii_digit()).count();
                    if digit_end > 0 && digit_end < trimmed.len() {
                        let spaced =
                            format!("{} {}", &trimmed[..digit_end], trimmed[digit_end..].trim());
                        self.housenumbers.get(&spaced)
                    } else {
                        None
                    }
                })
            })
        });

        let id = match housenumber {
            Some(hn) => {
                let trimmed = hn.trim();
                if let Some(hn_data) = hn_data_opt {
                    if let Some(serde_json::Value::String(orig_id)) = hn_data.extra.get("id") {
                        orig_id.clone()
                    } else if !trimmed.is_empty() {
                        format!("{}_{}", self.id, trimmed.replace(' ', "_"))
                    } else {
                        self.id.clone()
                    }
                } else if !trimmed.is_empty() {
                    format!("{}_{}", self.id, trimmed.replace(' ', "_"))
                } else {
                    self.id.clone()
                }
            }
            _ => self.id.clone(),
        };

        let x = match housenumber {
            Some(_) => hn_data_opt.and_then(|h| h.x),
            None => self.extra.get("x").and_then(|v| v.as_f64()),
        };

        let y = match housenumber {
            Some(_) => hn_data_opt.and_then(|h| h.y),
            None => self.extra.get("y").and_then(|v| v.as_f64()),
        };

        BanRecord {
            id,
            numero: num,
            rep,
            nom_voie: self.name.clone(),
            code_postal: self.postcode.clone().unwrap_or_default(),
            code_insee: self.citycode.clone(),
            nom_commune: self.city.clone().unwrap_or_default(),
            code_insee_commune_deleguee: None,
            nom_commune_deleguee: None,
            x,
            y,
            lon: coords.map(|c| c.1),
            lat: coords.map(|c| c.0),
            type_position: None,
            alias: None,
            nom_afnor: None,
            libelle_acheminement: None,
            nom_complementaire: None,
            address_type: if housenumber.is_some() {
                Some("housenumber".to_string())
            } else {
                self.address_type.clone()
            },
        }
    }
}

/// Helper function to parse f64 leniently from float, int, string or null.
fn option_f64_from_lenient_value<'de, D>(
    deserializer: D,
) -> std::result::Result<Option<f64>, D::Error>
where
    D: Deserializer<'de>,
{
    use serde::de::Error;
    let opt = Option::<serde_json::Value>::deserialize(deserializer)?;
    match opt {
        None | Some(serde_json::Value::Null) => Ok(None),
        Some(serde_json::Value::Number(n)) => Ok(n.as_f64()),
        Some(serde_json::Value::String(s)) => {
            let trimmed = s.trim().replace(',', ".");
            if trimmed.is_empty() {
                Ok(None)
            } else {
                trimmed.parse::<f64>().map(Some).map_err(D::Error::custom)
            }
        }
        _ => Err(D::Error::custom("expected float or string")),
    }
}

/// Helper function to parse Option<String> leniently from string, number, or array.
fn lenient_option_string<'de, D>(deserializer: D) -> std::result::Result<Option<String>, D::Error>
where
    D: Deserializer<'de>,
{
    let opt = Option::<serde_json::Value>::deserialize(deserializer)?;
    match opt {
        None | Some(serde_json::Value::Null) => Ok(None),
        Some(serde_json::Value::String(s)) => {
            let trimmed = s.trim();
            if trimmed.is_empty() {
                Ok(None)
            } else {
                Ok(Some(trimmed.to_string()))
            }
        }
        Some(serde_json::Value::Number(n)) => Ok(Some(n.to_string())),
        Some(serde_json::Value::Array(arr)) => {
            let non_empty: Vec<String> = arr
                .into_iter()
                .filter_map(|v| match v {
                    serde_json::Value::String(s) => {
                        let t = s.trim();
                        if t.is_empty() {
                            None
                        } else {
                            Some(t.to_string())
                        }
                    }
                    serde_json::Value::Number(n) => Some(n.to_string()),
                    _ => None,
                })
                .collect();
            if non_empty.is_empty() {
                Ok(None)
            } else {
                Ok(Some(non_empty[0].clone()))
            }
        }
        _ => Ok(None),
    }
}

/// Streaming reader that yields `Document` items line-by-line from an NDJSON source.
pub struct DocumentStream<R: BufRead> {
    reader: R,
    line_buf: String,
    line_number: usize,
}

impl<R: BufRead> DocumentStream<R> {
    pub fn new(reader: R) -> Self {
        Self {
            reader,
            line_buf: String::new(),
            line_number: 0,
        }
    }
}

impl<R: BufRead> Iterator for DocumentStream<R> {
    type Item = Result<Document>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            self.line_buf.clear();
            self.line_number += 1;
            match self.reader.read_line(&mut self.line_buf) {
                Ok(0) => return None, // EOF
                Ok(_) => {
                    let trimmed = self.line_buf.trim();
                    if trimmed.is_empty() {
                        continue;
                    }
                    match serde_json::from_str::<Document>(trimmed) {
                        Ok(doc) => return Some(Ok(doc)),
                        Err(e) => {
                            return Some(Err(CoreError::InvalidDocument {
                                line: self.line_number,
                                reason: e.to_string(),
                            }));
                        }
                    }
                }
                Err(e) => return Some(Err(CoreError::Io(e))),
            }
        }
    }
}

/// Open an NDJSON streaming reader from any `Read` source (e.g. file, pipe, stdin, in-memory buffer).
pub fn open_ndjson_reader<R: Read>(reader: R) -> DocumentStream<BufReader<R>> {
    DocumentStream::new(BufReader::new(reader))
}

/// Open an NDJSON file with automatic transparent gzip decompression if needed.
pub fn open_ndjson_file<P: AsRef<Path>>(
    path: P,
) -> Result<DocumentStream<Box<dyn BufRead + Send>>> {
    use std::io::Seek;
    let path_ref = path.as_ref();
    let mut file = File::open(path_ref)?;

    let mut magic = [0u8; 2];
    let is_gzip = match file.read_exact(&mut magic) {
        Ok(()) => magic == [0x1f, 0x8b],
        Err(_) => false,
    };

    file.seek(std::io::SeekFrom::Start(0))?;

    let buf_reader: Box<dyn BufRead + Send> = if is_gzip {
        Box::new(BufReader::new(GzDecoder::new(file)))
    } else {
        Box::new(BufReader::new(file))
    };

    Ok(DocumentStream::new(buf_reader))
}

/// Write a collection of documents to an NDJSON output writer.
pub fn write_ndjson<W: Write, I: IntoIterator<Item = Document>>(
    writer: &mut W,
    docs: I,
) -> Result<()> {
    for doc in docs {
        let json_line = serde_json::to_string(&doc)?;
        writer.write_all(json_line.as_bytes())?;
        writer.write_all(b"\n")?;
    }
    writer.flush()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_document_roundtrip_json() {
        let mut housenumbers = HashMap::new();
        housenumbers.insert(
            "8".to_string(),
            HousenumberData {
                lat: Some(48.8512),
                lon: Some(2.3431),
                x: Some(652100.0),
                y: Some(6861500.0),
                extra: HashMap::new(),
            },
        );

        let mut doc = Document::new("75105_0001", "Boulevard Saint-Michel")
            .with_type("street")
            .with_postcode("75005")
            .with_citycode("75105")
            .with_city("Paris")
            .with_context("75, Paris, Île-de-France")
            .with_importance(0.4)
            .with_housenumbers(housenumbers);

        doc.extra
            .insert("custom_key".to_string(), serde_json::json!("custom_value"));

        let serialized = serde_json::to_string(&doc).unwrap();
        assert!(serialized.contains("\"type\":\"street\""));
        assert!(serialized.contains("\"custom_key\":\"custom_value\""));

        let deserialized: Document = serde_json::from_str(&serialized).unwrap();
        assert_eq!(deserialized.id, "75105_0001");
        assert_eq!(deserialized.name, "Boulevard Saint-Michel");
        assert_eq!(deserialized.address_type.as_deref(), Some("street"));
        assert_eq!(deserialized.postcode.as_deref(), Some("75005"));
        assert_eq!(deserialized.housenumbers.len(), 1);
        assert_eq!(
            deserialized.housenumbers.get("8").unwrap().lat,
            Some(48.8512)
        );
        assert_eq!(
            deserialized.extra.get("custom_key").unwrap(),
            &serde_json::json!("custom_value")
        );
    }

    #[test]
    fn test_open_ndjson_reader() {
        let sample_ndjson = r#"{"id":"1","name":"Rue de Rivoli","type":"street","postcode":"75004","city":"Paris"}
{"id":"2","name":"Boulevard Saint-Germain","type":"street","postcode":"75005","city":"Paris","housenumbers":{"1":{"lat":48.852,"lon":2.346}}}
"#;
        let reader = open_ndjson_reader(sample_ndjson.as_bytes());
        let docs: Vec<Document> = reader.collect::<Result<Vec<_>>>().unwrap();

        assert_eq!(docs.len(), 2);
        assert_eq!(docs[0].id, "1");
        assert_eq!(docs[0].name, "Rue de Rivoli");
        assert_eq!(docs[1].housenumbers.len(), 1);
        assert_eq!(docs[1].housenumbers.get("1").unwrap().lat, Some(48.852));
    }

    #[test]
    fn test_make_labels_fidelity_with_addok_france() {
        // Matches official addok-france test: test_make_labels
        let doc = Document::new("xxxx", "rue des Lilas")
            .with_type("street")
            .with_city("Paris")
            .with_postcode("75010");

        let labels = doc.make_labels(Some("1 bis"), true);
        assert_eq!(
            labels,
            vec![
                "1 bis rue des Lilas 75010 Paris",
                "rue des Lilas 75010 Paris",
                "1 bis rue des Lilas 75010",
                "rue des Lilas 75010",
                "1 bis rue des Lilas Paris",
                "rue des Lilas Paris",
                "1 bis rue des Lilas",
                "rue des Lilas"
            ]
        );

        // Without housenumber
        let labels_no_hn = doc.make_labels(None, true);
        assert_eq!(
            labels_no_hn,
            vec![
                "rue des Lilas 75010 Paris",
                "rue des Lilas 75010",
                "rue des Lilas Paris",
                "rue des Lilas"
            ]
        );
    }

    #[test]
    fn test_make_municipality_labels_fidelity() {
        // Matches official addok-france test: test_make_municipality_labels
        let doc = Document::new("xxxx", "Lille")
            .with_type("municipality")
            .with_city("Lille")
            .with_postcode("59000");

        let labels = doc.make_labels(None, true);
        assert_eq!(labels, vec!["Lille", "59000 Lille", "Lille 59000"]);
    }
}
