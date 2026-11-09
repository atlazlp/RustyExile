use eframe::egui::{self, Color32, RichText, TextureHandle, TextureOptions};
use poe_core::{
    build_route, fetch_pob, install_build, vendor_gem_regex, visible_lines, AppState, CampaignEvent,
    CharacterFile, ChatCommand, EventGroup, EventKind, Experience, HudCorner, TriggerKind, UpdateOutcome,
    VERSION,
};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

pub fn run(state: Arc<Mutex<AppState>>) -> eframe::Result {
    eframe::run_native(
        "RustyExile",
        eframe::NativeOptions {
            vsync: false,
            viewport: egui::ViewportBuilder::default()
                .with_inner_size([1240.0, 820.0])
                .with_min_inner_size([980.0, 640.0])
                .with_title("RustyExile")
                .with_app_id("RustyExile"),
            ..Default::default()
        },
        Box::new(|cc| Ok(Box::new(UiApp::new(cc, state)))),
    )
}

struct UiApp {
    state: Arc<Mutex<AppState>>,
    icons: HashMap<String, TextureHandle>,
    selected: Option<String>,
    capture: Option<Capture>,
    tab: Tab,
    search: String,
    new_name: String,
    new_pob: String,
    launch_note: String,
    launch_rx: Option<std::sync::mpsc::Receiver<String>>,
    update_note: String,
    update_rx: Option<std::sync::mpsc::Receiver<UpdateOutcome>>,
    update_manual: bool,
    start_update: bool,
    ctx: egui::Context,
}

#[derive(Clone)]
enum Capture {
    Builtin(&'static str),
    Chat(String),
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
    Landmarks,
}

impl Tab {
    fn all() -> [Self; 8] {
        [
            Self::General,
            Self::Hotkeys,
            Self::Acts,
            Self::Gems,
            Self::Builds,
            Self::Database,
            Self::Regex,
            Self::Landmarks,
        ]
    }

    fn label(self) -> &'static str {
        match self {
            Self::General => "General",
            Self::Hotkeys => "Hotkeys",
            Self::Acts => "Acts",
            Self::Gems => "Gems",
            Self::Builds => "Builds",
            Self::Database => "Database",
            Self::Regex => "Regex",
            Self::Landmarks => "Landmarks",
        }
    }

    fn icon(self) -> &'static str {
        match self {
            Self::General => "town",
            Self::Hotkeys => "portalscroll",
            Self::Acts => "waypoint",
            Self::Gems => "gem:Fireball",
            Self::Builds => "trial",
            Self::Database => "quest",
            Self::Regex => "wisdom",
            Self::Landmarks => "crafting",
        }
    }

    fn blurb(self) -> &'static str {
        match self {
            Self::General => "Profile, overlay visibility, and where the panel sits on the game.",
            Self::Hotkeys => "Chords that run while Path of Exile is focused.",
            Self::Acts => "Campaign steps for the active character, one act at a time.",
            Self::Gems => "Gems to buy or pick up during the campaign.",
            Self::Builds => "Characters, and gems imported from a Path of Building link.",
            Self::Database => "The shared catalog copied into each new character.",
            Self::Regex => "Vendor search strings for the gems still on screen.",
            Self::Landmarks => "Optional campaign secrets. The hint shows while you are in that zone.",
        }
    }
}

