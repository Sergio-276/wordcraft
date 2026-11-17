//! Desktop adapter for the independent world library.

use std::path::{Path, PathBuf};
use wordcraft_ui_egui::world_library::{WorldLibraryBridge, WorldLibraryCommand, WorldLibraryCompletion, MAX_LIBRARY_ASSET_BYTES, MAX_LIBRARY_JSON_BYTES};

/// Dispatch queued storage operations to worker threads so image I/O never blocks an egui frame.
pub fn process(bridge: &WorldLibraryBridge, directory: Option<PathBuf>, ctx: &egui::Context) {
    for command in bridge.take_commands() {
        let bridge = bridge.clone();
        let directory = directory.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let request_id = request_id(&command);
            let result = match directory {
                Some(directory) => execute(&directory, command),
                None => Err("WordCraft could not determine the settings directory".into()),
            };
            bridge.complete(WorldLibraryCompletion { request_id, result });
            ctx.request_repaint();
        });
    }
}

fn request_id(command: &WorldLibraryCommand) -> u64 {
    match command {
        WorldLibraryCommand::LoadMetadata { request_id }
        | WorldLibraryCommand::SaveMetadata { request_id, .. }
        | WorldLibraryCommand::ReadAsset { request_id, .. }
        | WorldLibraryCommand::WriteAsset { request_id, .. }
        | WorldLibraryCommand::DeleteAsset { request_id, .. } => *request_id,
    }
}

fn execute(directory: &Path, command: WorldLibraryCommand) -> Result<Option<Vec<u8>>, String> {
    let assets = directory.join("assets");
    match command {
        WorldLibraryCommand::LoadMetadata { .. } => {
            let path = directory.join("library.json");
            match std::fs::metadata(&path) {
                Ok(meta) if meta.len() as usize <= MAX_LIBRARY_JSON_BYTES => std::fs::read(path).map(Some).map_err(storage_error),
                Ok(_) => Err("World library metadata exceeds its size limit".into()),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
                Err(error) => Err(storage_error(error)),
            }
        }
        WorldLibraryCommand::SaveMetadata { bytes, .. } => {
            std::fs::create_dir_all(directory).map_err(storage_error)?;
            super::write_prefs(&directory.join("library.json"), &bytes).map_err(storage_error)?;
            Ok(None)
        }
        WorldLibraryCommand::ReadAsset { asset_id, .. } => {
            let path = assets.join(asset_id);
            let metadata = std::fs::metadata(&path).map_err(storage_error)?;
            if metadata.len() == 0 || metadata.len() as usize > MAX_LIBRARY_ASSET_BYTES {
                return Err("Stored library asset has an invalid size".into());
            }
            std::fs::read(path).map(Some).map_err(storage_error)
        }
        WorldLibraryCommand::WriteAsset { asset_id, bytes, .. } => {
            std::fs::create_dir_all(&assets).map_err(storage_error)?;
            super::write_prefs(&assets.join(asset_id), &bytes).map_err(storage_error)?;
            Ok(None)
        }
        WorldLibraryCommand::DeleteAsset { asset_id, .. } => {
            match std::fs::remove_file(assets.join(asset_id)) {
                Ok(()) => Ok(None),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
                Err(error) => Err(storage_error(error)),
            }
        }
    }
}

fn storage_error(error: std::io::Error) -> String {
    format!("World library storage failed: {error}")
}
