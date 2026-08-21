use super::super::{loading::load_theme_from_disk, parsing::parse_color, rules::rgb};
use super::*;
use ratatui::style::Color;
use std::{
    env,
    ffi::OsString,
    sync::{Mutex, OnceLock},
};

fn env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

struct EnvVarGuard {
    key: &'static str,
    original: Option<OsString>,
}

impl EnvVarGuard {
    fn set_path(key: &'static str, value: &Path) -> Self {
        let original = env::var_os(key);
        unsafe {
            env::set_var(key, value);
        }
        Self { key, original }
    }
}

impl Drop for EnvVarGuard {
    fn drop(&mut self) {
        match self.original.as_ref() {
            Some(value) => unsafe {
                env::set_var(self.key, value);
            },
            None => unsafe {
                env::remove_var(self.key);
            },
        }
    }
}

fn write_theme_file(
    label: &str,
    contents: &str,
) -> (PathBuf, PathBuf, std::sync::MutexGuard<'static, ()>) {
    let guard = env_lock()
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    let config_home = temp_path(label);
    let theme_dir = config_home.join("elio");
    fs::create_dir_all(&theme_dir).expect("failed to create theme config dir");
    let path = theme_dir.join("theme.toml");
    fs::write(&path, contents).expect("failed to write theme file");
    (config_home, path, guard)
}

#[test]
fn load_theme_from_disk_reads_theme_file_from_xdg_config_home() {
    let (config_home, path, _guard) = write_theme_file(
        "load-theme-from-disk",
        r##"
[classes.code]
icon = "X"
color = "#112233"

[directories.projects]
icon = "P"
color = "#334455"

[preview.code]
keyword = "#abcdef"
"##,
    );
    let _xdg = EnvVarGuard::set_path("XDG_CONFIG_HOME", &config_home);

    let theme = load_theme_from_disk(None).expect("theme should load from XDG config home");

    assert_eq!(theme.preview.code.keyword, rgb(0xab, 0xcd, 0xef));
    assert_eq!(theme.classes.get(&FileClass::Code).unwrap().icon, "X");
    assert_eq!(
        theme.classes.get(&FileClass::Code).unwrap().color,
        rgb(0x11, 0x22, 0x33)
    );
    let projects = theme.resolve(Path::new("projects"), EntryKind::Directory);
    assert_eq!(projects.class, FileClass::Directory);
    assert_eq!(projects.icon, "P");
    assert_eq!(projects.color, rgb(0x33, 0x44, 0x55));

    fs::remove_file(path).expect("failed to remove theme file");
    fs::remove_dir_all(config_home).expect("failed to remove config root");
}

#[test]
fn load_theme_from_disk_falls_back_to_default_theme_for_invalid_theme_file() {
    let (config_home, path, _guard) = write_theme_file(
        "load-theme-invalid",
        r##"
[preview.code]
keyword = "#12"
"##,
    );
    let _xdg = EnvVarGuard::set_path("XDG_CONFIG_HOME", &config_home);

    let theme = load_theme_from_disk(None).expect("invalid theme should fall back");
    let default_theme = Theme::default_theme();

    assert_eq!(theme.palette.bg, default_theme.palette.bg);
    assert_eq!(
        theme.preview.code.keyword,
        default_theme.preview.code.keyword
    );
    assert_eq!(
        theme.resolve(Path::new("Cargo.lock"), EntryKind::File).icon,
        default_theme
            .resolve(Path::new("Cargo.lock"), EntryKind::File)
            .icon,
    );

    fs::remove_file(path).expect("failed to remove theme file");
    fs::remove_dir_all(config_home).expect("failed to remove config root");
}

#[test]
fn load_theme_from_disk_reads_explicit_theme_path() {
    let root = temp_path("load-explicit-theme");
    let path = root.join("custom-theme.toml");
    fs::create_dir_all(&root).expect("failed to create explicit theme directory");
    fs::write(
        &path,
        r##"
[preview.code]
keyword = "#123456"
"##,
    )
    .expect("failed to write explicit theme file");

    let theme = load_theme_from_disk(Some(&path)).expect("explicit theme should load");

    assert_eq!(theme.preview.code.keyword, rgb(0x12, 0x34, 0x56));
    fs::remove_dir_all(root).expect("failed to remove explicit theme directory");
}

#[test]
fn missing_explicit_theme_path_is_an_error() {
    let path = temp_path("missing-explicit-theme").join("theme.toml");

    let error = load_theme_from_disk(Some(&path))
        .err()
        .expect("missing explicit theme should fail");

    assert!(error.to_string().contains(&format!(
        "elio: failed to read theme from {}",
        path.display()
    )));
}

