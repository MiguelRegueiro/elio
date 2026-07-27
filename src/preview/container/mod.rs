mod archive;
mod iso;
mod torrent;

use super::{appearance as theme, *};
use crate::{
    core::{EntryKind, FileClass},
    file_info,
};
use ratatui::{
    style::Style,
    text::{Line, Span},
};
use std::{collections::BTreeMap, path::Path};

use self::archive::{ArchiveEntry, ArchiveTreeNode};

pub(super) use self::archive::build_archive_preview;
pub(super) use self::iso::build_iso_preview;
#[cfg(test)]
pub(in crate::preview) use self::iso::{
    ISO_BOOT_SYSTEM_ID, ISO_DESCRIPTOR_START_SECTOR, ISO_SECTOR_SIZE, IsoMetadata,
};
#[cfg(test)]
pub(super) use self::iso::{parse_iso_metadata, render_iso_preview};
pub(super) use self::torrent::build_torrent_preview;

pub(super) fn normalize_archive_entries<'a>(
    items: impl IntoIterator<Item = &'a str>,
    strip_version_suffix: bool,
) -> Vec<ArchiveEntry> {
    let mut normalized = BTreeMap::<String, bool>::new();
    for item in items {
        let Some(entry) = normalize_archive_entry(item, strip_version_suffix) else {
            continue;
        };
        insert_archive_entry(&mut normalized, &entry.path, entry.is_dir);
    }

    normalized
        .into_iter()
        .map(|(path, is_dir)| ArchiveEntry { path, is_dir })
        .collect()
}

fn expand_archive_entries(entries: Vec<ArchiveEntry>) -> Vec<ArchiveEntry> {
    let mut normalized = BTreeMap::<String, bool>::new();
    for entry in entries {
        insert_archive_entry(&mut normalized, &entry.path, entry.is_dir);
    }
    normalized
        .into_iter()
        .map(|(path, is_dir)| ArchiveEntry { path, is_dir })
        .collect()
}

fn normalize_archive_entry(item: &str, strip_version_suffix: bool) -> Option<ArchiveEntry> {
    let trimmed = trim_trailing_line_endings(item);
    let trimmed = trimmed.trim();
    if trimmed.is_empty() {
        return None;
    }

    let is_dir = trimmed.ends_with('/') || trimmed.ends_with('\\');
    let trimmed = trimmed
        .trim_start_matches("./")
        .trim_start_matches('/')
        .trim_end_matches(['/', '\\']);
    if trimmed.is_empty() || trimmed == "." {
        return None;
    }

    let mut segments = Vec::new();
    for segment in trimmed.split(['/', '\\']) {
        let segment = if strip_version_suffix {
            strip_iso_version_suffix(segment.trim())
        } else {
            segment.trim()
        };
        if segment.is_empty() || segment == "." {
            continue;
        }
        if segment == ".." {
            return None;
        }
        segments.push(segment.to_string());
    }

    if segments.is_empty() {
        return None;
    }

    Some(ArchiveEntry {
        path: segments.join("/"),
        is_dir,
    })
}

fn insert_archive_entry(entries: &mut BTreeMap<String, bool>, path: &str, is_dir: bool) {
    let mut built = String::new();
    let parts = path.split('/').collect::<Vec<_>>();
    for (index, segment) in parts.iter().enumerate() {
        if !built.is_empty() {
            built.push('/');
        }
        built.push_str(segment);
        let current_is_dir = index < parts.len().saturating_sub(1) || is_dir;
        entries
            .entry(built.clone())
            .and_modify(|existing| *existing |= current_is_dir)
            .or_insert(current_is_dir);
    }
}

fn insert_archive_tree_entry(root: &mut ArchiveTreeNode, entry: &ArchiveEntry) {
    let mut current = root;
    let mut built = String::new();
    let parts = entry.path.split('/').collect::<Vec<_>>();
    for (index, part) in parts.iter().enumerate() {
        if !built.is_empty() {
            built.push('/');
        }
        built.push_str(part);
        let is_last = index == parts.len().saturating_sub(1);
        current = current
            .children
            .entry((*part).to_string())
            .or_insert_with(|| ArchiveTreeNode {
                path: built.clone(),
                is_dir: !is_last || entry.is_dir,
                children: BTreeMap::new(),
            });
        current.path = built.clone();
        current.is_dir |= !is_last || entry.is_dir;
    }
}

fn ordered_archive_children(
    children: &BTreeMap<String, ArchiveTreeNode>,
) -> Vec<(&String, &ArchiveTreeNode)> {
    let mut ordered = children.iter().collect::<Vec<_>>();
    ordered.sort_by(|(left_name, left), (right_name, right)| {
        right
            .is_dir
            .cmp(&left.is_dir)
            .then_with(|| left_name.to_lowercase().cmp(&right_name.to_lowercase()))
    });
    ordered
}

