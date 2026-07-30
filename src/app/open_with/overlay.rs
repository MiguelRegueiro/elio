use std::path::Path;

use super::super::{
    App,
    state::{OpenWithApp, OpenWithOverlay, OpenWithRow, PendingTerminalTask},
};
use crate::fs::detached_open_command;
#[cfg(any(test, target_os = "macos", not(unix)))]
use crate::fs::open_in_system;
use anyhow::Result;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::app) enum FallbackOpenOutcome {
    #[cfg_attr(all(unix, not(target_os = "macos"), not(test)), allow(dead_code))]
    DefaultApp,
    #[cfg(target_os = "macos")]
    TextEditor,
}

// ── Read-only accessors ───────────────────────────────────────────────────────

impl App {
    pub fn open_with_is_open(&self) -> bool {
        self.overlays.open_with.is_some()
    }

    pub fn open_with_title(&self) -> &str {
        self.overlays
            .open_with
            .as_ref()
            .map(|overlay| overlay.title.as_str())
            .unwrap_or("")
    }

    pub fn open_with_row_count(&self) -> usize {
        self.overlays
            .open_with
            .as_ref()
            .map(|overlay| overlay.rows.len())
            .unwrap_or(0)
    }

    pub fn open_with_row_label(&self, index: usize) -> &str {
        self.overlays
            .open_with
            .as_ref()
            .and_then(|overlay| overlay.rows.get(index))
            .map(|row| row.label.as_str())
            .unwrap_or("")
    }

    pub fn open_with_row_shortcut(&self, index: usize) -> Option<char> {
        self.overlays
            .open_with
            .as_ref()
            .and_then(|overlay| overlay.rows.get(index))
            .and_then(|row| row.shortcut)
    }

    pub fn open_with_selected_index(&self) -> usize {
        self.overlays
            .open_with
            .as_ref()
            .map(|overlay| overlay.selected)
            .unwrap_or(0)
    }
}

// ── Overlay control and launch logic ─────────────────────────────────────────

impl App {
    pub(in crate::app) fn open_open_with_overlay(&mut self) {
        let Some(entry) = self.selected_entry() else {
            self.status = "Nothing selected".to_string();
            return;
        };
        self.open_open_with_overlay_for_entry(entry.clone());
    }

    pub(in crate::app) fn open_open_with_overlay_for_entry(&mut self, entry: crate::core::Entry) {
        let path = entry.path.clone();
        #[cfg(test)]
        let apps = super::discovered_open_with_apps_for_test()
            .unwrap_or_else(|| super::discovery::discover_open_with_apps_for_entry(&entry));
        #[cfg(not(test))]
        let apps = super::discovery::discover_open_with_apps_for_entry(&entry);
        self.handle_discovered_open_with_apps(&path, apps, open_with_fallback, |app| {
            detached_open_command(&app.program, &app.args)
        });
    }

    pub(super) fn confirm_open_with_index(&mut self, index: usize) -> Result<()> {
        let Some(row) = self
            .overlays
            .open_with
            .as_ref()
            .and_then(|overlay| overlay.rows.get(index))
        else {
            return Ok(());
        };
        let display_name = row.app.display_name.clone();
        let program = row.app.program.clone();
        let args = row.app.args.clone();
        let requires_terminal = row.app.requires_terminal;

        self.overlays.open_with = None;

        if requires_terminal {
            self.pending_terminal_task = Some(PendingTerminalTask::Command { program, args });
            self.status.clear();
        } else {
            match detached_open_command(&program, &args) {
                Ok(()) => self.status.clear(),
                Err(_) => self.status = format!("Failed to open with {display_name}"),
            }
        }

        Ok(())
    }

    /// Dispatches a discovered app list: falls back to the system opener for
    /// zero apps, launches directly for one, and opens the overlay for two or
    /// more.
    ///
    /// `launch_app` is called only for GUI apps (`requires_terminal == false`).
    /// Terminal apps set `pending_terminal_task` on `self` directly so that
    /// the caller in `lib.rs` can suspend the TUI before running them.
    pub(in crate::app) fn handle_discovered_open_with_apps<F, G>(
        &mut self,
        path: &Path,
        mut apps: Vec<OpenWithApp>,
        mut fallback_open: F,
        mut launch_app: G,
    ) where
        F: FnMut(&Path) -> std::result::Result<FallbackOpenOutcome, String>,
        G: FnMut(&OpenWithApp) -> std::io::Result<()>,
    {
        match apps.len() {
            0 => match fallback_open(path) {
                Ok(FallbackOpenOutcome::DefaultApp) => {
                    self.status = "No apps found, opened with default".to_string();
                }
                #[cfg(target_os = "macos")]
                Ok(FallbackOpenOutcome::TextEditor) => {
                    self.status = "No apps found, opened in text editor".to_string();
                }
                Err(e) if e == "No apps found" => self.status = e,
                Err(e) => self.status = format!("Failed to open: {e}"),
            },
            1 => {
                let app = apps.remove(0);
                if app.requires_terminal {
                    self.pending_terminal_task = Some(PendingTerminalTask::Command {
                        program: app.program.clone(),
                        args: app.args.clone(),
                    });
                    self.status.clear();
                } else {
                    match launch_app(&app) {
                        Ok(()) => self.status = format!("Opened with {}", app.display_name),
                        Err(_) => self.status = format!("Failed to open with {}", app.display_name),
                    }
                }
            }
            _ => {
                self.overlays.help = false;
                self.overlays.open_with = Some(build_open_with_overlay(
                    apps,
                    &crate::config::keys().open_with_reserved_shortcuts(),
                ));
                self.status.clear();
            }
        }
    }

