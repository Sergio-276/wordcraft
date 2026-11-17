//! Browser adapter for the independent world library, backed by IndexedDB.

use wasm_bindgen::{JsCast as _, JsValue};
use wasm_bindgen::closure::Closure;
use web_sys::{IdbDatabase, IdbRequest, IdbTransactionMode};
use wordcraft_ui_egui::world_library::{WorldLibraryBridge, WorldLibraryCommand, WorldLibraryCompletion, MAX_LIBRARY_ASSET_BYTES, MAX_LIBRARY_JSON_BYTES};

const DB_NAME: &str = "wordcraft";
const DB_VERSION: u32 = 2;
const METADATA_STORE: &str = "world-library";
const ASSET_STORE: &str = "world-assets";
const FONT_STORE: &str = "fonts";
const METADATA_KEY: &str = "metadata";

fn js(error: JsValue) -> String {
    format!("{error:?}")
}

async fn done(request: &IdbRequest) -> Result<JsValue, String> {
    let promise = js_sys::Promise::new(&mut |resolve: js_sys::Function, reject: js_sys::Function| {
        let ok_request = request.clone();
        let ok = Closure::once_into_js(move || {
            let value = ok_request.result().unwrap_or(JsValue::UNDEFINED);
            let _ = resolve.call1(&JsValue::NULL, &value);
        });
        let err = Closure::once_into_js(move || {
            let _ = reject.call1(&JsValue::NULL, &JsValue::from_str("IndexedDB request failed"));
        });
        request.set_onsuccess(Some(ok.unchecked_ref()));
        request.set_onerror(Some(err.unchecked_ref()));
    });
    wasm_bindgen_futures::JsFuture::from(promise).await.map_err(js)
}

async fn open() -> Result<IdbDatabase, String> {
    let factory = web_sys::window().ok_or("no window")?.indexed_db().map_err(js)?.ok_or("IndexedDB is unavailable")?;
    let request = factory.open_with_u32(DB_NAME, DB_VERSION).map_err(js)?;
    let upgrade_request = request.clone();
    let upgrade = Closure::<dyn FnMut()>::new(move || {
        if let Ok(database) = upgrade_request.result().and_then(|value| value.dyn_into::<IdbDatabase>()) {
            for name in [FONT_STORE, METADATA_STORE, ASSET_STORE] {
                if !database.object_store_names().contains(name)
                    && let Err(error) = database.create_object_store(name)
                {
                    log::error!("couldn't create IndexedDB store {name}: {error:?}");
                }
            }
        }
    });
    request.set_onupgradeneeded(Some(upgrade.as_ref().unchecked_ref()));
    let result = done(&request).await;
    request.set_onupgradeneeded(None);
    drop(upgrade);
    result?.dyn_into::<IdbDatabase>().map_err(js)
}

/// Dispatch queued bridge operations without blocking the browser frame.
pub fn process(bridge: &WorldLibraryBridge, ctx: &egui::Context) {
    for command in bridge.take_commands() {
        let bridge = bridge.clone();
        let ctx = ctx.clone();
        wasm_bindgen_futures::spawn_local(async move {
            let request_id = request_id(&command);
            let result = execute(command).await;
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

async fn execute(command: WorldLibraryCommand) -> Result<Option<Vec<u8>>, String> {
    let database = open().await?;
    match command {
        WorldLibraryCommand::LoadMetadata { .. } => {
            let store = database.transaction_with_str(METADATA_STORE).map_err(js)?.object_store(METADATA_STORE).map_err(js)?;
            let value = done(&store.get(&JsValue::from_str(METADATA_KEY)).map_err(js)?).await?;
            if value.is_undefined() || value.is_null() {
                return Ok(None);
            }
            let bytes = js_sys::Uint8Array::new(&value).to_vec();
            if bytes.len() > MAX_LIBRARY_JSON_BYTES {
                return Err("World library metadata exceeds its size limit".into());
            }
            Ok(Some(bytes))
        }
        WorldLibraryCommand::SaveMetadata { bytes, .. } => {
            if bytes.len() > MAX_LIBRARY_JSON_BYTES {
                return Err("World library metadata exceeds its size limit".into());
            }
            let store = database.transaction_with_str_and_mode(METADATA_STORE, IdbTransactionMode::Readwrite).map_err(js)?.object_store(METADATA_STORE).map_err(js)?;
            let request = store.put_with_key(&js_sys::Uint8Array::from(bytes.as_slice()), &JsValue::from_str(METADATA_KEY)).map_err(js)?;
            done(&request).await?;
            Ok(None)
        }
        WorldLibraryCommand::ReadAsset { asset_id, .. } => {
            let store = database.transaction_with_str(ASSET_STORE).map_err(js)?.object_store(ASSET_STORE).map_err(js)?;
            let value = done(&store.get(&JsValue::from_str(&asset_id)).map_err(js)?).await?;
            if value.is_undefined() || value.is_null() {
                return Err(format!("Library image asset {asset_id} was not found"));
            }
            let bytes = js_sys::Uint8Array::new(&value).to_vec();
            if bytes.is_empty() || bytes.len() > MAX_LIBRARY_ASSET_BYTES {
                return Err("Stored library asset has an invalid size".into());
            }
            Ok(Some(bytes))
        }
        WorldLibraryCommand::WriteAsset { asset_id, bytes, .. } => {
            if bytes.is_empty() || bytes.len() > MAX_LIBRARY_ASSET_BYTES {
                return Err("Library asset has an invalid size".into());
            }
            let store = database.transaction_with_str_and_mode(ASSET_STORE, IdbTransactionMode::Readwrite).map_err(js)?.object_store(ASSET_STORE).map_err(js)?;
            let request = store.put_with_key(&js_sys::Uint8Array::from(bytes.as_slice()), &JsValue::from_str(&asset_id)).map_err(js)?;
            done(&request).await?;
            Ok(None)
        }
        WorldLibraryCommand::DeleteAsset { asset_id, .. } => {
            let store = database.transaction_with_str_and_mode(ASSET_STORE, IdbTransactionMode::Readwrite).map_err(js)?.object_store(ASSET_STORE).map_err(js)?;
            let request = store.delete(&JsValue::from_str(&asset_id)).map_err(js)?;
            done(&request).await?;
            Ok(None)
        }
    }
}