fn render_archive_tree(
    children: &[(&String, &ArchiveTreeNode)],
    prefix: &str,
    remaining: &mut usize,
    rendered_items: &mut usize,
    lines: &mut Vec<Line<'static>>,
    palette: theme::Palette,
) {
    for (index, (name, node)) in children.iter().enumerate() {
        if *remaining == 0 {
            return;
        }

        let is_last = index == children.len().saturating_sub(1);
        lines.push(render_archive_tree_line(
            prefix, name, node, is_last, palette,
        ));
        *remaining = remaining.saturating_sub(1);
        *rendered_items += 1;

        if node.is_dir && !node.children.is_empty() {
            let mut next_prefix = prefix.to_string();
            next_prefix.push_str(if is_last { "    " } else { "│   " });
            let nested = ordered_archive_children(&node.children);
            render_archive_tree(
                &nested,
                &next_prefix,
                remaining,
                rendered_items,
                lines,
                palette,
            );
            if *remaining == 0 {
                return;
            }
        }
    }
}

fn render_archive_tree_line(
    prefix: &str,
    name: &str,
    node: &ArchiveTreeNode,
    is_last: bool,
    palette: theme::Palette,
) -> Line<'static> {
    let connector = if is_last { "└── " } else { "├── " };
    let path = Path::new(&node.path);
    let kind = if node.is_dir {
        EntryKind::Directory
    } else {
        EntryKind::File
    };
    let license_entry = !node.is_dir
        && path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(file_info::is_canonical_license_file_name);
    let appearance = if license_entry {
        theme::resolve_path_with_class(path, kind, FileClass::License)
    } else {
        theme::resolve_path(path, kind)
    };
    let mut display_name = name.to_string();
    if node.is_dir {
        display_name.push('/');
    }

    Line::from(vec![
        Span::styled(
            format!("{prefix}{connector}"),
            Style::default().fg(palette.muted),
        ),
        Span::styled(
            format!("{} ", appearance.icon),
            Style::default()
                .fg(appearance.color)
                .add_modifier(ratatui::style::Modifier::BOLD),
        ),
        Span::styled(display_name, Style::default().fg(palette.text)),
    ])
}

fn push_preview_section(
    lines: &mut Vec<Line<'static>>,
    title: &str,
    fields: &[(&str, Option<String>)],
    palette: theme::Palette,
) {
    let visible_fields = fields
        .iter()
        .filter_map(|(label, value)| value.as_deref().map(|value| (*label, value)))
        .collect::<Vec<_>>();
    if visible_fields.is_empty() {
        return;
    }

    if !lines.is_empty() {
        lines.push(Line::from(""));
    }
    lines.push(section_line(title, palette));
    let label_width = visible_fields
        .iter()
        .map(|(label, _)| label.len())
        .max()
        .unwrap_or(6);
    for (label, value) in visible_fields {
        lines.push(preview_field_line(label, value, label_width, palette));
    }
}

fn push_preview_values_section(
    lines: &mut Vec<Line<'static>>,
    title: &str,
    fields: &[(&str, String)],
    palette: theme::Palette,
) {
    if fields.is_empty() {
        return;
    }

    if !lines.is_empty() {
        lines.push(Line::from(""));
    }
    lines.push(section_line(title, palette));
    let label_width = fields
        .iter()
        .map(|(label, _)| label.len())
        .max()
        .unwrap_or(6);
    for (label, value) in fields {
        lines.push(preview_field_line(label, value, label_width, palette));
    }
}

fn push_preview_owned_values_section(
    lines: &mut Vec<Line<'static>>,
    title: &str,
    fields: &[(String, String)],
    palette: theme::Palette,
) {
    if fields.is_empty() {
        return;
    }

    if !lines.is_empty() {
        lines.push(Line::from(""));
    }
    lines.push(section_line(title, palette));
    let label_width = fields
        .iter()
        .map(|(label, _)| label.len())
        .max()
        .unwrap_or(6);
    for (label, value) in fields {
        lines.push(preview_field_line(label, value, label_width, palette));
    }
}

fn section_line(title: &str, palette: theme::Palette) -> Line<'static> {
    Line::from(Span::styled(
        title.to_string(),
        Style::default().fg(palette.accent),
    ))
}

fn preview_field_line(
    label: &str,
    value: &str,
    label_width: usize,
    palette: theme::Palette,
) -> Line<'static> {
    Line::from(vec![
        Span::styled(
            format!("{label:<width$} ", width = label_width + 1),
            Style::default().fg(palette.muted),
        ),
        Span::styled(value.to_string(), Style::default().fg(palette.text)),
    ])
}

fn strip_iso_version_suffix(segment: &str) -> &str {
    let Some((base, suffix)) = segment.rsplit_once(';') else {
        return segment;
    };
    if !base.is_empty() && !suffix.is_empty() && suffix.chars().all(|ch| ch.is_ascii_digit()) {
        base
    } else {
        segment
    }
}
