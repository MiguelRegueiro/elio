use super::super::*;

#[test]
fn keys_default_bindings_are_sane() {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    let config = Config::default_config();
    assert_eq!(config.keys.yank, 'y');
    assert_eq!(config.keys.cut, 'x');
    assert_eq!(config.keys.paste, 'p');
    assert_eq!(config.keys.toggle_preview, 'V');
    assert_eq!(config.keys.action_for('V'), Some(Action::TogglePreview));
    assert_eq!(config.keys.extract_archive, 'e');
    assert_eq!(config.keys.symlink_absolute, '-');
    assert_eq!(config.keys.symlink_relative, '_');
    assert_eq!(config.keys.trash.to_string(), "d/Del");
    assert_eq!(config.keys.action_for('d'), Some(Action::Trash));
    assert_eq!(
        config
            .keys
            .action_for_key(KeyEvent::new(KeyCode::Delete, KeyModifiers::NONE)),
        Some(Action::Trash)
    );
    assert_eq!(config.keys.delete_permanently.to_string(), "D/Shift+Del");
    assert_eq!(config.keys.choose.to_string(), "Enter");
    assert_eq!(config.keys.quit, 'q');
    assert_eq!(config.keys.quit_without_cd, 'Q');
    assert_eq!(config.keys.zoxide, 'z');
    assert_eq!(config.keys.shell, '!');
}

#[test]
fn keys_can_be_overridden() {
    let config = Config::from_str(
        r#"
[keys]
yank = "Y"
cut = "X"
"#,
    )
    .expect("config should parse");
    assert_eq!(config.keys.yank, 'Y');
    assert_eq!(config.keys.cut, 'X');
    assert_eq!(config.keys.paste, 'p');
}

#[test]
fn unknown_keys_are_ignored_without_dropping_valid_overrides() {
    let config = Config::from_str(
        r#"
[keys]
open_withh = "w"
open_with = "P"
"#,
    )
    .expect("config should parse");

    assert_eq!(config.keys.open_with, 'P');
    assert_eq!(config.keys.action_for('w'), None);
}

#[test]
fn symlink_keys_can_be_overridden() {
    let config = Config::from_str(
        r#"
[keys]
symlink_absolute = "m"
symlink_relative = "M"
"#,
    )
    .expect("config should parse");

    assert_eq!(config.keys.action_for('m'), Some(Action::SymlinkAbsolute));
    assert_eq!(config.keys.action_for('M'), Some(Action::SymlinkRelative));
    assert_eq!(config.keys.action_for('-'), None);
    assert_eq!(config.keys.action_for('_'), None);
}

#[test]
fn unknown_key_warning_names_config_path() {
    assert_eq!(
        super::super::keys::unknown_key_action_warning("open_withh"),
        "elio: keys.open_withh: unknown key action; ignoring"
    );
}

#[test]
fn keys_accept_array_overrides() {
    let config = Config::from_str(
        r#"
[keys]
extract_archive = []
open = ["o", "e"]
open_with = ["O"]
"#,
    )
    .expect("config should parse");
    assert_eq!(config.keys.action_for('o'), Some(Action::Open));
    assert_eq!(config.keys.action_for('e'), Some(Action::Open));
    assert_eq!(config.keys.action_for('O'), Some(Action::OpenWith));
}

#[test]
fn keys_accepts_empty_array_to_unbind_action() {
    let config = Config::from_str(
        r#"
[keys]
open = []
"#,
    )
    .expect("config should parse");
    assert_eq!(config.keys.open.to_string(), "");
    assert_eq!(config.keys.action_for('o'), None);
}

#[test]
fn unbound_action_frees_its_default_key_for_another_action() {
    let config = Config::from_str(
        r#"
[keys]
open = []
shell = "o"
"#,
    )
    .expect("config should parse");
    assert_eq!(config.keys.action_for('o'), Some(Action::Shell));
    assert_eq!(config.keys.action_for('!'), None);
}

#[test]
fn keys_rejects_duplicate_inside_array_and_uses_default() {
    let config = Config::from_str(
        r#"
[keys]
extract_archive = []
open = ["e", "e"]
"#,
    )
    .expect("config should parse");
    assert_eq!(config.keys.open, 'o');
    assert_eq!(config.keys.action_for('e'), None);
}

#[test]
fn keys_rejects_invalid_array_member_and_uses_default() {
    let config = Config::from_str(
        r#"
[keys]
extract_archive = []
open = ["e", "space"]
"#,
    )
    .expect("config should parse");
    assert_eq!(config.keys.open, 'o');
    assert_eq!(config.keys.action_for('e'), None);
}

#[test]
fn keys_rejects_array_collision_with_other_binding() {
    let config = Config::from_str(
        r#"
[keys]
open = ["o", "p"]
"#,
    )
    .expect("config should parse");
    assert_eq!(config.keys.open, 'o');
    assert_eq!(config.keys.paste, 'p');
    assert_eq!(config.keys.action_for('p'), Some(Action::Paste));
}

#[test]
fn key_display_joins_multiple_bindings() {
    let config = Config::from_str(
        r#"
[keys]
extract_archive = []
open = ["o", "e"]
"#,
    )
    .expect("config should parse");
    assert_eq!(config.keys.open.to_string(), "o/e");
}

#[test]
fn keys_accept_modifier_bindings() {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    let config = Config::from_str(
        r#"
[keys]
open = "ctrl+o"
open_with = "alt+o"
open_or_enter = ["enter", "ctrl+enter"]
nav_right = "shift+right"
nav_left = "ctrl+alt+up"
"#,
    )
    .expect("config should parse");

    assert_eq!(config.keys.open.to_string(), "Ctrl+O");
    assert_eq!(config.keys.open_with.to_string(), "Alt+O");
    assert_eq!(config.keys.open_or_enter.to_string(), "Enter/Ctrl+Enter");
    assert_eq!(config.keys.nav_right.to_string(), "Shift+→");
    assert_eq!(config.keys.nav_left.to_string(), "Ctrl+Alt+↑");
    assert_eq!(
        config
            .keys
            .action_for_key(KeyEvent::new(KeyCode::Char('o'), KeyModifiers::CONTROL)),
        Some(Action::Open)
    );
    assert_eq!(
        config
            .keys
            .action_for_key(KeyEvent::new(KeyCode::Char('o'), KeyModifiers::ALT)),
        Some(Action::OpenWith)
    );
    assert_eq!(
        config
            .keys
            .action_for_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::CONTROL)),
        Some(Action::OpenOrEnter)
    );
    assert_eq!(
        config
            .keys
            .action_for_key(KeyEvent::new(KeyCode::Right, KeyModifiers::SHIFT)),
        Some(Action::NavRight)
    );
    assert_eq!(
        config.keys.action_for_key(KeyEvent::new(
            KeyCode::Up,
            KeyModifiers::CONTROL | KeyModifiers::ALT
        )),
        Some(Action::NavLeft)
    );
}

