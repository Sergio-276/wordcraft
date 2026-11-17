//! Application-level library records for maps, points of interest and characters.
//!
//! These records deliberately live outside the document model. Binary image data is referenced
//! by opaque asset IDs and belongs in host-backed storage, never in preferences or a document.

use serde::{Deserialize, Serialize};
use std::io::{self, Write};

/// Current on-disk format version for a WorldLibrary.
pub const LIBRARY_VERSION: u32 = 1;

/// Maximum serialized metadata size accepted from host storage (images are stored separately).
pub const MAX_LIBRARY_JSON_BYTES: usize = 16 * 1024 * 1024;

/// A writer that stops serialization before metadata can exceed its storage budget.
struct LimitedWriter {
    bytes: Vec<u8>,
}

impl LimitedWriter {
    fn new() -> Self {
        Self { bytes: Vec::new() }
    }
}

impl Write for LimitedWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let Some(new_len) = self.bytes.len().checked_add(bytes.len()) else {
            return Err(io::Error::other("world library metadata size overflow"));
        };
        if new_len > MAX_LIBRARY_JSON_BYTES {
            return Err(io::Error::other("world library metadata size limit exceeded"));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// A collection of world-building records, independent of any open document.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
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

impl Default for WorldLibrary {
    fn default() -> Self {
        Self::new()
    }
}

impl WorldLibrary {
    /// Creates an empty library using the current schema version.
    pub fn new() -> Self {
        Self { version: LIBRARY_VERSION, maps: Vec::new(), points: Vec::new(), characters: Vec::new() }
    }

    /// Checks identifiers, map references and normalized marker coordinates.
    ///
    /// Asset IDs are opaque references; the host storage adapter is responsible for resolving
    /// them and reporting missing binary assets.
    pub fn validate(&self) -> Result<(), String> {
        if self.version != LIBRARY_VERSION {
            return Err(format!("Unsupported world library version: {}", self.version));
        }

        let valid_asset_id = |id: &str| !id.is_empty() && id.trim() == id;
        let mut map_ids = std::collections::HashSet::new();
        for map in &self.maps {
            if map.id.trim() != map.id || map.id.is_empty() || !map_ids.insert(map.id.as_str()) {
                return Err("Map IDs must be non-empty and unique".into());
            }
            if !valid_asset_id(&map.image_asset_id) {
                return Err(format!("Map {} has an invalid image asset ID", map.id));
            }
        }

        let mut point_ids = std::collections::HashSet::new();
        for point in &self.points {
            if point.id.trim() != point.id || point.id.is_empty() || !point_ids.insert(point.id.as_str()) {
                return Err("Point-of-interest IDs must be non-empty and unique".into());
            }
            if !map_ids.contains(point.map_id.as_str()) {
                return Err(format!("Point {} refers to an unknown map", point.id));
            }
            if point.image_asset_ids.iter().any(|id| !valid_asset_id(id)) {
                return Err(format!("Point {} has an invalid image asset ID", point.id));
            }
            if !point.x.is_finite() || !point.y.is_finite() || !(0.0..=1.0).contains(&point.x) || !(0.0..=1.0).contains(&point.y) {
                return Err(format!("Point {} has coordinates outside the normalized map bounds", point.id));
            }
        }

        let mut character_ids = std::collections::HashSet::new();
        for character in &self.characters {
            if character.id.trim() != character.id || character.id.is_empty() || !character_ids.insert(character.id.as_str()) {
                return Err("Character IDs must be non-empty and unique".into());
            }
            if character.portrait_asset_id.as_deref().is_some_and(|id| !valid_asset_id(id)) {
                return Err(format!("Character {} has an invalid portrait asset ID", character.id));
            }
        }
        Ok(())
    }

    /// Serializes validated metadata for host storage. Binary image data must use the asset store.
    pub fn to_json_bytes(&self) -> Result<Vec<u8>, String> {
        self.validate()?;
        let mut writer = LimitedWriter::new();
        serde_json::to_writer(&mut writer, self).map_err(|error| format!("Could not serialize world library: {error}"))?;
        Ok(writer.bytes)
    }

    /// Loads metadata from host storage, rejecting oversized, malformed or invalid libraries.
    pub fn from_json_bytes(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() > MAX_LIBRARY_JSON_BYTES {
            return Err(format!("World library metadata exceeds the {} byte limit", MAX_LIBRARY_JSON_BYTES));
        }
        let library: Self = serde_json::from_slice(bytes).map_err(|error| format!("Could not read world library: {error}"))?;
        library.validate()?;
        Ok(library)
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


/// Maximum size of one binary image asset accepted by the host storage bridge (64 MiB).
pub const MAX_LIBRARY_ASSET_BYTES: usize = 64 * 1024 * 1024;

/// Operations requested by the shared UI and executed by the platform host.
#[derive(Debug, PartialEq, Eq)]
pub enum WorldLibraryCommand {
    LoadMetadata { request_id: u64 },
    SaveMetadata { request_id: u64, bytes: Vec<u8> },
    ReadAsset { request_id: u64, asset_id: String },
    WriteAsset { request_id: u64, asset_id: String, bytes: Vec<u8> },
    DeleteAsset { request_id: u64, asset_id: String },
}

/// Result returned by a platform host. Bytes are present for metadata loads and asset reads.
#[derive(Debug, PartialEq, Eq)]
pub struct WorldLibraryCompletion {
    pub request_id: u64,
    pub result: Result<Option<Vec<u8>>, String>,
}

/// Cloneable, non-blocking bridge between the platform host and the shared UI.
///
/// The UI enqueues commands and drains completions. Hosts drain commands, perform their native
/// or asynchronous storage work, then post a completion with the same request ID.
#[derive(Clone, Default)]
pub struct WorldLibraryBridge {
    commands: std::sync::Arc<std::sync::Mutex<Vec<WorldLibraryCommand>>>,
    completions: std::sync::Arc<std::sync::Mutex<Vec<WorldLibraryCompletion>>>,
}

impl WorldLibraryBridge {
    /// Queue a metadata load.
    pub fn load_metadata(&self, request_id: u64) {
        self.enqueue(WorldLibraryCommand::LoadMetadata { request_id });
    }

    /// Queue validated metadata for persistence.
    pub fn save_metadata(&self, request_id: u64, bytes: Vec<u8>) -> Result<(), String> {
        if bytes.len() > MAX_LIBRARY_JSON_BYTES {
            return Err("World library metadata exceeds its size limit".into());
        }
        WorldLibrary::from_json_bytes(&bytes)?;
        self.enqueue(WorldLibraryCommand::SaveMetadata { request_id, bytes });
        Ok(())
    }

    /// Queue an asset read.
    pub fn read_asset(&self, request_id: u64, asset_id: String) -> Result<(), String> {
        validate_asset_id(&asset_id)?;
        self.enqueue(WorldLibraryCommand::ReadAsset { request_id, asset_id });
        Ok(())
    }

    /// Queue an asset write.
    pub fn write_asset(&self, request_id: u64, asset_id: String, bytes: Vec<u8>) -> Result<(), String> {
        validate_asset_id(&asset_id)?;
        if bytes.is_empty() || bytes.len() > MAX_LIBRARY_ASSET_BYTES {
            return Err(format!("Library assets must be between 1 and {MAX_LIBRARY_ASSET_BYTES} bytes"));
        }
        self.enqueue(WorldLibraryCommand::WriteAsset { request_id, asset_id, bytes });
        Ok(())
    }

    /// Queue deletion of an asset.
    pub fn delete_asset(&self, request_id: u64, asset_id: String) -> Result<(), String> {
        validate_asset_id(&asset_id)?;
        self.enqueue(WorldLibraryCommand::DeleteAsset { request_id, asset_id });
        Ok(())
    }

    /// Host-side: take all currently queued operations.
    pub fn take_commands(&self) -> Vec<WorldLibraryCommand> {
        self.commands.lock().unwrap_or_else(|error| error.into_inner()).drain(..).collect()
    }

    /// Host-side: publish the outcome of an operation.
    pub fn complete(&self, completion: WorldLibraryCompletion) {
        self.completions.lock().unwrap_or_else(|error| error.into_inner()).push(completion);
    }

    /// UI-side: take all completed operations.
    pub fn take_completions(&self) -> Vec<WorldLibraryCompletion> {
        self.completions.lock().unwrap_or_else(|error| error.into_inner()).drain(..).collect()
    }

    fn enqueue(&self, command: WorldLibraryCommand) {
        self.commands.lock().unwrap_or_else(|error| error.into_inner()).push(command);
    }
}

fn validate_asset_id(asset_id: &str) -> Result<(), String> {
    if asset_id.is_empty() || asset_id.trim() != asset_id || asset_id.contains('/') || asset_id.contains('\\\\') || asset_id == "." || asset_id == ".." {
        return Err("Library asset IDs must be non-empty canonical file names".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{CharacterRecord, MapRecord, PointOfInterest, WorldLibrary, LIBRARY_VERSION, MAX_LIBRARY_JSON_BYTES};

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

        let encoded = serde_json::to_string(&library);
        assert!(encoded.is_ok());
        if let Ok(encoded) = encoded {
            let decoded: Result<WorldLibrary, _> = serde_json::from_str(&encoded);
            assert!(decoded.is_ok());
            if let Ok(decoded) = decoded {
                assert_eq!(decoded, library);
                assert_eq!(decoded.version, LIBRARY_VERSION);
                assert!(decoded.validate().is_ok());
            }
        }
    }

    #[test]
    fn validated_storage_serialization_round_trips() {
        let mut library = WorldLibrary::new();
        library.maps.push(MapRecord {
            id: "map-1".into(),
            name: "Map".into(),
            image_asset_id: "asset-map".into(),
            ..MapRecord::default()
        });
        let encoded = library.to_json_bytes();
        assert!(encoded.is_ok());
        if let Ok(encoded) = encoded {
            let decoded = WorldLibrary::from_json_bytes(&encoded);
            assert_eq!(decoded, Ok(library));
        }
    }

    #[test]
    fn storage_serialization_stops_at_metadata_limit() {
        let mut library = WorldLibrary::new();
        library.characters.push(CharacterRecord {
            id: "character-1".into(),
            name: "Large description".into(),
            description: "x".repeat(MAX_LIBRARY_JSON_BYTES),
            portrait_asset_id: None,
        });
        assert!(library.to_json_bytes().is_err());
    }

    #[test]
    fn storage_deserialization_rejects_oversized_metadata() {
        let bytes = vec![b' '; MAX_LIBRARY_JSON_BYTES + 1];
        assert!(WorldLibrary::from_json_bytes(&bytes).is_err());
    }

    #[test]
    fn storage_serialization_rejects_invalid_asset_ids() {
        let mut library = WorldLibrary::new();
        library.maps.push(MapRecord {
            id: "map-1".into(),
            image_asset_id: " asset-map ".into(),
            ..MapRecord::default()
        });
        assert!(library.to_json_bytes().is_err());
    }

    #[test]
    fn storage_deserialization_rejects_malformed_json() {
        assert!(WorldLibrary::from_json_bytes(b"{not-json").is_err());
    }

    #[test]
    fn storage_deserialization_rejects_unsupported_versions() {
        assert!(WorldLibrary::from_json_bytes(br#"{"version":99}"#).is_err());
    }

    #[test]
    fn validation_rejects_ids_with_surrounding_whitespace() {
        let mut library = WorldLibrary::new();
        library.maps.push(MapRecord { id: " map-1 ".into(), image_asset_id: "asset-map".into(), ..MapRecord::default() });
        assert!(library.validate().is_err());
    }

    #[test]
    fn validation_rejects_points_outside_their_map() {
        let mut library = WorldLibrary::new();
        library.maps.push(MapRecord { id: "map-1".into(), image_asset_id: "asset-map".into(), ..MapRecord::default() });
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
