use super::model::{Action, KeyBindings, KeyCodeSpec, KeyList, KeySpec};
use super::parse::{KeyConfigOverride, KeysConfigOverride, parse_key_override};
use std::collections::BTreeMap;

/// Characters that are hard-wired to non-configurable actions and may not be
/// used as key binding values.
const RESERVED_CHARS: &[char] = &[
    '?', // help
    '+', '=', // grid zoom
];

/// Modified keys that are still hard-wired before configurable browser actions.
const RESERVED_MODIFIED_CHARS: &[char] = &[
    'c', // cancel/clear
    '+', '=', '-', '_', // grid zoom
];

struct RawBinding {
    name: &'static str,
    action: Action,
    override_value: Option<KeyConfigOverride>,
    default: KeyList,
}

fn warn_unknown_key_actions(unknown: &BTreeMap<String, toml::Value>) {
    for key in unknown.keys() {
        eprintln!("{}", unknown_key_action_warning(key));
    }
}

pub(super) fn unknown_key_action_warning(key: &str) -> String {
    format!("elio: keys.{key}: unknown key action; ignoring")
}

pub(super) fn resolve_key_overrides(
    overrides: KeysConfigOverride,
    defaults: &KeyBindings,
) -> KeyBindings {
    warn_unknown_key_actions(&overrides.unknown);

    let choose = overrides
        .choose
        .as_ref()
        .and_then(|value| parse_key_override("choose", value, &defaults.choose))
        .unwrap_or_else(|| defaults.choose.clone());

    let raw = vec![
        RawBinding {
            name: "quit",
            action: Action::Quit,
            override_value: overrides.quit,
            default: defaults.quit.clone(),
        },
        RawBinding {
            name: "quit_without_cd",
            action: Action::QuitWithoutCd,
            override_value: overrides.quit_without_cd,
            default: defaults.quit_without_cd.clone(),
        },
        RawBinding {
            name: "yank",
            action: Action::Yank,
            override_value: overrides.yank,
            default: defaults.yank.clone(),
        },
        RawBinding {
            name: "cut",
            action: Action::Cut,
            override_value: overrides.cut,
            default: defaults.cut.clone(),
        },
        RawBinding {
            name: "paste",
            action: Action::Paste,
            override_value: overrides.paste,
            default: defaults.paste.clone(),
        },
        RawBinding {
            name: "create_archive",
            action: Action::CreateArchive,
            override_value: overrides.create_archive,
            default: defaults.create_archive.clone(),
        },
        RawBinding {
            name: "extract_archive",
            action: Action::ExtractArchive,
            override_value: overrides.extract_archive,
            default: defaults.extract_archive.clone(),
        },
        RawBinding {
            name: "symlink_absolute",
            action: Action::SymlinkAbsolute,
            override_value: overrides.symlink_absolute,
            default: defaults.symlink_absolute.clone(),
        },
        RawBinding {
            name: "symlink_relative",
            action: Action::SymlinkRelative,
            override_value: overrides.symlink_relative,
            default: defaults.symlink_relative.clone(),
        },
        RawBinding {
            name: "trash",
            action: Action::Trash,
            override_value: overrides.trash,
            default: defaults.trash.clone(),
        },
        RawBinding {
            name: "delete_permanently",
            action: Action::DeletePermanently,
            override_value: overrides.delete_permanently,
            default: defaults.delete_permanently.clone(),
        },
        RawBinding {
            name: "create",
            action: Action::Create,
            override_value: overrides.create,
            default: defaults.create.clone(),
        },
        RawBinding {
            name: "rename",
            action: Action::Rename,
            override_value: overrides.rename,
            default: defaults.rename.clone(),
        },
        RawBinding {
            name: "rename_in_editor",
            action: Action::RenameInEditor,
            override_value: overrides.rename_in_editor,
            default: defaults.rename_in_editor.clone(),
        },
        RawBinding {
            name: "restore_from_trash",
            action: Action::RestoreFromTrash,
            override_value: overrides.restore_from_trash,
            default: defaults.restore_from_trash.clone(),
        },
        RawBinding {
            name: "copy_path",
            action: Action::CopyPath,
            override_value: overrides.copy_path,
            default: defaults.copy_path.clone(),
        },
        RawBinding {
            name: "search_folders",
            action: Action::SearchFolders,
            override_value: overrides.search_folders,
            default: defaults.search_folders.clone(),
        },
        RawBinding {
            name: "search_files",
            action: Action::SearchFiles,
            override_value: overrides.search_files,
            default: defaults.search_files.clone(),
        },
        RawBinding {
            name: "filter_directory",
            action: Action::FilterDirectory,
            override_value: overrides.filter_directory,
            default: defaults.filter_directory.clone(),
        },
        RawBinding {
            name: "zoxide",
            action: Action::Zoxide,
            override_value: overrides.zoxide,
            default: defaults.zoxide.clone(),
        },
        RawBinding {
            name: "shell",
            action: Action::Shell,
            override_value: overrides.shell,
            default: defaults.shell.clone(),
        },
        RawBinding {
            name: "open",
            action: Action::Open,
            override_value: overrides.open,
            default: defaults.open.clone(),
        },
        RawBinding {
            name: "open_with",
            action: Action::OpenWith,
            override_value: overrides.open_with,
            default: defaults.open_with.clone(),
        },
        RawBinding {
            name: "open_or_enter",
            action: Action::OpenOrEnter,
            override_value: overrides.open_or_enter,
            default: defaults.open_or_enter.clone(),
        },
        RawBinding {
            name: "go_to",
            action: Action::GoTo,
            override_value: overrides.go_to,
            default: defaults.go_to.clone(),
        },
        RawBinding {
            name: "toggle_selection",
            action: Action::ToggleSelection,
            override_value: overrides.toggle_selection,
            default: defaults.toggle_selection.clone(),
        },
        RawBinding {
            name: "cycle_places_next",
            action: Action::CyclePlacesNext,
            override_value: overrides.cycle_places_next,
            default: defaults.cycle_places_next.clone(),
        },
        RawBinding {
            name: "cycle_places_previous",
            action: Action::CyclePlacesPrevious,
            override_value: overrides.cycle_places_previous,
            default: defaults.cycle_places_previous.clone(),
        },
        RawBinding {
            name: "go_parent",
            action: Action::GoParent,
            override_value: overrides.go_parent,
            default: defaults.go_parent.clone(),
        },
        RawBinding {
            name: "page_up",
            action: Action::PageUp,
            override_value: overrides.page_up,
            default: defaults.page_up.clone(),
        },
        RawBinding {
            name: "page_down",
            action: Action::PageDown,
            override_value: overrides.page_down,
            default: defaults.page_down.clone(),
        },
        RawBinding {
            name: "jump_first",
            action: Action::JumpFirst,
            override_value: overrides.jump_first,
            default: defaults.jump_first.clone(),
        },
        RawBinding {
            name: "jump_last",
            action: Action::JumpLast,
            override_value: overrides.jump_last,
            default: defaults.jump_last.clone(),
        },
        RawBinding {
            name: "select_all",
            action: Action::SelectAll,
            override_value: overrides.select_all,
            default: defaults.select_all.clone(),
        },
        RawBinding {
            name: "history_back",
            action: Action::HistoryBack,
            override_value: overrides.history_back,
            default: defaults.history_back.clone(),
        },
        RawBinding {
            name: "history_forward",
            action: Action::HistoryForward,
            override_value: overrides.history_forward,
            default: defaults.history_forward.clone(),
        },
        RawBinding {
            name: "sort",
            action: Action::Sort,
            override_value: overrides.sort,
            default: defaults.sort.clone(),
        },
        RawBinding {
            name: "toggle_view",
            action: Action::ToggleView,
            override_value: overrides.toggle_view,
            default: defaults.toggle_view.clone(),
        },
        RawBinding {
            name: "toggle_preview",
            action: Action::TogglePreview,
            override_value: overrides.toggle_preview,
            default: defaults.toggle_preview.clone(),
        },
        RawBinding {
            name: "fullscreen_preview",
            action: Action::FullscreenPreview,
            override_value: overrides.fullscreen_preview,
            default: defaults.fullscreen_preview.clone(),
        },
        RawBinding {
            name: "toggle_hidden",
            action: Action::ToggleHidden,
            override_value: overrides.toggle_hidden,
            default: defaults.toggle_hidden.clone(),
        },
        RawBinding {
            name: "nav_left",
            action: Action::NavLeft,
            override_value: overrides.nav_left,
            default: defaults.nav_left.clone(),
        },
        RawBinding {
            name: "nav_down",
            action: Action::NavDown,
            override_value: overrides.nav_down,
            default: defaults.nav_down.clone(),
        },
        RawBinding {
            name: "nav_up",
            action: Action::NavUp,
            override_value: overrides.nav_up,
            default: defaults.nav_up.clone(),
        },
        RawBinding {
            name: "nav_right",
            action: Action::NavRight,
            override_value: overrides.nav_right,
            default: defaults.nav_right.clone(),
        },
        RawBinding {
            name: "scroll_preview_left",
            action: Action::ScrollPreviewLeft,
            override_value: overrides.scroll_preview_left,
            default: defaults.scroll_preview_left.clone(),
        },
        RawBinding {
            name: "scroll_preview_right",
            action: Action::ScrollPreviewRight,
            override_value: overrides.scroll_preview_right,
            default: defaults.scroll_preview_right.clone(),
        },
        RawBinding {
            name: "scroll_preview_up",
            action: Action::ScrollPreviewUp,
            override_value: overrides.scroll_preview_up,
            default: defaults.scroll_preview_up.clone(),
        },
        RawBinding {
            name: "scroll_preview_down",
            action: Action::ScrollPreviewDown,
            override_value: overrides.scroll_preview_down,
            default: defaults.scroll_preview_down.clone(),
        },
    ];

    // Step 1: parse each override independently, falling back to default on
    // any format or reserved-char error. Empty lists are valid unbinds.
    // (resolved_keys, is_user_set)
    let mut candidates: Vec<(KeyList, bool)> = raw
        .iter()
        .map(|entry| match &entry.override_value {
            None => (entry.default.clone(), false),
            Some(value) => match parse_key_override(entry.name, value, &entry.default) {
                Some(keys) => (keys, true),
                None => (entry.default.clone(), false),
            },
        })
        .collect();

    // Step 2: reject user-set bindings that collide with any other binding
    // (user-set or default). Loop until stable so that reverting one
    // binding does not silently leave a conflict with another.
    loop {
        let mut changed = false;
        for index in 0..raw.len() {
            if !candidates[index].1 {
                continue;
            }
            let collision = candidates[index].0.keys().find_map(|candidate| {
                (0..raw.len())
                    .filter(|&other_index| other_index != index)
                    .find(|&other_index| {
                        raw[index]
                            .action
                            .key_contexts()
                            .intersects(raw[other_index].action.key_contexts())
                            && candidates[other_index].0.contains(candidate)
                    })
                    .map(|other_index| (candidate, other_index))
            });

            if let Some((candidate, other_index)) = collision {
                eprintln!(
                    "elio: keys.{}: '{}' is already bound to {}; using default '{}'",
                    raw[index].name, candidate, raw[other_index].name, raw[index].default
                );
                candidates[index] = (raw[index].default.clone(), false);
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }

    // Step 3: build from the resolved candidates (order matches `raw`).
    let resolved = |index: usize| candidates[index].0.clone();
    KeyBindings {
        choose,
        quit: resolved(0),
        quit_without_cd: resolved(1),
        yank: resolved(2),
        cut: resolved(3),
        paste: resolved(4),
        create_archive: resolved(5),
        extract_archive: resolved(6),
        symlink_absolute: resolved(7),
        symlink_relative: resolved(8),
        trash: resolved(9),
        delete_permanently: resolved(10),
        create: resolved(11),
        rename: resolved(12),
        rename_in_editor: resolved(13),
        restore_from_trash: resolved(14),
        copy_path: resolved(15),
        search_folders: resolved(16),
        search_files: resolved(17),
        filter_directory: resolved(18),
        zoxide: resolved(19),
        shell: resolved(20),
        open: resolved(21),
        open_with: resolved(22),
        open_or_enter: resolved(23),
        go_to: resolved(24),
        toggle_selection: resolved(25),
        cycle_places_next: resolved(26),
        cycle_places_previous: resolved(27),
        go_parent: resolved(28),
        page_up: resolved(29),
        page_down: resolved(30),
        jump_first: resolved(31),
        jump_last: resolved(32),
        select_all: resolved(33),
        history_back: resolved(34),
        history_forward: resolved(35),
        sort: resolved(36),
        toggle_view: resolved(37),
        toggle_preview: resolved(38),
        fullscreen_preview: resolved(39),
        toggle_hidden: resolved(40),
        nav_left: resolved(41),
        nav_down: resolved(42),
        nav_up: resolved(43),
        nav_right: resolved(44),
        scroll_preview_left: resolved(45),
        scroll_preview_right: resolved(46),
        scroll_preview_up: resolved(47),
        scroll_preview_down: resolved(48),
    }
}

pub(super) fn is_reserved_plain_char(c: char) -> bool {
    RESERVED_CHARS.contains(&c)
}

pub(super) fn validate_key_spec(name: &str, spec: KeySpec, default: &KeyList) -> Option<KeySpec> {
    if is_reserved_key_spec(spec) {
        eprintln!(
            "elio: keys.{name}: '{spec}' is reserved and cannot be rebound; using default '{default}'"
        );
        return None;
    }

    Some(spec)
}

fn is_reserved_key_spec(spec: KeySpec) -> bool {
    match spec.code {
        KeyCodeSpec::Char(c) if spec.modifiers.is_empty() => RESERVED_CHARS.contains(&c),
        KeyCodeSpec::Char(c) if spec.modifiers.ctrl => {
            RESERVED_MODIFIED_CHARS.contains(&c.to_ascii_lowercase())
        }
        _ => false,
    }
}
