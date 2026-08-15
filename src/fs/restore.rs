use anyhow::Context;
use std::{
    fs,
    path::{Path, PathBuf},
};

#[cfg(any(test, target_os = "macos"))]
use std::io::Write;

#[cfg(any(test, target_os = "macos"))]
static RESTORE_ORIGINS_PROCESS_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

// ---------------------------------------------------------------------------
// Restore from trash
// ---------------------------------------------------------------------------

/// Restores a trashed item to its original location.
///
/// Two backends are supported:
///
/// - **FreeDesktop trash** (Linux, BSD, and any macOS installation that uses
///   XDG tools): `entry_path` must be inside a `Trash/files/` directory and a
///   sibling `Trash/info/<name>.trashinfo` file must exist.  The original path
///   is read from that file and the item is moved back.
///
/// - **macOS `~/.Trash`**: Elio reads the original location from its own
///   restore-origins store, falling back to Finder's `.DS_Store` metadata,
///   then moves the item back directly.
///
/// The FreeDesktop path is tried first (it works even on macOS if the XDG
/// layout happens to be present), then the macOS path, then an unsupported
/// error for any other layout (e.g. Windows Recycle Bin).
pub(crate) fn restore_trash_item(entry_path: &Path) -> anyhow::Result<()> {
    if let Some(info_dir) = freedesktop_info_dir(entry_path) {
        return restore_trash_item_freedesktop(entry_path, info_dir);
    }

    // macOS: no .trashinfo metadata; use Elio's restore-origins store or
    // Finder's .DS_Store metadata.
    #[cfg(target_os = "macos")]
    {
        let file_name = restore_trash_item_macos(entry_path)?;
        remove_restore_origins(&[&file_name]);
        return Ok(());
    }

    // Any other layout (e.g. Windows Recycle Bin) is not supported.
    #[cfg(not(target_os = "macos"))]
    anyhow::bail!("restore is not supported for this trash location")
}

/// Returns the FreeDesktop `info/` directory when `entry_path` is inside the
/// sibling `files/` directory of a valid Trash layout.
fn freedesktop_info_dir(entry_path: &Path) -> Option<PathBuf> {
    // Requiring the immediate parent to be named `files` prevents a macOS
    // ~/.Trash item from being misdetected merely because ~/info exists.
    let parent = entry_path.parent()?;
    if parent.file_name()? != "files" {
        return None;
    }
    let info_dir = parent.parent()?.join("info");
    info_dir.is_dir().then_some(info_dir)
}

/// Restores as usual but checks macOS restore-origin cleanup. A returned error
/// means the filesystem restore completed and only metadata cleanup failed.
#[cfg(target_os = "macos")]
pub(crate) fn restore_trash_item_checked_metadata(
    entry_path: &Path,
) -> anyhow::Result<Option<anyhow::Error>> {
    if let Some(info_dir) = freedesktop_info_dir(entry_path) {
        restore_trash_item_freedesktop(entry_path, info_dir)?;
        return Ok(None);
    }

    let file_name = restore_trash_item_macos(entry_path)?;
    Ok(remove_restore_origins_checked(&[&file_name]).err())
}

/// FreeDesktop-specific restore: reads the `.trashinfo` sidecar and moves the
/// item back to its original path.
fn restore_trash_item_freedesktop(entry_path: &Path, info_dir: PathBuf) -> anyhow::Result<()> {
    let name = entry_path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| anyhow::anyhow!("cannot determine file name for {:?}", entry_path))?;

    let info_path = info_dir.join(format!("{name}.trashinfo"));
    let content =
        fs::read_to_string(&info_path).with_context(|| format!("cannot read {:?}", info_path))?;

    let original = super::trashinfo::parse_original_path(&content)
        .ok_or_else(|| anyhow::anyhow!("cannot parse original path from {:?}", info_path))?;

    if original.exists() {
        anyhow::bail!("destination already exists: {:?}", original);
    }

    if let Some(parent) = original.parent()
        && !parent.exists()
    {
        fs::create_dir_all(parent)
            .with_context(|| format!("cannot create parent dir {:?}", parent))?;
    }

    fs::rename(entry_path, &original)
        .with_context(|| format!("cannot move {:?} to {:?}", entry_path, original))?;

    let _ = fs::remove_file(&info_path);

    Ok(())
}

