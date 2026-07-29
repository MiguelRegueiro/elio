use super::super::{
    App,
    state::{CopyOverlay, CopyOverlayRow},
};
use crate::fs::rect_contains;
use anyhow::{Result, anyhow};
use base64::Engine as _;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use std::{
    env,
    io::{self, IsTerminal, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

impl App {
    pub fn copy_is_open(&self) -> bool {
        self.overlays.copy.is_some()
    }

    pub fn copy_title(&self) -> &str {
        self.overlays
            .copy
            .as_ref()
            .map(|overlay| overlay.title.as_str())
            .unwrap_or("")
    }

    pub fn copy_row_count(&self) -> usize {
        self.overlays
            .copy
            .as_ref()
            .map(|overlay| overlay.rows.len())
            .unwrap_or(0)
    }

    pub fn copy_row_label(&self, index: usize) -> &str {
        self.overlays
            .copy
            .as_ref()
            .and_then(|overlay| overlay.rows.get(index))
            .map(|row| row.label.as_str())
            .unwrap_or("")
    }

    pub fn copy_row_shortcut(&self, index: usize) -> Option<char> {
        self.overlays
            .copy
            .as_ref()
            .and_then(|overlay| overlay.rows.get(index))
            .map(|row| row.shortcut)
    }
}

impl App {
    pub(in crate::app) fn open_copy_overlay(&mut self) {
        let paths = self.clipboard_target_paths();
        if paths.is_empty() {
            self.status = "Nothing to copy".to_string();
            return;
        }

        self.open_copy_overlay_for_paths(paths);
    }

    pub(in crate::app) fn open_copy_overlay_for_paths(&mut self, paths: Vec<PathBuf>) {
        if paths.is_empty() {
            self.status = "Nothing to copy".to_string();
            return;
        }

        self.overlays.help = false;
        self.overlays.copy = Some(build_copy_overlay(&self.navigation.cwd, &paths));
        self.status.clear();
    }

    pub(in crate::app) fn handle_copy_key(&mut self, key: KeyEvent) -> Result<()> {
        if key.modifiers.contains(KeyModifiers::CONTROL) && matches!(key.code, KeyCode::Char('c')) {
            self.overlays.copy = None;
            return Ok(());
        }

        match key.code {
            KeyCode::Esc => {
                self.overlays.copy = None;
            }
            KeyCode::Char(ch)
                if !key
                    .modifiers
                    .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
            {
                if let Some(index) = self.copy_row_index_for_shortcut(ch) {
                    self.confirm_copy_index(index)?;
                }
            }
            _ => {}
        }

        Ok(())
    }

    pub(in crate::app) fn handle_copy_mouse(&mut self, mouse: MouseEvent) -> Result<()> {
        if let MouseEventKind::Down(MouseButton::Left) = mouse.kind {
            let inside = self
                .input
                .frame_state
                .copy_panel
                .is_some_and(|panel| rect_contains(panel, mouse.column, mouse.row));
            if !inside {
                self.overlays.copy = None;
                return Ok(());
            }

            if let Some(hit) = self
                .input
                .frame_state
                .copy_hits
                .iter()
                .find(|hit| rect_contains(hit.rect, mouse.column, mouse.row))
                .cloned()
            {
                self.confirm_copy_index(hit.index)?;
            }
        }

        Ok(())
    }

    fn copy_row_index_for_shortcut(&self, ch: char) -> Option<usize> {
        let needle = ch.to_ascii_lowercase();
        self.overlays.copy.as_ref().and_then(|overlay| {
            overlay
                .rows
                .iter()
                .position(|row| row.shortcut.to_ascii_lowercase() == needle)
        })
    }

    fn confirm_copy_index(&mut self, index: usize) -> Result<()> {
        let Some((value, status_label)) = self.overlays.copy.as_ref().and_then(|overlay| {
            overlay
                .rows
                .get(index)
                .map(|row| (row.value.clone(), row.status_label.clone()))
        }) else {
            return Ok(());
        };

        match write_text_to_system_clipboard(&value) {
            Ok(()) => {
                self.overlays.copy = None;
                self.status = format!("Copied {status_label}");
            }
            Err(error) => {
                self.status = clipboard_status_message(&error);
            }
        }

        Ok(())
    }
}

fn clipboard_status_message(error: &anyhow::Error) -> String {
    let detail = error.to_string();
    if detail.contains("no clipboard tool succeeded") || detail.contains("os error 2") {
        "Clipboard helper not found".to_string()
    } else if detail.contains("osc52") {
        "Clipboard unavailable".to_string()
    } else {
        "Clipboard write failed".to_string()
    }
}

fn build_copy_overlay(cwd: &Path, paths: &[PathBuf]) -> CopyOverlay {
    let absolute_paths = paths
        .iter()
        .map(|path| absolute_path_for(path, cwd))
        .collect::<Vec<_>>();
    let directory_paths = absolute_paths
        .iter()
        .map(|path| directory_path_for(path))
        .collect::<Vec<_>>();
    let file_name_values = absolute_paths
        .iter()
        .map(|path| file_name_for_path(path))
        .collect::<Vec<_>>();
    let stem_values = absolute_paths
        .iter()
        .map(|path| name_without_extension_for_path(path))
        .collect::<Vec<_>>();
    let target_count = absolute_paths.len();
    let rows = vec![
        build_copy_row(
            'c',
            if target_count == 1 {
                "Copy file name"
            } else {
                "Copy file names"
            },
            if target_count == 1 {
                "file name"
            } else {
                "file names"
            },
            &file_name_values,
        ),
        build_copy_row(
            'n',
            if target_count == 1 {
                "Name without extension"
            } else {
                "Names without extension"
            },
            if target_count == 1 {
                "name without extension"
            } else {
                "names without extension"
            },
            &stem_values,
        ),
        build_copy_row(
            'p',
            if target_count == 1 {
                "File path"
            } else {
                "File paths"
            },
            if target_count == 1 {
                "file path"
            } else {
                "file paths"
            },
            &absolute_paths
                .iter()
                .map(|path| path.display().to_string())
                .collect::<Vec<_>>(),
        ),
        build_copy_row(
            'd',
            if target_count == 1 {
                "Directory path"
            } else {
                "Directory paths"
            },
            if target_count == 1 {
                "directory path"
            } else {
                "directory paths"
            },
            &directory_paths
                .iter()
                .map(|path| path.display().to_string())
                .collect::<Vec<_>>(),
        ),
    ];

    CopyOverlay {
        title: "Copy to clipboard".to_string(),
        rows,
    }
}

fn build_copy_row(
    shortcut: char,
    label: &str,
    status_label: &str,
    values: &[String],
) -> CopyOverlayRow {
    CopyOverlayRow {
        shortcut,
        label: label.to_string(),
        status_label: status_label.to_string(),
        value: values.join("\n"),
    }
}

fn absolute_path_for(path: &Path, cwd: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        cwd.join(path)
    }
}

fn file_name_for_path(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .or_else(|| {
            path.components()
                .next_back()
                .map(|component| component.as_os_str().to_string_lossy().into_owned())
        })
        .unwrap_or_else(|| path.display().to_string())
}

fn name_without_extension_for_path(path: &Path) -> String {
    path.file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_else(|| file_name_for_path(path))
}

fn directory_path_for(path: &Path) -> PathBuf {
    path.parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| path.to_path_buf())
}

