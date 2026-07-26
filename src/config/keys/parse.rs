use super::model::{KeyCodeSpec, KeyList, KeyModifierSpec, KeySpec, NamedKey};
use super::validate::{is_reserved_plain_char, validate_key_spec};
use serde::Deserialize;
use std::collections::BTreeMap;

#[derive(Clone, Debug, Deserialize)]
#[serde(untagged)]
pub(super) enum KeyConfigOverride {
    One(String),
    Many(Vec<String>),
}

#[derive(Deserialize, Default)]
pub(in crate::config) struct KeysConfigOverride {
    pub(super) choose: Option<KeyConfigOverride>,
    pub(super) quit: Option<KeyConfigOverride>,
    pub(super) quit_without_cd: Option<KeyConfigOverride>,
    pub(super) yank: Option<KeyConfigOverride>,
    pub(super) cut: Option<KeyConfigOverride>,
    pub(super) paste: Option<KeyConfigOverride>,
    pub(super) create_archive: Option<KeyConfigOverride>,
    pub(super) extract_archive: Option<KeyConfigOverride>,
    pub(super) symlink_absolute: Option<KeyConfigOverride>,
    pub(super) symlink_relative: Option<KeyConfigOverride>,
    pub(super) trash: Option<KeyConfigOverride>,
    pub(super) delete_permanently: Option<KeyConfigOverride>,
    pub(super) create: Option<KeyConfigOverride>,
    pub(super) rename: Option<KeyConfigOverride>,
    pub(super) rename_in_editor: Option<KeyConfigOverride>,
    pub(super) restore_from_trash: Option<KeyConfigOverride>,
    pub(super) copy_path: Option<KeyConfigOverride>,
    pub(super) search_folders: Option<KeyConfigOverride>,
    pub(super) search_files: Option<KeyConfigOverride>,
    pub(super) filter_directory: Option<KeyConfigOverride>,
    pub(super) zoxide: Option<KeyConfigOverride>,
    pub(super) shell: Option<KeyConfigOverride>,
    pub(super) open: Option<KeyConfigOverride>,
    pub(super) open_with: Option<KeyConfigOverride>,
    pub(super) open_or_enter: Option<KeyConfigOverride>,
    pub(super) go_to: Option<KeyConfigOverride>,
    pub(super) toggle_selection: Option<KeyConfigOverride>,
    pub(super) cycle_places_next: Option<KeyConfigOverride>,
    pub(super) cycle_places_previous: Option<KeyConfigOverride>,
    pub(super) go_parent: Option<KeyConfigOverride>,
    pub(super) page_up: Option<KeyConfigOverride>,
    pub(super) page_down: Option<KeyConfigOverride>,
    pub(super) jump_first: Option<KeyConfigOverride>,
    pub(super) jump_last: Option<KeyConfigOverride>,
    pub(super) select_all: Option<KeyConfigOverride>,
    pub(super) history_back: Option<KeyConfigOverride>,
    pub(super) history_forward: Option<KeyConfigOverride>,
    pub(super) sort: Option<KeyConfigOverride>,
    pub(super) toggle_view: Option<KeyConfigOverride>,
    pub(super) toggle_preview: Option<KeyConfigOverride>,
    pub(super) fullscreen_preview: Option<KeyConfigOverride>,
    pub(super) toggle_hidden: Option<KeyConfigOverride>,
    pub(super) nav_left: Option<KeyConfigOverride>,
    pub(super) nav_down: Option<KeyConfigOverride>,
    pub(super) nav_up: Option<KeyConfigOverride>,
    pub(super) nav_right: Option<KeyConfigOverride>,
    pub(super) scroll_preview_left: Option<KeyConfigOverride>,
    pub(super) scroll_preview_right: Option<KeyConfigOverride>,
    pub(super) scroll_preview_up: Option<KeyConfigOverride>,
    pub(super) scroll_preview_down: Option<KeyConfigOverride>,
    #[serde(flatten)]
    pub(super) unknown: BTreeMap<String, toml::Value>,
}

pub(super) fn parse_key_override(
    name: &str,
    value: &KeyConfigOverride,
    default: &KeyList,
) -> Option<KeyList> {
    let values: Vec<&str> = match value {
        KeyConfigOverride::One(value) => vec![value.as_str()],
        KeyConfigOverride::Many(values) => values.iter().map(String::as_str).collect(),
    };

    let mut parsed = Vec::with_capacity(values.len());
    for value in values {
        let spec = parse_key_spec(name, value, default)?;
        if parsed.contains(&spec) {
            eprintln!(
                "elio: keys.{name}: '{spec}' is listed more than once; using default '{default}'"
            );
            return None;
        }
        parsed.push(spec);
    }

    Some(KeyList(parsed))
}