#[test]
fn unreadable_explicit_theme_path_is_an_error() {
    let root = temp_path("unreadable-explicit-theme");
    let path = root.join("theme.toml");
    fs::create_dir_all(&root).expect("failed to create explicit theme directory");
    fs::write(&path, [0xff]).expect("failed to write invalid UTF-8 theme");

    let error = load_theme_from_disk(Some(&path))
        .err()
        .expect("unreadable explicit theme should fail");

    assert!(error.to_string().contains(&format!(
        "elio: failed to read theme from {}",
        path.display()
    )));
    fs::remove_dir_all(root).expect("failed to remove explicit theme directory");
}

#[test]
fn invalid_explicit_theme_falls_back_to_default_theme() {
    let root = temp_path("invalid-explicit-theme");
    let path = root.join("theme.toml");
    fs::create_dir_all(&root).expect("failed to create explicit theme directory");
    fs::write(
        &path,
        r##"
[preview.code]
keyword = "#12"
"##,
    )
    .expect("failed to write invalid explicit theme");

    let theme = load_theme_from_disk(Some(&path))
        .expect("invalid explicit theme should fall back to the default");
    let default_theme = Theme::default_theme();

    assert_eq!(theme.palette.bg, default_theme.palette.bg);
    assert_eq!(
        theme.preview.code.keyword,
        default_theme.preview.code.keyword
    );
    fs::remove_dir_all(root).expect("failed to remove explicit theme directory");
}

#[test]
fn exact_file_rules_override_extension_defaults() {
    let theme = Theme::default_theme();
    let resolved = theme.resolve(Path::new("Cargo.lock"), EntryKind::File);
    assert_eq!(resolved.class, FileClass::Data);
    assert_eq!(resolved.icon, "󰈡");
}

#[test]
fn theme_file_overrides_class_icon_and_palette() {
    let theme = Theme::from_config_str(
        r##"
[classes.code]
icon = "X"
color = "#112233"

[files."special.rs"]
icon = "Y"
color = "#abcdef"
class = "document"
"##,
    )
    .expect("theme should parse");

    let resolved = theme.resolve(Path::new("special.rs"), EntryKind::File);
    assert_eq!(resolved.class, FileClass::Document);
    assert_eq!(resolved.icon, "Y");
    assert_eq!(resolved.color, rgb(0xab, 0xcd, 0xef));
}

#[test]
fn extension_rules_can_be_overridden_from_config() {
    let theme = Theme::from_config_str(
        r##"
[extensions.lock]
class = "data"
icon = "L"
"##,
    )
    .expect("theme should parse");

    let resolved = theme.resolve(Path::new("custom.lock"), EntryKind::File);
    assert_eq!(resolved.class, FileClass::Data);
    assert_eq!(resolved.icon, "L");
}

#[test]
fn directory_rules_can_be_overridden_from_config() {
    let theme = Theme::from_config_str(
        r##"
[directories.docs]
class = "document"
icon = "D"
color = "#102030"
"##,
    )
    .expect("theme should parse");

    let resolved = theme.resolve(Path::new("docs"), EntryKind::Directory);
    assert_eq!(resolved.class, FileClass::Document);
    assert_eq!(resolved.icon, "D");
    assert_eq!(resolved.color, rgb(0x10, 0x20, 0x30));
}