#[test]
fn caps_lock_character_events_match_uppercase_bindings() {
    use crossterm::event::KeyModifiers;

    let key_bindings = KeyBindings::default();

    assert_eq!(
        key_bindings.action_for_key(caps_lock_char('o', KeyModifiers::NONE)),
        Some(Action::OpenWith)
    );
}

#[test]
fn caps_lock_with_shift_character_events_match_lowercase_bindings() {
    use crossterm::event::KeyModifiers;

    let key_bindings = KeyBindings::default();

    assert_eq!(
        key_bindings.action_for_key(caps_lock_char('o', KeyModifiers::SHIFT)),
        Some(Action::Open)
    );
    assert_eq!(
        key_bindings.action_for_key(caps_lock_char('O', KeyModifiers::SHIFT)),
        Some(Action::Open)
    );
}

#[test]
fn caps_lock_character_events_use_unicode_case_mapping() {
    use crossterm::event::KeyModifiers;

    let config = Config::from_str(
        r#"
[keys]
open = "ñ"
open_with = "Ñ"
"#,
    )
    .expect("config should parse");

    assert_eq!(
        config
            .keys
            .action_for_key(caps_lock_char('ñ', KeyModifiers::NONE)),
        Some(Action::OpenWith)
    );
    assert_eq!(
        config
            .keys
            .action_for_key(caps_lock_char('ñ', KeyModifiers::SHIFT)),
        Some(Action::Open)
    );
    assert_eq!(
        config
            .keys
            .action_for_key(caps_lock_char('Ñ', KeyModifiers::SHIFT)),
        Some(Action::Open)
    );
}

#[test]
fn caps_lock_does_not_change_ctrl_alt_character_matching() {
    use crossterm::event::KeyModifiers;

    let config = Config::from_str(
        r#"
[keys]
open = "ctrl+o"
open_with = "alt+o"
"#,
    )
    .expect("config should parse");

    assert_eq!(
        config
            .keys
            .action_for_key(caps_lock_char('o', KeyModifiers::CONTROL)),
        Some(Action::Open)
    );
    assert_eq!(
        config
            .keys
            .action_for_key(caps_lock_char('o', KeyModifiers::ALT)),
        Some(Action::OpenWith)
    );
}

fn caps_lock_char(
    c: char,
    modifiers: crossterm::event::KeyModifiers,
) -> crossterm::event::KeyEvent {
    use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState};

    KeyEvent::new_with_kind_and_state(
        KeyCode::Char(c),
        modifiers,
        KeyEventKind::Press,
        KeyEventState::CAPS_LOCK,
    )
}

#[test]
fn delete_named_key_supports_plain_and_shift_bindings() {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    let config = Config::from_str(
        r#"
[keys]
trash = ["d", "del"]
delete_permanently = ["D", "shift+delete"]
"#,
    )
    .expect("config should parse");

    assert_eq!(config.keys.trash.to_string(), "d/Del");
    assert_eq!(config.keys.delete_permanently.to_string(), "D/Shift+Del");
    assert_eq!(
        config
            .keys
            .action_for_key(KeyEvent::new(KeyCode::Delete, KeyModifiers::NONE)),
        Some(Action::Trash)
    );
    assert_eq!(
        config
            .keys
            .action_for_key(KeyEvent::new(KeyCode::Delete, KeyModifiers::SHIFT)),
        Some(Action::DeletePermanently)
    );
}

#[test]
fn keys_match_modifiers_exactly() {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    let config = Config::from_str(
        r#"
[keys]
nav_right = "right"
open = "shift+right"
"#,
    )
    .expect("config should parse");

    assert_eq!(
        config
            .keys
            .action_for_key(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE)),
        Some(Action::NavRight)
    );
    assert_eq!(
        config
            .keys
            .action_for_key(KeyEvent::new(KeyCode::Right, KeyModifiers::SHIFT)),
        Some(Action::Open)
    );
    assert_eq!(
        config
            .keys
            .action_for_key(KeyEvent::new(KeyCode::Char('o'), KeyModifiers::CONTROL)),
        None
    );
    assert_eq!(
        config
            .keys
            .action_for_key(KeyEvent::new(KeyCode::Right, KeyModifiers::SUPER)),
        None
    );
}

#[test]
fn modified_and_plain_bindings_do_not_collide() {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    let config = Config::from_str(
        r#"
[keys]
extract_archive = []
nav_right = []
open = "e"
search_folders = "ctrl+e"
open_or_enter = "right"
nav_left = "shift+right"
"#,
    )
    .expect("config should parse");

    assert_eq!(config.keys.action_for('e'), Some(Action::Open));
    assert_eq!(
        config
            .keys
            .action_for_key(KeyEvent::new(KeyCode::Char('e'), KeyModifiers::CONTROL)),
        Some(Action::SearchFolders)
    );
    assert_eq!(
        config
            .keys
            .action_for_key(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE)),
        Some(Action::OpenOrEnter)
    );
    assert_eq!(
        config
            .keys
            .action_for_key(KeyEvent::new(KeyCode::Right, KeyModifiers::SHIFT)),
        Some(Action::NavLeft)
    );
}

#[test]
fn modified_bindings_detect_collisions() {
    let config = Config::from_str(
        r#"
[keys]
open = "ctrl+e"
search_folders = "ctrl+e"
"#,
    )
    .expect("config should parse");

    assert_eq!(config.keys.open.to_string(), "Ctrl+E");
    assert_eq!(config.keys.search_folders.to_string(), "f");
}

#[test]
fn shifted_char_events_match_uppercase_char_bindings() {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    let key_bindings = KeyBindings::default();
    assert_eq!(
        key_bindings.action_for_key(KeyEvent::new(KeyCode::Char('O'), KeyModifiers::SHIFT)),
        Some(Action::OpenWith)
    );
}