    pub(in crate::app) fn move_open_with_selection(&mut self, delta: isize) {
        let Some(overlay) = self.overlays.open_with.as_mut() else {
            return;
        };
        if overlay.rows.is_empty() {
            overlay.selected = 0;
            return;
        }

        let max = overlay.rows.len().saturating_sub(1) as isize;
        overlay.selected = (overlay.selected as isize + delta).clamp(0, max) as usize;
    }

    pub(in crate::app) fn confirm_selected_open_with_row(&mut self) -> Result<()> {
        let Some(index) = self
            .overlays
            .open_with
            .as_ref()
            .map(|overlay| overlay.selected)
        else {
            return Ok(());
        };

        self.confirm_open_with_index(index)
    }
}

fn build_open_with_overlay(apps: Vec<OpenWithApp>, reserved_shortcuts: &[char]) -> OpenWithOverlay {
    let mut shortcuts = open_with_shortcuts(reserved_shortcuts);
    let rows = apps
        .into_iter()
        .map(|app| {
            let shortcut = shortcuts.next();
            let mut label = app.display_name.clone();
            if app.requires_terminal && !is_env_editor_label(&label) {
                label.push_str(" (terminal)");
            }
            if app.is_default {
                label.push_str(" (default)");
            }
            OpenWithRow {
                shortcut,
                label,
                app,
            }
        })
        .collect();

    OpenWithOverlay {
        title: "Open With".to_string(),
        rows,
        selected: 0,
    }
}

const OPEN_WITH_SHORTCUTS: &str = "123456789abcdefghijklmnopqrstuvwxyz";

fn open_with_shortcuts(reserved: &[char]) -> impl Iterator<Item = char> + '_ {
    OPEN_WITH_SHORTCUTS
        .chars()
        .filter(move |shortcut| !reserved.contains(shortcut))
}

fn is_env_editor_label(display_name: &str) -> bool {
    display_name.contains("($VISUAL)") || display_name.contains("($EDITOR)")
}

fn open_with_fallback(path: &Path) -> std::result::Result<FallbackOpenOutcome, String> {
    #[cfg(target_os = "macos")]
    {
        if super::path_is_text_like(path) {
            return open_in_text_editor(path).map(|()| FallbackOpenOutcome::TextEditor);
        }
        return open_in_system(path).map(|()| FallbackOpenOutcome::DefaultApp);
    }

    #[cfg(all(unix, not(target_os = "macos"), not(test)))]
    {
        let _ = path;
        Err("No apps found".to_string())
    }

    #[cfg(all(any(not(unix), test), not(target_os = "macos")))]
    open_in_system(path).map(|()| FallbackOpenOutcome::DefaultApp)
}

#[cfg(target_os = "macos")]
fn open_in_text_editor(path: &Path) -> std::result::Result<(), String> {
    use std::process::{Command, Stdio};

    let status = Command::new("open")
        .arg("-t")
        .arg(path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map_err(|e| format!("open: {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("open exited with {status}"))
    }
}

// ── Test seam ─────────────────────────────────────────────────────────────────

#[cfg(test)]
impl App {
    /// Injects a single-row open-with overlay pointing at the given command.
    /// Used only in tests to exercise the confirm/launch path without real discovery.
    pub(crate) fn inject_open_with_for_test(
        &mut self,
        display_name: &str,
        program: &str,
        args: Vec<String>,
        requires_terminal: bool,
    ) {
        self.inject_open_with_rows_for_test(vec![(
            display_name.to_string(),
            program.to_string(),
            args,
            requires_terminal,
        )]);
    }

    pub(crate) fn inject_open_with_rows_for_test(
        &mut self,
        rows: Vec<(String, String, Vec<String>, bool)>,
    ) {
        use super::super::state::{OpenWithApp, OpenWithOverlay, OpenWithRow};
        self.overlays.open_with = Some(OpenWithOverlay {
            title: "Open With".to_string(),
            rows: rows
                .into_iter()
                .enumerate()
                .map(
                    |(index, (display_name, program, args, requires_terminal))| OpenWithRow {
                        shortcut: char::from_digit((index + 1) as u32, 10),
                        label: display_name.clone(),
                        app: OpenWithApp {
                            display_name,
                            desktop_id: None,
                            program,
                            args,
                            is_default: false,
                            requires_terminal,
                        },
                    },
                )
                .collect(),
            selected: 0,
        });
    }
}
