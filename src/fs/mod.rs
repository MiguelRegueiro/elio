pub(crate) mod duplicates;
pub(crate) mod format;
pub(crate) mod item_count;
pub(crate) mod places;
pub(crate) mod search;
pub(crate) mod watch;

mod directory;
mod directory_stats;
mod opener;
mod restore;
mod sort;
mod trashinfo;

fn is_hidden(file_name: &std::ffi::OsStr) -> bool {
    file_name.to_string_lossy().starts_with('.')
}

/// Returns `true` if the directory entry should be treated as hidden.
///
/// On all platforms, entries whose names begin with `.` are hidden.
/// On Windows, entries with the `FILE_ATTRIBUTE_HIDDEN` attribute are also hidden.
fn is_hidden_entry(entry: &std::fs::DirEntry) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_HIDDEN: u32 = 0x2;
        if entry
            .metadata()
            .is_ok_and(|m| m.file_attributes() & FILE_ATTRIBUTE_HIDDEN != 0)
        {
            return true;
        }
    }
    is_hidden(entry.file_name().as_os_str())
}

pub(crate) use directory::{
    DirectoryFingerprint, DirectorySnapshot, entry_from_path, load_directory_snapshot,
    load_directory_snapshot_cancellable, scan_directory_fingerprint_cancellable,
};
pub(crate) use directory_stats::{DirectoryStats, DirectoryStatsScanResult, scan_directory_stats};
pub(crate) use format::{
    describe_io_error, format_item_count, format_size, format_size_parts, format_time_ago,
    rect_contains, sanitize_terminal_text, symlink_target_display_label,
};
pub(crate) use item_count::count_directory_items;
#[cfg(test)]
pub(crate) use opener::set_open_in_system_capture_for_test;
pub(crate) use opener::{detached_open_command, open_in_system};
pub(crate) use places::{build_sidebar_rows, trash_dir};
pub(crate) use restore::restore_trash_item;
#[cfg(target_os = "macos")]
pub(crate) use restore::{remove_restore_origins, save_restore_origins};
pub(crate) use sort::natural_cmp;
pub(crate) use watch::{
    DirectoryWatchEvent, DirectoryWatcher, directory_watch_debounce, event_affects_visible_entries,
    start_directory_watcher,
};
