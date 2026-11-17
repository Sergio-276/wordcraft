//! Standalone workspace for the app-wide maps, points and characters library.

use crate::WordApp;
use crate::world_library::{CharacterRecord, LibrarySection};

pub(super) fn show(app: &mut WordApp, ui: &mut egui::Ui) {
    let mut changed = false;
    egui::CentralPanel::default().show_inside(ui, |ui| {
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
                ui.label("Your maps will appear here. Importing a map image is the next part of this workspace.");
                if app.world_library.library.maps.is_empty() {
                    ui.label("No maps have been added yet.");
                } else {
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        for map in &app.world_library.library.maps {
                            ui.group(|ui| {
                                ui.strong(&map.name);
                                if !map.description.is_empty() {
                                    ui.label(&map.description);
                                }
                            });
                        }
                    });
                }
            }
            LibrarySection::Points => {
                ui.heading("Points of interest");
                if app.world_library.library.points.is_empty() {
                    ui.label("No points of interest have been added yet. Add a map first.");
                } else {
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        for point in &app.world_library.library.points {
                            let map_name = app.world_library.library.maps.iter()
                                .find(|map| map.id == point.map_id)
                                .map(|map| map.name.as_str())
                                .unwrap_or("Unknown map");
                            ui.group(|ui| {
                                ui.strong(&point.name);
                                ui.small(map_name);
                                if !point.description.is_empty() {
                                    ui.label(&point.description);
                                }
                            });
                        }
                    });
                }
            }
            LibrarySection::Characters => {
                ui.heading("Characters");
                ui.label("Keep character records here, independently of the document you are editing.");
                ui.horizontal(|ui| {
                    ui.label("Name");
                    ui.text_edit_singleline(&mut app.world_library.character_name);
                });
                ui.label("Description");
                ui.add(egui::TextEdit::multiline(&mut app.world_library.character_description).desired_rows(3));
                let can_add = !app.world_library.character_name.trim().is_empty();
                if ui.add_enabled(can_add, egui::Button::new("Add character")).clicked() {
                    let id = format!(
                        "character-{}",
                        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)
                            .map(|duration| duration.as_nanos())
                            .unwrap_or_default()
                    );
                    app.world_library.library.characters.push(CharacterRecord {
                        id,
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
                        for (index, character) in app.world_library.library.characters.iter().enumerate() {
                            ui.group(|ui| {
                                ui.horizontal(|ui| {
                                    ui.strong(&character.name);
                                    if ui.small_button("Delete").clicked() {
                                        remove = Some(index);
                                    }
                                });
                                if !character.description.is_empty() {
                                    ui.label(&character.description);
                                }
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
