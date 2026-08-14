use super::Config;
#[cfg(unix)]
use super::invoking_user::InvocationContext;
#[cfg(unix)]
use std::path::Path;
use std::{env, fs, io, path::PathBuf, sync::OnceLock};

static ACTIVE_CONFIG: OnceLock<Config> = OnceLock::new();

pub(super) fn initialize() {
    let _ = ACTIVE_CONFIG.get_or_init(load_config_from_disk);
}

pub(super) fn active_config() -> &'static Config {
    ACTIVE_CONFIG.get_or_init(Config::default_config)
}

fn config_home() -> Option<PathBuf> {
    #[cfg(unix)]
    {
        let process_xdg_home = env::var_os("XDG_CONFIG_HOME").map(PathBuf::from);
        let process_home = dirs::home_dir();
        config_home_for_context(
            super::invoking_user::context(),
            process_xdg_home.as_deref(),
            process_home.as_deref(),
        )
    }

    #[cfg(windows)]
    {
        // XDG_CONFIG_HOME is honoured on Windows so developers can redirect
        // the config location regardless of OS.
        if let Some(config_home) = env::var_os("XDG_CONFIG_HOME") {
            return Some(PathBuf::from(config_home));
        }
        dirs::config_dir()
    }
}

pub(crate) fn config_dir() -> Option<PathBuf> {
    config_home().map(|home| home.join("elio"))
}

#[cfg(unix)]
fn config_home_for_context(
    context: &InvocationContext,
    process_xdg_home: Option<&Path>,
    process_home: Option<&Path>,
) -> Option<PathBuf> {
    match context {
        InvocationContext::Normal | InvocationContext::RootSession => {
            platform_config_home(process_xdg_home, process_home)
        }
        InvocationContext::Elevated(user) => {
            platform_config_home(user.xdg_config_home.as_deref(), Some(&user.home))
        }
        InvocationContext::ElevatedUnresolved => None,
    }
}

#[cfg(unix)]
fn platform_config_home(xdg_home: Option<&Path>, home: Option<&Path>) -> Option<PathBuf> {
    if let Some(xdg_home) = xdg_home {
        return Some(xdg_home.to_path_buf());
    }
    let home = home?;

    #[cfg(target_os = "macos")]
    {
        // Prefer XDG-style config on macOS only when it contains Elio config
        // files, avoiding empty ~/.config/elio directories shadowing the native
        // Application Support location.
        let xdg_home = home.join(".config");
        let xdg_dir = xdg_home.join("elio");
        if xdg_dir.join("config.toml").is_file() || xdg_dir.join("theme.toml").is_file() {
            return Some(xdg_home);
        }
        return Some(home.join("Library/Application Support"));
    }

    #[cfg(not(target_os = "macos"))]
    Some(home.join(".config"))
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

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use crate::config::InvokingUser;
    use std::ffi::OsString;

    fn test_user(xdg_config_home: Option<&str>) -> InvokingUser {
        InvokingUser {
            uid: 1000,
            gid: 1000,
            name: OsString::from("paco"),
            home: PathBuf::from("/home/paco"),
            shell: OsString::from("/bin/sh"),
            groups: vec![1000],
            session_environment: Vec::new(),
            xdg_config_home: xdg_config_home.map(PathBuf::from),
            xdg_data_home: None,
        }
    }

    #[test]
    fn config_home_follows_invocation_context() {
        let root_default = if cfg!(target_os = "macos") {
            Path::new("/root/Library/Application Support")
        } else {
            Path::new("/root/.config")
        };
        let user_default = if cfg!(target_os = "macos") {
            Path::new("/home/paco/Library/Application Support")
        } else {
            Path::new("/home/paco/.config")
        };
        let process_xdg = Some(Path::new("/root/custom"));
        let cases = [
            (
                InvocationContext::Normal,
                process_xdg,
                Some(Path::new("/root/custom")),
            ),
            (
                InvocationContext::RootSession,
                process_xdg,
                Some(Path::new("/root/custom")),
            ),
            (InvocationContext::Normal, None, Some(root_default)),
            (InvocationContext::RootSession, None, Some(root_default)),
            (
                InvocationContext::Elevated(test_user(Some("/home/paco/custom-config"))),
                process_xdg,
                Some(Path::new("/home/paco/custom-config")),
            ),
            (
                InvocationContext::Elevated(test_user(None)),
                process_xdg,
                Some(user_default),
            ),
            (InvocationContext::ElevatedUnresolved, process_xdg, None),
        ];

        for (context, process_xdg, expected) in cases {
            let actual = config_home_for_context(&context, process_xdg, Some(Path::new("/root")));
            assert_eq!(actual.as_deref(), expected, "context: {context:?}");
        }
    }
}