#[test]
fn keys_reject_invalid_modifier_bindings_and_use_default() {
    let cases = [
        "open = \"ctrl+\"",
        "open = \"ctrl++f\"",
        "open = \"ctrl+ctrl+f\"",
        "open = \"cmd+f\"",
        "open = \"ctrl+spacebar\"",
        "open = \"shift+space\"",
        "open = \"ctrl+f\"",
        "open = \"ctrl+c\"",
        "open = \"ctrl+a\"",
        "open = \"ctrl+=\"",
        "open = \"ctrl+-\"",
        "open = \"alt+right\"",
        "open = \"alt+left\"",
    ];

    for override_toml in cases {
        let config =
            Config::from_str(&format!("[keys]\n{override_toml}")).expect("config should parse");
        assert_eq!(config.keys.open.to_string(), "o");
    }

    let config = Config::from_str("[keys]\nopen_with = \"shift+o\"").expect("config should parse");
    assert_eq!(config.keys.open_with.to_string(), "O");
}

#[test]
fn modified_bindings_accept_case_and_order_variants_safely() {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    let config = Config::from_str(
        r#"
[keys]
open = "CTRL+O"
open_with = "alt+ctrl+e"
"#,
    )
    .expect("config should parse");

    // Modifier names are case-insensitive, while the final character key keeps its case.
    assert_eq!(config.keys.open.to_string(), "Ctrl+O");
    assert_eq!(config.keys.open_with.to_string(), "Ctrl+Alt+E");
    assert_eq!(
        config.keys.action_for_key(KeyEvent::new(
            KeyCode::Char('e'),
            KeyModifiers::CONTROL | KeyModifiers::ALT
        )),
        Some(Action::OpenWith)
    );
    assert_eq!(
        config.keys.action_for_key(KeyEvent::new(
            KeyCode::Char('e'),
            KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SUPER
        )),
        None,
        "extra terminal modifiers must not accidentally match"
    );
}

#[test]
fn ctrl_alt_character_bindings_match_shifted_terminal_events_without_leaking_plain() {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    let config = Config::from_str(
        r#"
[keys]
open = "ctrl+o"
open_with = "alt+o"
"#,
    )
    .expect("config should parse");

    assert_eq!(
        config.keys.action_for_key(KeyEvent::new(
            KeyCode::Char('O'),
            KeyModifiers::CONTROL | KeyModifiers::SHIFT
        )),
        Some(Action::Open),
        "terminals may report Ctrl+Shift+letter as an uppercase char plus Shift"
    );
    assert_eq!(
        config.keys.action_for_key(KeyEvent::new(
            KeyCode::Char('O'),
            KeyModifiers::ALT | KeyModifiers::SHIFT
        )),
        Some(Action::OpenWith)
    );
    assert_eq!(
        config
            .keys
            .action_for_key(KeyEvent::new(KeyCode::Char('o'), KeyModifiers::NONE)),
        None,
        "plain o must stay unbound after open moves to Ctrl+O"
    );
}

#[test]
fn parser_edge_cases_fall_back_without_panicking() {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    let suspicious_values = [
        "",
        "+",
        "++",
        "ctrl+",
        "+ctrl+o",
        "ctrl++o",
        "ctrl+alt+",
        "ctrl+alt+shift",
        "ctrl+shift+o",
        "shift++",
        "super+o",
        "cmd+o",
        "ctrl+spacebar",
        "shift+space",
        "right+ctrl",
        "ctrl+alt+right+extra",
        "enter+ctrl",
        "\t",
        "\n",
        "🔥🔥",
    ];

    for value in suspicious_values {
        let escaped = value
            .replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('\t', "\\t")
            .replace('\n', "\\n");
        let config = Config::from_str(&format!("[keys]\nopen = \"{escaped}\""))
            .expect("config should parse or safely fall back");
        assert!(
            config.keys.open.to_string() == "o"
                || config.keys.action_for_key(KeyEvent::new(
                    KeyCode::Right,
                    KeyModifiers::CONTROL | KeyModifiers::ALT,
                )) == Some(Action::Open),
            "unexpected parse result for {value:?}: {}",
            config.keys.open
        );
    }
}

#[test]
fn literal_space_collides_with_space_named_key() {
    let config = Config::from_str(
        r#"
[keys]
open = " "
"#,
    )
    .expect("config should parse");
    assert_eq!(config.keys.action_for(' '), Some(Action::ToggleSelection));
    assert_eq!(config.keys.action_for('o'), Some(Action::Open));
}

#[test]
fn keys_rejects_multi_char_string_and_uses_default() {
    let config = Config::from_str(
        r#"
[keys]
yank = "yy"
"#,
    )
    .expect("config should parse");
    assert_eq!(config.keys.yank, 'y');
}

#[test]
fn keys_rejects_empty_string_and_uses_default() {
    let config = Config::from_str(
        r#"
[keys]
yank = ""
"#,
    )
    .expect("config should parse");
    assert_eq!(config.keys.yank, 'y');
}

#[test]
fn keys_rejects_reserved_char_and_uses_default() {
    let config = Config::from_str(
        r#"
[keys]
yank = "?"
"#,
    )
    .expect("config should parse");
    assert_eq!(config.keys.yank, 'y');
}

#[test]
fn keys_rejects_control_characters_and_uses_default() {
    let config = Config::from_str("[keys]\nquit = \"\\t\"").expect("config should parse");
    assert_eq!(config.keys.quit, 'q');

    let config = Config::from_str("[keys]\nquit = \"\\n\"").expect("config should parse");
    assert_eq!(config.keys.quit, 'q');
}

#[test]
fn keys_rejects_user_user_duplicate_and_uses_defaults() {
    let config = Config::from_str(
        r#"
[keys]
yank = "p"
paste = "p"
"#,
    )
    .expect("config should parse");
    assert_eq!(config.keys.yank, 'y');
    assert_eq!(config.keys.paste, 'p');
}

#[test]
fn keys_rejects_user_default_collision_and_uses_default() {
    let config = Config::from_str(
        r#"
[keys]
yank = "d"
"#,
    )
    .expect("config should parse");
    assert_eq!(config.keys.yank, 'y');
    assert_eq!(config.keys.trash.to_string(), "d/Del");
}

