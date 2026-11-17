//! Application-level library records for maps, points of interest and characters.
//!
//! These records deliberately live outside the document model. Binary image data is referenced
//! by opaque asset IDs and belongs in host-backed storage, never in preferences or a document.

use serde::{Deserialize, Serialize};

/// Current on-disk format version for a WorldLibrary.
pub const LIBRARY_VERSION: u32 = 1;

/// A collection of world-building records, independent of any open document.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct WorldLibrary {
    /// Version used to migrate persisted libraries when the schema changes.
    #[serde(default = "default_library_version")]
    pub version: u32,
    /// Maps available in the library.
    #[serde(default)]
    pub maps: Vec<MapRecord>,
    /// Points of interest, indexed independently so they can be browsed without opening a map.
    #[serde(default)]
    pub points: Vec<PointOfInterest>,
    /// Characters available in the library.
    #[serde(default)]
    pub characters: Vec<CharacterRecord>,
}

fn default_library_version() -> u32 {
    LIBRARY_VERSION
}

impl WorldLibrary {
    /// Creates an empty library using the current schema version.
    pub fn new() -> Self {
        Self { version: LIBRARY_VERSION, ..Self::default() }
    }

    /// Checks identifiers, map references and normalized marker coordinates.
    ///
    /// Asset IDs are opaque references; the host storage adapter is responsible for resolving
    /// them and reporting missing binary assets.
    pub fn validate(&self) -> Result<(), String> {
        if self.version != LIBRARY_VERSION {
            return Err(format!("Unsupported world library version: {}", self.version));
        }

        let mut map_ids = std::collections::HashSet::new();
        for map in &self.maps {
            if map.id.trim().is_empty() || !map_ids.insert(map.id.as_str()) {
                return Err("Map IDs must be non-empty and unique".into());
            }
        }

        let mut point_ids = std::collections::HashSet::new();
        for point in &self.points {
            if point.id.trim().is_empty() || !point_ids.insert(point.id.as_str()) {
                return Err("Point-of-interest IDs must be non-empty and unique".into());
            }
            if !map_ids.contains(point.map_id.as_str()) {
                return Err(format!("Point {} refers to an unknown map", point.id));
            }
            if !point.x.is_finite() || !point.y.is_finite() || !(0.0..=1.0).contains(&point.x) || !(0.0..=1.0).contains(&point.y) {
                return Err(format!("Point {} has coordinates outside the normalized map bounds", point.id));
            }
        }

        let mut character_ids = std::collections::HashSet::new();
        for character in &self.characters {
            if character.id.trim().is_empty() || !character_ids.insert(character.id.as_str()) {
                return Err("Character IDs must be non-empty and unique".into());
            }
        }
        Ok(())
    }
}

/// A map image and its descriptive metadata.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MapRecord {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    /// Opaque ID of the map image in the host's binary asset store.
    pub image_asset_id: String,
}

/// A marker stored in normalized image coordinates (0.0–1.0).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PointOfInterest {
    pub id: String,
    pub map_id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub x: f32,
    pub y: f32,
    /// Opaque IDs of images attached to this point.
    #[serde(default)]
    pub image_asset_ids: Vec<String>,
}

/// A character card independent of a map or document.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CharacterRecord {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    /// Optional opaque ID of the character's primary image.
    #[serde(default)]
    pub portrait_asset_id: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::{CharacterRecord, MapRecord, PointOfInterest, WorldLibrary, LIBRARY_VERSION};

    #[test]
    fn library_round_trips_as_versioned_json() {
        let mut library = WorldLibrary::new();
        library.maps.push(MapRecord {
            id: "map-1".into(),
            name: "The Old City".into(),
            description: "A city map".into(),
            image_asset_id: "asset-map".into(),
        });
        library.points.push(PointOfInterest {
            id: "poi-1".into(),
            map_id: "map-1".into(),
            name: "The Gate".into(),
            description: "North entrance".into(),
            x: 0.25,
            y: 0.75,
            image_asset_ids: vec!["asset-gate".into()],
        });
        library.characters.push(CharacterRecord {
            id: "character-1".into(),
            name: "Mira".into(),
            description: "Cartographer".into(),
            portrait_asset_id: Some("asset-mira".into()),
        });

        let encoded = serde_json::to_string(&library).expect("library serializes");
        let decoded: WorldLibrary = serde_json::from_str(&encoded).expect("library deserializes");
        assert_eq!(decoded, library);
        assert_eq!(decoded.version, LIBRARY_VERSION);
        assert!(decoded.validate().is_ok());
    }

    #[test]
    fn validation_rejects_points_outside_their_map() {
        let mut library = WorldLibrary::new();
        library.maps.push(MapRecord { id: "map-1".into(), ..MapRecord::default() });
        library.points.push(PointOfInterest {
            id: "poi-1".into(),
            map_id: "map-1".into(),
            x: 1.1,
            y: 0.5,
            ..PointOfInterest::default()
        });
        assert!(library.validate().is_err());
    }

    #[test]
    fn validation_rejects_points_with_missing_maps() {
        let mut library = WorldLibrary::new();
        library.points.push(PointOfInterest {
            id: "poi-1".into(),
            map_id: "missing".into(),
            x: 0.5,
            y: 0.5,
            ..PointOfInterest::default()
        });
        assert!(library.validate().is_err());
    }
}
