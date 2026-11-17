//! Standalone workspace for the app-wide maps, points and characters library.

use crate::WordApp;
use crate::world_library::{CharacterRecord, LibrarySection, MapRecord, PointOfInterest};

fn new_id(prefix: &str) -> String {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    format!("{prefix}-{stamp}")
}

fn map_texture(ctx: &egui::Context, asset_id: &str, bytes: &[u8]) -> Option<egui::TextureHandle> {
    let cache_id = egui::Id::new(("world_library_map_texture", asset_id));
    if let Some(texture) = ctx.data(|data| data.get_temp::<egui::TextureHandle>(cache_id)) {
        return Some(texture);
    }
    let image = wordcraft_geom::picture::decode(bytes, wordcraft_geom::MAX_DECODED_BYTES).ok()?;
    let rgba = image.to_rgba8();
    let color = egui::ColorImage::from_rgba_unmultiplied(
        [rgba.width() as usize, rgba.height() as usize],
        rgba.as_raw(),
    );
    let texture = ctx.load_texture(format!("world_library_map_{asset_id}"), color, egui::TextureOptions::LINEAR);
    ctx.data_mut(|data| data.insert_temp(cache_id, texture.clone()));
    Some(texture)
}

fn show_map_preview(
    ui: &mut egui::Ui,
    texture: &egui::TextureHandle,
    points: &[PointOfInterest],
    allow_placement: bool,
) -> Option<(f32, f32)> {
    let source = texture.size_vec2();
    let width = ui.available_width().clamp(180.0, 960.0);
    let height = (width * source.y / source.x.max(1.0)).min(560.0).max(120.0);
    let size = egui::vec2(width, height);
    let (rect, response) = ui.allocate_exact_size(size, if allow_placement { egui::Sense::click() } else { egui::Sense::hover() });
    ui.painter().image(
        texture.id(),
        rect,
        egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
        egui::Color32::WHITE,
    );
    for point in points {
        let position = egui::pos2(
            rect.left() + point.x.clamp(0.0, 1.0) * rect.width(),
            rect.top() + point.y.clamp(0.0, 1.0) * rect.height(),
        );
        ui.painter().circle_filled(position, 6.0, egui::Color32::from_rgb(220, 55, 55));
        ui.painter().text(
            position + egui::vec2(0.0, -8.0),
            egui::Align2::CENTER_BOTTOM,
            &point.name,
            egui::FontId::proportional(12.0),
            egui::Color32::WHITE,
        );
    }
    if allow_placement && response.clicked() {
        if let Some(position) = response.interact_pointer_pos() {
            return Some((
                ((position.x - rect.left()) / rect.width()).clamp(0.0, 1.0),
                ((position.y - rect.top()) / rect.height()).clamp(0.0, 1.0),
            ));
        }
    }
    None
}

