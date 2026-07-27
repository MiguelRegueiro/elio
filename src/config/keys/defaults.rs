use super::model::{KeyBindings, KeyList, KeySpec, NamedKey};

impl Default for KeyBindings {
    fn default() -> Self {
        Self {
            choose: KeyList(vec![KeySpec::named(NamedKey::Enter)]),
            quit: KeyList::one('q'),
            quit_without_cd: KeyList::one('Q'),
            yank: KeyList::one('y'),
            cut: KeyList::one('x'),
            paste: KeyList::one('p'),
            create_archive: KeyList::one('C'),
            extract_archive: KeyList::one('e'),
            symlink_absolute: KeyList::one('-'),
            symlink_relative: KeyList::one('_'),
            trash: KeyList(vec![KeySpec::char('d'), KeySpec::named(NamedKey::Delete)]),
            delete_permanently: KeyList(vec![
                KeySpec::char('D'),
                KeySpec::shift_named(NamedKey::Delete),
            ]),
            create: KeyList::one('a'),
            rename: KeyList(vec![
                KeySpec::char('r'),
                KeySpec::named(NamedKey::Function(2)),
            ]),
            rename_in_editor: KeyList(vec![
                KeySpec::char('R'),
                KeySpec::shift_named(NamedKey::Function(2)),
            ]),
            restore_from_trash: KeyList::one('r'),
            copy_path: KeyList::one('c'),
            search_folders: KeyList::one('f'),
            search_files: KeyList(vec![KeySpec::ctrl_char('f')]),
            filter_directory: KeyList::one('/'),
            zoxide: KeyList::one('z'),
            shell: KeyList::one('!'),
            open: KeyList::one('o'),
            open_with: KeyList::one('O'),
            open_or_enter: KeyList(vec![KeySpec::named(NamedKey::Enter)]),
            go_to: KeyList::one('g'),
            toggle_selection: KeyList(vec![KeySpec::named(NamedKey::Space)]),
            cycle_places_next: KeyList(vec![KeySpec::named(NamedKey::Tab)]),
            cycle_places_previous: KeyList(vec![KeySpec::named(NamedKey::BackTab)]),
            go_parent: KeyList(vec![KeySpec::named(NamedKey::Backspace)]),
            page_up: KeyList(vec![KeySpec::named(NamedKey::PageUp)]),
            page_down: KeyList(vec![KeySpec::named(NamedKey::PageDown)]),
            jump_first: KeyList(vec![KeySpec::named(NamedKey::Home)]),
            jump_last: KeyList(vec![KeySpec::char('G'), KeySpec::named(NamedKey::End)]),
            select_all: KeyList(vec![KeySpec::ctrl_char('a')]),
            history_back: KeyList(vec![KeySpec::alt_named(NamedKey::Left)]),
            history_forward: KeyList(vec![KeySpec::alt_named(NamedKey::Right)]),
            sort: KeyList::one('s'),
            toggle_view: KeyList::one('v'),
            toggle_preview: KeyList::one('V'),
            fullscreen_preview: KeyList::one('P'),
            toggle_hidden: KeyList::one('.'),
            nav_left: KeyList(vec![KeySpec::char('h'), KeySpec::named(NamedKey::Left)]),
            nav_down: KeyList(vec![KeySpec::char('j'), KeySpec::named(NamedKey::Down)]),
            nav_up: KeyList(vec![KeySpec::char('k'), KeySpec::named(NamedKey::Up)]),
            nav_right: KeyList(vec![KeySpec::char('l'), KeySpec::named(NamedKey::Right)]),
            scroll_preview_left: KeyList(vec![
                KeySpec::char('H'),
                KeySpec::shift_named(NamedKey::Left),
            ]),
            scroll_preview_right: KeyList(vec![
                KeySpec::char('L'),
                KeySpec::shift_named(NamedKey::Right),
            ]),
            scroll_preview_up: KeyList(vec![
                KeySpec::char('K'),
                KeySpec::shift_named(NamedKey::Up),
            ]),
            scroll_preview_down: KeyList(vec![
                KeySpec::char('J'),
                KeySpec::shift_named(NamedKey::Down),
            ]),
        }
    }
}