fn parse_key_spec(name: &str, value: &str, default: &KeyList) -> Option<KeySpec> {
    let (mut modifiers, key_name) = parse_key_modifiers(name, value, default)?;

    if modifiers.shift && key_name.eq_ignore_ascii_case("tab") {
        modifiers.shift = false;
        return validate_key_spec(
            name,
            KeySpec {
                code: KeyCodeSpec::Named(NamedKey::BackTab),
                modifiers,
            },
            default,
        );
    }

    if modifiers.shift && key_name.eq_ignore_ascii_case("backtab") {
        eprintln!(
            "elio: keys.{name}: {value:?} uses shift with backtab; use \"shift+tab\" or \"backtab\" instead; using default '{default}'"
        );
        return None;
    }

    if modifiers.shift && key_name.eq_ignore_ascii_case("space") {
        eprintln!(
            "elio: keys.{name}: {value:?} uses shift with space, which terminals do not report reliably; using default '{default}'"
        );
        return None;
    }

    if modifiers.shift && key_name.len() == 1 {
        let c = key_name.chars().next().expect("single-char key");
        let suggestion = if c.is_ascii_alphabetic() {
            format!("; use {:?} instead", c.to_ascii_uppercase().to_string())
        } else {
            String::new()
        };
        eprintln!(
            "elio: keys.{name}: {value:?} uses shift with a character{suggestion}; using default '{default}'"
        );
        return None;
    }

    if key_name.eq_ignore_ascii_case("shift")
        || key_name.eq_ignore_ascii_case("ctrl")
        || key_name.eq_ignore_ascii_case("alt")
    {
        eprintln!(
            "elio: keys.{name}: {value:?} is missing a key after modifiers; using default '{default}'"
        );
        return None;
    }

    if let Some(named) = NamedKey::parse(key_name) {
        let spec = KeySpec {
            code: KeyCodeSpec::Named(named),
            modifiers,
        };
        return validate_key_spec(name, spec, default);
    }

    let mut chars = key_name.chars();
    let Some(mut c) = chars.next() else {
        eprintln!(
            "elio: keys.{name}: empty strings cannot be used as key bindings; use [] to unbind this action; using default '{default}'"
        );
        return None;
    };
    if chars.next().is_some() {
        eprintln!(
            "elio: keys.{name}: {value:?} is not a single character, modifier binding, or supported named key; using default '{default}'"
        );
        return None;
    }
    if is_reserved_plain_char(c) && modifiers.is_empty() {
        eprintln!(
            "elio: keys.{name}: '{c}' is reserved and cannot be rebound; using default '{default}'"
        );
        return None;
    }
    if c.is_control() {
        eprintln!(
            "elio: keys.{name}: control characters cannot be used as key bindings; using default '{default}'"
        );
        return None;
    }
    if c == ' ' && modifiers.is_empty() {
        return validate_key_spec(
            name,
            KeySpec {
                code: KeyCodeSpec::Named(NamedKey::Space),
                modifiers,
            },
            default,
        );
    }

    if (modifiers.ctrl || modifiers.alt) && c.is_ascii_alphabetic() {
        c = c.to_ascii_lowercase();
    }

    validate_key_spec(
        name,
        KeySpec {
            code: KeyCodeSpec::Char(c),
            modifiers,
        },
        default,
    )
}

fn parse_key_modifiers<'a>(
    name: &str,
    value: &'a str,
    default: &KeyList,
) -> Option<(KeyModifierSpec, &'a str)> {
    let mut parts = value.split('+').peekable();
    let mut modifiers = KeyModifierSpec::NONE;
    let mut key = None;

    while let Some(part) = parts.next() {
        if part.is_empty() {
            if value.is_empty() {
                eprintln!(
                    "elio: keys.{name}: empty strings cannot be used as key bindings; use [] to unbind this action; using default '{default}'"
                );
            } else {
                eprintln!(
                    "elio: keys.{name}: {value:?} contains an empty key component; using default '{default}'"
                );
            }
            return None;
        }

        if parts.peek().is_none() {
            key = Some(part);
            break;
        }

        match part.to_ascii_lowercase().as_str() {
            "ctrl" => {
                if modifiers.ctrl {
                    eprintln!(
                        "elio: keys.{name}: duplicate modifier 'ctrl' in {value:?}; using default '{default}'"
                    );
                    return None;
                }
                modifiers.ctrl = true;
            }
            "alt" => {
                if modifiers.alt {
                    eprintln!(
                        "elio: keys.{name}: duplicate modifier 'alt' in {value:?}; using default '{default}'"
                    );
                    return None;
                }
                modifiers.alt = true;
            }
            "shift" => {
                if modifiers.shift {
                    eprintln!(
                        "elio: keys.{name}: duplicate modifier 'shift' in {value:?}; using default '{default}'"
                    );
                    return None;
                }
                modifiers.shift = true;
            }
            _ => {
                eprintln!(
                    "elio: keys.{name}: unknown modifier {part:?} in {value:?}; supported modifiers are ctrl, alt, and shift; using default '{default}'"
                );
                return None;
            }
        }
    }

    let Some(key) = key else {
        eprintln!(
            "elio: keys.{name}: {value:?} is missing a key after modifiers; using default '{default}'"
        );
        return None;
    };

    Some((modifiers, key))
}
