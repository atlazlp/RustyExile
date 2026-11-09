use eframe::egui::{self, Color32, RichText, TextureHandle, TextureOptions};
use poe_core::{
    build_route, fetch_pob, install_build, vendor_gem_regex, visible_lines, AppState, CampaignEvent,
    CharacterFile,
    EventGroup, EventKind, Experience, HudCorner, TriggerKind,
};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

pub fn run(state: Arc<Mutex<AppState>>) -> eframe::Result {
    eframe::run_native(
        "PoE Campaign",
        eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default()
                .with_inner_size([1120.0, 760.0])
                .with_title("PoE Campaign"),
            ..Default::default()
        },
        Box::new(|cc| Ok(Box::new(UiApp::new(cc, state)))),
    )
}

struct UiApp {
    state: Arc<Mutex<AppState>>,
    icons: HashMap<String, TextureHandle>,
    selected: Option<String>,
    capture: Option<&'static str>,
    tab: Tab,
    search: String,
    new_name: String,
    new_class: String,
    new_pob: String,
    ctx: egui::Context,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    General,
    Hotkeys,
    Acts,
    Gems,
    Builds,
    Database,
    Regex,
}

impl UiApp {
    fn new(cc: &eframe::CreationContext<'_>, state: Arc<Mutex<AppState>>) -> Self {
        Self {
            state,
            icons: HashMap::new(),
            selected: None,
            capture: None,
            tab: Tab::General,
            search: String::new(),
            new_name: String::new(),
            new_class: "Templar".into(),
            new_pob: String::new(),
            ctx: cc.egui_ctx.clone(),
        }
    }

    fn icon(&mut self, key: &str) -> Option<TextureHandle> {
        if let Some(tex) = self.icons.get(key) {
            return Some(tex.clone());
        }
        let image = poe_data::load_icon(key)?;
        let color = egui::ColorImage::from_rgba_unmultiplied(
            [image.width() as usize, image.height() as usize],
            image.as_raw(),
        );
        let tex = self.ctx.load_texture(key, color, TextureOptions::LINEAR);
        self.icons.insert(key.to_string(), tex.clone());
        Some(tex)
    }

    fn icon_ui(&mut self, ui: &mut egui::Ui, key: &str, size: f32) {
        if let Some(tex) = self.icon(key) {
            ui.image((tex.id(), egui::vec2(size, size)));
        } else {
            let (rect, _) = ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::hover());
            ui.painter().circle_filled(rect.center(), size * 0.35, Color32::from_rgb(180, 140, 60));
        }
    }
}

impl eframe::App for UiApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.ctx = ctx.clone();
        let chosen = self.state.lock().map(|s| s.settings.experience_chosen).unwrap_or(false);
        if !chosen {
            self.wizard(ctx);
            return;
        }
        let show = self.state.lock().map(|s| s.show_config).unwrap_or(false);
        if show {
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
            ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
            if let Ok(mut state) = self.state.lock() {
                state.show_config = false;
            }
        }
        if ctx.input(|i| i.viewport().close_requested()) {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
        }
        self.capture_hotkey(ctx);
        ctx.request_repaint_after(std::time::Duration::from_millis(200));
        egui::TopBottomPanel::top("tabs").show(ctx, |ui| {
            ui.horizontal_wrapped(|ui| {
                for (tab, label, icon) in [
                    (Tab::General, "General", "town"),
                    (Tab::Hotkeys, "Hotkeys", "portalscroll"),
                    (Tab::Acts, "Acts", "waypoint"),
                    (Tab::Gems, "Gems", "gem:Fireball"),
                    (Tab::Builds, "Builds", "trial"),
                    (Tab::Database, "Database", "quest"),
                    (Tab::Regex, "Regex", "wisdom"),
                ] {
                    if let Some(tex) = self.icon(icon) {
                        ui.image((tex.id(), egui::vec2(16.0, 16.0)));
                    }
                    if ui.selectable_label(self.tab == tab, label).clicked() {
                        self.tab = tab;
                    }
                    ui.add_space(8.0);
                }
            });
        });
        egui::CentralPanel::default().show(ctx, |ui| match self.tab {
            Tab::General => self.general(ui),
            Tab::Hotkeys => self.hotkeys(ui),
            Tab::Acts => self.events(ui, false),
            Tab::Gems => self.events(ui, true),
            Tab::Builds => self.builds(ui),
            Tab::Database => self.database(ui),
            Tab::Regex => self.regex_tab(ui),
        });
    }
}