// ---------------------------------------------------------------------------
// macOS restore-origins store
// ---------------------------------------------------------------------------
// trash 5.2.6 defaults to asking Finder to move files to Trash through
// `osascript`. Finder's Put Back metadata is private and `.DS_Store` ptbL/ptbN
// records are not reliable enough for Elio's direct restore implementation.
//
// To work around this, whenever Elio trashes a file it immediately records
// the original path in its own JSON store at
//   ~/Library/Application Support/elio/trash-origins.json
// keyed by the actual filename assigned in ~/.Trash.  Restore checks this store
// first.  The DS_Store parser is kept as a fallback for files trashed
// directly by Finder (which does write ptbL).

/// Returns the path to the restore-origins metadata store.
#[cfg(target_os = "macos")]
fn restore_origins_path() -> Option<PathBuf> {
    dirs::data_dir().map(|d| d.join("elio").join("trash-origins.json"))
}

/// Records `(actual_trash_name, original_path)` pairs in the restore-origins
/// store. Finder may rename an item on collision, so callers must supply the
/// name observed after the Trash operation. All failures are reported.
#[cfg(target_os = "macos")]
pub(crate) fn save_restore_origins_checked(
    items: &[(String, PathBuf)],
) -> anyhow::Result<Vec<PathBuf>> {
    let Some(path) = restore_origins_path() else {
        anyhow::bail!("cannot determine restore-origins path");
    };
    save_restore_origins_at_path_checked(&path, items)
}

#[cfg(any(test, target_os = "macos"))]
fn save_restore_origins_at_path_checked(
    path: &Path,
    items: &[(String, PathBuf)],
) -> anyhow::Result<Vec<PathBuf>> {
    save_restore_origins_at_path_with(path, items, write_restore_origins_atomically)
}

#[cfg(any(test, target_os = "macos"))]
fn save_restore_origins_at_path_with(
    path: &Path,
    items: &[(String, PathBuf)],
    persist: impl FnOnce(&Path, &[u8]) -> std::io::Result<()>,
) -> anyhow::Result<Vec<PathBuf>> {
    if items.is_empty() {
        return Ok(Vec::new());
    }

    let mut additions = Vec::with_capacity(items.len());
    let mut rejected = Vec::new();
    for (name, original) in items {
        if let Some(original) = original.to_str() {
            additions.push((name.clone(), original.to_owned()));
        } else {
            rejected.push(original.clone());
        }
    }

    if additions.is_empty() {
        return Ok(rejected);
    }

    with_restore_origins_transaction(path, || {
        let mut map: std::collections::HashMap<String, String> = match fs::read(path) {
            Ok(bytes) => serde_json::from_slice(&bytes)
                .with_context(|| format!("cannot parse {:?}", path))?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Default::default(),
            Err(error) => return Err(error).with_context(|| format!("cannot read {:?}", path)),
        };
        map.extend(additions);

        let json = serde_json::to_vec_pretty(&map)
            .with_context(|| format!("cannot serialize {:?}", path))?;
        persist(path, &json).with_context(|| format!("cannot write {:?}", path))?;
        Ok(rejected)
    })
}

/// Removes exact `trash_names` from the restore-origins store. Finder-assigned
/// collision names are stored directly, so cleanup never infers another key.
/// Best-effort for historical normal/direct-root call sites.
#[cfg(target_os = "macos")]
pub(crate) fn remove_restore_origins(trash_names: &[&str]) {
    let _ = remove_restore_origins_checked(trash_names);
}

