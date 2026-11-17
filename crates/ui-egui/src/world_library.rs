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
            if !map.image_asset_id.is_empty() && !valid_asset_id(&map.image_asset_id) {
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


/// The currently selected page in the standalone library workspace.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LibrarySection {
    #[default]
    Maps,
    Points,
    Characters,
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


/// Shared UI lifecycle for the host-backed library.
#[derive(Clone, Debug)]
pub struct WorldLibraryState {
    pub library: WorldLibrary,
    pub loaded: bool,
    pub error: Option<String>,
    next_request_id: u64,
    load_requested: bool,
    pending_load: Option<u64>,
    pending_saves: std::collections::BTreeSet<u64>,
    /// Asset-operation completions are retained for the UI instead of being discarded by tick.
    asset_completions: Vec<WorldLibraryCompletion>,
    pub workspace_open: bool,
    pub section: LibrarySection,
    pub character_name: String,
    pub character_description: String,
    pub map_name: String,
    pub map_description: String,
    pub point_name: String,
    pub point_description: String,
    pub point_map_id: Option<String>,
    pub point_x: f32,
    pub point_y: f32,
}

impl Default for WorldLibraryState {
    fn default() -> Self {
        Self {
            library: WorldLibrary::new(),
            loaded: false,
            error: None,
            next_request_id: 1,
            load_requested: false,
            pending_load: None,
            pending_saves: std::collections::BTreeSet::new(),
            asset_completions: Vec::new(),
            workspace_open: false,
            section: LibrarySection::Maps,
            character_name: String::new(),
            character_description: String::new(),
            map_name: String::new(),
            map_description: String::new(),
            point_name: String::new(),
            point_description: String::new(),
            point_map_id: None,
            point_x: 0.5,
            point_y: 0.5,
        }
    }
}

impl WorldLibraryState {
    fn request_id(&mut self) -> u64 {
        let id = self.next_request_id;
        self.next_request_id = self.next_request_id.saturating_add(1);
        id
    }

    /// Start the initial load once a host bridge is available and consume completed operations.
    pub fn tick(&mut self, bridge: Option<&WorldLibraryBridge>) {
        let Some(bridge) = bridge else { return };
        if !self.load_requested {
            let request_id = self.request_id();
            bridge.load_metadata(request_id);
            self.pending_load = Some(request_id);
            self.load_requested = true;
        }
        for completion in bridge.take_completions() {
            if self.pending_saves.remove(&completion.request_id) {
                if let Err(error) = completion.result {
                    self.error = Some(error);
                } else {
                    self.error = None;
                }
                continue;
            }
            if self.pending_load == Some(completion.request_id) {
                self.pending_load = None;
                match completion.result {
                    Ok(Some(bytes)) => match WorldLibrary::from_json_bytes(&bytes) {
                        Ok(library) => {
                            self.library = library;
                            self.error = None;
                        }
                        Err(error) => self.error = Some(error),
                    },
                    Ok(None) => {
                        self.library = WorldLibrary::new();
                        self.error = None;
                    }
                    Err(error) => self.error = Some(error),
                }
                self.loaded = true;
                continue;
            }
            // Asset reads/writes/deletes are consumed by the library UI, which owns their
            // request lifecycle. Keep them available rather than silently losing their results.
            self.asset_completions.push(completion);
        }
    }

    /// Take asset-operation results accumulated by `tick`.
    pub fn take_asset_completions(&mut self) -> Vec<WorldLibraryCompletion> {
        std::mem::take(&mut self.asset_completions)
    }

    /// Persist the current metadata snapshot through the platform host.
    pub fn save(&mut self, bridge: &WorldLibraryBridge) -> Result<(), String> {
        let bytes = self.library.to_json_bytes()?;
        let request_id = self.request_id();
        bridge.save_metadata(request_id, bytes)?;
        self.pending_saves.insert(request_id);
        Ok(())
    }
}

fn validate_asset_id(asset_id: &str) -> Result<(), String> {
    if asset_id.is_empty() || asset_id.trim() != asset_id || asset_id.contains('/') || asset_id.contains('\\') || asset_id == "." || asset_id == ".." {
        return Err("Library asset IDs must be non-empty canonical file names".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{CharacterRecord, MapRecord, PointOfInterest, WorldLibrary, LIBRARY_VERSION, MAX_LIBRARY_JSON_BYTES};


    #[test]
    fn concurrent_metadata_saves_are_tracked_independently() {
        use super::{WorldLibraryBridge, WorldLibraryCompletion, WorldLibraryState};
        let bridge = WorldLibraryBridge::default();
        let mut state = WorldLibraryState::default();
        state.loaded = true;
        assert!(state.save(&bridge).is_ok());
        assert!(state.save(&bridge).is_ok());
        let commands = bridge.take_commands();
        let request_ids: Vec<u64> = commands.iter().filter_map(|command| match command {
            super::WorldLibraryCommand::SaveMetadata { request_id, .. } => Some(*request_id),
            _ => None,
        }).collect();
        assert_eq!(request_ids.len(), 2);
        for request_id in request_ids.into_iter().rev() {
            bridge.complete(WorldLibraryCompletion { request_id, result: Ok(None) });
        }
        state.tick(Some(&bridge));
        assert!(state.pending_saves.is_empty());
        assert!(state.error.is_none());
    }

    #[test]
    fn library_state_preserves_asset_completions_for_the_ui() {
        use super::{WorldLibraryBridge, WorldLibraryCompletion, WorldLibraryState};
        let bridge = WorldLibraryBridge::default();
        let mut state = WorldLibraryState::default();
        state.tick(Some(&bridge));
        let _ = bridge.take_commands();
        let completion = WorldLibraryCompletion { request_id: 99, result: Ok(Some(vec![1, 2, 3])) };
        bridge.complete(completion);
        state.tick(Some(&bridge));
        assert_eq!(state.take_asset_completions(), vec![WorldLibraryCompletion {
            request_id: 99,
            result: Ok(Some(vec![1, 2, 3])),
        }]);
        assert!(state.take_asset_completions().is_empty());
    }

    #[test]
    fn storage_bridge_delivers_commands_and_correlated_completions() {
        use super::{WorldLibraryBridge, WorldLibraryCommand, WorldLibraryCompletion};
        let bridge = WorldLibraryBridge::default();
        bridge.load_metadata(7);
        assert_eq!(bridge.take_commands(), vec![WorldLibraryCommand::LoadMetadata { request_id: 7 }]);
        bridge.complete(WorldLibraryCompletion { request_id: 7, result: Ok(Some(b"{}".to_vec())) });
        assert_eq!(bridge.take_completions(), vec![WorldLibraryCompletion { request_id: 7, result: Ok(Some(b"{}".to_vec())) }]);
    }

    #[test]
    fn storage_bridge_rejects_invalid_metadata_and_asset_paths() {
        use super::WorldLibraryBridge;
        let bridge = WorldLibraryBridge::default();
        assert!(bridge.save_metadata(1, b"{invalid}".to_vec()).is_err());
        assert!(bridge.read_asset(2, "../outside.png".into()).is_err());
        assert!(bridge.write_asset(3, "folder/image.png".into(), vec![1]).is_err());
        assert!(bridge.take_commands().is_empty());
    }


    #[test]
    fn library_state_loads_host_metadata_and_tracks_save_completion() {
        use super::{WorldLibrary, WorldLibraryBridge, WorldLibraryState, WorldLibraryCompletion};
        let bridge = WorldLibraryBridge::default();
        let mut state = WorldLibraryState::default();
        state.tick(Some(&bridge));
        assert_eq!(bridge.take_commands(), vec![super::WorldLibraryCommand::LoadMetadata { request_id: 1 }]);

        let mut library = WorldLibrary::new();
        library.characters.push(super::CharacterRecord {
            id: "character-1".into(),
            name: "Ariadne".into(),
            description: String::new(),
            portrait_asset_id: None,
        });
        let bytes = library.to_json_bytes().unwrap_or_default();
        bridge.complete(WorldLibraryCompletion { request_id: 1, result: Ok(Some(bytes)) });
        state.tick(Some(&bridge));
        assert!(state.loaded);
        assert_eq!(state.library.characters.len(), 1);

        assert!(state.save(&bridge).is_ok());
        let commands = bridge.take_commands();
        let save_id = match commands.first() {
            Some(super::WorldLibraryCommand::SaveMetadata { request_id, .. }) => *request_id,
            _ => 0,
        };
        assert_ne!(save_id, 0);
        bridge.complete(WorldLibraryCompletion { request_id: save_id, result: Ok(None) });
        state.tick(Some(&bridge));
        assert!(state.pending_saves.is_empty());
        assert!(state.error.is_none());
    }

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
    fn map_metadata_can_be_saved_before_an_image_is_imported() {
        let mut library = WorldLibrary::new();
        library.maps.push(MapRecord {
            id: "map-1".into(),
            name: "The Old City".into(),
            description: "Map image will be added later".into(),
            image_asset_id: String::new(),
        });
        assert!(library.validate().is_ok());
        assert!(library.to_json_bytes().is_ok());
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
