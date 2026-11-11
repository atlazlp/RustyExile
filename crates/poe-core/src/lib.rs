pub mod discover;
pub mod engine;
pub mod landmarks;
pub mod logparse;
pub mod markup;
pub mod model;
pub mod pob;
pub mod regexes;
pub mod route;
pub mod state;
pub mod store;
pub mod update;

pub use discover::{detect_running, launch_game, GameLocation};
pub use logparse::{LogParser, ParsedLog};
pub use engine::{apply_parsed, force_complete, note_current_zone, nudge_events, sync_skipped, visible_lines};
pub use landmarks::{default_landmarks, reconcile_landmarks, visible_landmarks, Landmark};
pub use markup::{parse_markup, plain_text, tone_rgb, TextSpan, TextTone};
pub use model::*;
pub use pob::{apply_pob, decode_pob, fetch_pob, PobBuild};
pub use regexes::{default_presets, default_regex_index, vendor_gem_regex};
pub use route::{build_route, install_build, refresh_generated_route};
pub use state::AppState;
pub use update::{check_for_update, install_archive, restart_installed, UpdateOutcome, VERSION};
pub use store::{
    list_characters, load_catalog, load_character, load_regex_presets, load_settings, read_log_tail,
    save_catalog, save_character, save_regex_presets, save_settings, LogFollow,
};