impl UiApp {
    fn new(cc: &eframe::CreationContext<'_>, state: Arc<Mutex<AppState>>) -> Self {
        apply_style(&cc.egui_ctx);
        let start_update = state.lock().map(|state| state.settings.auto_update).unwrap_or(true);
        Self {
            state,
            icons: HashMap::new(),
            selected: None,
            capture: None,
            tab: Tab::General,
            search: String::new(),
            new_name: String::new(),
            new_pob: String::new(),
            launch_note: String::new(),
            launch_rx: None,
            update_note: String::new(),
            update_rx: None,
            update_manual: false,
            start_update,
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
        apply_style(ctx);
        if let Some(rx) = &self.launch_rx {
            if let Ok(note) = rx.try_recv() {
                self.launch_note = note;
            }
        }
        if self.start_update {
            self.start_update = false;
            self.check_updates(false);
        }
        let update = self.update_rx.as_ref().and_then(|rx| rx.try_recv().ok());
        if let Some(outcome) = update {
            self.update_rx = None;
            self.finish_update(outcome);
        }
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
        egui::SidePanel::left("nav")
            .exact_width(248.0)
            .resizable(false)
            .frame(sidebar_frame())
            .show(ctx, |ui| self.nav(ui));
        egui::CentralPanel::default().frame(page_frame()).show(ctx, |ui| self.page(ui));
    }
}

impl UiApp {
    fn nav(&mut self, ui: &mut egui::Ui) {
        let auto_update = self.state.lock().map(|state| state.settings.auto_update).unwrap_or(true);
        let mut footer = 150.0;
        if !self.launch_note.is_empty() {
            footer += 22.0;
        }
        if !self.update_note.is_empty() {
            footer += 22.0;
        }
        if !auto_update {
            footer += 44.0;
        }
        let list_h = (ui.available_height() - footer).max(120.0);
        egui::Frame::NONE
            .inner_margin(egui::Margin { left: 10, right: 0, top: 0, bottom: 0 })
            .show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .id_salt("nav")
                    .auto_shrink([false, false])
                    .max_height(list_h)
                    .show(ui, |ui| {
                        ui.add_space(8.0);
                        ui.label(RichText::new("RustyExile").size(22.0).strong().color(ink()));
                        ui.label(RichText::new("Configuration").size(13.0).color(muted()));
                        ui.add_space(22.0);
                        for tab in Tab::all() {
                            self.nav_button(ui, tab);
                        }
                    });
                ui.add_space(8.0);
                self.action_buttons(ui, true);
                if !auto_update {
                    ui.add_space(8.0);
                    let width = ui.available_width();
                    if ui.add_sized([width, 36.0], egui::Button::new("Check for updates")).clicked() {
                        self.check_updates(true);
                    }
                }
            });
        ui.label(RichText::new(format!("v{VERSION}")).size(11.0).color(muted()));
    }

    fn action_buttons(&mut self, ui: &mut egui::Ui, stacked: bool) {
        let width = if stacked { ui.available_width() } else { 180.0 };
        let open = egui::Button::new("Open PoE").fill(accent_soft());
        if ui.add_sized([width, 36.0], open).clicked() {
            self.open_poe();
        }
        if stacked {
            ui.add_space(8.0);
        }
        if ui.add_sized([width, 36.0], egui::Button::new("Exit Overlay")).clicked() {
            self.exit_overlay();
        }
        if !self.launch_note.is_empty() {
            ui.add_space(6.0);
            ui.label(RichText::new(&self.launch_note).size(12.0).color(muted()));
        }
        if !self.update_note.is_empty() {
            ui.add_space(6.0);
            ui.label(RichText::new(&self.update_note).size(12.0).color(muted()));
        }
    }

    fn close_config(&self) {
        self.ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
    }

    fn exit_overlay(&self) {
        self.persist();
        std::process::exit(0);
    }

    fn persist(&self) {
        if let Ok(state) = self.state.lock() {
            if state.dirty_settings {
                let _ = poe_core::save_settings(&state.settings);
            }
            if state.dirty_catalog {
                let _ = poe_core::save_catalog(&state.catalog);
            }
            if state.dirty_regex {
                let _ = poe_core::save_regex_presets(&state.regex_presets);
            }
            if state.dirty_characters {
                for character in &state.characters {
                    let _ = poe_core::save_character(character);
                }
            }
        }
    }

    fn check_updates(&mut self, manual: bool) {
        if self.update_rx.is_some() {
            return;
        }
        self.update_manual = manual;
        self.update_note = "Checking for updates.".into();
        let (tx, rx) = std::sync::mpsc::channel();
        self.update_rx = Some(rx);
        std::thread::Builder::new()
            .name("rusty-update".into())
            .spawn(move || {
                let _ = tx.send(poe_core::check_for_update());
            })
            .ok();
    }

    fn finish_update(&mut self, outcome: UpdateOutcome) {
        match outcome {
            UpdateOutcome::UpToDate => {
                self.update_note = if self.update_manual {
                    format!("RustyExile {VERSION} is up to date.")
                } else {
                    String::new()
                };
            }
            UpdateOutcome::NoRelease => {
                self.update_note = if self.update_manual {
                    "No release has been published yet.".into()
                } else {
                    String::new()
                };
            }
            UpdateOutcome::Installed { version } => {
                self.update_note = format!("Updating to {version}.");
                self.persist();
                if let Err(err) = poe_core::restart_installed() {
                    self.update_note = format!("Could not restart. {err}");
                }
            }
            UpdateOutcome::Failed(err) => {
                self.update_note = err;
            }
        }
    }

    fn open_poe(&mut self) {
        let install = self.state.lock().ok().and_then(|state| state.settings.install.clone());
        let (tx, rx) = std::sync::mpsc::channel();
        self.launch_rx = Some(rx);
        self.launch_note = "Opening Path of Exile.".into();
        std::thread::Builder::new()
            .name("poe-launch".into())
            .spawn(move || {
                let note = match poe_core::launch_game(install.as_ref()) {
                    Ok(()) => "Opening Path of Exile.".into(),
                    Err(err) => format!("Could not open Path of Exile. {err}"),
                };
                let _ = tx.send(note);
            })
            .ok();
    }

    fn nav_button(&mut self, ui: &mut egui::Ui, tab: Tab) {
        let selected = self.tab == tab;
        let width = ui.available_width();
        let (rect, response) = ui.allocate_exact_size(egui::vec2(width, 52.0), egui::Sense::click());
        let fill = if selected {
            accent_soft()
        } else if response.hovered() {
            Color32::from_rgb(32, 35, 42)
        } else {
            Color32::TRANSPARENT
        };
        if fill != Color32::TRANSPARENT {
            ui.painter().rect_filled(rect, egui::CornerRadius::same(10), fill);
        }
        if selected {
            let bar = egui::Rect::from_min_size(rect.min + egui::vec2(0.0, 10.0), egui::vec2(3.0, rect.height() - 20.0));
            ui.painter().rect_filled(bar, egui::CornerRadius::same(2), accent());
        }
        if let Some(tex) = self.icon(tab.icon()) {
            let icon_rect = egui::Rect::from_min_size(rect.left_center() + egui::vec2(16.0, -12.0), egui::vec2(24.0, 24.0));
            ui.painter().image(
                tex.id(),
                icon_rect,
                egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                Color32::WHITE,
            );
        }
        let color = if selected { ink() } else { Color32::from_rgb(196, 190, 176) };
        ui.painter().text(
            rect.left_center() + egui::vec2(52.0, 0.0),
            egui::Align2::LEFT_CENTER,
            tab.label(),
            egui::FontId::proportional(17.0),
            color,
        );
        if response.clicked() {
            self.tab = tab;
        }
        response.on_hover_cursor(egui::CursorIcon::PointingHand);
        ui.add_space(6.0);
    }

    fn page_heading(&mut self, ui: &mut egui::Ui, title: &str, blurb: &str) {
        let width = ui.available_width();
        ui.allocate_ui_with_layout(
            egui::vec2(width, 64.0),
            egui::Layout::left_to_right(egui::Align::Min),
            |ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                ui.vertical(|ui| {
                    ui.set_width((width - 44.0).max(80.0));
                    ui.label(RichText::new(title).size(30.0).strong().color(ink()));
                    ui.add_space(4.0);
                    ui.label(RichText::new(blurb).size(15.0).color(muted()));
                });
                let close = egui::Button::new(RichText::new("X").size(20.0).strong().color(ink())).frame(false);
                if ui.add_sized([36.0, 36.0], close).clicked() {
                    self.close_config();
                }
            },
        );
    }

    fn page(&mut self, ui: &mut egui::Ui) {
        self.page_heading(ui, self.tab.label(), self.tab.blurb());
        ui.add_space(20.0);
        match self.tab {
            Tab::Acts => self.events(ui, false),
            Tab::Gems => self.events(ui, true),
            Tab::Database => self.database(ui),
            _ => {
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .id_salt("page")
                    .show(ui, |ui| match self.tab {
                    Tab::General => self.general(ui),
                    Tab::Hotkeys => self.hotkeys(ui),
                    Tab::Builds => self.builds(ui),
                    Tab::Regex => self.regex_tab(ui),
                    Tab::Landmarks => self.landmarks(ui),
                    _ => {}
                });
            }
        }
    }

    fn wizard(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().frame(page_frame()).show(ctx, |ui| {
            ui.add_space(12.0);
            self.page_heading(
                ui,
                "How should the overlay start?",
                "This only sets the first-run defaults. Every option can be changed later.",
            );
            ui.add_space(20.0);
            ui.horizontal_wrapped(|ui| self.action_buttons(ui, false));
            ui.add_space(20.0);
            ui.columns(3, |cols| {
                for (index, experience) in [Experience::Beginner, Experience::Intermediate, Experience::Advanced]
                    .into_iter()
                    .enumerate()
                {
                    let column = &mut cols[index];
                    column.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
                    card().show(column, |ui| {
                        ui.set_min_height(240.0);
                        ui.label(RichText::new(experience.label()).size(22.0).strong().color(ink()));
                        ui.add_space(12.0);
                        ui.label(RichText::new(experience.explanation()).color(muted()));
                        ui.add_space(20.0);
                        let button = egui::Button::new(format!("Start as {}", experience.label()));
                        if ui.add_sized(egui::vec2(ui.available_width(), 38.0), button).clicked() {
                            if let Ok(mut state) = self.state.lock() {
                                state.settings.apply_experience(experience);
                                state.dirty_settings = true;
                            }
                        }
                    });
                }
            });
        });
    }

    fn general(&mut self, ui: &mut egui::Ui) {
        let Ok(mut state) = self.state.lock() else { return };
        let mut schedule_update = false;
        ui.style_mut().spacing.slider_width = 260.0;
        section(ui, "Profile");
        ui.columns(3, |cols| {
            for (index, experience) in [Experience::Beginner, Experience::Intermediate, Experience::Advanced]
                .into_iter()
                .enumerate()
            {
                let column = &mut cols[index];
                column.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
                let selected = state.settings.experience == experience;
                let mut frame = card();
                if selected {
                    frame.stroke = egui::Stroke::new(1.0_f32, accent());
                }
                frame.show(column, |ui| {
                    ui.set_min_height(176.0);
                    ui.label(
                        RichText::new(experience.label())
                            .size(18.0)
                            .strong()
                            .color(if selected { accent() } else { ink() }),
                    );
                    ui.add_space(8.0);
                    ui.label(RichText::new(experience.explanation()).color(muted()));
                    ui.add_space(12.0);
                    if selected {
                        ui.label(RichText::new("Current profile").color(accent()));
                    } else if ui.button(format!("Use {}", experience.label())).clicked() {
                        state.settings.apply_experience(experience);
                        state.dirty_settings = true;
                    }
                });
            }
        });

        section(ui, "Overlay");
        card().show(ui, |ui| {
            egui::Grid::new("overlay_toggles")
                .num_columns(2)
                .spacing(egui::vec2(28.0, 12.0))
                .show(ui, |ui| {
                    let tips = ui.checkbox(&mut state.settings.tips, "Zone tips").changed();
                    let acts = ui.checkbox(&mut state.settings.acts, "Act list").changed();
                    ui.end_row();
                    let gems = ui.checkbox(&mut state.settings.gems, "Gem list").changed();
                    let league = ui.checkbox(&mut state.settings.league_start, "League-start route").changed();
                    ui.end_row();
                    let landmarks = ui.checkbox(&mut state.settings.show_landmarks, "Landmarks").changed();
                    let hud = ui.checkbox(&mut state.hud_enabled, "Show while the game is focused").changed();
                    ui.end_row();
                    let was_auto = state.settings.auto_update;
                    let auto = ui.checkbox(&mut state.settings.auto_update, "Automatic updates").changed();
                    ui.end_row();
                    if tips || acts || gems || league || landmarks || hud || auto {
                        state.dirty_settings = true;
                    }
                    schedule_update = auto && state.settings.auto_update && !was_auto;
                });
        });

        section(ui, "Appearance");
        card().show(ui, |ui| {
            egui::Grid::new("appearance")
                .num_columns(2)
                .spacing(egui::vec2(24.0, 16.0))
                .show(ui, |ui| {
                    ui.label("Opacity");
                    if ui.add(egui::Slider::new(&mut state.settings.opacity, 0.2..=1.0)).changed() {
                        state.dirty_settings = true;
                    }
                    ui.end_row();
                    ui.label("Scale");
                    if ui.add(egui::Slider::new(&mut state.settings.scale, 0.7..=2.0)).changed() {
                        state.dirty_settings = true;
                    }
                    ui.end_row();
                    ui.label("Corner");
                    ui.vertical(|ui| {
                        corner_pair(ui, &mut state, [HudCorner::TopLeft, HudCorner::TopRight]);
                        corner_pair(ui, &mut state, [HudCorner::BottomLeft, HudCorner::BottomRight]);
                    });
                    ui.end_row();
                    if color_row(ui, "Background", &mut state.settings.theme.background) {
                        state.dirty_settings = true;
                    }
                    ui.end_row();
                    if color_row(ui, "Text", &mut state.settings.theme.text) {
                        state.dirty_settings = true;
                    }
                    ui.end_row();
                    if color_row(ui, "Accent", &mut state.settings.theme.accent) {
                        state.dirty_settings = true;
                    }
                    ui.end_row();
                });
        });

        section(ui, "Game");
        card().show(ui, |ui| {
            if let Some(install) = &state.settings.install {
                field(ui, "Install", &format!("{:?}", install.kind));
                field(ui, "Folder", &install.install_dir);
                field(ui, "Log", &install.log_path);
            } else if let Some(running) = &state.running {
                field(ui, "Detected", &format!("{:?} at {}", running.kind, running.install_dir.display()));
            } else {
                ui.label(RichText::new("Path of Exile is not running. The folder and log path are saved the next time it is detected.").color(muted()));
            }
            ui.add_space(8.0);
            let mut override_path = state.settings.log_path_override.clone().unwrap_or_default();
            egui::Grid::new("log_override").num_columns(2).spacing(egui::vec2(16.0, 8.0)).show(ui, |ui| {
                ui.label("Log override");
                let edit = line_edit(&mut override_path).desired_width(360.0);
                if ui.add(edit).changed() {
                    state.settings.log_path_override = if override_path.is_empty() { None } else { Some(override_path) };
                    state.dirty_settings = true;
                }
                ui.end_row();
            });
            ui.add_space(8.0);
            ui.hyperlink_to("poe-leveling.com", &state.settings.poe_leveling_url);
            if !state.status.is_empty() {
                ui.add_space(8.0);
                ui.label(state.status.as_str());
            }
        });
        drop(state);
        if schedule_update {
            self.check_updates(false);
        }
    }

    fn hotkeys(&mut self, ui: &mut egui::Ui) {
        ui.label(RichText::new("Ctrl+Alt+click on a task clears it even if its end zone has not been entered.").color(muted()));
        ui.add_space(12.0);
        card().show(ui, |ui| {
            egui::Grid::new("hotkeys")
                .num_columns(3)
                .spacing(egui::vec2(20.0, 14.0))
                .striped(true)
                .show(ui, |ui| {
                    ui.label(RichText::new("Action").strong());
                    ui.label(RichText::new("Chord").strong());
                    ui.label("");
                    ui.end_row();
                    self.hotkey_row(ui, "Toggle overlay", "toggle_hud");
                    self.hotkey_row(ui, "Open configuration", "open_config");
                    self.hotkey_row(ui, "Paste vendor regex", "paste_regex");
                    self.hotkey_row(ui, "Clear-task modifiers", "clear_modifiers");
                });
        });
        self.chat_commands(ui);
        if self.capture.is_some() {
            ui.add_space(12.0);
            card().show(ui, |ui| {
                ui.label(RichText::new("Press the new chord. Escape cancels.").color(accent()).size(15.0));
            });
        }
    }

    fn hotkey_row(&mut self, ui: &mut egui::Ui, label: &str, field: &'static str) {
        let current = self.state.lock().ok().map(|s| match field {
            "toggle_hud" => s.settings.hotkeys.toggle_hud.clone(),
            "open_config" => s.settings.hotkeys.open_config.clone(),
            "paste_regex" => s.settings.hotkeys.paste_regex.clone(),
            _ => s.settings.hotkeys.clear_modifiers.clone(),
        }).unwrap_or_default();
        ui.label(RichText::new(label).size(15.0));
        ui.label(RichText::new(current).monospace().size(15.0).color(accent()));
        if ui.add_sized([108.0, 32.0], egui::Button::new("Change")).clicked() {
            self.begin_capture(Capture::Builtin(field));
        }
        ui.end_row();
    }

    fn chat_commands(&mut self, ui: &mut egui::Ui) {
        section(ui, "Chat commands");
        ui.label(RichText::new("A hotkey opens chat in Path of Exile and sends the text. /hideout takes you to your hideout.").color(muted()));
        ui.add_space(8.0);
        let mut commands = self.state.lock().ok().map(|s| s.settings.hotkeys.chat.clone()).unwrap_or_default();
        let mut changed = false;
        let mut delete = None;
        let mut capture_id = None;
        let full = ui.available_width();
        let (width, cols) = tile_columns(full, 340.0);
        let mut start = 0;
        while start < commands.len() {
            let end = (start + cols).min(commands.len());
            tile_row(ui, full, |ui| {
                for command in &mut commands[start..end] {
                    ui.allocate_ui_with_layout(egui::vec2(width, 0.0), egui::Layout::top_down(egui::Align::Min), |ui| {
                        card().show(ui, |ui| {
                            changed |= wide_edit(ui, ui.available_width(), &mut command.label).changed();
                            ui.add_space(8.0);
                            changed |= wide_edit(ui, ui.available_width(), &mut command.text).changed();
                            ui.add_space(8.0);
                            ui.horizontal(|ui| {
                                if ui.add_sized([88.0, 36.0], egui::Button::new("Paste")).clicked() {
                                    if let Some(text) = clipboard_text() {
                                        command.text = text;
                                        changed = true;
                                    }
                                }
                                if ui.add_sized([88.0, 36.0], egui::Button::new("Delete")).clicked() {
                                    delete = Some(command.id.clone());
                                }
                            });
                            ui.add_space(8.0);
                            ui.horizontal(|ui| {
                                let hotkey = if command.hotkey.is_empty() {
                                    "No hotkey".to_string()
                                } else {
                                    command.hotkey.clone()
                                };
                                ui.label(RichText::new(hotkey).monospace().size(15.0).color(accent()));
                                if ui.add_sized([120.0, 36.0], egui::Button::new("Set hotkey")).clicked() {
                                    capture_id = Some(command.id.clone());
                                }
                            });
                        });
                    });
                }
            });
            start = end;
        }
        ui.add_space(12.0);
        if ui.button("Add chat command").clicked() {
            commands.push(ChatCommand::new("Custom", "/", ""));
            changed = true;
        }
        if let Some(id) = delete {
            commands.retain(|command| command.id != id);
            changed = true;
        }
        if changed {
            if let Ok(mut state) = self.state.lock() {
                state.settings.hotkeys.chat = commands;
                state.dirty_settings = true;
            }
        }
        if let Some(id) = capture_id {
            self.begin_capture(Capture::Chat(id));
        }
    }

    fn begin_capture(&mut self, target: Capture) {
        self.capture = Some(target);
        if let Ok(mut state) = self.state.lock() {
            state.capturing_hotkey = true;
        }
    }

    fn end_capture(&mut self) {
        self.capture = None;
        if let Ok(mut state) = self.state.lock() {
            state.capturing_hotkey = false;
        }
    }

    fn capture_hotkey(&mut self, ctx: &egui::Context) {
        let Some(field) = self.capture.clone() else { return };
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
            self.end_capture();
            if chord.is_empty() {
                return;
            }
            if let Ok(mut state) = self.state.lock() {
                match field {
                    Capture::Builtin("toggle_hud") => state.settings.hotkeys.toggle_hud = chord,
                    Capture::Builtin("open_config") => state.settings.hotkeys.open_config = chord,
                    Capture::Builtin("paste_regex") => state.settings.hotkeys.paste_regex = chord,
                    Capture::Builtin(_) => state.settings.hotkeys.clear_modifiers = chord,
                    Capture::Chat(id) => {
                        if let Some(cmd) = state.settings.hotkeys.chat.iter_mut().find(|cmd| cmd.id == id) {
                            cmd.hotkey = chord;
                        }
                    }
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
        let reserve = if self.selected.is_some() { 280.0 } else { 0.0 };
        let max_h = (ui.available_height() - reserve).max(180.0);
        egui::ScrollArea::vertical().max_height(max_h).show(ui, |ui| {
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
                        ui.add_space(4.0);
                        if self.selected.as_deref() == Some(event.id.as_str()) {
                            edit = Some(event);
                        }
                    }
                });
            }
        });
        if let Some(event) = edit {
            ui.add_space(12.0);
            card().show(ui, |ui| self.edit_event(ui, &event));
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
                    if ui.add(line_edit(&mut name)).changed() {
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
        ui.label(RichText::new("Edit event").size(16.0).strong().color(ink()));
        ui.add_space(8.0);
        let mut text = event.text.clone();
        let mut detail = event.detail.clone();
        let mut changed = false;
        egui::Grid::new("edit_event").num_columns(2).spacing(egui::vec2(16.0, 12.0)).show(ui, |ui| {
            ui.label("Text");
            changed |= ui.add(line_edit(&mut text).desired_width(420.0)).changed();
            ui.end_row();
            ui.label("Tip");
            changed |= ui.add(line_edit(&mut detail).desired_width(420.0)).changed();
            ui.end_row();
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
        section(ui, "Characters");
        if names.is_empty() {
            ui.label(RichText::new("No characters yet.").color(muted()));
        }
        for (name, class_name, gems, nodes) in names {
            let selected = active.as_deref() == Some(name.as_str());
            let mut frame = card();
            if selected {
                frame.stroke = egui::Stroke::new(1.0_f32, accent());
            }
            frame.show(ui, |ui| {
                ui.horizontal(|ui| {
                    self.icon_ui(ui, "trial", 22.0);
                    ui.add_space(8.0);
                    if ui.selectable_label(selected, RichText::new(&name).size(16.0).strong()).clicked() {
                        if let Ok(mut state) = self.state.lock() {
                            state.settings.active_character = Some(name);
                            state.dirty_settings = true;
                        }
                    }
                    ui.add_space(16.0);
                    ui.label(RichText::new(class_name).color(muted()));
                    ui.label(RichText::new(format!("{gems} gems")).color(muted()));
                    ui.label(RichText::new(format!("{nodes} tree nodes")).color(muted()));
                });
            });
            ui.add_space(8.0);
        }
        section(ui, "Import");
        ui.label(RichText::new("Paste a Path of Building Community link, usually pobb.in or pastebin. Campaign gems from that build are added to the character.").color(muted()));
        ui.add_space(8.0);
        card().show(ui, |ui| {
            egui::Grid::new("import_build").num_columns(2).spacing(egui::vec2(16.0, 14.0)).show(ui, |ui| {
                ui.label("Name");
                ui.add(line_edit(&mut self.new_name).desired_width(320.0));
                ui.end_row();
                ui.label("PoB link");
                ui.add(line_edit(&mut self.new_pob).desired_width(320.0));
                ui.end_row();
            });
            ui.add_space(8.0);
            if ui.add_sized([140.0, 36.0], egui::Button::new("Import")).clicked() {
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
        let fetched = fetch_pob(&link)
            .map_err(|err| err.to_string())
            .and_then(|body| poe_core::decode_pob(&body).map_err(|err| err.to_string()));
        let Ok(mut state) = self.state.lock() else { return };
        match fetched {
            Ok(build) if build.class_name.is_empty() => {
                state.status = "That build does not say which class it is.".into();
            }
            Ok(build) => {
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
        let page_width = ui.available_width();
        card().show(ui, |ui| {
            ui.set_min_width(page_width - 32.0);
            ui.label(RichText::new("Search").color(muted()));
            ui.add_space(8.0);
            let search = line_edit(&mut self.search)
                .desired_width(ui.available_width())
                .hint_text("Gem, area, or quest");
            ui.add_sized([ui.available_width(), 36.0], search);
        });
        ui.add_space(8.0);
        let query = self.search.to_ascii_lowercase();
        let Ok(mut state) = self.state.lock() else { return };
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            let full = ui.available_width();
            section(ui, "Gems");
            let mut gem_delete = None;
            let mut dirty = false;
            table_head(ui, &["Id", "Name", ""]);
            let mut shown = 0;
            for (index, gem) in state.catalog.gems.iter_mut().enumerate() {
                if !query.is_empty() && !gem.name.to_ascii_lowercase().contains(&query) {
                    continue;
                }
                let (id_w, name_w, tail_w) = split3(full, 0.34, 110.0);
                let stripe = shown % 2 == 0;
                let mut delete = false;
                let mut edited = false;
                striped(ui, stripe, |ui| {
                    id_cell(ui, id_w, &gem.id);
                    edited = wide_edit(ui, name_w, &mut gem.name).changed();
                    delete = ui.add_sized([tail_w, 36.0], egui::Button::new("Delete")).clicked();
                });
                dirty |= edited;
                if delete {
                    gem_delete = Some(index);
                }
                shown += 1;
                if index > 40 && query.is_empty() {
                    ui.label(RichText::new("Search to see more gems.").color(muted()));
                    break;
                }
            }
            if let Some(index) = gem_delete {
                state.catalog.gems.remove(index);
                dirty = true;
            }
            state.dirty_catalog |= dirty;

            section(ui, "Areas");
            table_head(ui, &["Id", "Name", "Act"]);
            shown = 0;
            let mut dirty = false;
            for area in &mut state.catalog.areas {
                if !query.is_empty() && !area.name.to_ascii_lowercase().contains(&query) && !area.id.contains(&query) {
                    continue;
                }
                let (id_w, name_w, act_w) = split3(full, 0.38, 96.0);
                let stripe = shown % 2 == 0;
                let mut edited = false;
                striped(ui, stripe, |ui| {
                    id_cell(ui, id_w, &area.id);
                    edited = wide_edit(ui, name_w, &mut area.name).changed();
                    edited |= ui
                        .add_sized([act_w, 36.0], egui::DragValue::new(&mut area.act).range(1..=10))
                        .changed();
                });
                dirty |= edited;
                shown += 1;
                if shown > 40 && query.is_empty() {
                    break;
                }
            }
            state.dirty_catalog |= dirty;

            section(ui, "Quests");
            table_head(ui, &["Id", "Name", "NPC"]);
            shown = 0;
            let mut dirty = false;
            for quest in &mut state.catalog.quests {
                if !query.is_empty() && !quest.name.to_ascii_lowercase().contains(&query) {
                    continue;
                }
                let id_w = (full * 0.28).clamp(220.0, 460.0);
                let npc_w = (full * 0.30).clamp(200.0, 420.0);
                let name_w = (full - id_w - npc_w - 32.0).max(240.0);
                let stripe = shown % 2 == 0;
                let mut edited = false;
                striped(ui, stripe, |ui| {
                    id_cell(ui, id_w, &quest.id);
                    edited = wide_edit(ui, name_w, &mut quest.name).changed();
                    edited |= wide_edit(ui, npc_w, &mut quest.npc).changed();
                });
                dirty |= edited;
                shown += 1;
                if shown > 30 && query.is_empty() {
                    break;
                }
            }
            state.dirty_catalog |= dirty;
        });
    }

    fn landmarks(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label(RichText::new("These are optional. Flooded Depths, the Chemist's strongbox, the Karui Fortress tattoos, and the Ossuary Facetor's Lens start on. The hint appears on the overlay while you are in the zone.").color(muted()));
            if ui.button("Restore defaults").clicked() {
                if let Ok(mut state) = self.state.lock() {
                    state.settings.landmarks = poe_core::default_landmarks();
                    state.dirty_settings = true;
                }
            }
        });
        ui.add_space(12.0);
        let mut landmarks = self.state.lock().ok().map(|s| s.settings.landmarks.clone()).unwrap_or_default();
        let mut changed = false;
        let full = ui.available_width();
        let (width, cols) = tile_columns(full, 420.0);
        let mut start = 0;
        while start < landmarks.len() {
            let end = (start + cols).min(landmarks.len());
            tile_row(ui, full, |ui| {
                for landmark in &mut landmarks[start..end] {
                    ui.allocate_ui_with_layout(egui::vec2(width, 0.0), egui::Layout::top_down(egui::Align::Min), |ui| {
                        card().show(ui, |ui| {
                            ui.horizontal(|ui| {
                                changed |= ui.checkbox(&mut landmark.enabled, "").changed();
                                ui.label(RichText::new(format!("Act {}", landmark.act)).color(accent()).strong());
                                ui.label(RichText::new(&landmark.areas).strong());
                            });
                            ui.add_space(8.0);
                            changed |= area_edit(ui, ui.available_width(), &mut landmark.text).changed();
                            ui.add_space(6.0);
                            ui.label(RichText::new(format!("Shows while you are in {}.", landmark.areas)).size(13.0).color(muted()));
                        });
                    });
                }
            });
            start = end;
        }
        if changed {
            if let Ok(mut state) = self.state.lock() {
                state.settings.landmarks = landmarks;
                state.dirty_settings = true;
            }
        }
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
        section(ui, "Generated");
        card().show(ui, |ui| {
            ui.label(RichText::new("From the gems still showing for this character.").color(muted()));
            ui.add_space(8.0);
            ui.label(RichText::new(&generated).monospace().size(14.0));
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                if ui.button("Paste into the game").clicked() {
                    if let Ok(mut state) = self.state.lock() {
                        state.status = format!("paste:{generated}");
                    }
                }
                if ui.button("Copy").clicked() {
                    if let Ok(mut state) = self.state.lock() {
                        state.status = format!("copy:{generated}");
                    }
                }
            });
        });
        section(ui, "Presets");
        let mut presets = self.state.lock().ok().map(|s| s.regex_presets.clone()).unwrap_or_default();
        let mut active = self.state.lock().ok().map(|s| s.settings.active_regex).unwrap_or(0);
        let mut changed = false;
        let mut delete = None;
        let full = ui.available_width();
        let (width, cols) = tile_columns(full, 560.0);
        let mut start = 0;
        while start < presets.len() {
            let end = (start + cols).min(presets.len());
            tile_row(ui, full, |ui| {
                for (index, preset) in presets.iter_mut().enumerate().skip(start).take(end - start) {
                    ui.allocate_ui_with_layout(egui::vec2(width, 0.0), egui::Layout::top_down(egui::Align::Min), |ui| {
                        let mut frame = card();
                        if active == index {
                            frame.stroke = egui::Stroke::new(1.0_f32, accent());
                        }
                        frame.show(ui, |ui| {
                            ui.horizontal(|ui| {
                                if ui.radio(active == index, "").clicked() {
                                    active = index;
                                    changed = true;
                                }
                                let name_w = (ui.available_width() - 100.0).max(120.0);
                                changed |= wide_edit(ui, name_w, &mut preset.name).changed();
                                if ui.add_sized([92.0, 36.0], egui::Button::new("Delete")).clicked() {
                                    delete = Some(index);
                                }
                            });
                            ui.add_space(8.0);
                            ui.horizontal(|ui| {
                                let copy_w = 92.0;
                                let regex_w = (ui.available_width() - copy_w - ui.spacing().item_spacing.x).max(120.0);
                                changed |= wide_edit(ui, regex_w, &mut preset.regex).changed();
                                if ui.add_sized([copy_w, 36.0], egui::Button::new("Copy")).clicked() {
                                    if let Ok(mut state) = self.state.lock() {
                                        state.status = format!("copy:{}", preset.regex);
                                    }
                                }
                            });
                        });
                    });
                }
            });
            start = end;
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
    ui.label(label);
    let mut c = Color32::from_rgba_unmultiplied(color[0], color[1], color[2], color[3]);
    let changed = ui.color_edit_button_srgba(&mut c).changed();
    if changed {
        *color = [c.r(), c.g(), c.b(), c.a()];
    }
    changed
}

fn corner_pair(ui: &mut egui::Ui, state: &mut AppState, corners: [HudCorner; 2]) {
    ui.horizontal(|ui| {
        for corner in corners {
            let label = match corner {
                HudCorner::TopLeft => "Top left",
                HudCorner::TopRight => "Top right",
                HudCorner::BottomLeft => "Bottom left",
                HudCorner::BottomRight => "Bottom right",
            };
            if ui.selectable_label(state.settings.corner == corner, label).clicked() {
                state.settings.corner = corner;
                state.dirty_settings = true;
            }
        }
    });
}

fn field(ui: &mut egui::Ui, label: &str, value: &str) {
    ui.horizontal(|ui| {
        ui.add_sized([88.0, 18.0], egui::Label::new(RichText::new(label).color(muted())));
        ui.label(value);
    });
}

fn split3(full: f32, id_share: f32, tail: f32) -> (f32, f32, f32) {
    let id_w = (full * id_share).clamp(220.0, 520.0);
    let name_w = (full - id_w - tail - 32.0).max(240.0);
    (id_w, name_w, tail)
}

fn table_head(ui: &mut egui::Ui, titles: &[&str]) {
    let full = ui.available_width();
    let (id_w, name_w, tail_w) = if titles.len() == 3 && titles[2] == "NPC" {
        let id_w = (full * 0.28).clamp(220.0, 460.0);
        let npc_w = (full * 0.30).clamp(200.0, 420.0);
        let name_w = (full - id_w - npc_w - 32.0).max(240.0);
        (id_w, name_w, npc_w)
    } else if titles.len() == 3 && titles[2] == "Act" {
        split3(full, 0.38, 96.0)
    } else {
        split3(full, 0.34, 110.0)
    };
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 16.0;
        for (title, width) in titles.iter().zip([id_w, name_w, tail_w]) {
            ui.allocate_ui_with_layout(
                egui::vec2(width, 22.0),
                egui::Layout::left_to_right(egui::Align::Center),
                |ui| {
                    ui.label(RichText::new(*title).strong().color(muted()));
                },
            );
        }
    });
    ui.add_space(4.0);
}

fn striped(ui: &mut egui::Ui, on: bool, add: impl FnOnce(&mut egui::Ui)) {
    let width = ui.available_width();
    let mut frame = egui::Frame::NONE.inner_margin(egui::Margin::symmetric(8, 6));
    if on {
        frame.fill = Color32::from_rgb(30, 32, 38);
        frame.corner_radius = egui::CornerRadius::same(8);
    }
    frame.show(ui, |ui| {
        ui.set_min_width(width - 16.0);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 16.0;
            add(ui);
        });
    });
}

