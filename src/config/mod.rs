mod goto;
mod invoking_user;
mod keys;
mod layout;
mod loading;
mod open;
mod places;
#[cfg(test)]
mod tests;
mod ui;

use serde::Deserialize;
use std::path::Path;

pub(crate) use self::invoking_user::env_var as invoking_user_env_var;

#[cfg(unix)]
pub(crate) use self::invoking_user::{
    InvocationContext, InvokingUser, SESSION_ENVIRONMENT_KEYS, context as invoking_user_context,
    user_environment_value,
};

#[cfg(all(unix, not(target_os = "macos")))]
pub(crate) use self::invoking_user::trash_data_dir;

pub(crate) use self::{
    goto::{BuiltinGoto, GotoConfig, GotoEntrySpec},
    invoking_user::{home_dir as invoking_user_home_dir, trash_home_dir},
    keys::{Action, ChooserKeyAction, KeyBindings, KeyContext, KeyList, normalized_plain_key_char},
    layout::{LayoutConfig, PaneWeights},
    loading::config_dir,
    open::{OpenConfig, OpenPlatform, OpenRule, OpenTargetType},
    places::{BuiltinPlace, PlaceEntrySpec, PlacesConfig},
    ui::UiConfig,
};

struct Config {
    ui: UiConfig,
    goto: GotoConfig,
    places: PlacesConfig,
    layout: LayoutConfig,
    keys: KeyBindings,
    open: OpenConfig,
}

#[derive(Deserialize, Default)]
struct ConfigFile {
    ui: Option<ui::UiConfigOverride>,
    goto: Option<goto::GotoConfigOverride>,
    places: Option<places::PlacesConfigOverride>,
    layout: Option<layout::LayoutConfigOverride>,
    keys: Option<keys::KeysConfigOverride>,
    open: Option<open::OpenConfigOverride>,
}

pub(crate) fn initialize(path: Option<&Path>) -> anyhow::Result<()> {
    #[cfg(unix)]
    // Snapshot the invocation context before loading config or starting workers.
    let _ = invoking_user::context();
    loading::initialize(path)
}

pub(crate) fn ui() -> UiConfig {
    loading::active_config().ui
}

pub(crate) fn goto() -> &'static GotoConfig {
    &loading::active_config().goto
}

pub(crate) fn places() -> &'static PlacesConfig {
    &loading::active_config().places
}

pub(crate) fn layout() -> LayoutConfig {
    loading::active_config().layout
}

pub(crate) fn keys() -> &'static KeyBindings {
    &loading::active_config().keys
}

pub(crate) fn open() -> &'static OpenConfig {
    &loading::active_config().open
}

impl Config {
    fn default_config() -> Self {
        Self {
            ui: UiConfig::default(),
            goto: GotoConfig::default(),
            places: PlacesConfig::default(),
            layout: LayoutConfig::default(),
            keys: KeyBindings::default(),
            open: OpenConfig::default(),
        }
    }

    fn from_str(config: &str) -> anyhow::Result<Self> {
        let parsed: ConfigFile = toml::from_str(config)?;
        let mut resolved = Self::default_config();
        if let Some(ui) = parsed.ui {
            resolved.ui.apply_override(ui);
        }
        if let Some(goto) = parsed.goto {
            resolved.goto = GotoConfig::from_override(goto, &resolved.goto);
        }
        if let Some(places) = parsed.places {
            resolved.places = PlacesConfig::from_override(places, &resolved.places);
        }
        if let Some(layout) = parsed.layout {
            match LayoutConfig::from_override(layout) {
                Ok(layout) => resolved.layout = layout,
                Err(error) => eprintln!("elio: invalid [layout.panes] config: {error}"),
            }
        }
        if let Some(keys) = parsed.keys {
            resolved.keys = KeyBindings::from_override(keys, &KeyBindings::default());
        }
        if let Some(open) = parsed.open {
            resolved.open = OpenConfig::from_override(open, &resolved.open);
        }
        Ok(resolved)
    }
}
