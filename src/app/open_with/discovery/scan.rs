// This module is only compiled on Linux / BSD (gated in discovery/mod.rs).

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use super::super::super::state::OpenWithApp;
use super::desktop_file::{
    DesktopEntryCandidate, parse_desktop_entry, parse_mimeapps_defaults, parse_mimeapps_removed,
};
use super::exec::expand_exec_template;

/// Manual desktop-file scan: walks all desktop entry directories and returns
/// apps that explicitly list `mime` in their `MimeType=` field.
/// Used as a fallback when `gio` is unavailable.
pub(super) fn discover_via_desktop_scan(mime: &str, path: &Path) -> Vec<OpenWithApp> {
    discover_via_desktop_scan_in_dirs(mime, path, &desktop_entry_dirs())
}

/// Inner scan that accepts an explicit directory list (allows hermetic testing).
pub(super) fn discover_via_desktop_scan_in_dirs(
    mime: &str,
    path: &Path,
    dirs: &[PathBuf],
) -> Vec<OpenWithApp> {
    discover_via_desktop_scan_inner(mime, path, dirs, &mimeapps_paths())
}

/// Innermost scan accepting both explicit desktop dirs and explicit mimeapps
/// paths, enabling fully hermetic tests without environment-variable mutation.
fn discover_via_desktop_scan_inner(
    mime: &str,
    path: &Path,
    dirs: &[PathBuf],
    mime_paths: &[PathBuf],
) -> Vec<OpenWithApp> {
    let desktops = super::current_desktops();

    // Collect all desktop entries that declare this MIME type, keyed by
    // desktop-id.  Higher-priority directories come first; once a desktop-id
    // is seen it is never overwritten by a lower-priority directory.
    // The recursive walk derives desktop-ids from relative paths per XDG spec
    // (e.g. `kde/konsole.desktop` → desktop-id `kde-konsole.desktop`).
    let mut candidates: HashMap<String, DesktopEntryCandidate> = HashMap::new();
    for dir in dirs {
        for (desktop_id, entry_path) in collect_desktop_entries(dir) {
            if candidates.contains_key(&desktop_id) {
                continue; // already claimed by a higher-priority dir
            }
            let Ok(contents) = std::fs::read_to_string(&entry_path) else {
                continue;
            };
            let Some(candidate) = parse_desktop_entry(&contents) else {
                continue;
            };
            if candidate.mime_types.iter().any(|m| m == mime) && candidate.is_shown_in(&desktops) {
                candidates.insert(desktop_id, candidate);
            }
        }
    }

    // Default ordering: first file in priority order that mentions this MIME type.
    let ordered_defaults: Vec<String> = mime_paths
        .iter()
        .find_map(|p| {
            let contents = std::fs::read_to_string(p).ok()?;
            let defaults = parse_mimeapps_defaults(&contents, mime);
            if defaults.is_empty() {
                None
            } else {
                Some(defaults)
            }
        })
        .unwrap_or_default();

    // Remove any candidate explicitly suppressed in any mimeapps.list
    // [Removed Associations] section for this MIME type.
    let mut removed: HashSet<String> = HashSet::new();
    for p in mime_paths {
        if let Ok(contents) = std::fs::read_to_string(p) {
            removed.extend(parse_mimeapps_removed(&contents, mime));
        }
    }
    candidates.retain(|id, _| !removed.contains(id));

    // Build result: defaults first (in declared order), then the rest sorted by
    // display name (case-insensitive) for stable ordering.
    // Only the first resolved default gets is_default=true — that is the user's
    // explicit preferred handler; subsequent entries in the defaults list are
    // listed before non-defaults but are not flagged as the default.
    let mut apps: Vec<OpenWithApp> = Vec::new();
    let mut first_default_emitted = false;

    for desktop_id in &ordered_defaults {
        let Some(candidate) = candidates.remove(desktop_id) else {
            continue;
        };
        let Some((program, args)) = expand_exec_template(&candidate.exec, path) else {
            continue;
        };
        let is_default = !first_default_emitted;
        first_default_emitted = true;
        apps.push(OpenWithApp {
            display_name: candidate.name,
            desktop_id: Some(desktop_id.clone()),
            program,
            args,
            is_default,
            requires_terminal: candidate.terminal,
        });
    }

    let mut remaining: Vec<_> = candidates.into_iter().collect();
    remaining.sort_by_key(|a| a.1.name.to_lowercase());

    for (desktop_id, candidate) in remaining {
        let Some((program, args)) = expand_exec_template(&candidate.exec, path) else {
            continue;
        };
        apps.push(OpenWithApp {
            display_name: candidate.name,
            desktop_id: Some(desktop_id),
            program,
            args,
            is_default: false,
            requires_terminal: candidate.terminal,
        });
    }

    apps
}