impl UiApp {
    fn wizard(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("How do you want the overlay to start?");
            ui.label("This only sets the first-run defaults. Every option here can be changed later in the configuration.");
            ui.add_space(12.0);
            for experience in [Experience::Beginner, Experience::Intermediate, Experience::Advanced] {
                ui.group(|ui| {
                    ui.heading(experience.label());
                    ui.label(experience.explanation());
                    if ui.button(format!("Start as {}", experience.label())).clicked() {
                        if let Ok(mut state) = self.state.lock() {
                            state.settings.apply_experience(experience);
                            state.dirty_settings = true;
                        }
                    }
                });
                ui.add_space(8.0);
            }
        });
    }

    fn general(&mut self, ui: &mut egui::Ui) {
        let Ok(mut state) = self.state.lock() else { return };
        ui.horizontal(|ui| {
            ui.label("Profile");
            for experience in [Experience::Beginner, Experience::Intermediate, Experience::Advanced] {
                if ui.selectable_label(state.settings.experience == experience, experience.label()).clicked() {
                    state.settings.apply_experience(experience);
                    state.dirty_settings = true;
                }
            }
        });
        ui.label(state.settings.experience.explanation());
        if ui.checkbox(&mut state.settings.tips, "Zone tips").changed() {
            state.dirty_settings = true;
        }
        if ui.checkbox(&mut state.settings.acts, "Act list").changed() {
            state.dirty_settings = true;
        }
        if ui.checkbox(&mut state.settings.gems, "Gem list").changed() {
            state.dirty_settings = true;
        }
        if ui.checkbox(&mut state.settings.league_start, "League-start route").changed() {
            state.dirty_settings = true;
        }
        if ui.checkbox(&mut state.hud_enabled, "Show the overlay while Path of Exile is focused").changed() {
            state.dirty_settings = true;
        }
        ui.horizontal(|ui| {
            ui.label("Opacity");
            if ui.add(egui::Slider::new(&mut state.settings.opacity, 0.2..=1.0)).changed() {
                state.dirty_settings = true;
            }
        });
        ui.horizontal(|ui| {
            ui.label("Scale");
            if ui.add(egui::Slider::new(&mut state.settings.scale, 0.7..=2.0)).changed() {
                state.dirty_settings = true;
            }
        });
        ui.horizontal(|ui| {
            ui.label("Corner");
            for corner in [HudCorner::TopLeft, HudCorner::TopRight, HudCorner::BottomLeft, HudCorner::BottomRight] {
                let label = format!("{corner:?}");
                if ui.selectable_label(state.settings.corner == corner, label).clicked() {
                    state.settings.corner = corner;
                    state.dirty_settings = true;
                }
            }
        });
        if color_row(ui, "Background", &mut state.settings.theme.background) {
            state.dirty_settings = true;
        }
        if color_row(ui, "Text", &mut state.settings.theme.text) {
            state.dirty_settings = true;
        }
        if color_row(ui, "Accent", &mut state.settings.theme.accent) {
            state.dirty_settings = true;
        }
        ui.separator();
        if let Some(install) = &state.settings.install {
            ui.label(format!("Install: {:?}", install.kind));
            ui.label(format!("Folder: {}", install.install_dir));
            ui.label(format!("Log: {}", install.log_path));
        } else if let Some(running) = &state.running {
            ui.label(format!("Detected {:?} at {}", running.kind, running.install_dir.display()));
        } else {
            ui.label("Path of Exile is not running. The folder and log path are saved the next time it is detected.");
        }
        let mut override_path = state.settings.log_path_override.clone().unwrap_or_default();
        ui.horizontal(|ui| {
            ui.label("Log override");
            if ui.text_edit_singleline(&mut override_path).changed() {
                state.settings.log_path_override = if override_path.is_empty() { None } else { Some(override_path) };
                state.dirty_settings = true;
            }
        });
        ui.hyperlink_to("poe-leveling.com", &state.settings.poe_leveling_url);
        ui.label(state.status.as_str());
    }

    fn hotkeys(&mut self, ui: &mut egui::Ui) {
        ui.label("Hotkeys are delivered while the game is focused. Ctrl+Alt+click on a task clears it even if its end zone has not been entered.");
        self.hotkey_row(ui, "Toggle overlay", "toggle_hud");
        self.hotkey_row(ui, "Open configuration", "open_config");
        self.hotkey_row(ui, "Paste vendor regex", "paste_regex");
        self.hotkey_row(ui, "Clear-task modifiers", "clear_modifiers");
        if self.capture.is_some() {
            ui.colored_label(Color32::LIGHT_YELLOW, "Press the new chord. Escape cancels.");
        }
    }

    fn hotkey_row(&mut self, ui: &mut egui::Ui, label: &str, field: &'static str) {
        let current = self.state.lock().ok().map(|s| match field {
            "toggle_hud" => s.settings.hotkeys.toggle_hud.clone(),
            "open_config" => s.settings.hotkeys.open_config.clone(),
            "paste_regex" => s.settings.hotkeys.paste_regex.clone(),
            _ => s.settings.hotkeys.clear_modifiers.clone(),
        }).unwrap_or_default();
        ui.horizontal(|ui| {
            ui.label(label);
            ui.monospace(&current);
            if ui.button("Change").clicked() {
                self.capture = Some(field);
            }
        });
    }

    fn capture_hotkey(&mut self, ctx: &egui::Context) {
        let Some(field) = self.capture else { return };
        let mut pressed = None;
        ctx.input(|input| {
            for event in &input.events {
                if let egui::Event::Key { key, pressed: true, modifiers, .. } = event {
                    if *key == egui::Key::Escape {
                        pressed = Some(String::new());
                    } else {
                        let mut parts = Vec::new();
                        if modifiers.ctrl { parts.push("ctrl".to_string()); }
                        if modifiers.alt { parts.push("alt".to_string()); }
                        if modifiers.shift { parts.push("shift".to_string()); }
                        parts.push(format!("{key:?}").to_ascii_lowercase());
                        pressed = Some(parts.join("+"));
                    }
                }
            }
        });
        if let Some(chord) = pressed {
            self.capture = None;
            if chord.is_empty() {
                return;
            }
            if let Ok(mut state) = self.state.lock() {
                match field {
                    "toggle_hud" => state.settings.hotkeys.toggle_hud = chord,
                    "open_config" => state.settings.hotkeys.open_config = chord,
                    "paste_regex" => state.settings.hotkeys.paste_regex = chord,
                    _ => state.settings.hotkeys.clear_modifiers = chord,
                }
                state.dirty_settings = true;
            }
        }
    }

    fn events(&mut self, ui: &mut egui::Ui, gems: bool) {
        let snapshot = self.state.lock().ok().and_then(|s| s.active_character().cloned());
        let Some(character) = snapshot else {
            ui.label("Add a character on the Builds tab. The act list is saved separately for each character.");
            return;
        };
        let mut delete_id = None;
        let mut drop_pair = None;
        let mut edit: Option<CampaignEvent> = None;
        self.groups(ui, gems);
        egui::ScrollArea::vertical().show(ui, |ui| {
            for act in 1..=10u8 {
                let events: Vec<_> = character.route.events.iter().filter(|e| e.act == act && e.kind.is_gem() == gems).cloned().collect();
                if events.is_empty() && gems {
                    continue;
                }
                egui::CollapsingHeader::new(RichText::new(format!("Act {act}")).strong()).default_open(act == 1).show(ui, |ui| {
                    for event in events {
                        let icon = if event.gem_name.is_some() { format!("gem:{}", event.gem_name.as_deref().unwrap_or("")) } else { event.icon.clone() };
                        ui.horizontal(|ui| {
                            self.icon_ui(ui, &icon, 22.0);
                            if event.kind == EventKind::GemBuy {
                                self.icon_ui(ui, "wisdom", 14.0);
                            }
                            let response = ui.add(egui::Label::new(&event.text).sense(egui::Sense::click_and_drag()));
                            if response.drag_started() {
                                response.dnd_set_drag_payload(event.id.clone());
                            }
                            if let Some(payload) = response.dnd_release_payload::<String>() {
                                if payload.as_str() != event.id {
                                    drop_pair = Some((payload.to_string(), event.id.clone()));
                                }
                            }
                            if ui.button("Edit").clicked() {
                                self.selected = Some(event.id.clone());
                            }
                            if ui.button("Delete").clicked() {
                                delete_id = Some(event.id.clone());
                            }
                        });
                        if self.selected.as_deref() == Some(event.id.as_str()) {
                            edit = Some(event);
                        }
                    }
                });
            }
        });
        if let Some(event) = edit {
            self.edit_event(ui, &event);
        }
        if delete_id.is_some() || drop_pair.is_some() {
            if let Ok(mut state) = self.state.lock() {
                if let Some(character) = state.active_character_mut() {
                    character.user_edited = true;
                    if let Some(id) = delete_id {
                        character.route.events.retain(|e| e.id != id);
                        for group in &mut character.route.groups {
                            group.event_ids.retain(|event_id| event_id != &id);
                        }
                    }
                    if let Some((from, onto)) = drop_pair {
                        group_drop(&mut character.route, &from, &onto);
                    }
                }
                state.dirty_characters = true;
            }
        }
    }

    fn groups(&mut self, ui: &mut egui::Ui, gems: bool) {
        let groups = self.state.lock().ok().and_then(|state| {
            state.active_character().map(|character| {
                character
                    .route
                    .groups
                    .iter()
                    .filter(|group| {
                        group.event_ids.iter().any(|id| {
                            character
                                .route
                                .events
                                .iter()
                                .any(|event| event.id == *id && event.kind.is_gem() == gems)
                        })
                    })
                    .map(|group| (group.id.clone(), group.name.clone(), group.start.clone()))
                    .collect::<Vec<_>>()
            })
        }).unwrap_or_default();
        if groups.is_empty() {
            return;
        }
        egui::CollapsingHeader::new("Groups").show(ui, |ui| {
            ui.label("Drop one event onto another to make a named group. Its start trigger shows every event in the group.");
            for (id, mut name, mut start) in groups {
                ui.horizontal(|ui| {
                    ui.label("Name");
                    if ui.text_edit_singleline(&mut name).changed() {
                        let id = id.clone();
                        if let Ok(mut state) = self.state.lock() {
                            if let Some(character) = state.active_character_mut() {
                                if let Some(group) = character.route.groups.iter_mut().find(|group| group.id == id) {
                                    group.name = name;
                                    character.user_edited = true;
                                }
                            }
                            state.dirty_characters = true;
                        }
                    }
                });
                if trigger_list(ui, &mut start) {
                    if let Ok(mut state) = self.state.lock() {
                        if let Some(character) = state.active_character_mut() {
                            if let Some(group) = character.route.groups.iter_mut().find(|group| group.id == id) {
                                group.start = start;
                                character.user_edited = true;
                            }
                        }
                        state.dirty_characters = true;
                    }
                }
            }
        });
    }

    fn edit_event(&mut self, ui: &mut egui::Ui, event: &CampaignEvent) {
        ui.separator();
        ui.heading("Edit event");
        let mut text = event.text.clone();
        let mut detail = event.detail.clone();
        let mut changed = false;
        ui.horizontal(|ui| {
            ui.label("Text");
            changed |= ui.text_edit_singleline(&mut text).changed();
        });
        ui.horizontal(|ui| {
            ui.label("Tip");
            changed |= ui.text_edit_singleline(&mut detail).changed();
        });
        ui.label("Start triggers");
        let mut start = event.start.clone();
        let mut end = event.end.clone();
        changed |= trigger_list(ui, &mut start);
        ui.label("End triggers");
        changed |= trigger_list(ui, &mut end);
        if changed {
            if let Ok(mut state) = self.state.lock() {
                if let Some(character) = state.active_character_mut() {
                    if let Some(found) = character.route.events.iter_mut().find(|e| e.id == event.id) {
                        found.text = text;
                        found.detail = detail;
                        found.start = start;
                        found.end = end;
                        character.user_edited = true;
                    }
                }
                state.dirty_characters = true;
            }
        }
    }

    fn builds(&mut self, ui: &mut egui::Ui) {
        let names = self.state.lock().ok().map(|s| s.characters.iter().map(|c| (c.name.clone(), c.class_name.clone(), c.route.pob_gems.len(), c.route.tree_specs.iter().map(|t| t.nodes.len()).sum::<usize>())).collect::<Vec<_>>()).unwrap_or_default();
        let active = self.state.lock().ok().and_then(|s| s.settings.active_character.clone());
        for (name, class_name, gems, nodes) in names {
            ui.horizontal(|ui| {
                self.icon_ui(ui, "trial", 18.0);
                if ui.selectable_label(active.as_deref() == Some(name.as_str()), format!("{name}  {class_name}  {gems} gems  {nodes} tree nodes")).clicked() {
                    if let Ok(mut state) = self.state.lock() {
                        state.settings.active_character = Some(name);
                        state.dirty_settings = true;
                    }
                }
            });
        }
        ui.separator();
        ui.label("Add a character, then paste a Path of Building Community link. The export is usually a pobb.in or pastebin link. Gems that can be picked up or bought during the campaign are added to that character's acts.");
        ui.horizontal(|ui| {
            ui.label("Name");
            ui.text_edit_singleline(&mut self.new_name);
            ui.label("Class");
            egui::ComboBox::from_id_salt("class").selected_text(&self.new_class).show_ui(ui, |ui| {
                for class_name in ["Scion", "Marauder", "Ranger", "Witch", "Duelist", "Templar", "Shadow"] {
                    ui.selectable_value(&mut self.new_class, class_name.to_string(), class_name);
                }
            });
        });
        ui.horizontal(|ui| {
            ui.label("PoB link");
            ui.text_edit_singleline(&mut self.new_pob);
            if ui.button("Import").clicked() {
                self.import_build();
            }
        });
        if let Ok(state) = self.state.lock() {
            if let Some(character) = state.active_character() {
                if !character.route.pob_notes.is_empty() {
                    ui.separator();
                    ui.label("Build notes");
                    ui.label(&character.route.pob_notes);
                }
                ui.label("The passive tree from this build is saved and will be drawn on the in-game tree in a later version.");
            }
        }
    }

    fn import_build(&mut self) {
        let name = self.new_name.trim().to_string();
        if name.is_empty() || self.new_pob.trim().is_empty() {
            return;
        }
        let link = self.new_pob.clone();
        let class_name = self.new_class.clone();
        let fetched = fetch_pob(&link)
            .map_err(|err| err.to_string())
            .and_then(|body| poe_core::decode_pob(&body).map_err(|err| err.to_string()));
        let Ok(mut state) = self.state.lock() else { return };
        match fetched {
            Ok(mut build) => {
                if build.class_name.is_empty() {
                    build.class_name = class_name.clone();
                }
                let league_start = state.settings.league_start;
                if !state.characters.iter().any(|c| c.name == name) {
                    let route = build_route(&state.catalog, &build.class_name, None, league_start);
                    state.characters.push(CharacterFile {
                        name: name.clone(),
                        class_name: build.class_name.clone(),
                        route,
                        progress: Default::default(),
                        user_edited: false,
                    });
                }
                state.settings.active_character = Some(name.clone());
                let catalog = state.catalog.clone();
                if let Some(character) = state.characters.iter_mut().find(|c| c.name == name) {
                    install_build(character, &catalog, &build, league_start);
                    character.route.pob_url = link;
                }
                state.dirty_characters = true;
                state.dirty_settings = true;
                state.status = format!("Imported {} gems for {name}.", build.gems.len());
            }
            Err(err) => state.status = format!("Could not import that build: {err}"),
        }
    }

    fn database(&mut self, ui: &mut egui::Ui) {
        ui.label("This is the shared catalog new characters are copied from. Editing it does not rewrite characters you already created.");
        ui.horizontal(|ui| {
            ui.label("Search");
            ui.text_edit_singleline(&mut self.search);
        });
        let query = self.search.to_ascii_lowercase();
        let Ok(mut state) = self.state.lock() else { return };
        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.heading("Gems");
            let mut gem_delete = None;
            let mut dirty = false;
            for (index, gem) in state.catalog.gems.iter_mut().enumerate() {
                if !query.is_empty() && !gem.name.to_ascii_lowercase().contains(&query) {
                    continue;
                }
                ui.horizontal(|ui| {
                    ui.label(&gem.id);
                    if ui.text_edit_singleline(&mut gem.name).changed() {
                        dirty = true;
                    }
                    if ui.button("Delete").clicked() {
                        gem_delete = Some(index);
                    }
                });
                if index > 40 && query.is_empty() {
                    ui.label("Search to see more gems.");
                    break;
                }
            }
            if let Some(index) = gem_delete {
                state.catalog.gems.remove(index);
                dirty = true;
            }
            state.dirty_catalog |= dirty;
            ui.heading("Areas");
            let mut shown = 0;
            let mut dirty = false;
            for area in &mut state.catalog.areas {
                if !query.is_empty() && !area.name.to_ascii_lowercase().contains(&query) && !area.id.contains(&query) {
                    continue;
                }
                ui.horizontal(|ui| {
                    ui.monospace(&area.id);
                    if ui.text_edit_singleline(&mut area.name).changed() {
                        dirty = true;
                    }
                    if ui.add(egui::DragValue::new(&mut area.act).range(1..=10)).changed() {
                        dirty = true;
                    }
                });
                shown += 1;
                if shown > 40 && query.is_empty() {
                    break;
                }
            }
            state.dirty_catalog |= dirty;
            ui.heading("Quests");
            shown = 0;
            let mut dirty = false;
            for quest in &mut state.catalog.quests {
                if !query.is_empty() && !quest.name.to_ascii_lowercase().contains(&query) {
                    continue;
                }
                ui.horizontal(|ui| {
                    ui.monospace(&quest.id);
                    if ui.text_edit_singleline(&mut quest.name).changed() {
                        dirty = true;
                    }
                    ui.label("NPC");
                    if ui.text_edit_singleline(&mut quest.npc).changed() {
                        dirty = true;
                    }
                });
                shown += 1;
                if shown > 30 && query.is_empty() {
                    break;
                }
            }
            state.dirty_catalog |= dirty;
        });
    }

    fn regex_tab(&mut self, ui: &mut egui::Ui) {
        let gem_names = self.state.lock().ok().and_then(|s| {
            s.active_character().map(|c| {
                visible_lines(&c.route, &c.progress, s.settings.acts, s.settings.gems, s.settings.tips)
                    .into_iter()
                    .filter_map(|line| line.gem_name)
                    .collect::<Vec<_>>()
            })
        }).unwrap_or_default();
        let generated = vendor_gem_regex(&gem_names);
        ui.label("Generated from the gems still showing for this character:");
        ui.monospace(&generated);
        if ui.button("Paste generated regex into the game").clicked() {
            if let Ok(mut state) = self.state.lock() {
                state.status = format!("paste:{generated}");
            }
        }
        ui.separator();
        ui.label("Presets imported from poe-leveling-overlay. Edit them here.");
        let mut presets = self.state.lock().ok().map(|s| s.regex_presets.clone()).unwrap_or_default();
        let mut active = self.state.lock().ok().map(|s| s.settings.active_regex).unwrap_or(0);
        let mut changed = false;
        let mut delete = None;
        for (index, preset) in presets.iter_mut().enumerate() {
            ui.horizontal(|ui| {
                if ui.radio(active == index, "").clicked() {
                    active = index;
                    changed = true;
                }
                changed |= ui.text_edit_singleline(&mut preset.name).changed();
                if ui.button("Delete").clicked() {
                    delete = Some(index);
                }
            });
            changed |= ui.text_edit_singleline(&mut preset.regex).changed();
        }
        if ui.button("Add preset").clicked() {
            presets.push(poe_core::RegexPreset { name: "Custom".into(), regex: String::new() });
            changed = true;
        }
        if let Some(index) = delete {
            presets.remove(index);
            changed = true;
        }
        if changed {
            if let Ok(mut state) = self.state.lock() {
                state.regex_presets = presets;
                state.settings.active_regex = active;
                state.dirty_regex = true;
                state.dirty_settings = true;
            }
        }
    }
}