fn write_text_to_system_clipboard(text: &str) -> Result<()> {
    let mut errors = Vec::new();

    if terminal_supports_osc52_clipboard() {
        match write_text_to_terminal_clipboard(text) {
            Ok(()) => return Ok(()),
            Err(error) => errors.push(format!("osc52: {error}")),
        }
    }

    for (program, args) in clipboard_command_candidates() {
        match run_clipboard_command(&program, &args, text) {
            Ok(()) => return Ok(()),
            Err(error) => errors.push(format!("{program}: {error}")),
        }
    }

    Err(anyhow!(
        "no clipboard tool succeeded ({})",
        errors.join("; ")
    ))
}

fn write_text_to_terminal_clipboard(text: &str) -> Result<()> {
    #[cfg(test)]
    if let Some(path) = env::var_os("ELIO_TEST_OSC52_CAPTURE") {
        std::fs::write(path, build_osc52_set_clipboard_sequence(text))
            .map_err(|error| anyhow!("failed to capture osc52 clipboard output: {error}"))?;
        return Ok(());
    }

    let mut stdout = io::stdout();
    if !stdout.is_terminal() {
        return Err(anyhow!("stdout is not a terminal"));
    }

    stdout
        .write_all(build_osc52_set_clipboard_sequence(text).as_bytes())
        .map_err(|error| anyhow!("failed to write osc52 clipboard escape: {error}"))?;
    stdout
        .flush()
        .map_err(|error| anyhow!("failed to flush osc52 clipboard escape: {error}"))?;
    Ok(())
}