#[test]
fn keys_allows_swapping_two_defaults() {
    let config = Config::from_str(
        r#"
[keys]
yank = "x"
cut = "y"
"#,
    )
    .expect("config should parse");
    assert_eq!(config.keys.yank, 'x');
    assert_eq!(config.keys.cut, 'y');
}

#[test]
fn function_keys_can_be_bound_and_displayed() {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    let config = Config::from_str(
        r#"
[keys]
open = "F5"
open_with = ["O", "f12"]
"#,
    )
    .expect("config should parse");

    assert_eq!(config.keys.open.to_string(), "F5");
    assert_eq!(config.keys.open_with.to_string(), "O/F12");
    assert_eq!(
        config
            .keys
            .action_for_key(KeyEvent::new(KeyCode::F(5), KeyModifiers::NONE)),
        Some(Action::Open)
    );
    assert_eq!(
        config
            .keys
            .action_for_key(KeyEvent::new(KeyCode::F(12), KeyModifiers::NONE)),
        Some(Action::OpenWith)
    );
}

#[test]
fn named_keys_and_modifiers_are_case_insensitive() {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    let config = Config::from_str(
        r#"
[keys]
page_up = "PageUp"
cycle_places_previous = "Shift+Tab"
open = "Alt+Up"
open_with = "O"
open_or_enter = "SHIft+Enter"
rename = ["r", "F2"]
"#,
    )
    .expect("config should parse");

    assert_eq!(config.keys.page_up.to_string(), "PageUp");
    assert_eq!(config.keys.cycle_places_previous.to_string(), "Shift+Tab");
    assert_eq!(config.keys.open.to_string(), "Alt+↑");
    assert_eq!(config.keys.open_with.to_string(), "O");
    assert_eq!(config.keys.open_or_enter.to_string(), "Shift+Enter");
    assert_eq!(config.keys.rename.to_string(), "r/F2");
    assert_eq!(
        config
            .keys
            .action_for_key(KeyEvent::new(KeyCode::PageUp, KeyModifiers::NONE)),
        Some(Action::PageUp)
    );
    assert_eq!(
        config
            .keys
            .action_for_key(KeyEvent::new(KeyCode::BackTab, KeyModifiers::NONE)),
        Some(Action::CyclePlacesPrevious)
    );
    assert_eq!(
        config
            .keys
            .action_for_key(KeyEvent::new(KeyCode::Up, KeyModifiers::ALT)),
        Some(Action::Open)
    );
    assert_eq!(
        config
            .keys
            .action_for_key(KeyEvent::new(KeyCode::Char('O'), KeyModifiers::SHIFT)),
        Some(Action::OpenWith)
    );
    assert_eq!(
        config
            .keys
            .action_for_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::SHIFT)),
        Some(Action::OpenOrEnter)
    );
    assert_eq!(
        config
            .keys
            .action_for_key(KeyEvent::new(KeyCode::F(2), KeyModifiers::NONE)),
        Some(Action::Rename)
    );
}

#[test]
fn shift_character_bindings_keep_using_uppercase_character_form() {
    let cases = ["Shift+O", "Shift+o", "SHIft+O", "SHIft+o"];

    for value in cases {
        let config =
            Config::from_str(&format!("[keys]\nopen = \"{value}\"")).expect("config should parse");

        assert_eq!(
            config.keys.open.to_string(),
            "o",
            "{value:?} should fall back; shifted characters are written as uppercase chars"
        );
    }

    let config = Config::from_str("[keys]\nopen_with = \"O\"").expect("config should parse");
    assert_eq!(config.keys.open_with.to_string(), "O");
}

#[test]
fn shift_backtab_bindings_fall_back_because_backtab_already_means_shift_tab() {
    let cases = ["Shift+BackTab", "shift+backtab", "SHIft+BackTab"];

    for value in cases {
        let config = Config::from_str(&format!("[keys]\ncycle_places_previous = \"{value}\""))
            .expect("config should parse");

        assert_eq!(
            config.keys.cycle_places_previous.to_string(),
            "Shift+Tab",
            "{value:?} should fall back; use shift+tab or backtab instead"
        );
    }
}

#[test]
fn default_rename_and_restore_share_r_in_disjoint_contexts() {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    let key_bindings = KeyBindings::default();
    let r = KeyEvent::new(KeyCode::Char('r'), KeyModifiers::NONE);

    assert_eq!(key_bindings.rename.to_string(), "r/F2");
    assert_eq!(key_bindings.restore_from_trash.to_string(), "r");
    assert_eq!(
        key_bindings.action_for_key_in_context(r, KeyContext::Normal),
        Some(Action::Rename)
    );
    assert_eq!(
        key_bindings.action_for_key_in_context(r, KeyContext::Trash),
        Some(Action::RestoreFromTrash)
    );
    assert_eq!(
        key_bindings.action_for_key_in_context(
            KeyEvent::new(KeyCode::F(2), KeyModifiers::NONE),
            KeyContext::Normal,
        ),
        Some(Action::Rename)
    );
    assert_eq!(
        key_bindings.action_for_key_in_context(
            KeyEvent::new(KeyCode::F(2), KeyModifiers::NONE),
            KeyContext::Trash,
        ),
        None
    );
}

#[test]
fn normal_actions_cannot_reuse_contextual_r_default() {
    let config = Config::from_str(
        r#"
[keys]
open = "r"
"#,
    )
    .expect("config should parse");

    assert_eq!(config.keys.open.to_string(), "o");
    assert_eq!(config.keys.rename.to_string(), "r/F2");
    assert_eq!(config.keys.restore_from_trash.to_string(), "r");
}

#[test]
fn contextual_rename_and_restore_bindings_can_overlap() {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    let config = Config::from_str(
        r#"
[keys]
extract_archive = []
rename = "e"
restore_from_trash = "e"
"#,
    )
    .expect("config should parse");
    let e = KeyEvent::new(KeyCode::Char('e'), KeyModifiers::NONE);

    assert_eq!(
        config.keys.action_for_key_in_context(e, KeyContext::Normal),
        Some(Action::Rename)
    );
    assert_eq!(
        config.keys.action_for_key_in_context(e, KeyContext::Trash),
        Some(Action::RestoreFromTrash)
    );
}

