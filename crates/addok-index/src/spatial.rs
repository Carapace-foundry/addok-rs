use std::fs::File;
use std::io::{BufReader, BufWriter};
use std::path::Path;

use rstar::{PointDistance, RTree, RTreeObject, AABB};
use serde::{Deserialize, Serialize};

use crate::error::Result;

/// Earth radius in meters for geodetic calculations
const EARTH_RADIUS_METERS: f64 = 6_371_000.0;

/// Computes the great-circle distance between two WGS84 points in meters using the Haversine formula.
pub fn haversine_distance_meters(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let dlat = (lat2 - lat1).to_radians();
    let dlon = (lon2 - lon1).to_radians();
    let lat1_rad = lat1.to_radians();
    let lat2_rad = lat2.to_radians();

    let a =
        (dlat / 2.0).sin().powi(2) + lat1_rad.cos() * lat2_rad.cos() * (dlon / 2.0).sin().powi(2);
    let c = 2.0 * a.sqrt().atan2((1.0 - a).sqrt());
    EARTH_RADIUS_METERS * c
}

/// Item stored in the spatial R-Tree index.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SpatialItem {
    /// Coordinate in [longitude, latitude]
    pub point: [f64; 2],
    pub id: String,
    pub label: String,
    pub ban_type: String,
    pub postcode: String,
    pub city: String,
    pub housenumber: Option<String>,
    #[serde(default)]
    pub citycode: Option<String>,
}

impl RTreeObject for SpatialItem {
    type Envelope = AABB<[f64; 2]>;

    fn envelope(&self) -> Self::Envelope {
        AABB::from_point(self.point)
    }
}

impl PointDistance for SpatialItem {
    fn distance_2(&self, point: &[f64; 2]) -> f64 {
        // Project longitude difference using latitude cosine for accurate local distance
        let cos_lat = point[1].to_radians().cos();
        let dx = (self.point[0] - point[0]) * cos_lat;
        let dy = self.point[1] - point[1];
        dx * dx + dy * dy
    }
}

/// Result of a reverse geocoding nearest neighbor lookup
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SpatialResult {
    pub item: SpatialItem,
    pub distance_meters: f64,
}

/// Spatial index wrapping an R-Tree for fast nearest-neighbor spatial lookups.
pub struct SpatialIndex {
    tree: RTree<SpatialItem>,
}

impl SpatialIndex {
    pub fn new() -> Self {
        Self { tree: RTree::new() }
    }

    /// Bulk-load items into a balanced R-Tree index
    pub fn from_items(items: Vec<SpatialItem>) -> Self {
        Self {
            tree: RTree::bulk_load(items),
        }
    }

    /// Number of points in the spatial index
    pub fn len(&self) -> usize {
        self.tree.size()
    }

    pub fn is_empty(&self) -> bool {
        self.tree.size() == 0
    }

    /// Extract all items from the spatial tree
    pub fn into_items(self) -> Vec<SpatialItem> {
        self.tree.into_iter().collect()
    }

    /// Find the `limit` nearest addresses to given (lat, lon) WGS84 coordinates.
    pub fn nearest_neighbors(&self, lat: f64, lon: f64, limit: usize) -> Vec<SpatialResult> {
        self.nearest_neighbors_filtered(lat, lon, limit, None)
    }

    /// Find the `limit` nearest addresses to given (lat, lon) WGS84 coordinates,
    /// optionally filtered by allowed address types (e.g. `["housenumber", "street"]`).
    pub fn nearest_neighbors_filtered(
        &self,
        lat: f64,
        lon: f64,
        limit: usize,
        type_filter: Option<&[String]>,
    ) -> Vec<SpatialResult> {
        self.nearest_neighbors_with_filters(lat, lon, limit, type_filter, None, None)
    }

    /// Find the `limit` nearest addresses to given (lat, lon) WGS84 coordinates,
    /// filtered by allowed address types, postcodes, and/or citycodes.
    pub fn nearest_neighbors_with_filters(
        &self,
        lat: f64,
        lon: f64,
        limit: usize,
        type_filter: Option<&[String]>,
        postcode_filter: Option<&[String]>,
        citycode_filter: Option<&[String]>,
    ) -> Vec<SpatialResult> {
        let query_point = [lon, lat];
        self.tree
            .nearest_neighbor_iter(&query_point)
            .filter(|item| {
                if let Some(types) = type_filter {
                    if !types.is_empty()
                        && !types.iter().any(|t| t.eq_ignore_ascii_case(&item.ban_type))
                    {
                        return false;
                    }
                }
                if let Some(postcodes) = postcode_filter {
                    if !postcodes.is_empty()
                        && !postcodes
                            .iter()
                            .any(|p| p.eq_ignore_ascii_case(&item.postcode))
                    {
                        return false;
                    }
                }
                if let Some(citycodes) = citycode_filter {
                    if !citycodes.is_empty() {
                        let matched = match &item.citycode {
                            Some(c) => citycodes.iter().any(|cc| cc.eq_ignore_ascii_case(c)),
                            None => false,
                        };
                        if !matched {
                            return false;
                        }
                    }
                }
                true
            })
            .take(limit)
            .map(|item| {
                let dist = haversine_distance_meters(lat, lon, item.point[1], item.point[0]);
                SpatialResult {
                    item: item.clone(),
                    distance_meters: dist,
                }
            })
            .collect()
    }