pub(super) fn show(app: &mut WordApp, ui: &mut egui::Ui) {
    let mut changed = false;

    // Keep map image imports on their own host queue: they must never enter the document.
    if let Some(inbox) = &app.services.world_library_image_inbox {
        let picked = inbox.lock().unwrap_or_else(|error| error.into_inner()).drain(..).collect::<Vec<_>>();
        for (_name, bytes) in picked {
            let Some(map_id) = app.world_library.pending_map_image_for.take() else {
                app.world_library.error = Some("Choose a map before importing its image.".into());
                continue;
            };
            if let Some(bridge) = app.services.world_library.clone() {
                if let Err(error) = app.world_library.write_map_image(&bridge, map_id, bytes) {
                    app.world_library.error = Some(error);
                }
            } else {
                app.world_library.error = Some("Persistent library storage is unavailable.".into());
            }
        }
    }

    if let Some(bridge) = app.services.world_library.clone() {
        let asset_ids = app.world_library.library.maps.iter()
            .map(|map| map.image_asset_id.clone())
            .filter(|asset_id| !asset_id.is_empty())
            .collect::<Vec<_>>();
        for asset_id in asset_ids {
            if let Err(error) = app.world_library.request_map_image(&bridge, &asset_id) {
                app.world_library.error = Some(error);
            }
        }
    }

    let completions = app.world_library.take_asset_completions();
    for completion in completions {
        if let Some((map_id, asset_id)) = app.world_library.pending_map_image_writes.remove(&completion.request_id) {
            match completion.result {
                Ok(_) => {
                    if let Some(map) = app.world_library.library.maps.iter_mut().find(|map| map.id == map_id) {
                        map.image_asset_id = asset_id;
                        changed = true;
                    }
                }
                Err(error) => app.world_library.error = Some(error),
            }
        } else if let Some(asset_id) = app.world_library.pending_map_image_reads.remove(&completion.request_id) {
            match completion.result {
                Ok(Some(bytes)) => {
                    app.world_library.map_image_bytes.insert(asset_id, bytes);
                }
                Ok(None) => app.world_library.error = Some("A map image is missing from library storage.".into()),
                Err(error) => app.world_library.error = Some(error),
            }
        }
    }
    ui.vertical(|ui| {
        ui.horizontal(|ui| {
            ui.heading("World library");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("Back to editor").clicked() {
                    app.world_library.workspace_open = false;
                }
            });
        });
        ui.separator();
        ui.horizontal_wrapped(|ui| {
            for (section, label) in [
                (LibrarySection::Maps, "Maps"),
                (LibrarySection::Points, "Points of interest"),
                (LibrarySection::Characters, "Characters"),
            ] {
                if ui.selectable_label(app.world_library.section == section, label).clicked() {
                    app.world_library.section = section;
                }
            }
        });
        ui.separator();
        if !app.world_library.loaded {
            ui.spinner();
            ui.label("Loading your library…");
            return;
        }
        if let Some(error) = &app.world_library.error {
            ui.colored_label(ui.visuals().error_fg_color, error);
        }
        match app.world_library.section {
            LibrarySection::Maps => {
                ui.heading("Maps");
                ui.label("Create maps and import their background images. Interactive point placement is managed in the Points of interest section.");
                ui.label("Name");
                ui.text_edit_singleline(&mut app.world_library.map_name);
                ui.label("Description");
                ui.add(egui::TextEdit::multiline(&mut app.world_library.map_description).desired_rows(2));
                if ui.add_enabled(!app.world_library.map_name.trim().is_empty(), egui::Button::new("Add map")).clicked() {
                    app.world_library.library.maps.push(MapRecord {
                        id: new_id("map"),
                        name: app.world_library.map_name.trim().to_owned(),
                        description: app.world_library.map_description.trim().to_owned(),
                        image_asset_id: String::new(),
                    });
                    app.world_library.map_name.clear();
                    app.world_library.map_description.clear();
                    changed = true;
                }
                ui.separator();
                if app.world_library.library.maps.is_empty() {
                    ui.label("No maps have been added yet.");
                } else {
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        let mut remove = None;
                        for (index, map) in app.world_library.library.maps.iter_mut().enumerate() {
                            ui.group(|ui| {
                                ui.horizontal(|ui| {
                                    changed |= ui.text_edit_singleline(&mut map.name).changed();
                                    if ui.small_button("Delete map").clicked() {
                                        remove = Some(index);
                                    }
                                });
                                changed |= ui.add(egui::TextEdit::multiline(&mut map.description).desired_rows(2)).changed();
                                let map_id = map.id.clone();
                                if ui.button(if map.image_asset_id.is_empty() { "Import map image" } else { "Replace map image" }).clicked() {
                                    app.world_library.pending_map_image_for = Some(map_id);
                                    if let Some(picker) = &app.services.world_library_image_picker {
                                        picker();
                                    } else {
                                        app.world_library.error = Some("Map image import is unavailable on this platform.".into());
                                    }
                                }
                                if map.image_asset_id.is_empty() {
                                    ui.weak("No map image imported yet.");
                                } else {
                                    ui.weak("Map image stored.");
                                }
                            });
                        }
                        if let Some(index) = remove {
                            let map = app.world_library.library.maps.remove(index);
                            app.world_library.library.points.retain(|point| point.map_id != map.id);
                            changed = true;
                        }
                    });
                }
            }
            LibrarySection::Points => {
                ui.heading("Points of interest");
                if app.world_library.library.maps.is_empty() {
                    ui.label("Add a map before creating points of interest.");
                } else {
                    if app.world_library.point_map_id.as_ref().is_none_or(|id| !app.world_library.library.maps.iter().any(|map| &map.id == id)) {
                        app.world_library.point_map_id = app.world_library.library.maps.first().map(|map| map.id.clone());
                    }
                    ui.label("Map");
                    egui::ComboBox::from_id_salt("world-library-point-map")
                        .selected_text(app.world_library.point_map_id.as_ref()
                            .and_then(|id| app.world_library.library.maps.iter().find(|map| &map.id == id))
                            .map(|map| map.name.as_str()).unwrap_or("Choose map"))
                        .show_ui(ui, |ui| {
                            for map in &app.world_library.library.maps {
                                ui.selectable_value(&mut app.world_library.point_map_id, Some(map.id.clone()), &map.name);
                            }
                        });
                    ui.label("Name");
                    ui.text_edit_singleline(&mut app.world_library.point_name);
                    ui.label("Description");
                    ui.add(egui::TextEdit::multiline(&mut app.world_library.point_description).desired_rows(2));
                    ui.add(egui::Slider::new(&mut app.world_library.point_x, 0.0..=1.0).text("Horizontal position"));
                    ui.add(egui::Slider::new(&mut app.world_library.point_y, 0.0..=1.0).text("Vertical position"));
                    if ui.add_enabled(!app.world_library.point_name.trim().is_empty() && app.world_library.point_map_id.is_some(), egui::Button::new("Add point")).clicked() {
                        if let Some(map_id) = app.world_library.point_map_id.clone() {
                            app.world_library.library.points.push(PointOfInterest {
                                id: new_id("point"),
                                map_id,
                                name: app.world_library.point_name.trim().to_owned(),
                                description: app.world_library.point_description.trim().to_owned(),
                                x: app.world_library.point_x.clamp(0.0, 1.0),
                                y: app.world_library.point_y.clamp(0.0, 1.0),
                                image_asset_ids: Vec::new(),
                            });
                            app.world_library.point_name.clear();
                            app.world_library.point_description.clear();
                            changed = true;
                        }
                    }
                }
                ui.separator();
                if app.world_library.library.points.is_empty() {
                    ui.label("No points of interest have been added yet.");
                } else {
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        let mut remove = None;
                        for (index, point) in app.world_library.library.points.iter_mut().enumerate() {
                            let map_name = app.world_library.library.maps.iter()
                                .find(|map| map.id == point.map_id)
                                .map(|map| map.name.clone()).unwrap_or_else(|| "Unknown map".to_owned());
                            ui.group(|ui| {
                                ui.horizontal(|ui| {
                                    changed |= ui.text_edit_singleline(&mut point.name).changed();
                                    if ui.small_button("Delete").clicked() {
                                        remove = Some(index);
                                    }
                                });
                                ui.small(map_name);
                                changed |= ui.add(egui::TextEdit::multiline(&mut point.description).desired_rows(2)).changed();
                                changed |= ui.add(egui::Slider::new(&mut point.x, 0.0..=1.0).text("Horizontal")).changed();
                                changed |= ui.add(egui::Slider::new(&mut point.y, 0.0..=1.0).text("Vertical")).changed();
                            });
                        }
                        if let Some(index) = remove {
                            app.world_library.library.points.remove(index);
                            changed = true;
                        }
                    });
                }
            }
            LibrarySection::Characters => {
                ui.heading("Characters");
                ui.label("Keep character records here, independently of the document you are editing.");
                ui.label("Name");
                ui.text_edit_singleline(&mut app.world_library.character_name);
                ui.label("Description");
                ui.add(egui::TextEdit::multiline(&mut app.world_library.character_description).desired_rows(3));
                if ui.add_enabled(!app.world_library.character_name.trim().is_empty(), egui::Button::new("Add character")).clicked() {
                    app.world_library.library.characters.push(CharacterRecord {
                        id: new_id("character"),
                        name: app.world_library.character_name.trim().to_owned(),
                        description: app.world_library.character_description.trim().to_owned(),
                        portrait_asset_id: None,
                    });
                    app.world_library.character_name.clear();
                    app.world_library.character_description.clear();
                    changed = true;
                }
                ui.separator();
                if app.world_library.library.characters.is_empty() {
                    ui.label("No characters have been added yet.");
                } else {
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        let mut remove = None;
                        for (index, character) in app.world_library.library.characters.iter_mut().enumerate() {
                            ui.group(|ui| {
                                ui.horizontal(|ui| {
                                    changed |= ui.text_edit_singleline(&mut character.name).changed();
                                    if ui.small_button("Delete").clicked() {
                                        remove = Some(index);
                                    }
                                });
                                changed |= ui.add(egui::TextEdit::multiline(&mut character.description).desired_rows(2)).changed();
                            });
                        }
                        if let Some(index) = remove {
                            app.world_library.library.characters.remove(index);
                            changed = true;
                        }
                    });
                }
            }
        }
    });
    if changed {
        if let Some(bridge) = app.services.world_library.clone() {
            if let Err(error) = app.world_library.save(&bridge) {
                app.world_library.error = Some(error);
            }
        } else {
            app.world_library.error = Some("Persistent library storage is unavailable.".into());
        }
    }
}