fn build_osc52_set_clipboard_sequence(text: &str) -> String {
    let payload = base64::engine::general_purpose::STANDARD.encode(text.as_bytes());
    format!("\x1b]52;c;{payload}\x1b\\")
}

fn terminal_supports_osc52_clipboard() -> bool {
    if env::var_os("ELIO_CLIPBOARD_OSC52").is_some() {
        return true;
    }

    if env::var_os("TMUX").is_some() && !tmux_accepts_application_osc52() {
        return false;
    }

    #[cfg(test)]
    if env::var_os("ELIO_TEST_OSC52_CAPTURE").is_some() {
        return true;
    }

    let term = env::var("TERM").unwrap_or_default().to_ascii_lowercase();
    let term_program = env::var("TERM_PROGRAM")
        .unwrap_or_default()
        .to_ascii_lowercase();

    env::var_os("KITTY_WINDOW_ID").is_some()
        || term.contains("xterm-kitty")
        || term_program == "kitty"
        || term.contains("ghostty")
        || term_program == "ghostty"
        || term == "foot"
        || term == "foot-extra"
        || term.contains("wezterm")
        || term_program == "wezterm"
        || term_program == "iterm.app"
        || term_program.contains("warp")
        || env::var_os("WARP_SESSION_ID").is_some()
        || term.contains("alacritty")
        || term_program.contains("alacritty")
        || env::var_os("ALACRITTY_SOCKET").is_some()
        || env::var_os("VTE_VERSION").is_some()
        || env::var_os("WT_SESSION").is_some()
}

fn tmux_accepts_application_osc52() -> bool {
    #[cfg(test)]
    if let Some(value) = env::var_os("ELIO_TEST_TMUX_SET_CLIPBOARD") {
        return value.to_string_lossy().trim().eq_ignore_ascii_case("on");
    }

    Command::new("tmux")
        .args(["show-options", "-gqv", "set-clipboard"])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .is_some_and(|value| value.trim().eq_ignore_ascii_case("on"))
}

fn run_clipboard_command(program: &str, args: &[String], text: &str) -> Result<()> {
    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| anyhow!("{error}"))?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(text.as_bytes())
            .map_err(|error| anyhow!("failed to write stdin: {error}"))?;
    }

    let status = child
        .wait()
        .map_err(|error| anyhow!("failed to wait for command: {error}"))?;
    if status.success() {
        return Ok(());
    }

    Err(anyhow!("exited with {status}"))
}

fn clipboard_command_candidates() -> Vec<(String, Vec<String>)> {
    let mut commands = Vec::new();

    #[cfg(test)]
    if let Some(tool) = env::var_os("ELIO_TEST_CLIPBOARD_TOOL") {
        commands.push((tool.to_string_lossy().into_owned(), Vec::new()));
    }

    if cfg!(target_os = "macos") {
        commands.push(("pbcopy".to_string(), Vec::new()));
        return commands;
    }

    if cfg!(windows) {
        commands.push(("clip.exe".to_string(), Vec::new()));
        commands.push(("clip".to_string(), Vec::new()));
        return commands;
    }

    let wayland = env::var_os("WAYLAND_DISPLAY").is_some()
        || env::var_os("XDG_SESSION_TYPE")
            .is_some_and(|value| value.to_string_lossy().eq_ignore_ascii_case("wayland"));
    let x11 = env::var_os("DISPLAY").is_some();

    if wayland {
        commands.push((
            "wl-copy".to_string(),
            vec!["--type".to_string(), "text/plain;charset=utf-8".to_string()],
        ));
    }
    if x11 {
        commands.push((
            "xclip".to_string(),
            vec![
                "-selection".to_string(),
                "clipboard".to_string(),
                "-in".to_string(),
            ],
        ));
        commands.push((
            "xsel".to_string(),
            vec!["--clipboard".to_string(), "--input".to_string()],
        ));
    }
    if !wayland {
        commands.push((
            "wl-copy".to_string(),
            vec!["--type".to_string(), "text/plain;charset=utf-8".to_string()],
        ));
    }
    if !x11 {
        commands.push((
            "xclip".to_string(),
            vec![
                "-selection".to_string(),
                "clipboard".to_string(),
                "-in".to_string(),
            ],
        ));
        commands.push((
            "xsel".to_string(),
            vec!["--clipboard".to_string(), "--input".to_string()],
        ));
    }

    commands
}