/// Checked variant used by the invoking-user helper. Missing metadata and
/// names with no matching entry are successful no-ops; malformed metadata and
/// real I/O failures are returned to the privileged parent.
#[cfg(target_os = "macos")]
pub(crate) fn remove_restore_origins_checked(trash_names: &[&str]) -> anyhow::Result<()> {
    let Some(path) = restore_origins_path() else {
        anyhow::bail!("cannot determine restore-origins path");
    };
    remove_restore_origins_at_path_checked(&path, trash_names)
}

#[cfg(any(test, target_os = "macos"))]
fn remove_restore_origins_at_path_checked(path: &Path, trash_names: &[&str]) -> anyhow::Result<()> {
    remove_restore_origins_at_path_with(path, trash_names, write_restore_origins_atomically)
}

#[cfg(any(test, target_os = "macos"))]
fn remove_restore_origins_at_path_with(
    path: &Path,
    trash_names: &[&str],
    persist: impl FnOnce(&Path, &[u8]) -> std::io::Result<()>,
) -> anyhow::Result<()> {
    with_restore_origins_transaction(path, || {
        let bytes = match fs::read(path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(error) => {
                return Err(error).with_context(|| format!("cannot read {:?}", path));
            }
        };
        let mut map: std::collections::HashMap<String, String> =
            serde_json::from_slice(&bytes).with_context(|| format!("cannot parse {:?}", path))?;
        if remove_from_origins_map(&mut map, trash_names) {
            let json = serde_json::to_vec_pretty(&map)
                .with_context(|| format!("cannot serialize {:?}", path))?;
            persist(path, &json).with_context(|| format!("cannot write {:?}", path))?;
        }
        Ok(())
    })
}

/// Core map-mutation logic for [`remove_restore_origins`]. Returns `true` if
/// at least one exact key was removed.
#[cfg(any(test, target_os = "macos"))]
fn remove_from_origins_map(
    map: &mut std::collections::HashMap<String, String>,
    trash_names: &[&str],
) -> bool {
    let mut changed = false;
    for &name in trash_names {
        if map.remove(name).is_some() {
            changed = true;
        }
    }
    changed
}

/// Looks up the original path for the exact filename currently in Trash.
#[cfg(target_os = "macos")]
fn load_restore_origin(trash_name: &str) -> Option<PathBuf> {
    let path = restore_origins_path()?;
    with_restore_origins_transaction(&path, || {
        let map: std::collections::HashMap<String, String> =
            serde_json::from_slice(&fs::read(&path)?)
                .with_context(|| format!("cannot parse {:?}", path))?;
        Ok(restore_origin_from_map(&map, trash_name))
    })
    .ok()
    .flatten()
}

#[cfg(any(test, target_os = "macos"))]
fn restore_origin_from_map(
    map: &std::collections::HashMap<String, String>,
    trash_name: &str,
) -> Option<PathBuf> {
    map.get(trash_name).map(PathBuf::from)
}

#[cfg(any(test, target_os = "macos"))]
/// Holds the complete metadata transaction under both a process-wide mutex
/// and a stable sidecar file lock shared by independent Elio/helper processes.
fn with_restore_origins_transaction<T>(
    path: &Path,
    operation: impl FnOnce() -> anyhow::Result<T>,
) -> anyhow::Result<T> {
    let _process_guard = RESTORE_ORIGINS_PROCESS_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)
        .with_context(|| format!("cannot create restore-origins directory {:?}", parent))?;

    let _file_guard = RestoreOriginsFileLock::acquire(path)?;

    operation()
}

#[cfg(any(test, target_os = "macos"))]
struct RestoreOriginsFileLock(fs::File);

#[cfg(any(test, target_os = "macos"))]
impl RestoreOriginsFileLock {
    fn acquire(path: &Path) -> anyhow::Result<Self> {
        let lock_path = restore_origins_lock_path(path)?;
        let mut options = fs::OpenOptions::new();
        options.read(true).write(true).create(true).truncate(false);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let file = options
            .open(&lock_path)
            .with_context(|| format!("cannot open restore-origins lock {:?}", lock_path))?;
        file.lock()
            .with_context(|| format!("cannot lock restore-origins store {:?}", lock_path))?;
        Ok(Self(file))
    }
}