#[test]
fn disabled_rename_removes_f2_without_disabling_restore() {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    let config = Config::from_str(
        r#"
[keys]
rename = []
"#,
    )
    .expect("config should parse");

    assert_eq!(
        config.keys.action_for_key_in_context(
            KeyEvent::new(KeyCode::F(2), KeyModifiers::NONE),
            KeyContext::Normal,
        ),
        None
    );
    assert_eq!(
        config.keys.action_for_key_in_context(
            KeyEvent::new(KeyCode::Char('r'), KeyModifiers::NONE),
            KeyContext::Trash,
        ),
        Some(Action::RestoreFromTrash)
    );
}

#[test]
fn disabled_restore_from_trash_keeps_normal_rename_bindings() {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    let config = Config::from_str(
        r#"
[keys]
restore_from_trash = []
"#,
    )
    .expect("config should parse");

    assert_eq!(
        config.keys.action_for_key_in_context(
            KeyEvent::new(KeyCode::Char('r'), KeyModifiers::NONE),
            KeyContext::Trash,
        ),
        None
    );
    assert_eq!(
        config.keys.action_for_key_in_context(
            KeyEvent::new(KeyCode::Char('r'), KeyModifiers::NONE),
            KeyContext::Normal,
        ),
        Some(Action::Rename)
    );
    assert_eq!(
        config.keys.action_for_key_in_context(
            KeyEvent::new(KeyCode::F(2), KeyModifiers::NONE),
            KeyContext::Normal,
        ),
        Some(Action::Rename)
    );
}

#[test]
fn restore_from_trash_cannot_reuse_global_open_binding() {
    let config = Config::from_str(
        r#"
[keys]
restore_from_trash = "o"
"#,
    )
    .expect("config should parse");

    assert_eq!(config.keys.open.to_string(), "o");
    assert_eq!(config.keys.restore_from_trash.to_string(), "r");
}

#[test]
fn action_for_returns_correct_action_for_default_bindings() {
    let key_bindings = KeyBindings::default();
    assert_eq!(key_bindings.action_for('y'), Some(Action::Yank));
    assert_eq!(key_bindings.action_for('x'), Some(Action::Cut));
    assert_eq!(key_bindings.action_for('p'), Some(Action::Paste));
    assert_eq!(key_bindings.action_for('e'), Some(Action::ExtractArchive));
    assert_eq!(
        key_bindings.action_for('D'),
        Some(Action::DeletePermanently)
    );
    assert_eq!(key_bindings.action_for('q'), Some(Action::Quit));
    assert_eq!(key_bindings.action_for('Q'), Some(Action::QuitWithoutCd));
    assert_eq!(key_bindings.action_for('o'), Some(Action::Open));
    assert_eq!(key_bindings.action_for('O'), Some(Action::OpenWith));
    assert_eq!(
        key_bindings.action_for_key(crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Enter,
            crossterm::event::KeyModifiers::NONE,
        )),
        Some(Action::OpenOrEnter)
    );
    assert_eq!(key_bindings.action_for('z'), Some(Action::Zoxide));
    assert_eq!(key_bindings.action_for('!'), Some(Action::Shell));
    assert_eq!(key_bindings.action_for('h'), Some(Action::NavLeft));
    assert_eq!(key_bindings.action_for('j'), Some(Action::NavDown));
    assert_eq!(key_bindings.action_for('k'), Some(Action::NavUp));
    assert_eq!(key_bindings.action_for('l'), Some(Action::NavRight));
    assert_eq!(
        key_bindings.action_for_key(crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char('f'),
            crossterm::event::KeyModifiers::CONTROL,
        )),
        Some(Action::SearchFiles)
    );
    assert_eq!(
        key_bindings.action_for_key(crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char('a'),
            crossterm::event::KeyModifiers::CONTROL,
        )),
        Some(Action::SelectAll)
    );
    assert_eq!(
        key_bindings.action_for_key(crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Left,
            crossterm::event::KeyModifiers::ALT,
        )),
        Some(Action::HistoryBack)
    );
    assert_eq!(
        key_bindings.action_for_key(crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Right,
            crossterm::event::KeyModifiers::ALT,
        )),
        Some(Action::HistoryForward)
    );
}

#[test]
fn nav_defaults_include_vim_keys_and_arrow_keys() {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    let key_bindings = KeyBindings::default();
    assert_eq!(key_bindings.nav_left.to_string(), "h/←");
    assert_eq!(key_bindings.nav_down.to_string(), "j/↓");
    assert_eq!(key_bindings.nav_up.to_string(), "k/↑");
    assert_eq!(key_bindings.nav_right.to_string(), "l/→");
    assert_eq!(
        key_bindings.action_for_key(KeyEvent::new(KeyCode::Left, KeyModifiers::NONE)),
        Some(Action::NavLeft)
    );
    assert_eq!(
        key_bindings.action_for_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE)),
        Some(Action::NavDown)
    );
    assert_eq!(
        key_bindings.action_for_key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE)),
        Some(Action::NavUp)
    );
    assert_eq!(
        key_bindings.action_for_key(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE)),
        Some(Action::NavRight)
    );
}

#[test]
fn nav_keys_can_be_overridden_with_chars_and_arrows() {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    let config = Config::from_str(
        r#"
[keys]
nav_down = ["n", "down"]
nav_up = "u"
nav_right = "b"
"#,
    )
    .expect("config should parse");
    assert_eq!(config.keys.action_for('n'), Some(Action::NavDown));
    assert_eq!(
        config
            .keys
            .action_for_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE)),
        Some(Action::NavDown)
    );
    assert_eq!(config.keys.action_for('u'), Some(Action::NavUp));
    assert_eq!(config.keys.action_for('b'), Some(Action::NavRight));
    assert_eq!(config.keys.action_for('j'), None);
    assert_eq!(config.keys.action_for('k'), None);
    assert_eq!(config.keys.action_for('l'), None);
}

#[test]
fn overriding_nav_right_frees_l_for_another_action() {
    let config = Config::from_str(
        r#"
[keys]
nav_right = "right"
open = ["o", "l"]
"#,
    )
    .expect("config should parse");
    assert_eq!(config.keys.action_for('l'), Some(Action::Open));
    assert_eq!(config.keys.action_for('o'), Some(Action::Open));
}

#[test]
fn keys_rejects_collision_with_default_nav_key() {
    let config = Config::from_str(
        r#"
[keys]
open = "l"
"#,
    )
    .expect("config should parse");
    assert_eq!(config.keys.action_for('l'), Some(Action::NavRight));
    assert_eq!(config.keys.action_for('o'), Some(Action::Open));
}

