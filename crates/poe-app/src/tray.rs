use ksni::blocking::TrayMethods;
use poe_core::{save_catalog, save_character, save_regex_presets, save_settings, AppState};
use std::sync::{Arc, Mutex};

struct RustyTray {
    state: Arc<Mutex<AppState>>,
    icon: ksni::Icon,
}

impl ksni::Tray for RustyTray {
    fn id(&self) -> String {
        "RustyExile".into()
    }

    fn title(&self) -> String {
        "RustyExile".into()
    }

    fn icon_pixmap(&self) -> Vec<ksni::Icon> {
        vec![self.icon.clone()]
    }

    fn activate(&mut self, _x: i32, _y: i32) {
        open_config(&self.state);
    }

    fn menu(&self) -> Vec<ksni::MenuItem<Self>> {
        use ksni::menu::StandardItem;
        vec![
            StandardItem {
                label: "Config".into(),
                activate: Box::new(|tray: &mut RustyTray| open_config(&tray.state)),
                ..Default::default()
            }
            .into(),
            StandardItem {
                label: "Exit Rusty".into(),
                activate: Box::new(|tray: &mut RustyTray| exit_rusty(&tray.state)),
                ..Default::default()
            }
            .into(),
        ]
    }
}

fn open_config(state: &Mutex<AppState>) {
    if let Ok(mut app) = state.lock() {
        app.show_config = true;
    }
}

fn exit_rusty(state: &Mutex<AppState>) {
    if let Ok(app) = state.lock() {
        if app.dirty_settings {
            let _ = save_settings(&app.settings);
        }
        if app.dirty_catalog {
            let _ = save_catalog(&app.catalog);
        }
        if app.dirty_regex {
            let _ = save_regex_presets(&app.regex_presets);
        }
        if app.dirty_characters {
            for character in &app.characters {
                let _ = save_character(character);
            }
        }
    }
    std::process::exit(0);
}

pub fn run(state: Arc<Mutex<AppState>>) -> anyhow::Result<()> {
    let (width, height, data) = poe_data::tray_icon_argb(32).unwrap_or((32, 32, vec![255, 212, 164, 84].repeat(32 * 32)));
    let tray = RustyTray {
        state,
        icon: ksni::Icon { width, height, data },
    };
    let _handle = tray.spawn().map_err(|err| anyhow::anyhow!(err))?;
    std::thread::park();
    Ok(())
}