/// Returns the ordered list of directories to search for `.desktop` files,
/// from highest to lowest priority, following the XDG Base Dir spec.
///
/// Includes Flatpak and Snap export paths so apps installed via those package
/// managers are discovered even when they are not in `XDG_DATA_DIRS`.
pub(super) fn desktop_entry_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    let data_dirs = super::xdg_data_dirs();

    // XDG_DATA_HOME/applications — highest-priority user directory.
    if let Some(data_home) = data_dirs.first() {
        dirs.push(data_home.join("applications"));
    }

    // Flatpak user exports sit between user data home and system dirs.
    // On many systems Flatpak adds this to XDG_DATA_DIRS itself, so the
    // deduplication step below will handle the overlap.
    if let Some(home) = super::invoking_home_dir() {
        dirs.push(home.join(".local/share/flatpak/exports/share/applications"));
    }

    // XDG_DATA_DIRS/applications — system-level directories.
    for data_dir in data_dirs.iter().skip(1) {
        dirs.push(data_dir.join("applications"));
    }

    // System-level package manager exports (usually not in XDG_DATA_DIRS).
    dirs.push(PathBuf::from("/var/lib/flatpak/exports/share/applications"));
    dirs.push(PathBuf::from("/var/lib/snapd/desktop/applications"));

    // Deduplicate while preserving priority order.
    let mut seen = HashSet::new();
    dirs.retain(|d| seen.insert(d.clone()));

    dirs
}

/// Returns the ordered list of `mimeapps.list` paths to consult, from highest
/// to lowest priority, per the XDG MIME Applications spec.
///
/// The lookup order is:
/// 1. `$XDG_CONFIG_HOME/$desktop-mimeapps.list` (per-desktop user override)
/// 2. `$XDG_CONFIG_HOME/mimeapps.list`
/// 3. `$XDG_CONFIG_DIRS/$desktop-mimeapps.list`
/// 4. `$XDG_CONFIG_DIRS/mimeapps.list`
/// 5. `$XDG_DATA_HOME/applications/$desktop-mimeapps.list`
/// 6. `$XDG_DATA_HOME/applications/mimeapps.list`
/// 7. `$XDG_DATA_DIRS/applications/$desktop-mimeapps.list`
/// 8. `$XDG_DATA_DIRS/applications/mimeapps.list`
pub(super) fn mimeapps_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();

    // Desktop names (lowercased) for per-desktop filename variants.
    // XDG spec: "$desktop" is each component of XDG_CURRENT_DESKTOP, lowercased.
    let desktops: Vec<String> = super::current_desktops()
        .into_iter()
        .map(|s| s.to_lowercase())
        .collect();

    // ── Config-dir section ────────────────────────────────────────────────────

    // $XDG_CONFIG_HOME defaults to ~/.config
    if let Some(config_home) = super::invoking_config_home()
        && !config_home.as_os_str().is_empty()
    {
        for desktop in &desktops {
            paths.push(config_home.join(format!("{desktop}-mimeapps.list")));
        }
        paths.push(config_home.join("mimeapps.list"));
    }

    // $XDG_CONFIG_DIRS defaults to /etc/xdg
    for dir in crate::config::invoking_user_env_var("XDG_CONFIG_DIRS")
        .and_then(|value| value.into_string().ok())
        .unwrap_or_else(|| "/etc/xdg".to_string())
        .split(':')
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
    {
        for desktop in &desktops {
            paths.push(dir.join(format!("{desktop}-mimeapps.list")));
        }
        paths.push(dir.join("mimeapps.list"));
    }

    // ── Data-dir/applications section ─────────────────────────────────────────

    for data_dir in super::xdg_data_dirs() {
        let apps = data_dir.join("applications");
        for desktop in &desktops {
            paths.push(apps.join(format!("{desktop}-mimeapps.list")));
        }
        paths.push(apps.join("mimeapps.list"));
    }

    paths
}

// ── Desktop entry collection ──────────────────────────────────────────────────

/// Recursively collects `(desktop_id, file_path)` pairs from `dir`.
///
/// Desktop IDs are derived from the path relative to `dir` by replacing the
/// directory separator with `-`, per the XDG Desktop Entry spec:
///   `{dir}/applications/kde/konsole.desktop` → `kde-konsole.desktop`
///
/// Entries are returned in deterministic order (sorted by desktop-id).
fn collect_desktop_entries(dir: &Path) -> Vec<(String, PathBuf)> {
    let mut results = Vec::new();
    collect_desktop_entries_recursive(dir, dir, &mut results);
    // Sort by desktop-id for a stable, deterministic order.
    results.sort_unstable_by(|a, b| a.0.cmp(&b.0));
    results
}

fn collect_desktop_entries_recursive(
    base: &Path,
    current: &Path,
    results: &mut Vec<(String, PathBuf)>,
) {
    let Ok(read_dir) = std::fs::read_dir(current) else {
        return;
    };

    // Sort entries within each directory for determinism before recursing.
    let mut entries: Vec<_> = read_dir.flatten().collect();
    entries.sort_unstable_by_key(|e| e.file_name());

    for entry in entries {
        let path = entry.path();
        // Use path.is_dir() rather than file_type to follow symlinks.
        if path.is_dir() {
            collect_desktop_entries_recursive(base, &path, results);
        } else if path.extension().and_then(|e| e.to_str()) == Some("desktop") {
            let Ok(rel) = path.strip_prefix(base) else {
                continue;
            };
            // Derive desktop-id: join path components with '-'.
            let components: Vec<_> = rel
                .components()
                .map(|c| c.as_os_str().to_string_lossy().into_owned())
                .collect();
            results.push((components.join("-"), path));
        }
    }
}

// ── tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests;