#[test]
fn keys_rejects_collision_with_default_arrow_key() {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    let config = Config::from_str(
        r#"
[keys]
open = "right"
"#,
    )
    .expect("config should parse");
    assert_eq!(
        config
            .keys
            .action_for_key(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE)),
        Some(Action::NavRight)
    );
    assert_eq!(config.keys.action_for('o'), Some(Action::Open));
}

#[test]
fn action_for_reflects_overridden_binding() {
    let config = Config::from_str(
        r#"
[keys]
yank = "Y"
"#,
    )
    .expect("config should parse");
    assert_eq!(config.keys.action_for('Y'), Some(Action::Yank));
    assert_eq!(config.keys.action_for('y'), None);
}

#[test]
fn delete_permanently_can_be_overridden() {
    let config = Config::from_str(
        r#"
[keys]
delete_permanently = "X"
"#,
    )
    .expect("config should parse");
    assert_eq!(config.keys.action_for('X'), Some(Action::DeletePermanently));
    assert_eq!(config.keys.action_for('D'), None);
}

#[test]
fn quit_without_cd_can_be_overridden() {
    let config = Config::from_str(
        r#"
[keys]
quit_without_cd = "u"
"#,
    )
    .expect("config should parse");
    assert_eq!(config.keys.action_for('u'), Some(Action::QuitWithoutCd));
    assert_eq!(config.keys.action_for('Q'), None);
}

#[test]
fn open_with_defaults_to_capital_o() {
    let key_bindings = KeyBindings::default();
    assert_eq!(key_bindings.open_with, 'O');
    assert_eq!(key_bindings.action_for('O'), Some(Action::OpenWith));
}

#[test]
fn open_with_can_be_overridden() {
    let config = Config::from_str(
        r#"
[keys]
open_with = "w"
"#,
    )
    .expect("config should parse");
    assert_eq!(config.keys.action_for('w'), Some(Action::OpenWith));
    assert_eq!(config.keys.action_for('O'), None);
}

#[test]
fn open_with_reserved_shortcuts_follow_custom_menu_controls() {
    let config = Config::from_str(
        r#"
[keys]
nav_down = "n"
nav_up = "m"
nav_right = []
open_or_enter = ["enter", "l", "right"]
"#,
    )
    .expect("config should parse");

    assert_eq!(
        config.keys.open_with_reserved_shortcuts(),
        vec!['n', 'm', 'l']
    );
}

#[test]
fn open_or_enter_defaults_to_enter() {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    let key_bindings = KeyBindings::default();
    assert_eq!(key_bindings.open_or_enter.to_string(), "Enter");
    assert_eq!(
        key_bindings.action_for_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
        Some(Action::OpenOrEnter)
    );
    assert_eq!(
        key_bindings.action_for_key(KeyEvent::new(KeyCode::Char('\n'), KeyModifiers::NONE)),
        Some(Action::OpenOrEnter)
    );
}

#[test]
fn choose_defaults_to_enter_but_normal_enter_stays_open_or_enter() {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    let key_bindings = KeyBindings::default();
    assert_eq!(key_bindings.choose.to_string(), "Enter");
    assert_eq!(
        key_bindings.action_for_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
        Some(Action::OpenOrEnter)
    );
    assert_eq!(
        key_bindings.chooser_action_for_key(
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
            KeyContext::Normal,
        ),
        Some(ChooserKeyAction::Choose)
    );
}

#[test]
fn chooser_lookup_prioritizes_choose_over_smart_open_or_enter() {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    let config = Config::from_str(
        r#"
[keys]
nav_right = []
open_or_enter = ["enter", "l", "right"]
"#,
    )
    .expect("config should parse");

    assert_eq!(
        config
            .keys
            .action_for_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
        Some(Action::OpenOrEnter)
    );
    assert_eq!(
        config.keys.chooser_action_for_key(
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
            KeyContext::Normal,
        ),
        Some(ChooserKeyAction::Choose)
    );
    assert_eq!(
        config.keys.chooser_action_for_key(
            KeyEvent::new(KeyCode::Char('l'), KeyModifiers::NONE),
            KeyContext::Normal,
        ),
        Some(ChooserKeyAction::Normal(Action::OpenOrEnter))
    );
    assert_eq!(
        config.keys.chooser_action_for_key(
            KeyEvent::new(KeyCode::Right, KeyModifiers::NONE),
            KeyContext::Normal,
        ),
        Some(ChooserKeyAction::Normal(Action::OpenOrEnter))
    );
}

#[test]
fn choose_can_be_unbound_or_rebound_without_changing_normal_enter() {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    let unbound = Config::from_str(
        r#"
[keys]
choose = []
"#,
    )
    .expect("config should parse");
    assert_eq!(
        unbound.keys.chooser_action_for_key(
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
            KeyContext::Normal,
        ),
        Some(ChooserKeyAction::Normal(Action::OpenOrEnter))
    );

    let rebound = Config::from_str(
        r#"
[keys]
choose = "ctrl+enter"
"#,
    )
    .expect("config should parse");
    assert_eq!(
        rebound.keys.chooser_action_for_key(
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
            KeyContext::Normal,
        ),
        Some(ChooserKeyAction::Normal(Action::OpenOrEnter))
    );
    assert_eq!(
        rebound.keys.chooser_action_for_key(
            KeyEvent::new(KeyCode::Enter, KeyModifiers::CONTROL),
            KeyContext::Normal,
        ),
        Some(ChooserKeyAction::Choose)
    );
}

#[test]
fn open_or_enter_can_add_l_after_nav_right_frees_it() {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    let config = Config::from_str(
        r#"
[keys]
nav_right = "right"
open_or_enter = ["enter", "l"]
"#,
    )
    .expect("config should parse");
    assert_eq!(config.keys.action_for('l'), Some(Action::OpenOrEnter));
    assert_eq!(
        config
            .keys
            .action_for_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
        Some(Action::OpenOrEnter)
    );
    assert_eq!(
        config
            .keys
            .action_for_key(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE)),
        Some(Action::NavRight)
    );
}

#[test]
fn open_or_enter_rejects_l_while_nav_right_owns_it() {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    let config = Config::from_str(
        r#"
[keys]
open_or_enter = ["enter", "l"]
"#,
    )
    .expect("config should parse");
    assert_eq!(config.keys.action_for('l'), Some(Action::NavRight));
    assert_eq!(config.keys.open_or_enter.to_string(), "Enter");
    assert_eq!(
        config
            .keys
            .action_for_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
        Some(Action::OpenOrEnter)
    );
}

