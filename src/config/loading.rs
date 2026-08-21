use super::Config;
#[cfg(unix)]
use super::invoking_user::InvocationContext;
use std::{
    env, fs, io,
    path::{Path, PathBuf},
    sync::OnceLock,
};

static ACTIVE_CONFIG: OnceLock<Config> = OnceLock::new();

pub(super) fn initialize(path: Option<&Path>) -> anyhow::Result<()> {
    if ACTIVE_CONFIG.get().is_none() {
        let config = load_config_from_disk(path)?;
        let _ = ACTIVE_CONFIG.set(config);
    }
    Ok(())
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

fn load_config_from_disk(override_path: Option<&Path>) -> anyhow::Result<Config> {
    let is_override = override_path.is_some();
    let Some(path) = override_path.map(Path::to_path_buf).or_else(config_path) else {
        return Ok(Config::default_config());
    };
    let contents = match fs::read_to_string(&path) {
        Ok(contents) => contents,
        Err(error) if !is_override && error.kind() == io::ErrorKind::NotFound => {
            return Ok(Config::default_config());
        }
        Err(error) if is_override => {
            anyhow::bail!(
                "elio: failed to read config from {}: {error}",
                path.display()
            );
        }
        Err(error) => {
            eprintln!(
                "elio: failed to read config from {}: {error}",
                path.display()
            );
            return Ok(Config::default_config());
        }
    };

    Ok(match Config::from_str(&contents) {
        Ok(config) => config,
        Err(error) => {
            eprintln!(
                "elio: failed to load config from {}: {error}",
                path.display()
            );
            Config::default_config()
        }
    })
}

fn config_path() -> Option<PathBuf> {
    config_dir().map(|dir| dir.join("config.toml"))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(unix)]
    use crate::config::InvokingUser;
    #[cfg(unix)]
    use std::ffi::OsString;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[cfg(unix)]
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

    fn temp_path(label: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after unix epoch")
            .as_nanos();
        std::env::temp_dir().join(format!("elio-config-{label}-{unique}"))
    }

    #[cfg(unix)]
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

    #[test]
    fn explicit_config_path_is_loaded() {
        let root = temp_path("explicit");
        let path = root.join("custom-settings.toml");
        fs::create_dir_all(&root).expect("config directory should be created");
        fs::write(&path, "[ui]\nshow_hidden = true\n").expect("explicit config should be written");

        let config = load_config_from_disk(Some(&path)).expect("explicit config should load");

        assert!(config.ui.show_hidden);
        fs::remove_dir_all(root).expect("config directory should be removed");
    }

    #[test]
    fn missing_explicit_config_path_is_an_error() {
        let path = temp_path("missing").join("config.toml");

        let error = load_config_from_disk(Some(&path))
            .err()
            .expect("missing explicit config should fail");

        assert!(error.to_string().contains(&format!(
            "elio: failed to read config from {}",
            path.display()
        )));
    }

    #[test]
    fn unreadable_explicit_config_path_is_an_error() {
        let root = temp_path("unreadable");
        let path = root.join("config.toml");
        fs::create_dir_all(&root).expect("config directory should be created");
        fs::write(&path, [0xff]).expect("invalid UTF-8 config should be written");

        let error = load_config_from_disk(Some(&path))
            .err()
            .expect("unreadable explicit config should fail");

        assert!(error.to_string().contains(&format!(
            "elio: failed to read config from {}",
            path.display()
        )));
        fs::remove_dir_all(root).expect("config directory should be removed");
    }

    #[test]
    fn invalid_explicit_config_falls_back_to_defaults() {
        let root = temp_path("invalid");
        let path = root.join("config.toml");
        fs::create_dir_all(&root).expect("config directory should be created");
        fs::write(&path, "[ui\nshow_hidden = true\n").expect("invalid config should be written");

        let config =
            load_config_from_disk(Some(&path)).expect("invalid explicit config should fall back");

        assert!(!config.ui.show_hidden);
        fs::remove_dir_all(root).expect("config directory should be removed");
    }
}
