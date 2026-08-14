use super::super::{
    App, SidebarItemKind,
    state::{GoToDestination, GoToOverlay, GoToOverlayRow},
};
use crate::{
    config::{BuiltinGoto, GotoEntrySpec},
    fs::{rect_contains, trash_dir},
};
use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use std::path::PathBuf;

impl App {
    pub fn goto_is_open(&self) -> bool {
        self.overlays.goto.is_some()
    }

    pub fn goto_title(&self) -> &str {
        self.overlays
            .goto
            .as_ref()
            .map(|overlay| overlay.title.as_str())
            .unwrap_or("")
    }

    pub fn goto_row_count(&self) -> usize {
        self.overlays
            .goto
            .as_ref()
            .map(|overlay| overlay.rows.len())
            .unwrap_or(0)
    }

    pub fn goto_row_label(&self, index: usize) -> &str {
        self.overlays
            .goto
            .as_ref()
            .and_then(|overlay| overlay.rows.get(index))
            .map(|row| row.label.as_str())
            .unwrap_or("")
    }

    pub fn goto_row_shortcut(&self, index: usize) -> Option<char> {
        self.overlays
            .goto
            .as_ref()
            .and_then(|overlay| overlay.rows.get(index))
            .map(|row| row.shortcut)
    }
}

impl App {
    pub(in crate::app) fn open_goto_overlay(&mut self) {
        self.overlays.help = false;
        self.overlays.goto = Some(build_goto_overlay(self));
        self.status.clear();
    }

    pub(in crate::app) fn handle_goto_key(&mut self, key: KeyEvent) -> Result<()> {
        if key.modifiers.contains(KeyModifiers::CONTROL) && matches!(key.code, KeyCode::Char('c')) {
            self.overlays.goto = None;
            return Ok(());
        }

        match key.code {
            KeyCode::Esc => {
                self.overlays.goto = None;
            }
            _ => {
                if let Some(index) = crate::config::normalized_plain_key_char(key)
                    .and_then(|ch| self.goto_row_index_for_shortcut(ch))
                {
                    self.confirm_goto_index(index)?;
                }
            }
        }

        Ok(())
    }

    pub(in crate::app) fn handle_goto_mouse(&mut self, mouse: MouseEvent) -> Result<()> {
        if let MouseEventKind::Down(MouseButton::Left) = mouse.kind {
            let inside = self
                .input
                .frame_state
                .goto_panel
                .is_some_and(|panel| rect_contains(panel, mouse.column, mouse.row));
            if !inside {
                self.overlays.goto = None;
                return Ok(());
            }

            if let Some(hit) = self
                .input
                .frame_state
                .goto_hits
                .iter()
                .find(|hit| rect_contains(hit.rect, mouse.column, mouse.row))
                .cloned()
            {
                self.confirm_goto_index(hit.index)?;
            }
        }

        Ok(())
    }

    fn goto_row_index_for_shortcut(&self, ch: char) -> Option<usize> {
        self.overlays
            .goto
            .as_ref()
            .and_then(|overlay| overlay.rows.iter().position(|row| row.shortcut == ch))
    }

    fn confirm_goto_index(&mut self, index: usize) -> Result<()> {
        let Some(destination) = self
            .overlays
            .goto
            .as_ref()
            .and_then(|overlay| overlay.rows.get(index).map(|row| row.destination.clone()))
        else {
            return Ok(());
        };

        match destination {
            GoToDestination::Top => {
                self.overlays.goto = None;
                self.select_index(0);
            }
            GoToDestination::Path(path) => {
                self.overlays.goto = None;
                self.set_dir(path)?;
            }
            GoToDestination::Missing(status) => {
                self.status = status;
            }
        }

        Ok(())
    }
}

fn build_goto_overlay(app: &App) -> GoToOverlay {
    let rows = crate::config::goto()
        .entries
        .iter()
        .map(|entry| build_configured_goto_row(app, entry))
        .collect();

    GoToOverlay {
        title: "Go to".to_string(),
        rows,
    }
}

fn build_configured_goto_row(app: &App, entry: &GotoEntrySpec) -> GoToOverlayRow {
    match entry {
        GotoEntrySpec::Builtin { destination, key } => {
            let (label, destination) = builtin_goto_destination(app, *destination);
            build_goto_row(*key, label, destination)
        }
        GotoEntrySpec::Custom { title, path, key } => {
            let destination = if path.exists() {
                GoToDestination::Path(path.clone())
            } else {
                GoToDestination::Missing(format!("{title} not available"))
            };
            build_goto_row(*key, title, destination)
        }
    }
}