#[test]
fn enter_can_move_to_another_action_when_open_or_enter_frees_it() {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    let config = Config::from_str(
        r#"
[keys]
open_or_enter = "b"
open = "enter"
"#,
    )
    .expect("config should parse");
    assert_eq!(config.keys.action_for('b'), Some(Action::OpenOrEnter));
    assert_eq!(
        config
            .keys
            .action_for_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
        Some(Action::Open)
    );
}

#[test]
fn zoxide_can_be_overridden() {
    let config = Config::from_str(
        r#"
[keys]
zoxide = "Z"
"#,
    )
    .expect("config should parse");
    assert_eq!(config.keys.action_for('Z'), Some(Action::Zoxide));
    assert_eq!(config.keys.action_for('z'), None);
}

#[test]
fn shell_defaults_to_bang() {
    let key_bindings = KeyBindings::default();
    assert_eq!(key_bindings.shell, '!');
    assert_eq!(key_bindings.action_for('!'), Some(Action::Shell));
}

#[test]
fn shell_can_be_overridden() {
    let config = Config::from_str(
        r#"
[keys]
shell = "S"
"#,
    )
    .expect("config should parse");
    assert_eq!(config.keys.action_for('S'), Some(Action::Shell));
    assert_eq!(config.keys.action_for('!'), None);
}

#[test]
fn preview_scroll_defaults_map_to_shift_h_j_k_l_and_brackets() {
    let key_bindings = KeyBindings::default();
    assert_eq!(key_bindings.scroll_preview_up.to_string(), "K/[");
    assert_eq!(key_bindings.scroll_preview_down.to_string(), "J/]");
    assert_eq!(key_bindings.scroll_preview_left, 'H');
    assert_eq!(key_bindings.scroll_preview_right, 'L');
    assert_eq!(key_bindings.action_for('K'), Some(Action::ScrollPreviewUp));
    assert_eq!(key_bindings.action_for('['), Some(Action::ScrollPreviewUp));
    assert_eq!(
        key_bindings.action_for('J'),
        Some(Action::ScrollPreviewDown)
    );
    assert_eq!(
        key_bindings.action_for(']'),
        Some(Action::ScrollPreviewDown)
    );
    assert_eq!(
        key_bindings.action_for('H'),
        Some(Action::ScrollPreviewLeft)
    );
    assert_eq!(
        key_bindings.action_for('L'),
        Some(Action::ScrollPreviewRight)
    );
}

#[test]
fn scroll_preview_up_can_be_overridden() {
    let config = Config::from_str(
        r#"
[keys]
scroll_preview_up = "U"
"#,
    )
    .expect("config should parse");
    assert_eq!(config.keys.scroll_preview_up, 'U');
    assert_eq!(config.keys.action_for('U'), Some(Action::ScrollPreviewUp));
    assert_eq!(config.keys.action_for('K'), None);
    assert_eq!(
        config.keys.action_for('J'),
        Some(Action::ScrollPreviewDown),
        "untouched bindings should keep their defaults"
    );
}

#[test]
fn scroll_preview_keys_reject_collision_with_other_default() {
    let config = Config::from_str(
        r#"
[keys]
scroll_preview_up = "y"
"#,
    )
    .expect("config should parse");
    assert_eq!(
        config.keys.scroll_preview_up.to_string(),
        "K/[",
        "user override colliding with default 'y' (yank) must fall back to default 'K'"
    );
    assert_eq!(config.keys.yank, 'y');
    assert_eq!(config.keys.action_for('y'), Some(Action::Yank));
    assert_eq!(
        config.keys.action_for('K'),
        Some(Action::ScrollPreviewUp),
        "default 'K' must remain bound to ScrollPreviewUp"
    );
}

#[test]
fn scroll_preview_keys_reject_user_user_duplicate() {
    // When two user-set bindings collide, the first one in iteration order
    // falls back to its default; the second keeps the user-set value (since
    // the collision is gone after the first reset). This matches the
    // existing yank/paste collision behavior.
    let config = Config::from_str(
        r#"
[keys]
scroll_preview_up   = "U"
scroll_preview_down = "U"
"#,
    )
    .expect("config should parse");
    assert_eq!(config.keys.scroll_preview_up.to_string(), "K/[");
    assert_eq!(config.keys.scroll_preview_down, 'U');
    assert_eq!(config.keys.action_for('U'), Some(Action::ScrollPreviewDown));
    assert_eq!(config.keys.action_for('K'), Some(Action::ScrollPreviewUp));
    assert_eq!(
        config.keys.action_for('J'),
        None,
        "default 'J' is no longer bound because scroll_preview_down was overridden to 'U'"
    );
}

#[test]
fn remaining_browser_shortcuts_can_be_overridden_and_freed() {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    let config = Config::from_str(
        r#"
[keys]
search_files = "ctrl+s"
select_all = "A"
history_back = "alt+h"
history_forward = "alt+l"
open = ["o", "ctrl+f", "ctrl+a", "alt+left", "alt+right"]
"#,
    )
    .expect("config should parse");

    assert_eq!(
        config
            .keys
            .action_for_key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL)),
        Some(Action::SearchFiles)
    );
    assert_eq!(config.keys.action_for('A'), Some(Action::SelectAll));
    assert_eq!(
        config
            .keys
            .action_for_key(KeyEvent::new(KeyCode::Char('h'), KeyModifiers::ALT)),
        Some(Action::HistoryBack)
    );
    assert_eq!(
        config
            .keys
            .action_for_key(KeyEvent::new(KeyCode::Char('l'), KeyModifiers::ALT)),
        Some(Action::HistoryForward)
    );
    assert_eq!(
        config
            .keys
            .action_for_key(KeyEvent::new(KeyCode::Char('f'), KeyModifiers::CONTROL)),
        Some(Action::Open)
    );
    assert_eq!(
        config
            .keys
            .action_for_key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::CONTROL)),
        Some(Action::Open)
    );
    assert_eq!(
        config
            .keys
            .action_for_key(KeyEvent::new(KeyCode::Left, KeyModifiers::ALT)),
        Some(Action::Open)
    );
    assert_eq!(
        config
            .keys
            .action_for_key(KeyEvent::new(KeyCode::Right, KeyModifiers::ALT)),
        Some(Action::Open)
    );
}

