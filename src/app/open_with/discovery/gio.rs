// This module is only compiled on Linux / BSD (gated in discovery/mod.rs).

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::preview::process::run_command_capture_stdout_cancellable;

use super::super::super::state::OpenWithApp;
use super::desktop_file::{DesktopEntryCandidate, parse_desktop_entry};
use super::exec::expand_exec_template;
use super::scan::desktop_entry_dirs;

/// Asks `gio mime <mime>` for the full list of registered applications,
/// including those that handle parent MIME types via inheritance.
///
/// Returns `None` if gio is unavailable or was canceled; returns `Some(vec![])`
/// if gio ran successfully but found no applications.
pub(super) fn discover_via_gio(
    mime: &str,
    path: &Path,
    canceled: &impl Fn() -> bool,
) -> Option<Vec<OpenWithApp>> {
    let mut cmd = Command::new("gio");
    cmd.args(["mime", mime]);
    crate::invoking_user_command::prepare_external(&mut cmd, None).ok()?;
    let output = run_command_capture_stdout_cancellable(cmd, "open-with-gio", canceled)?;
    let text = String::from_utf8_lossy(&output);

    let entries = parse_gio_mime_output(&text);
    if entries.is_empty() {
        return Some(vec![]);
    }

    let dirs = desktop_entry_dirs();
    let desktops = super::current_desktops();
    let mut apps = Vec::new();
    for (desktop_id, is_default) in entries {
        if let Some(app) =
            read_desktop_entry_for_id(&desktop_id, &dirs, path, is_default, &desktops)
        {
            apps.push(app);
        }
    }

    Some(apps)
}

/// Reads and parses the `.desktop` file for `desktop_id` from the first
/// directory in `dirs` that contains it.
///
/// Returns `None` if the file is missing, malformed, or excluded by
/// `OnlyShowIn` / `NotShowIn` for the current desktop environment.
///
/// Once a file is found (at any candidate path within a directory) the search
/// stops — a higher-priority entry that is hidden or fails the desktop filter
/// wins over a lower-priority entry that would be valid.
fn read_desktop_entry_for_id(
    desktop_id: &str,
    dirs: &[PathBuf],
    target: &Path,
    is_default: bool,
    desktops: &[String],
) -> Option<OpenWithApp> {
    for dir in dirs {
        // A desktop ID like "kde-konsole.desktop" may correspond to either a
        // flat file "kde-konsole.desktop" or a nested one "kde/konsole.desktop".
        // Try each candidate path in left-to-right order.
        for entry_path in candidate_paths_for_desktop_id(dir, desktop_id) {
            let Ok(contents) = std::fs::read_to_string(&entry_path) else {
                continue; // not found at this candidate — try the next one
            };
            // File found — stop searching all candidates and all dirs.
            let candidate: DesktopEntryCandidate = parse_desktop_entry(&contents)?;
            if !candidate.is_shown_in(desktops) {
                return None;
            }
            let (program, args) = expand_exec_template(&candidate.exec, target)?;
            return Some(OpenWithApp {
                display_name: candidate.name,
                desktop_id: Some(desktop_id.to_string()),
                program,
                args,
                is_default,
                requires_terminal: candidate.terminal,
            });
        }
    }
    None
}

/// Generates candidate file paths for a desktop ID by treating each `-`
/// character as a possible directory separator, left-to-right.
///
/// XDG desktop IDs are formed by replacing path separators `/` with `-`, so
/// the mapping from ID to path is ambiguous: `kde-konsole.desktop` could be
/// the flat file `kde-konsole.desktop` or the nested file `kde/konsole.desktop`.
///
/// This function generates all O(n) left-to-right interpretations for an ID
/// with n dashes, in order of increasing depth.  The caller tries them in
/// sequence and stops at the first path that exists.
///
/// Examples:
///   `"gedit.desktop"`      → `["{base}/gedit.desktop"]`
///   `"kde-konsole.desktop"` → `["{base}/kde-konsole.desktop",
///                               "{base}/kde/konsole.desktop"]`
fn candidate_paths_for_desktop_id(base: &Path, desktop_id: &str) -> Vec<PathBuf> {
    let segments: Vec<&str> = desktop_id.split('-').collect();
    (0..segments.len())
        .map(|k| {
            let file_part = segments[k..].join("-");
            if k == 0 {
                base.join(&file_part)
            } else {
                let dir_part = segments[..k].join("/");
                base.join(&dir_part).join(&file_part)
            }
        })
        .collect()
}

/// Parses the output of `gio mime <mime-type>` into an ordered list of
/// `(desktop_id, is_default)` pairs.
///
/// The default application (if any) is placed first with `is_default = true`.
/// Subsequent entries from the Registered/Recommended sections follow in
/// first-seen order, deduplicated.  If an entry appears in both sections it
/// is emitted once (at the position it was first seen).
fn parse_gio_mime_output(text: &str) -> Vec<(String, bool)> {
    let mut default_app: Option<String> = None;
    let mut ordered: Vec<String> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();

    for line in text.lines() {
        // gio mime prints the default application line in one of two formats
        // depending on the platform and GLib version:
        //   ASCII quotes:   Default application for 'mime/type': app.desktop
        //   Curly quotes:   Default application for \u{201C}mime/type\u{201D}: app.desktop
        // We strip the known prefix and then find the closing quote + ": " separator,
        // trying the curly-quote form first since that is what GNOME uses.
        if line.starts_with("Default application for ") {
            let desktop_id = line
                .find("\u{201D}: ")
                .map(|i| &line[i + "\u{201D}: ".len()..])
                .or_else(|| line.find("': ").map(|i| &line[i + 3..]))
                .or_else(|| line.find("\": ").map(|i| &line[i + 3..]))
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string);
            if let Some(id) = desktop_id {
                default_app = Some(id);
            }
            continue;
        }
        // Tab-indented lines are desktop IDs in Registered / Recommended sections.
        if line.starts_with('\t') {
            let desktop_id = line.trim().to_string();
            if !desktop_id.is_empty() && seen.insert(desktop_id.clone()) {
                ordered.push(desktop_id);
            }
        }
    }

    let mut result: Vec<(String, bool)> = Vec::new();
    if let Some(ref def) = default_app {
        result.push((def.clone(), true));
    }
    for id in ordered {
        if default_app.as_deref() != Some(&id) {
            result.push((id, false));
        }
    }

    result
}

#[cfg(test)]
mod tests;