#[test]
fn code_preview_colors_can_be_overridden_from_config() {
    let theme = Theme::from_config_str(
        r##"
[preview.code]
keyword = "#123456"
function = "#abcdef"
macro = "#fedcba"
"##,
    )
    .expect("theme should parse");

    assert_eq!(theme.preview.code.keyword, rgb(0x12, 0x34, 0x56));
    assert_eq!(theme.preview.code.function, rgb(0xab, 0xcd, 0xef));
    assert_eq!(theme.preview.code.r#macro, rgb(0xfe, 0xdc, 0xba));
}

#[test]
fn unknown_rule_classes_are_rejected_during_theme_parsing() {
    let error = match Theme::from_config_str(
        r##"
[extensions.rs]
class = "not-a-real-class"
"##,
    ) {
        Ok(_) => panic!("theme parsing should reject unknown classes"),
        Err(error) => error,
    };

    assert!(
        error.to_string().contains("unknown class"),
        "unexpected parse error: {error}",
    );
}

#[test]
fn exact_name_rules_win_over_extension_rules() {
    let theme = Theme::from_config_str(
        r##"
[extensions.toml]
class = "data"
icon = "E"

[files."Cargo.toml"]
class = "config"
icon = "F"
"##,
    )
    .expect("theme should parse");

    let resolved = theme.resolve(Path::new("Cargo.toml"), EntryKind::File);
    assert_eq!(resolved.class, FileClass::Config);
    assert_eq!(resolved.icon, "F");
}

#[test]
fn palette_accepts_transparent_sentinels() {
    let theme = Theme::from_config_str(
        r##"
[palette]
bg = "none"
chrome = "transparent"
panel = "  None  "
path_bg = "  Transparent  "

[preview.code]
bg = "none"
"##,
    )
    .expect("theme should parse");

    assert_eq!(theme.palette.bg, Color::Reset);
    assert_eq!(theme.palette.chrome, Color::Reset);
    assert_eq!(theme.palette.panel, Color::Reset);
    assert_eq!(theme.palette.path_bg, Color::Reset);
    assert_eq!(theme.preview.code.bg, Color::Reset);

    let default_theme = Theme::default_theme();
    assert_eq!(theme.palette.chrome_alt, default_theme.palette.chrome_alt);
    assert_eq!(theme.palette.text, default_theme.palette.text);
}

#[test]
fn parse_color_accepts_all_terminal_ansi_names() {
    for (name, expected) in [
        ("ansi-black", Color::Black),
        ("ansi-red", Color::Red),
        ("ansi-green", Color::Green),
        ("ansi-yellow", Color::Yellow),
        ("ansi-blue", Color::Blue),
        ("ansi-magenta", Color::Magenta),
        ("ansi-cyan", Color::Cyan),
        ("ansi-white", Color::Gray),
        ("ansi-bright-black", Color::DarkGray),
        ("ansi-bright-red", Color::LightRed),
        ("ansi-bright-green", Color::LightGreen),
        ("ansi-bright-yellow", Color::LightYellow),
        ("ansi-bright-blue", Color::LightBlue),
        ("ansi-bright-magenta", Color::LightMagenta),
        ("ansi-bright-cyan", Color::LightCyan),
        ("ansi-bright-white", Color::White),
    ] {
        assert_eq!(parse_color(name).unwrap(), expected);
    }
}

#[test]
fn palette_accepts_terminal_ansi_colors() {
    let theme = Theme::from_config_str(
        r##"
[palette]
bg = "none"
text = "ansi-white"
muted = "  ANSI-BRIGHT-BLACK  "
accent = "indexed-12"
accent_text = "ansi-bright-white"

[classes.code]
color = "ansi-cyan"
"##,
    )
    .expect("theme should parse");

    assert_eq!(theme.palette.bg, Color::Reset);
    assert_eq!(theme.palette.text, Color::Gray);
    assert_eq!(theme.palette.muted, Color::DarkGray);
    assert_eq!(theme.palette.accent, Color::Indexed(12));
    assert_eq!(theme.palette.accent_text, Color::White);
    assert_eq!(
        theme.classes.get(&FileClass::Code).unwrap().color,
        Color::Cyan
    );
}

#[test]
fn chip_colors_default_to_semantic_contrast_and_are_overridable() {
    let default_theme = Theme::default_theme();
    assert_eq!(default_theme.palette.chip_text, rgb(0x0c, 0x0c, 0x0c));
    assert_eq!(default_theme.palette.progress_bar, rgb(0x41, 0xa0, 0xdc));

    let custom = Theme::from_config_str(
        r##"
[palette]
chip_text = "#ffffff"
progress_bar = "#123456"
"##,
    )
    .expect("theme should parse");
    assert_eq!(custom.palette.chip_text, rgb(0xff, 0xff, 0xff));
    assert_eq!(custom.palette.progress_bar, rgb(0x12, 0x34, 0x56));
}

#[test]
fn class_and_rule_colors_accept_transparent_sentinel() {
    let theme = Theme::from_config_str(
        r##"
[classes.code]
color = "none"

[extensions.rs]
color = "transparent"
"##,
    )
    .expect("theme should parse");

    assert_eq!(
        theme.classes.get(&FileClass::Code).unwrap().color,
        Color::Reset
    );

    let rs = theme.resolve(Path::new("main.rs"), EntryKind::File);
    assert_eq!(rs.color, Color::Reset);
}

#[test]
fn invalid_color_strings_still_fail_to_parse() {
    let error = match Theme::from_config_str(
        r##"
[palette]
bg = "almost-transparent"
"##,
    ) {
        Ok(_) => panic!("unknown sentinel should fail to parse"),
        Err(error) => error,
    };

    assert!(
        error.to_string().contains("invalid color"),
        "unexpected parse error: {error}",
    );
}

#[test]
fn matching_is_case_insensitive_and_trimmed() {
    let theme = Theme::from_config_str(
        r##"
[classes." folder "]
icon = "D"
color = "#010203"

[extensions." LOCK "]
class = "data"
icon = "L"

[files." cargo.lock "]
class = "data"
icon = "F"
"##,
    )
    .expect("theme should parse");

    let dir = theme.resolve(Path::new("projects"), EntryKind::Directory);
    assert_eq!(dir.class, FileClass::Directory);
    assert_eq!(theme.classes.get(&FileClass::Directory).unwrap().icon, "D");

    let file = theme.resolve(Path::new("CARGO.LOCK"), EntryKind::File);
    assert_eq!(file.class, FileClass::Data);
    assert_eq!(file.icon, "F");
}