#[test]
fn browser_control_defaults_are_configurable_actions() {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    let key_bindings = KeyBindings::default();
    assert_eq!(key_bindings.action_for('g'), Some(Action::GoTo));
    assert_eq!(key_bindings.action_for('G'), Some(Action::JumpLast));
    assert_eq!(key_bindings.action_for(' '), Some(Action::ToggleSelection));
    assert_eq!(
        key_bindings.action_for_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE)),
        Some(Action::CyclePlacesNext)
    );
    assert_eq!(
        key_bindings.action_for_key(KeyEvent::new(KeyCode::BackTab, KeyModifiers::NONE)),
        Some(Action::CyclePlacesPrevious)
    );
    assert_eq!(
        key_bindings.action_for_key(KeyEvent::new(KeyCode::BackTab, KeyModifiers::SHIFT)),
        Some(Action::CyclePlacesPrevious)
    );

    let shift_tab = Config::from_str("[keys]\ncycle_places_previous = \"shift+tab\"")
        .expect("config should parse");
    assert_eq!(
        shift_tab
            .keys
            .action_for_key(KeyEvent::new(KeyCode::BackTab, KeyModifiers::NONE)),
        Some(Action::CyclePlacesPrevious)
    );
    assert_eq!(
        shift_tab
            .keys
            .action_for_key(KeyEvent::new(KeyCode::BackTab, KeyModifiers::SHIFT)),
        Some(Action::CyclePlacesPrevious)
    );
    assert_eq!(
        key_bindings.action_for_key(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE)),
        Some(Action::GoParent)
    );
    assert_eq!(
        key_bindings.action_for_key(KeyEvent::new(KeyCode::PageUp, KeyModifiers::NONE)),
        Some(Action::PageUp)
    );
    assert_eq!(
        key_bindings.action_for_key(KeyEvent::new(KeyCode::PageDown, KeyModifiers::NONE)),
        Some(Action::PageDown)
    );
    assert_eq!(
        key_bindings.action_for_key(KeyEvent::new(KeyCode::Home, KeyModifiers::NONE)),
        Some(Action::JumpFirst)
    );
    assert_eq!(
        key_bindings.action_for_key(KeyEvent::new(KeyCode::End, KeyModifiers::NONE)),
        Some(Action::JumpLast)
    );
}

#[test]
fn browser_control_defaults_can_be_freed_for_other_actions() {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    let config = Config::from_str(
        r#"
[keys]
go_to = []
toggle_selection = []
cycle_places_next = []
cycle_places_previous = []
go_parent = []
page_up = []
page_down = []
jump_first = []
jump_last = []
open = ["o", "g", "G", "space", "tab", "backtab", "backspace", "pageup", "pagedown", "home", "end"]
"#,
    )
    .expect("config should parse");

    assert_eq!(config.keys.action_for('g'), Some(Action::Open));
    assert_eq!(config.keys.action_for('G'), Some(Action::Open));
    assert_eq!(config.keys.action_for(' '), Some(Action::Open));
    assert_eq!(
        config
            .keys
            .action_for_key(KeyEvent::new(KeyCode::BackTab, KeyModifiers::NONE)),
        Some(Action::Open)
    );
    assert_eq!(
        config
            .keys
            .action_for_key(KeyEvent::new(KeyCode::BackTab, KeyModifiers::SHIFT)),
        Some(Action::Open)
    );

    let nav_right = Config::from_str(
        r#"
[keys]
cycle_places_previous = []
nav_right = "backtab"
"#,
    )
    .expect("config should parse");
    assert_eq!(
        nav_right
            .keys
            .action_for_key(KeyEvent::new(KeyCode::BackTab, KeyModifiers::NONE)),
        Some(Action::NavRight)
    );
    assert_eq!(
        nav_right
            .keys
            .action_for_key(KeyEvent::new(KeyCode::BackTab, KeyModifiers::SHIFT)),
        Some(Action::NavRight)
    );

    let shift_tab_nav_right = Config::from_str(
        r#"
[keys]
cycle_places_previous = []
nav_right = "shift+tab"
"#,
    )
    .expect("config should parse");
    assert_eq!(
        shift_tab_nav_right
            .keys
            .action_for_key(KeyEvent::new(KeyCode::BackTab, KeyModifiers::NONE)),
        Some(Action::NavRight)
    );
    assert_eq!(
        shift_tab_nav_right
            .keys
            .action_for_key(KeyEvent::new(KeyCode::BackTab, KeyModifiers::SHIFT)),
        Some(Action::NavRight)
    );

    let default_keys = KeyBindings::default();
    assert_eq!(
        default_keys.action_for_key(KeyEvent::new(KeyCode::BackTab, KeyModifiers::NONE)),
        Some(Action::CyclePlacesPrevious)
    );
    assert_eq!(
        default_keys.action_for_key(KeyEvent::new(KeyCode::BackTab, KeyModifiers::SHIFT)),
        Some(Action::CyclePlacesPrevious)
    );

    let literal_space = Config::from_str(
        r#"
[keys]
toggle_selection = []
open = " "
"#,
    )
    .expect("config should parse");
    assert_eq!(literal_space.keys.action_for(' '), Some(Action::Open));
    assert_eq!(
        config
            .keys
            .action_for_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE)),
        Some(Action::Open)
    );
    assert_eq!(
        config
            .keys
            .action_for_key(KeyEvent::new(KeyCode::BackTab, KeyModifiers::NONE)),
        Some(Action::Open)
    );
    assert_eq!(
        config
            .keys
            .action_for_key(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE)),
        Some(Action::Open)
    );
    assert_eq!(
        config
            .keys
            .action_for_key(KeyEvent::new(KeyCode::PageUp, KeyModifiers::NONE)),
        Some(Action::Open)
    );
    assert_eq!(
        config
            .keys
            .action_for_key(KeyEvent::new(KeyCode::PageDown, KeyModifiers::NONE)),
        Some(Action::Open)
    );
    assert_eq!(
        config
            .keys
            .action_for_key(KeyEvent::new(KeyCode::Home, KeyModifiers::NONE)),
        Some(Action::Open)
    );
    assert_eq!(
        config
            .keys
            .action_for_key(KeyEvent::new(KeyCode::End, KeyModifiers::NONE)),
        Some(Action::Open)
    );
}