fn id_cell(ui: &mut egui::Ui, width: f32, id: &str) {
    ui.allocate_ui_with_layout(
        egui::vec2(width, 36.0),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            ui.add(egui::Label::new(RichText::new(id).monospace()).truncate());
        },
    );
}

fn tile_width(available: f32, min_card: f32) -> f32 {
    let gap = 12.0;
    let cols = ((available + gap) / (min_card + gap)).floor().max(1.0);
    (available - gap * (cols - 1.0)) / cols
}

fn tile_columns(available: f32, min_card: f32) -> (f32, usize) {
    let width = tile_width(available, min_card);
    let cols = ((available + 12.0) / (width + 12.0)).round().max(1.0) as usize;
    (width, cols)
}

fn tile_row(ui: &mut egui::Ui, width: f32, add: impl FnOnce(&mut egui::Ui)) {
    ui.allocate_ui_with_layout(
        egui::vec2(width, 0.0),
        egui::Layout::left_to_right(egui::Align::Min),
        |ui| {
            ui.spacing_mut().item_spacing.x = 12.0;
            add(ui);
        },
    );
    ui.add_space(12.0);
}

fn clipboard_text() -> Option<String> {
    arboard::Clipboard::new().ok()?.get_text().ok()
}

fn area_edit(ui: &mut egui::Ui, width: f32, text: &mut String) -> egui::Response {
    ui.add_sized(
        [width, 76.0],
        egui::TextEdit::multiline(text)
            .margin(egui::Margin::symmetric(12, 6))
            .desired_rows(3)
            .desired_width(width),
    )
}

