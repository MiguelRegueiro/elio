use serde::Deserialize;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct PlacesConfig {
    pub show_devices: bool,
    pub entries: Vec<PlaceEntrySpec>,
}

impl Default for PlacesConfig {
    fn default() -> Self {
        Self {
            show_devices: true,
            entries: vec![
                PlaceEntrySpec::builtin(BuiltinPlace::Home),
                PlaceEntrySpec::builtin(BuiltinPlace::Desktop),
                PlaceEntrySpec::builtin(BuiltinPlace::Documents),
                PlaceEntrySpec::builtin(BuiltinPlace::Downloads),
                PlaceEntrySpec::builtin(BuiltinPlace::Pictures),
                PlaceEntrySpec::builtin(BuiltinPlace::Music),
                PlaceEntrySpec::builtin(BuiltinPlace::Videos),
                PlaceEntrySpec::builtin(BuiltinPlace::Root),
                PlaceEntrySpec::builtin(BuiltinPlace::Trash),
            ],
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum BuiltinPlace {
    Home,
    Desktop,
    Documents,
    Downloads,
    Pictures,
    Music,
    Videos,
    Root,
    Trash,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum PlaceEntrySpec {
    Builtin {
        place: BuiltinPlace,
        icon: Option<String>,
    },
    Custom {
        title: String,
        path: PathBuf,
        icon: Option<String>,
    },
}

#[derive(Deserialize, Default)]
pub(super) struct PlacesConfigOverride {
    show_devices: Option<bool>,
    entries: Option<Vec<toml::Value>>,
}

impl PlacesConfig {
    pub(super) fn from_override(overrides: PlacesConfigOverride, defaults: &Self) -> Self {
        let mut resolved = defaults.clone();
        if let Some(show_devices) = overrides.show_devices {
            resolved.show_devices = show_devices;
        }
        if let Some(entries) = overrides.entries {
            resolved.entries = entries
                .iter()
                .enumerate()
                .filter_map(|(index, entry)| {
                    PlaceEntrySpec::from_toml_value(entry, &format!("places.entries[{index}]"))
                })
                .collect();
        }
        resolved
    }
}

impl PlaceEntrySpec {
    fn builtin(place: BuiltinPlace) -> Self {
        Self::Builtin { place, icon: None }
    }

    fn from_toml_value(value: &toml::Value, field_name: &str) -> Option<Self> {
        match value {
            toml::Value::String(name) => BuiltinPlace::parse(name).map(Self::builtin),
            toml::Value::Table(table) => {
                let icon = parse_place_icon(table.get("icon"), field_name);
                if let Some(builtin) = table.get("builtin") {
                    let Some(name) = builtin
                        .as_str()
                        .map(str::trim)
                        .filter(|name| !name.is_empty())
                    else {
                        eprintln!(
                            "elio: {field_name}: builtin places require a non-empty string builtin name; \
                             skipping entry"
                        );
                        return None;
                    };
                    let place = BuiltinPlace::parse(name)?;
                    if table.contains_key("title") || table.contains_key("path") {
                        eprintln!(
                            "elio: {field_name}: builtin places only support {{ builtin, icon }}; \
                             ignoring extra fields"
                        );
                    }
                    return Some(Self::Builtin { place, icon });
                }

                let title = table
                    .get("title")
                    .and_then(toml::Value::as_str)
                    .map(str::trim)
                    .filter(|title| !title.is_empty());
                let Some(title) = title else {
                    eprintln!(
                        "elio: {field_name}: custom places require a non-empty string title; \
                         skipping entry"
                    );
                    return None;
                };

                let path = table
                    .get("path")
                    .and_then(toml::Value::as_str)
                    .map(str::trim)
                    .filter(|path| !path.is_empty());
                let Some(path) = path else {
                    eprintln!(
                        "elio: {field_name}: custom places require a non-empty string path; \
                         skipping entry"
                    );
                    return None;
                };

                match expand_custom_place_path(path) {
                    Ok(path) => Some(Self::Custom {
                        title: title.to_string(),
                        path,
                        icon,
                    }),
                    Err(error) => {
                        eprintln!("elio: {field_name}: {error}; skipping entry");
                        None
                    }
                }
            }
            _ => {
                eprintln!(
                    "elio: {field_name}: expected a built-in name, {{ builtin, icon? }}, or \
                     {{ title, path, icon? }} object; skipping entry"
                );
                None
            }
        }
    }
}

fn parse_place_icon(value: Option<&toml::Value>, field_name: &str) -> Option<String> {
    let value = value?;
    match value {
        toml::Value::String(icon) => {
            let icon = icon.trim();
            if icon.is_empty() {
                eprintln!("elio: {field_name}: icon must be a non-empty string; using default");
                None
            } else {
                Some(icon.to_string())
            }
        }
        _ => {
            eprintln!("elio: {field_name}: icon must be a string; using default");
            None
        }
    }
}

impl BuiltinPlace {
    fn parse(name: &str) -> Option<Self> {
        match name.trim().to_ascii_lowercase().as_str() {
            "home" => Some(Self::Home),
            "desktop" => Some(Self::Desktop),
            "documents" => Some(Self::Documents),
            "downloads" => Some(Self::Downloads),
            "pictures" => Some(Self::Pictures),
            "music" => Some(Self::Music),
            "videos" => Some(Self::Videos),
            "root" => Some(Self::Root),
            "trash" => Some(Self::Trash),
            _ => {
                eprintln!(
                    "elio: unknown places entry {name:?}; expected one of: \
                     home, desktop, documents, downloads, pictures, music, videos, root, trash \
                     (use semantic ids like \"downloads\", not localized folder names)"
                );
                None
            }
        }
    }
}

pub(super) fn expand_custom_place_path(path: &str) -> anyhow::Result<PathBuf> {
    let expanded = if path == "~" {
        super::invoking_user_home_dir()
            .ok_or_else(|| anyhow::anyhow!("could not resolve home directory"))?
    } else if let Some(rest) = path.strip_prefix("~/").or_else(|| path.strip_prefix("~\\")) {
        super::invoking_user_home_dir()
            .ok_or_else(|| anyhow::anyhow!("could not resolve home directory"))?
            .join(rest)
    } else {
        PathBuf::from(path)
    };

    if !expanded.is_absolute() {
        anyhow::bail!("custom place paths must be absolute or start with ~/");
    }

    Ok(normalize_absolute_path(&expanded))
}

fn normalize_absolute_path(path: &Path) -> PathBuf {
    use std::path::Component;

    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                let _ = normalized.pop();
            }
            _ => normalized.push(component.as_os_str()),
        }
    }
    normalized
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_absolute_path_drops_current_and_parent_segments() {
        let path = PathBuf::from("/tmp/elio/./config/../theme.toml");
        assert_eq!(
            normalize_absolute_path(&path),
            PathBuf::from("/tmp/elio/theme.toml")
        );
    }

    #[test]
    fn expand_custom_place_path_accepts_absolute_paths() {
        let path = std::env::temp_dir().join("elio-projects");
        let path_str = path.to_string_lossy().into_owned();
        assert_eq!(
            expand_custom_place_path(&path_str).expect("path should parse"),
            path
        );
    }
}
