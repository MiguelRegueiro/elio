use super::Config;
use std::{env, fs, io, path::PathBuf, sync::OnceLock};

static ACTIVE_CONFIG: OnceLock<Config> = OnceLock::new();

pub(super) fn initialize() {
    let _ = ACTIVE_CONFIG.get_or_init(load_config_from_disk);
}

pub(super) fn active_config() -> &'static Config {
    ACTIVE_CONFIG.get_or_init(Config::default_config)
}

pub(crate) fn config_dir() -> Option<PathBuf> {
    // XDG_CONFIG_HOME is honoured on all platforms so developers can redirect
    // the config location regardless of OS.
    if let Some(config_home) = env::var_os("XDG_CONFIG_HOME") {
        return Some(PathBuf::from(config_home).join("elio"));
    }

    #[cfg(target_os = "macos")]
    {
        // Prefer XDG-style config on macOS only when it contains Elio config
        // files, avoiding empty ~/.config/elio directories shadowing the native
        // Application Support location.
        if let Some(home) = super::invoking_user::home_dir() {
            let xdg_dir = home.join(".config/elio");
            if xdg_dir.join("config.toml").is_file() || xdg_dir.join("theme.toml").is_file() {
                return Some(xdg_dir);
            }
        }
    }

    // dirs::config_dir() returns:
    //   Linux/BSD : $HOME/.config
    //   macOS     : $HOME/Library/Application Support
    //   Windows   : %APPDATA% (Roaming)
    #[cfg(all(unix, not(target_os = "macos")))]
    return super::invoking_user::home_dir().map(|home| home.join(".config/elio"));

    #[cfg(target_os = "macos")]
    return super::invoking_user::home_dir()
        .map(|home| home.join("Library/Application Support/elio"));

    #[cfg(windows)]
    dirs::config_dir().map(|dir| dir.join("elio"))
}

fn load_config_from_disk() -> Config {
    let Some(path) = config_path() else {
        return Config::default_config();
    };
    let contents = match fs::read_to_string(&path) {
        Ok(contents) => contents,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Config::default_config(),
        Err(error) => {
            eprintln!(
                "elio: failed to read config from {}: {error}",
                path.display()
            );
            return Config::default_config();
        }
    };

    match Config::from_str(&contents) {
        Ok(config) => config,
        Err(error) => {
            eprintln!(
                "elio: failed to load config from {}: {error}",
                path.display()
            );
            Config::default_config()
        }
    }
}

fn config_path() -> Option<PathBuf> {
    config_dir().map(|dir| dir.join("config.toml"))
}