fn line_edit(text: &mut String) -> egui::TextEdit<'_> {
    egui::TextEdit::singleline(text)
        .margin(egui::Margin::symmetric(12, 0))
        .vertical_align(egui::Align::Center)
        .min_size(egui::vec2(0.0, 36.0))
}

fn wide_edit(ui: &mut egui::Ui, width: f32, text: &mut String) -> egui::Response {
    ui.add_sized(
        [width, 36.0],
        line_edit(text).desired_width(width).clip_text(true),
    )
}

fn section(ui: &mut egui::Ui, title: &str) {
    ui.add_space(10.0);
    ui.label(RichText::new(title).size(16.0).strong().color(accent()));
    ui.add_space(10.0);
}

fn ink() -> Color32 {
    Color32::from_rgb(236, 230, 218)
}

fn muted() -> Color32 {
    Color32::from_rgb(168, 160, 146)
}

fn accent() -> Color32 {
    Color32::from_rgb(212, 164, 84)
}

fn accent_soft() -> Color32 {
    Color32::from_rgb(58, 46, 28)
}

fn card() -> egui::Frame {
    egui::Frame::NONE
        .fill(Color32::from_rgb(32, 35, 42))
        .corner_radius(egui::CornerRadius::same(12))
        .inner_margin(egui::Margin::same(16))
        .stroke(egui::Stroke::new(1.0_f32, Color32::from_rgb(48, 52, 60)))
}

