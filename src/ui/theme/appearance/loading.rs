use super::Theme;
use crate::config;
use std::{
    fs, io,
    path::{Path, PathBuf},
};

pub(super) fn load_theme_from_disk(override_path: Option<&Path>) -> anyhow::Result<Theme> {
    let is_override = override_path.is_some();
    let Some(path) = override_path.map(Path::to_path_buf).or_else(theme_path) else {
        return Ok(Theme::default_theme());
    };
    let contents = match fs::read_to_string(&path) {
        Ok(contents) => contents,
        Err(error) if !is_override && error.kind() == io::ErrorKind::NotFound => {
            return Ok(Theme::default_theme());
        }
        Err(error) if is_override => {
            anyhow::bail!(
                "elio: failed to read theme from {}: {error}",
                path.display()
            );
        }
        Err(error) => {
            eprintln!(
                "elio: failed to read theme from {}: {error}",
                path.display()
            );
            return Ok(Theme::default_theme());
        }
    };

    Ok(match Theme::from_config_str(&contents) {
        Ok(theme) => theme,
        Err(error) => {
            eprintln!(
                "elio: failed to load theme from {}: {error}",
                path.display()
            );
            Theme::default_theme()
        }
    })
}

fn theme_path() -> Option<PathBuf> {
    config::config_dir().map(|dir| dir.join("theme.toml"))
}