    /// Find all items within a maximum radius (in meters) from given (lat, lon) coordinates.
    pub fn locate_within_distance(
        &self,
        lat: f64,
        lon: f64,
        max_distance_meters: f64,
    ) -> Vec<SpatialResult> {
        let query_point = [lon, lat];
        let mut results = Vec::new();
        for item in self.tree.nearest_neighbor_iter(&query_point) {
            let dist = haversine_distance_meters(lat, lon, item.point[1], item.point[0]);
            if dist <= max_distance_meters {
                results.push(SpatialResult {
                    item: item.clone(),
                    distance_meters: dist,
                });
            } else if dist > max_distance_meters * 1.5 {
                break;
            }
        }
        results.sort_by(|a, b| {
            a.distance_meters
                .partial_cmp(&b.distance_meters)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        results
    }

    /// Save the spatial index to a file (JSON)
    pub fn save_to_file<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        let file = File::create(path)?;
        let writer = BufWriter::new(file);
        let items: Vec<&SpatialItem> = self.tree.iter().collect();
        serde_json::to_writer(writer, &items)?;
        Ok(())
    }

    /// Load the spatial index from a file (JSON) using memory mapping
    pub fn load_from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let file = File::open(&path)?;
        if let Ok(mmap) = unsafe { memmap2::Mmap::map(&file) } {
            if let Ok(items) = serde_json::from_slice::<Vec<SpatialItem>>(&mmap) {
                return Ok(Self::from_items(items));
            }
        }
        let reader = BufReader::new(File::open(path)?);
        let items: Vec<SpatialItem> = serde_json::from_reader(reader)?;
        Ok(Self::from_items(items))
    }

    /// Save the spatial index to a binary file (bincode)
    pub fn save_to_bin<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        let file = File::create(path)?;
        let mut writer = BufWriter::new(file);
        let items: Vec<&SpatialItem> = self.tree.iter().collect();
        bincode::serialize_into(&mut writer, &items)?;
        std::io::Write::flush(&mut writer)?;
        Ok(())
    }

    /// Load the spatial index from a binary file (bincode) using memory mapping
    pub fn load_from_bin<P: AsRef<Path>>(path: P) -> Result<Self> {
        let file = File::open(&path)?;
        if let Ok(mmap) = unsafe { memmap2::Mmap::map(&file) } {
            if let Ok(items) = bincode::deserialize::<Vec<SpatialItem>>(&mmap) {
                return Ok(Self::from_items(items));
            }
        }
        let reader = BufReader::new(File::open(path)?);
        let items: Vec<SpatialItem> = bincode::deserialize_from(reader)?;
        Ok(Self::from_items(items))
    }
}

impl Default for SpatialIndex {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_haversine_distance() {
        // Paris Notre-Dame (48.8530, 2.3499) to Tour Eiffel (48.8584, 2.2945) ~ 4.1 km
        let dist = haversine_distance_meters(48.8530, 2.3499, 48.8584, 2.2945);
        assert!(dist > 4000.0 && dist < 4200.0, "dist = {}", dist);
    }

    #[test]
    fn test_spatial_nearest_neighbor() {
        let items = vec![
            SpatialItem {
                point: [2.3431, 48.8512],
                id: "1".to_string(),
                label: "8 Boulevard Saint-Michel 75005 Paris".to_string(),
                ban_type: "housenumber".to_string(),
                postcode: "75005".to_string(),
                city: "Paris".to_string(),
                housenumber: Some("8".to_string()),
                citycode: Some("75105".to_string()),
            },
            SpatialItem {
                point: [2.3082, 48.8698],
                id: "2".to_string(),
                label: "10 Avenue des Champs-Élysées 75008 Paris".to_string(),
                ban_type: "housenumber".to_string(),
                postcode: "75008".to_string(),
                city: "Paris".to_string(),
                housenumber: Some("10".to_string()),
                citycode: Some("75108".to_string()),
            },
        ];

        let index = SpatialIndex::from_items(items);
        assert_eq!(index.len(), 2);

        // Query close to St-Michel
        let results = index.nearest_neighbors(48.8513, 2.3430, 1);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].item.id, "1");
        assert!(results[0].distance_meters < 50.0);
    }
}