#[cfg(any(test, target_os = "macos"))]
fn restore_origins_lock_path(path: &Path) -> anyhow::Result<PathBuf> {
    let file_name = path
        .file_name()
        .ok_or_else(|| anyhow::anyhow!("restore-origins path has no file name"))?;
    let mut lock_name = file_name.to_os_string();
    lock_name.push(".lock");
    Ok(path.with_file_name(lock_name))
}

#[cfg(any(test, target_os = "macos"))]
impl Drop for RestoreOriginsFileLock {
    fn drop(&mut self) {
        let _ = self.0.unlock();
    }
}

#[cfg(any(test, target_os = "macos"))]
fn write_restore_origins_atomically(path: &Path, contents: &[u8]) -> std::io::Result<()> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let file_name = path.file_name().unwrap_or_default().to_string_lossy();
    let process_id = std::process::id();

    for attempt in 0..100 {
        let temp_path = parent.join(format!(".{file_name}.elio-tmp-{process_id}-{attempt}"));
        let mut options = fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = match options.open(&temp_path) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        };

        let result = (|| {
            file.write_all(contents)?;
            file.sync_all()?;
            drop(file);

            #[cfg(windows)]
            if path.exists() {
                fs::remove_file(path)?;
            }
            fs::rename(&temp_path, path)?;

            #[cfg(unix)]
            fs::File::open(parent)?.sync_all()?;
            Ok(())
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temp_path);
        }
        return result;
    }

    Err(std::io::Error::new(
        std::io::ErrorKind::AlreadyExists,
        "could not create a temporary restore-origins file",
    ))
}

/// Moves `entry_path` to `original_path`, creating parent directories as
/// needed.  Shared by both restore paths (our store and DS_Store fallback).
#[cfg(target_os = "macos")]
fn perform_restore(entry_path: &Path, original_path: &Path) -> anyhow::Result<()> {
    if original_path.exists() {
        anyhow::bail!("destination already exists: {:?}", original_path);
    }
    if let Some(parent) = original_path.parent()
        && !parent.exists()
    {
        fs::create_dir_all(parent)
            .with_context(|| format!("cannot create parent dir {:?}", parent))?;
    }
    fs::rename(entry_path, original_path)
        .with_context(|| format!("cannot move {:?} to {:?}", entry_path, original_path))
}

/// macOS-specific restore.  Checks the Elio restore-origins store first
/// (populated whenever Elio trashes a file), then falls back to parsing
/// `.DS_Store` for files trashed directly by Finder.
#[cfg(target_os = "macos")]
fn restore_trash_item_macos(entry_path: &Path) -> anyhow::Result<String> {
    let file_name = entry_path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| anyhow::anyhow!("cannot determine file name for {:?}", entry_path))?;
    let trash_dir = entry_path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("cannot determine trash dir for {:?}", entry_path))?;
    let ds_store_path = trash_dir.join(".DS_Store");

    // Guard: never treat the metadata file itself as the item to restore.
    if entry_path == ds_store_path {
        anyhow::bail!("cannot restore \".DS_Store\" — it is a system metadata file");
    }

    // ── Primary: our own restore-origins store ──────────────────────────────
    if let Some(original_path) = load_restore_origin(file_name) {
        perform_restore(entry_path, &original_path)?;
        return Ok(file_name.to_owned());
    }

    // ── Fallback: parse .DS_Store written by Finder ─────────────────────────
    if !ds_store_path.exists() {
        anyhow::bail!(
            "no Put Back metadata found for \"{file_name}\" \
             (the file was not trashed via Finder or Elio)"
        );
    }

    let data =
        fs::read(&ds_store_path).with_context(|| format!("cannot read {:?}", ds_store_path))?;

    let (parent_dir, original_name) =
        macos_ds_store_find_ptb(&data, file_name).ok_or_else(|| {
            anyhow::anyhow!(
                "no Put Back metadata found for \"{file_name}\" \
                 (the file was not trashed via Finder or Elio)"
            )
        })?;

    // ptbL stores a volume-relative path without a leading slash.
    let original_path = if parent_dir.is_empty() {
        PathBuf::from(format!("/{original_name}"))
    } else {
        PathBuf::from(format!("/{parent_dir}/{original_name}"))
    };

    perform_restore(entry_path, &original_path)?;

    Ok(file_name.to_owned())
}