fn builtin_goto_destination(
    app: &App,
    destination: BuiltinGoto,
) -> (&'static str, GoToDestination) {
    match destination {
        BuiltinGoto::Top => ("top", GoToDestination::Top),
        BuiltinGoto::Downloads => (
            "downloads",
            downloads_destination(app)
                .map(GoToDestination::Path)
                .unwrap_or_else(|| GoToDestination::Missing("Downloads not available".to_string())),
        ),
        BuiltinGoto::Home => (
            "home",
            crate::config::invoking_user_home_dir()
                .map(GoToDestination::Path)
                .unwrap_or_else(|| GoToDestination::Missing("Home not available".to_string())),
        ),
        BuiltinGoto::Config => (
            config_label(),
            config_directory()
                .map(GoToDestination::Path)
                .unwrap_or_else(|| {
                    GoToDestination::Missing(format!("{} not available", config_label()))
                }),
        ),
        BuiltinGoto::Trash => (
            "trash",
            trash_destination(app)
                .map(GoToDestination::Path)
                .unwrap_or_else(|| GoToDestination::Missing("Trash not available".to_string())),
        ),
    }
}

fn build_goto_row(shortcut: char, label: &str, destination: GoToDestination) -> GoToOverlayRow {
    GoToOverlayRow {
        shortcut,
        label: label.to_string(),
        destination,
    }
}

fn config_label() -> &'static str {
    if cfg!(target_os = "macos") {
        "App Support"
    } else if cfg!(windows) {
        "AppData"
    } else {
        ".config"
    }
}

fn downloads_destination(app: &App) -> Option<PathBuf> {
    app.navigation
        .sidebar
        .iter()
        .filter_map(|row| row.item())
        .find(|item| item.kind == SidebarItemKind::Downloads)
        .map(|item| item.path.clone())
        .filter(|path| path.exists())
}

/// Returns the platform config home.
///
/// - Linux / BSD: `~/.config` (or `$XDG_CONFIG_HOME`)
/// - macOS: `~/Library/Application Support`
/// - Windows: `%APPDATA%`
fn config_directory() -> Option<PathBuf> {
    dirs::config_dir()
}

fn trash_destination(app: &App) -> Option<PathBuf> {
    app.navigation
        .sidebar
        .iter()
        .filter_map(|row| row.item())
        .find(|item| item.kind == SidebarItemKind::Trash)
        .map(|item| item.path.clone())
        .or_else(|| crate::config::trash_home_dir().and_then(|home| trash_dir(&home)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyEventKind, KeyEventState};
    use std::{fs, time::SystemTime};

    #[test]
    fn goto_shortcuts_respect_caps_lock_normalization() {
        let root = temp_dir("goto-caps-lock-root");
        fs::create_dir_all(&root).expect("failed to create temp dir");
        for name in ["a.txt", "b.txt"] {
            fs::write(root.join(name), name).expect("failed to write temp file");
        }

        let mut app = App::new_at(root.clone()).expect("failed to create app");
        app.jump_last();
        app.overlays.goto = Some(GoToOverlay {
            title: "Go to".to_string(),
            rows: vec![GoToOverlayRow {
                shortcut: 'W',
                label: "top".to_string(),
                destination: GoToDestination::Top,
            }],
        });

        app.handle_goto_key(caps_lock_char('w', KeyModifiers::NONE))
            .expect("caps-lock W shortcut should activate");

        assert_eq!(app.navigation.selected, 0);
        assert!(app.overlays.goto.is_none());

        fs::remove_dir_all(root).expect("failed to remove temp dir");
    }

    fn caps_lock_char(c: char, modifiers: KeyModifiers) -> KeyEvent {
        KeyEvent::new_with_kind_and_state(
            KeyCode::Char(c),
            modifiers,
            KeyEventKind::Press,
            KeyEventState::CAPS_LOCK,
        )
    }

    fn temp_dir(name: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .expect("system clock should be after unix epoch")
            .as_nanos();
        std::env::temp_dir().join(format!("elio-{name}-{unique}"))
    }
}