fn color_row(ui: &mut egui::Ui, label: &str, color: &mut [u8; 4]) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.label(label);
        let mut c = Color32::from_rgba_unmultiplied(color[0], color[1], color[2], color[3]);
        if ui.color_edit_button_srgba(&mut c).changed() {
            *color = [c.r(), c.g(), c.b(), c.a()];
            changed = true;
        }
    });
    changed
}

fn trigger_list(ui: &mut egui::Ui, triggers: &mut Vec<TriggerKind>) -> bool {
    let mut changed = false;
    let mut delete = None;
    for (index, trigger) in triggers.iter_mut().enumerate() {
        ui.horizontal(|ui| {
            ui.label(trigger.label());
            match trigger {
                TriggerKind::ZoneEnter { area_id } => changed |= ui.text_edit_singleline(area_id).changed(),
                TriggerKind::ZoneName { name } => changed |= ui.text_edit_singleline(name).changed(),
                TriggerKind::ActStart { act } | TriggerKind::ActEnd { act } => {
                    changed |= ui.add(egui::DragValue::new(act).range(1..=10)).changed();
                }
                TriggerKind::LevelAtLeast { level } => changed |= ui.add(egui::DragValue::new(level)).changed(),
                TriggerKind::PassiveAllocated { id } => changed |= ui.text_edit_singleline(id).changed(),
                TriggerKind::Manual => {}
            }
            if ui.button("x").clicked() {
                delete = Some(index);
            }
        });
    }
    if let Some(index) = delete {
        triggers.remove(index);
        changed = true;
    }
    if ui.button("Add zone trigger").clicked() {
        triggers.push(TriggerKind::ZoneEnter { area_id: String::new() });
        changed = true;
    }
    changed
}

fn group_drop(route: &mut poe_core::CharacterRoute, from: &str, onto: &str) {
    for group in &mut route.groups {
        group.event_ids.retain(|id| id != from);
    }
    if let Some(group) = route.groups.iter_mut().find(|g| g.event_ids.iter().any(|id| id == onto)) {
        if !group.event_ids.iter().any(|id| id == from) {
            group.event_ids.push(from.to_string());
        }
        return;
    }
    let act = route.events.iter().find(|e| e.id == onto).map(|e| e.act).unwrap_or(1);
    let start = route.events.iter().find(|e| e.id == onto).map(|e| e.start.clone()).unwrap_or_default();
    route.groups.push(EventGroup {
        id: format!("user-{from}-{onto}"),
        act,
        name: "Group".into(),
        start,
        event_ids: vec![onto.to_string(), from.to_string()],
    });
}