// ---------------------------------------------------------------------------
// macOS DS_Store parser
// ---------------------------------------------------------------------------
// When Finder moves a file to the Trash it writes `ptbL` (original parent
// directory, volume-relative, no leading slash) and `ptbN` (original file
// name, when renamed on collision) into the `.DS_Store` file in `~/.Trash`.
// These are the same records that Finder's "Put Back" command consults.
//
// DS_Store uses a buddy allocator to store a B-tree of
// (filename, property, type, value) records.  We parse just enough to locate
// ptbL/ptbN for the target filename without pulling in an external dependency.
// ---------------------------------------------------------------------------

/// Searches a `.DS_Store` binary for the `ptbL` (original parent directory)
/// and `ptbN` (original file name) records associated with `file_name`.
///
/// Returns `(parent_dir, original_name)` on success, where `parent_dir` is
/// volume-relative (no leading slash).  Returns `None` if the records are not
/// found or the binary cannot be parsed.
#[cfg(target_os = "macos")]
fn macos_ds_store_find_ptb(data: &[u8], file_name: &str) -> Option<(String, String)> {
    // ── Buddy-allocator header ──────────────────────────────────────────────
    // data[0..4]  — alignment marker \x00\x00\x00\x01
    // data[4..8]  — "Bud1" magic
    // data[8..12] — info_offset (u32 BE, relative to data[4..])
    // data[12..16]— info_size   (u32 BE)
    if data.len() < 36 || &data[4..8] != b"Bud1" {
        return None;
    }
    let info_offset = u32::from_be_bytes(data[8..12].try_into().ok()?) as usize;
    let info_size = u32::from_be_bytes(data[12..16].try_into().ok()?) as usize;

    let info_start = 4usize.checked_add(info_offset)?;
    let info_end = info_start.checked_add(info_size)?;
    if info_end > data.len() || info_end < info_start + 8 {
        return None;
    }
    let info = &data[info_start..info_end];

    // ── Offset table ────────────────────────────────────────────────────────
    // info[0..4]  — num_offsets (u32 BE)
    // info[4..8]  — 0x00000000 (padding)
    // info[8..]   — num_offsets × u32 BE block addresses
    let num_offsets = u32::from_be_bytes(info[0..4].try_into().ok()?) as usize;
    let table_bytes = num_offsets.checked_mul(4)?;
    let table_end = 8usize.checked_add(table_bytes)?;
    if table_end > info.len() {
        return None;
    }
    let mut offsets = Vec::with_capacity(num_offsets);
    for i in 0..num_offsets {
        let o = 8 + i * 4;
        offsets.push(u32::from_be_bytes(info[o..o + 4].try_into().ok()?));
    }

    // Pad offset table to next 256-entry boundary.
    let pad = (256usize.wrapping_sub(num_offsets % 256)) % 256;
    let toc_start = table_end.checked_add(pad.checked_mul(4)?)?;

    // ── Table of Contents ───────────────────────────────────────────────────
    // toc[0..4]  — num_entries (u32 BE)
    // toc[4..]   — entries: name_len (u8) + name + block_id (u32 BE)
    if toc_start + 4 > info.len() {
        return None;
    }
    let num_toc = u32::from_be_bytes(info[toc_start..toc_start + 4].try_into().ok()?) as usize;

    let mut pos = toc_start + 4;
    let mut dsdb_block_id: Option<u32> = None;
    for _ in 0..num_toc {
        if pos >= info.len() {
            return None;
        }
        let name_len = info[pos] as usize;
        pos += 1;
        let name_end = pos.checked_add(name_len)?;
        if name_end + 4 > info.len() {
            return None;
        }
        let toc_name = std::str::from_utf8(&info[pos..name_end]).ok()?;
        let block_id = u32::from_be_bytes(info[name_end..name_end + 4].try_into().ok()?);
        if toc_name == "DSDB" {
            dsdb_block_id = Some(block_id);
        }
        pos = name_end + 4;
    }

    // ── DSDB block → root B-tree node ───────────────────────────────────────
    let dsdb_block = ds_store_block(data, &offsets, dsdb_block_id?)?;
    if dsdb_block.len() < 4 {
        return None;
    }
    let root_node = u32::from_be_bytes(dsdb_block[0..4].try_into().ok()?);

    // ── Traverse B-tree ─────────────────────────────────────────────────────
    let mut ptbl: Option<String> = None;
    let mut ptbn: Option<String> = None;
    let mut visited = std::collections::HashSet::new();
    ds_store_traverse(
        data,
        &offsets,
        root_node,
        file_name,
        &mut ptbl,
        &mut ptbn,
        &mut visited,
    )?;

    match (ptbl, ptbn) {
        (Some(l), Some(n)) => Some((l, n)),
        // ptbN is absent when the file name was not changed on trashing.
        (Some(l), None) => Some((l, file_name.to_owned())),
        _ => None,
    }
}

