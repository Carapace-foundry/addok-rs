use std::collections::HashMap;

use addok_index::{km_to_score, DocumentStore, SearchHit, SpatialResult};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GeoJsonPoint {
    #[serde(rename = "type")]
    pub point_type: String,
    /// [longitude, latitude]
    pub coordinates: [f64; 2],
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AddressProperties {
    pub label: String,
    pub score: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub housenumber: Option<String>,
    pub id: String,
    #[serde(rename = "type")]
    pub address_type: String,
    pub name: String,
    pub postcode: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub citycode: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub x: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub y: Option<f64>,
    pub city: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub importance: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub street: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub distance: Option<i64>,
    #[serde(flatten, default, skip_serializing_if = "HashMap::is_empty")]
    pub extra: HashMap<String, serde_json::Value>,
}

pub const RESERVED_PROPERTY_KEYS: &[&str] = &[
    "id",
    "type",
    "address_type",
    "label",
    "name",
    "score",
    "housenumber",
    "postcode",
    "citycode",
    "city",
    "context",
    "importance",
    "street",
    "distance",
    "x",
    "y",
    "lat",
    "lon",
];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AddressFeature {
    #[serde(rename = "type")]
    pub feature_type: String,
    pub geometry: Option<GeoJsonPoint>,
    pub properties: AddressProperties,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FeatureCollection {
    #[serde(rename = "type")]
    pub collection_type: String,
    pub version: String,
    pub features: Vec<AddressFeature>,
    pub attribution: String,
    pub licence: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub query: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filters: Option<HashMap<String, serde_json::Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub center: Option<[f64; 2]>,
    pub limit: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ApiError {
    #[serde(rename = "type")]
    pub error_type: String,
    pub detail: String,
    pub error: String,
}

impl ApiError {
    pub fn invalid_query(detail: impl Into<String>) -> Self {
        let d = detail.into();
        Self {
            error_type: "invalid_query".to_string(),
            error: d.clone(),
            detail: d,
        }
    }
}

impl FeatureCollection {
    pub fn from_search_hits(hits: Vec<SearchHit>, query: &str, limit: usize) -> Self {
        Self::from_search_hits_with_center(hits, query, limit, None)
    }

    pub fn from_search_hits_with_center(
        hits: Vec<SearchHit>,
        query: &str,
        limit: usize,
        center: Option<[f64; 2]>,
    ) -> Self {
        let features = hits
            .into_iter()
            .map(|hit| {
                let rec = hit.record;
                let geometry = match (rec.lon, rec.lat) {
                    (Some(lon), Some(lat)) => Some(GeoJsonPoint {
                        point_type: "Point".to_string(),
                        coordinates: [lon, lat],
                    }),
                    _ => None,
                };

                let importance = hit.document.as_ref().map(|d| d.importance);

                let address_type = if hit.matched_housenumber.is_some() {
                    "housenumber".to_string()
                } else if let Some(doc) = &hit.document {
                    doc.address_type
                        .clone()
                        .unwrap_or_else(|| rec.ban_type().to_string())
                } else {
                    rec.ban_type().to_string()
                };

                let hn_xy = if let (Some(d), Some(hn)) = (&hit.document, &hit.matched_housenumber) {
                    if let Some(hnd) = d.housenumbers.get(hn) {
                        let x = hnd.extra.get("x").and_then(|v| v.as_f64());
                        let y = hnd.extra.get("y").and_then(|v| v.as_f64());
                        (x, y)
                    } else {
                        (None, None)
                    }
                } else {
                    (None, None)
                };
                let x = hn_xy.0.or(rec.x);
                let y = hn_xy.1.or(rec.y);

                let mut extra = hit
                    .document
                    .as_ref()
                    .map(|d| d.extra.clone())
                    .unwrap_or_default();
                if let (Some(d), Some(hn)) = (&hit.document, &hit.matched_housenumber) {
                    if let Some(hnd) = d.housenumbers.get(hn) {
                        for (k, v) in &hnd.extra {
                            extra.insert(k.clone(), v.clone());
                        }
                    }
                }
                for key in RESERVED_PROPERTY_KEYS {
                    extra.remove(*key);
                }

                let street = if address_type == "municipality" || rec.nom_voie.trim().is_empty() {
                    None
                } else {
                    Some(rec.nom_voie.clone())
                };

                let name = if address_type == "municipality" {
                    rec.nom_commune.clone()
                } else if address_type == "housenumber" {
                    rec.street_and_number()
                } else {
                    rec.nom_voie.clone()
                };

                let properties = AddressProperties {
                    label: rec.label(),
                    score: hit.score,
                    housenumber: rec.full_housenumber(),
                    id: rec.id.clone(),
                    address_type,
                    name,
                    postcode: rec.code_postal.clone(),
                    citycode: rec.code_insee.clone(),
                    x,
                    y,
                    city: rec.nom_commune.clone(),
                    context: hit
                        .document
                        .as_ref()
                        .and_then(|d| d.context.clone())
                        .or_else(|| {
                            rec.code_insee.as_ref().map(|code| {
                                let dep = if code.starts_with("97") && code.len() >= 3 {
                                    &code[..3]
                                } else if code.len() >= 2 {
                                    &code[..2]
                                } else {
                                    code.as_str()
                                };
                                format!("{}, {}", dep, rec.nom_commune)
                            })
                        }),
                    importance,
                    street,
                    distance: hit.distance_meters.map(|d| d.round() as i64),
                    extra,
                };

                AddressFeature {
                    feature_type: "Feature".to_string(),
                    geometry,
                    properties,
                }
            })
            .collect();

        Self {
            collection_type: "FeatureCollection".to_string(),
            version: "draft".to_string(),
            features,
            attribution: "BAN (Base Adresse Nationale)".to_string(),
            licence: "ODbL 1.0".to_string(),
            query: Some(query.to_string()),
            filters: None,
            center,
            limit,
        }
    }

    pub fn with_filters(mut self, filters: HashMap<String, serde_json::Value>) -> Self {
        if !filters.is_empty() {
            self.filters = Some(filters);
        }
        self
    }

    pub fn with_attribution_and_licence(
        mut self,
        attribution: impl Into<String>,
        licence: impl Into<String>,
    ) -> Self {
        self.attribution = attribution.into();
        self.licence = licence.into();
        self
    }

    pub fn from_spatial_results(results: Vec<SpatialResult>, limit: usize) -> Self {
        Self::from_spatial_results_with_details(results, limit, None, None)
    }

    pub fn from_spatial_results_with_details(
        results: Vec<SpatialResult>,
        limit: usize,
        center: Option<[f64; 2]>,
        doc_store: Option<&DocumentStore>,
    ) -> Self {
        let features = results
            .into_iter()
            .map(|res| {
                let item = res.item;
                let geometry = Some(GeoJsonPoint {
                    point_type: "Point".to_string(),
                    coordinates: item.point,
                });

                let doc = doc_store.and_then(|ds| {
                    ds.get_by_id(&item.id).or_else(|| {
                        let parts: Vec<&str> = item.id.split('_').collect();
                        if parts.len() >= 3 {
                            let street_id = format!("{}_{}", parts[0], parts[1]);
                            ds.get_by_id(&street_id)
                        } else {
                            let (parent_id, _) = item.id.rsplit_once('_')?;
                            ds.get_by_id(parent_id)
                        }
                    })
                });

                let citycode = item
                    .citycode
                    .clone()
                    .or_else(|| doc.and_then(|d| d.citycode.clone()));
                let context = doc.and_then(|d| d.context.clone()).or_else(|| {
                    let pc = &item.postcode;
                    let dep = if pc.starts_with("97") && pc.len() >= 3 {
                        &pc[..3]
                    } else if pc.len() >= 2 {
                        &pc[..2]
                    } else {
                        pc.as_str()
                    };
                    if !dep.is_empty() {
                        Some(format!("{}, {}", dep, item.city))
                    } else {
                        Some(item.city.clone())
                    }
                });

                let hn_xy = if let (Some(d), Some(hn)) = (doc, &item.housenumber) {
                    if let Some(hnd) = d.housenumbers.get(hn) {
                        let x = hnd.extra.get("x").and_then(|v| v.as_f64());
                        let y = hnd.extra.get("y").and_then(|v| v.as_f64());
                        (x, y)
                    } else {
                        (None, None)
                    }
                } else {
                    (None, None)
                };
                let x = hn_xy
                    .0
                    .or_else(|| doc.and_then(|d| d.extra.get("x").and_then(|v| v.as_f64())));
                let y = hn_xy
                    .1
                    .or_else(|| doc.and_then(|d| d.extra.get("y").and_then(|v| v.as_f64())));

                let mut extra = doc.map(|d| d.extra.clone()).unwrap_or_default();
                if let (Some(d), Some(hn)) = (doc, &item.housenumber) {
                    if let Some(hnd) = d.housenumbers.get(hn) {
                        for (k, v) in &hnd.extra {
                            extra.insert(k.clone(), v.clone());
                        }
                    }
                }
                for key in RESERVED_PROPERTY_KEYS {
                    extra.remove(*key);
                }

                let importance = doc.map(|d| d.importance);

                let dist_km = res.distance_meters / 1000.0;
                let score = km_to_score(dist_km) as f32;

                let street_name = doc.map(|d| d.name.clone());
                let name = match (&item.housenumber, &street_name) {
                    (Some(hn), Some(st)) => format!("{} {}", hn, st),
                    (None, Some(st)) => st.clone(),
                    _ => {
                        if let Some(hn) = &item.housenumber {
                            let street_part = item
                                .label
                                .strip_prefix(hn)
                                .map(str::trim)
                                .and_then(|s| s.strip_suffix(&item.city))
                                .map(str::trim)
                                .and_then(|s| s.strip_suffix(&item.postcode))
                                .map(str::trim)
                                .unwrap_or(&item.label);
                            format!("{} {}", hn, street_part)
                        } else {
                            item.label.clone()
                        }
                    }
                };

                let properties = AddressProperties {
                    label: item.label.clone(),
                    score,
                    housenumber: item.housenumber.clone(),
                    id: item.id.clone(),
                    address_type: item.ban_type.clone(),
                    name,
                    postcode: item.postcode.clone(),
                    citycode,
                    x,
                    y,
                    city: item.city.clone(),
                    context,
                    importance,
                    street: street_name,
                    distance: Some(res.distance_meters.round() as i64),
                    extra,
                };

                AddressFeature {
                    feature_type: "Feature".to_string(),
                    geometry,
                    properties,
                }
            })
            .collect();

        Self {
            collection_type: "FeatureCollection".to_string(),
            version: "draft".to_string(),
            features,
            attribution: "BAN (Base Adresse Nationale)".to_string(),
            licence: "ODbL 1.0".to_string(),
            query: None,
            filters: None,
            center,
            limit,
        }
    }
}