fn sidebar_frame() -> egui::Frame {
    egui::Frame::NONE
        .fill(Color32::from_rgb(16, 18, 22))
        .inner_margin(egui::Margin { left: 6, right: 16, top: 16, bottom: 2 })
}

fn page_frame() -> egui::Frame {
    egui::Frame::NONE
        .fill(Color32::from_rgb(24, 26, 31))
        .inner_margin(egui::Margin::same(28))
}

fn apply_style(ctx: &egui::Context) {
    let mut visuals = egui::Visuals::dark();
    visuals.window_fill = Color32::from_rgb(24, 26, 31);
    visuals.panel_fill = Color32::from_rgb(24, 26, 31);
    visuals.faint_bg_color = Color32::from_rgb(32, 35, 42);
    visuals.extreme_bg_color = Color32::from_rgb(16, 17, 20);
    visuals.code_bg_color = Color32::from_rgb(16, 17, 20);
    visuals.hyperlink_color = accent();
    visuals.selection.bg_fill = Color32::from_rgba_unmultiplied(212, 164, 84, 48);
    visuals.selection.stroke = egui::Stroke::new(1.0_f32, accent());
    let radius = egui::CornerRadius::same(8);
    for widget in [
        &mut visuals.widgets.noninteractive,
        &mut visuals.widgets.inactive,
        &mut visuals.widgets.hovered,
        &mut visuals.widgets.active,
        &mut visuals.widgets.open,
    ] {
        widget.corner_radius = radius;
    }
    visuals.widgets.inactive.weak_bg_fill = Color32::from_rgb(36, 39, 46);
    visuals.widgets.hovered.weak_bg_fill = Color32::from_rgb(48, 52, 62);
    visuals.widgets.active.weak_bg_fill = accent_soft();
    visuals.widgets.active.bg_fill = Color32::from_rgb(92, 70, 36);
    visuals.widgets.hovered.bg_stroke = egui::Stroke::new(1.0_f32, accent());
    ctx.set_visuals(visuals);
    ctx.style_mut(|style| {
        style.spacing.item_spacing = egui::vec2(16.0, 12.0);
        style.spacing.button_padding = egui::vec2(16.0, 10.0);
        style.spacing.interact_size.y = 34.0;
        if let Some(font) = style.text_styles.get_mut(&egui::TextStyle::Body) {
            font.size = 15.0;
        }
        if let Some(font) = style.text_styles.get_mut(&egui::TextStyle::Button) {
            font.size = 15.0;
        }
        if let Some(font) = style.text_styles.get_mut(&egui::TextStyle::Heading) {
            font.size = 24.0;
        }
        if let Some(font) = style.text_styles.get_mut(&egui::TextStyle::Monospace) {
            font.size = 14.0;
        }
    });
}

fn trigger_list(ui: &mut egui::Ui, triggers: &mut Vec<TriggerKind>) -> bool {
    let mut changed = false;
    let mut delete = None;
    for (index, trigger) in triggers.iter_mut().enumerate() {
        ui.horizontal(|ui| {
            ui.label(trigger.label());
            match trigger {
                TriggerKind::ZoneEnter { area_id } => changed |= ui.add(line_edit(area_id)).changed(),
                TriggerKind::ZoneName { name } => changed |= ui.add(line_edit(name)).changed(),
                TriggerKind::ActStart { act } | TriggerKind::ActEnd { act } => {
                    changed |= ui.add(egui::DragValue::new(act).range(1..=10)).changed();
                }
                TriggerKind::LevelAtLeast { level } => changed |= ui.add(egui::DragValue::new(level)).changed(),
                TriggerKind::PassiveAllocated { id } => changed |= ui.add(line_edit(id)).changed(),
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