/// Returns the payload slice for the given block ID, or `None` on any error.
///
/// Block address encoding: `offset = addr & !0x1f` (absolute in `data`),
/// `size = 1 << (addr & 0x1f)`.  The 4 bytes at `data[offset..]` are a
/// block size header; the payload starts at `data[offset + 4..]`.
#[cfg(target_os = "macos")]
fn ds_store_block<'a>(data: &'a [u8], offsets: &[u32], id: u32) -> Option<&'a [u8]> {
    let addr = *offsets.get(id as usize)?;
    if addr == 0 {
        return None;
    }
    let offset = (addr & !0x1f) as usize;
    let size = 1usize << (addr & 0x1f);
    let start = offset.checked_add(4)?;
    let end = start.checked_add(size)?;
    if end > data.len() {
        return None;
    }
    Some(&data[start..end])
}

/// Recursively traverses a B-tree node, collecting `ptbL`/`ptbN` values for
/// `target_name`.  Returns `None` on any parse error.
#[cfg(target_os = "macos")]
fn ds_store_traverse(
    data: &[u8],
    offsets: &[u32],
    node_id: u32,
    target_name: &str,
    ptbl: &mut Option<String>,
    ptbn: &mut Option<String>,
    visited: &mut std::collections::HashSet<u32>,
) -> Option<()> {
    // Guard against cycles in corrupt DS_Store files — skip silently, don't abort.
    if !visited.insert(node_id) {
        return Some(());
    }

    let block = ds_store_block(data, offsets, node_id)?;
    let mut cur = DsStoreCursor::new(block);

    let pair_count = cur.read_u32()?;

    if pair_count == 0 {
        // Leaf node: record count then records.
        let record_count = cur.read_u32()?;
        for _ in 0..record_count {
            // Unknown type in a record means we can't determine its size and
            // must stop reading this node, but don't abort the whole traversal.
            if ds_store_read_record(&mut cur, target_name, ptbl, ptbn).is_none() {
                break;
            }
        }
    } else {
        // Internal node: alternating child_id and record, then one final child.
        for _ in 0..pair_count {
            let child_id = cur.read_u32()?;
            // Child failures don't corrupt our cursor — skip and continue.
            ds_store_traverse(data, offsets, child_id, target_name, ptbl, ptbn, visited);
            // Record failure means we can't find the boundary of this record,
            // so we can't safely continue reading this node.
            if ds_store_read_record(&mut cur, target_name, ptbl, ptbn).is_none() {
                return Some(());
            }
        }
        let last_child = cur.read_u32()?;
        ds_store_traverse(data, offsets, last_child, target_name, ptbl, ptbn, visited);
    }

    Some(())
}

/// Reads one B-tree record and, if it belongs to `target_name`, stores the
/// `ptbL` or `ptbN` value.  Returns `None` on any parse error.
#[cfg(target_os = "macos")]
fn ds_store_read_record(
    cur: &mut DsStoreCursor<'_>,
    target_name: &str,
    ptbl: &mut Option<String>,
    ptbn: &mut Option<String>,
) -> Option<()> {
    // Filename: u32 code-unit count + UTF-16BE data.
    let name_len = cur.read_u32()? as usize;
    let name_bytes = cur.read_bytes(name_len * 2)?;
    let name = decode_utf16be(name_bytes)?;

    // Property code and type code (4 ASCII bytes each).
    let prop4: [u8; 4] = cur.read_bytes(4)?.try_into().ok()?;
    let typ4: [u8; 4] = cur.read_bytes(4)?.try_into().ok()?;

    let is_target = name == target_name;
    let is_ptbl = prop4 == *b"ptbL";
    let is_ptbn = prop4 == *b"ptbN";

    match (&prop4, &typ4) {
        (_, b"ustr") => {
            let val_len = cur.read_u32()? as usize;
            let val_bytes = cur.read_bytes(val_len * 2)?;
            if is_target && (is_ptbl || is_ptbn) {
                let val = decode_utf16be(val_bytes)?;
                if is_ptbl {
                    *ptbl = Some(val);
                } else {
                    *ptbn = Some(val);
                }
            }
        }
        (_, b"bool") => {
            cur.skip(1)?;
        }
        (_, b"shor") => {
            cur.skip(2)?;
        }
        (_, b"long") | (_, b"type") => {
            cur.skip(4)?;
        }
        (_, b"comp") | (_, b"dutc") => {
            cur.skip(8)?;
        }
        // BKGD blob has no length prefix — it is always exactly 12 bytes.
        (b"BKGD", b"blob") => {
            cur.skip(12)?;
        }
        (_, b"blob") => {
            let len = cur.read_u32()? as usize;
            cur.skip(len)?;
        }
        _ => {
            // Unknown type — cannot determine record size, so abort traversal.
            return None;
        }
    }

    Some(())
}

/// Cursor over a `&[u8]` slice with big-endian integer reads.
#[cfg(target_os = "macos")]
struct DsStoreCursor<'a> {
    data: &'a [u8],
    pos: usize,
}

#[cfg(target_os = "macos")]
impl<'a> DsStoreCursor<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    fn skip(&mut self, n: usize) -> Option<()> {
        let end = self.pos.checked_add(n)?;
        if end > self.data.len() {
            return None;
        }
        self.pos = end;
        Some(())
    }

    fn read_u32(&mut self) -> Option<u32> {
        let b = self.read_bytes(4)?;
        Some(u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
    }

    fn read_bytes(&mut self, n: usize) -> Option<&'a [u8]> {
        let end = self.pos.checked_add(n)?;
        if end > self.data.len() {
            return None;
        }
        let slice = &self.data[self.pos..end];
        self.pos = end;
        Some(slice)
    }
}

/// Decodes a UTF-16BE byte sequence into a `String`.
/// Returns `None` if the byte count is odd or the data is not valid UTF-16.
#[cfg(target_os = "macos")]
fn decode_utf16be(bytes: &[u8]) -> Option<String> {
    if bytes.len() % 2 != 0 {
        return None;
    }
    let units: Vec<u16> = bytes
        .chunks_exact(2)
        .map(|c| u16::from_be_bytes([c[0], c[1]]))
        .collect();
    String::from_utf16(&units).ok()
}

#[cfg(test)]
mod tests;
