use super::*;
use std::{
    fs,
    path::{Path, PathBuf},
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};

#[cfg(target_os = "linux")]
use std::process::{Command, Stdio};

/// Minimum time between intermediate progress results sent to the UI.
/// Only applies to permanent delete, which processes files one at a time.
/// Non-permanent trash is a single batched OS call with no intermediate
/// progress.
const PROGRESS_SEND_INTERVAL: Duration = Duration::from_millis(80);

#[cfg(target_os = "linux")]
const GIO_TRASH_ARG_BUDGET: usize = 128 * 1024;

#[cfg(target_os = "linux")]
const GIO_TRASH_COMMAND_OVERHEAD: usize = "gio".len() + 1 + "trash".len() + 1 + "--".len() + 1;

pub(in crate::app::jobs) struct TrashPool {
    shared: Arc<TrashShared>,
    workers: Vec<thread::JoinHandle<()>>,
}

struct TrashShared {
    state: Mutex<TrashState>,
    available: Condvar,
    cancelled: AtomicBool,
    cancel_token: AtomicU64,
}

struct TrashState {
    pending: Option<TrashRequest>,
    active: bool,
    closed: bool,
}

impl TrashPool {
    pub(in crate::app::jobs) fn new(result_tx: mpsc::Sender<JobResult>) -> Self {
        let shared = Arc::new(TrashShared {
            state: Mutex::new(TrashState {
                pending: None,
                active: false,
                closed: false,
            }),
            available: Condvar::new(),
            cancelled: AtomicBool::new(false),
            cancel_token: AtomicU64::new(0),
        });
        let shared_worker = Arc::clone(&shared);
        let worker = thread::spawn(move || {
            while let Some(request) = TrashShared::pop(&shared_worker) {
                TrashShared::set_active(&shared_worker, true);
                let (completed, errors, stopped_early) = run_trash(
                    &request,
                    &result_tx,
                    &shared_worker.cancelled,
                    &shared_worker.cancel_token,
                );
                TrashShared::set_active(&shared_worker, false);

                let verb = if request.permanent {
                    "Permanently deleted"
                } else {
                    "Trashed"
                };
                let total = request.targets.len();
                let single_name = (total == 1).then(|| request.targets[0].name.as_str());
                let status = if stopped_early && errors.is_empty() {
                    match completed {
                        0 => "Trash cancelled".to_string(),
                        1 => format!("Trash cancelled — {verb} 1 item"),
                        n => format!("Trash cancelled — {verb} {n} items"),
                    }
                } else if stopped_early {
                    // Cancelled but some items also errored — surface the errors.
                    // Errors can come from either the direct remove path or staged
                    // cleanup, so use a neutral label rather than "cleanup error".
                    let base = match completed {
                        0 => "Trash cancelled".to_string(),
                        1 => format!("Trash cancelled — {verb} 1 item"),
                        n => format!("Trash cancelled — {verb} {n} items"),
                    };
                    format!("{base}; {} error(s) — first: {}", errors.len(), errors[0])
                } else if errors.is_empty() {
                    match (completed, single_name) {
                        (0, _) => "Nothing was deleted".to_string(),
                        (1, Some(name)) => format!("{verb} \"{name}\""),
                        (n, _) => format!("{verb} {n} items"),
                    }
                } else if completed == 0 {
                    if errors.len() == 1 {
                        errors[0].clone()
                    } else {
                        format!("{} errors — first: {}", errors.len(), errors[0])
                    }
                } else {
                    format!(
                        "{verb} {completed} item(s); {} error(s) — first: {}",
                        errors.len(),
                        errors[0]
                    )
                };

                if result_tx
                    .send(JobResult::Trash(TrashBuild {
                        token: request.token,
                        completed,
                        done: true,
                        status: Some(status),
                    }))
                    .is_err()
                {
                    break;
                }
            }
        });
        Self {
            shared,
            workers: vec![worker],
        }
    }

    pub(in crate::app::jobs) fn submit(&self, request: TrashRequest) -> bool {
        let mut state = lock_unpoison(&self.shared.state);
        if state.closed {
            return false;
        }
        state.pending = Some(request);
        self.shared.available.notify_one();
        true
    }

    /// Signal the worker to stop after the current item if it is processing
    /// the trash request with the given token.  A concurrent or future request
    /// with a different token is unaffected.
    pub(in crate::app::jobs) fn cancel_trash(&self, token: u64) {
        self.shared.cancel_token.store(token, Ordering::Relaxed);
    }

    pub(in crate::app::jobs) fn has_pending_work(&self) -> bool {
        let state = lock_unpoison(&self.shared.state);
        state.pending.is_some() || state.active
    }
}

impl Drop for TrashPool {
    fn drop(&mut self) {
        {
            let mut state = lock_unpoison(&self.shared.state);
            state.closed = true;
            // Do NOT clear `pending` and do NOT set `cancelled`: the worker must
            // finish any in-flight and queued requests completely before exiting.
            // Setting `cancelled` here (as PastePool does) would abandon targets
            // mid-batch and leave them neither deleted nor untouched, which is
            // worse than a momentary delay on exit.
        }
        self.shared.available.notify_all();
        for worker in self.workers.drain(..) {
            let _ = worker.join();
        }
    }
}

impl TrashShared {
    fn pop(shared: &Arc<Self>) -> Option<TrashRequest> {
        let mut state = lock_unpoison(&shared.state);
        loop {
            // Drain any queued request before honoring the close signal so
            // that a pending job submitted just before shutdown is not lost.
            if let Some(request) = state.pending.take() {
                return Some(request);
            }
            if state.closed {
                return None;
            }
            state = wait_unpoison(&shared.available, state);
        }
    }

    fn set_active(shared: &Arc<Self>, active: bool) {
        lock_unpoison(&shared.state).active = active;
    }
}

fn run_trash(
    request: &TrashRequest,
    result_tx: &mpsc::Sender<JobResult>,
    cancelled: &AtomicBool,
    cancel_token: &AtomicU64,
) -> (usize, Vec<String>, bool) {
    if request.permanent {
        run_permanent_delete(request, result_tx, cancelled, cancel_token)
    } else {
        run_trash_batch(request, cancelled, cancel_token)
    }
}

/// Delete each target permanently.
///
/// Directories are first renamed into a staging area on the same filesystem
/// (O(1), atomic), counted as completed immediately, then deleted in parallel
/// background workers with bounded concurrency.  This makes large-directory
/// deletion appear instant to the user while the actual unlink work happens
/// concurrently.  Files are removed in-place with `remove_file` as before.
///
/// Cancellation stops processing new targets but always joins all staged
/// cleanup workers before returning, so no staging entries are orphaned by
/// a clean cancel.  If the process is killed before cleanup finishes, the
/// startup sweep will reclaim leftover staging entries on next launch.
///
/// Sends throttled intermediate progress results so the UI chip updates
/// during long operations.
fn run_permanent_delete(
    request: &TrashRequest,
    result_tx: &mpsc::Sender<JobResult>,
    cancelled: &AtomicBool,
    cancel_token: &AtomicU64,
) -> (usize, Vec<String>, bool) {
    let staging = staging_dir();
    let mut staged: Vec<(String, PathBuf)> = Vec::new();
    let mut completed = 0usize;
    let mut errors: Vec<String> = Vec::new();
    let mut stopped_early = false;
    let mut last_progress_at: Option<Instant> = None;
    // Collect names of items successfully removed from trash so their restore
    // origins can be pruned after the loop.  Staged directories are included
    // here even if their background cleanup later fails: the item is already
    // gone from the trash regardless of whether the staging area is reclaimed.
    #[cfg(target_os = "macos")]
    let mut deleted_names: Vec<&str> = Vec::new();

    for target in &request.targets {
        if cancelled.load(Ordering::Relaxed)
            || cancel_token.load(Ordering::Relaxed) == request.token
        {
            stopped_early = true;
            break;
        }

        if target.is_dir {
            // Try rename-to-staging first.  If staging is unavailable or the
            // rename fails (wrong filesystem, permissions), fall back to an
            // in-place remove_dir_all.
            match staging
                .as_ref()
                .and_then(|s| rename_into_staging(&target.path, s))
            {
                Some(staged_path) => {
                    staged.push((target.name.clone(), staged_path));
                    completed += 1;
                    #[cfg(target_os = "macos")]
                    deleted_names.push(target.name.as_str());
                }
                None => match fs::remove_dir_all(&target.path) {
                    Ok(()) => {
                        completed += 1;
                        #[cfg(target_os = "macos")]
                        deleted_names.push(target.name.as_str());
                    }
                    Err(e) => {
                        errors.push(format!("Could not delete \"{}\": {e}", target.name));
                    }
                },
            }
        } else {
            match fs::remove_file(&target.path) {
                Ok(()) => {
                    completed += 1;
                    #[cfg(target_os = "macos")]
                    deleted_names.push(target.name.as_str());
                }
                Err(e) => errors.push(format!("Could not delete \"{}\": {e}", target.name)),
            }
        }

        if !send_trash_progress(result_tx, request.token, completed, &mut last_progress_at) {
            break;
        }
    }

    // Drain all staged directories even when stopped early — they are already
    // gone from the user's view and must be fully reclaimed before we return.
    // Any cleanup failures are surfaced as errors and the item is no longer
    // counted as completed, so the final status accurately reflects reality.
    let cleanup_errors = run_staged_cleanup(staged);
    completed = completed.saturating_sub(cleanup_errors.len());
    for name in cleanup_errors {
        errors.push(format!("Could not delete \"{name}\": cleanup failed"));
    }

    #[cfg(target_os = "macos")]
    if !deleted_names.is_empty() {
        crate::fs::remove_restore_origins(&deleted_names);
    }

    (completed, errors, stopped_early)
}

/// Returns the path used as a staging area for rename-first directory deletion.
///
/// Uses a PID-scoped subdirectory of the XDG data dir:
/// `~/.local/share/elio/cleanup/{pid}/` on Linux.  Scoping by PID means the
/// startup sweep can identify and reclaim entries from *previous* sessions by
/// checking whether the subdirectory name matches the current PID, with no
/// risk of a false positive from a file whose base name happens to embed the
/// same number.  The parent dir (`cleanup/`) is on the same filesystem as the
/// home trash so `rename(2)` never crosses a device boundary.
fn staging_root_for(data_dir: Option<PathBuf>, effective_uid: u32) -> Option<PathBuf> {
    if effective_uid == 0 {
        return None;
    }
    data_dir.map(|dir| dir.join("elio").join("cleanup"))
}

fn staging_root() -> Option<PathBuf> {
    #[cfg(unix)]
    let effective_uid = unsafe { libc::geteuid() };
    #[cfg(not(unix))]
    let effective_uid = 1;
    staging_root_for(dirs::data_dir(), effective_uid)
}

fn staging_dir() -> Option<PathBuf> {
    let pid = std::process::id();
    staging_root().map(|root| root.join(pid.to_string()))
}

/// Atomically moves `path` into `staging`, giving it a unique name.
/// Returns `None` if the staging directory cannot be created or the rename
/// fails (e.g. cross-device — should not happen given `staging_dir()`'s
/// placement, but handled defensively).
fn rename_into_staging(path: &Path, staging: &Path) -> Option<PathBuf> {
    fs::create_dir_all(staging).ok()?;
    let base = path.file_name().and_then(|n| n.to_str()).unwrap_or("dir");
    let pid = std::process::id();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let dest = staging.join(format!("{base}-{pid}-{nanos}"));
    fs::rename(path, &dest).ok()?;
    Some(dest)
}

/// Deletes all staged directories using a bounded worker pool.
/// Cap: `min(available_parallelism, 4)`.
///
/// Returns the original names of any directories that could not be deleted,
/// so the caller can decrement `completed` and surface them as errors.
fn run_staged_cleanup(staged: Vec<(String, PathBuf)>) -> Vec<String> {
    if staged.is_empty() {
        return Vec::new();
    }
    let cap = thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1)
        .min(4)
        .min(staged.len());

    let (tx, rx) = mpsc::channel::<(String, PathBuf)>();
    let rx = Arc::new(Mutex::new(rx));

    // Each worker sends back the name on failure.
    let (err_tx, err_rx) = mpsc::channel::<String>();

    let workers: Vec<_> = (0..cap)
        .map(|_| {
            let rx = Arc::clone(&rx);
            let err_tx = err_tx.clone();
            thread::spawn(move || {
                while let Ok((name, path)) = rx.lock().unwrap().recv() {
                    if fs::remove_dir_all(&path).is_err() {
                        let _ = err_tx.send(name);
                    }
                }
            })
        })
        .collect();
    drop(err_tx);

    for item in staged {
        let _ = tx.send(item);
    }
    drop(tx);

    for w in workers {
        let _ = w.join();
    }

    err_rx.iter().collect()
}

/// Spawns a background thread that sweeps any PID subdirectories left in the
/// staging root from sessions that were killed before cleanup could finish.
/// Best-effort: errors are silently ignored.
pub(in crate::app::jobs) fn sweep_staging_on_startup() {
    let current_pid = std::process::id();
    let Some(cleanup_root) = staging_root() else {
        return;
    };
    // If cleanup_root/{current_pid}/ already exists at startup it must be a
    // leftover from a previous session whose PID the OS reused.  Delete it
    // synchronously now, before this session ever calls rename_into_staging,
    // so the async sweep (which skips the current-pid subdir to avoid racing
    // with live staged deletes) cannot leave it behind.
    let current_pid_dir = cleanup_root.join(current_pid.to_string());
    if current_pid_dir.exists() {
        let _ = fs::remove_dir_all(&current_pid_dir);
    }
    thread::spawn(move || sweep_staging_dir(&cleanup_root, current_pid));
}

/// Sweeps PID subdirectories inside `cleanup_root` that do not match
/// `current_pid` and whose owner process is no longer running.
///
/// Each session writes into `cleanup_root/{pid}/`.  Before deleting a
/// directory whose name is a PID other than `current_pid`, the sweep checks
/// whether a process with that PID still exists.  If it does, the directory
/// belongs to a concurrently running instance and must not be touched.  Only
/// directories whose owning process is confirmed dead are removed.
///
/// The current-pid subdir is skipped entirely: it may be populated by a
/// concurrent permanent delete in this session, and any stale dir with the
/// same PID was already removed synchronously in `sweep_staging_on_startup`.
/// Best-effort: individual entry errors are silently ignored.
fn sweep_staging_dir(cleanup_root: &Path, current_pid: u32) {
    let current_pid_str = current_pid.to_string();
    let Ok(entries) = fs::read_dir(cleanup_root) else {
        return;
    };
    for entry in entries.flatten() {
        let p = entry.path();
        let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if name == current_pid_str {
            continue;
        }
        // Only remove directories whose owning process is dead.  This prevents
        // one instance from deleting another live instance's staged dirs.
        if let Ok(pid) = name.parse::<u32>()
            && pid_is_alive(pid)
        {
            continue;
        }
        if p.is_dir() {
            let _ = fs::remove_dir_all(&p);
        }
    }
}

/// Returns `true` if a process with `pid` is currently running.
///
/// On Unix, `kill(pid, 0)` probes for process existence without delivering a
/// signal: it returns 0 if the process exists (even if we lack permission to
/// signal it — `EPERM` still means the process is alive).
///
/// On non-Unix platforms, conservatively returns `true` so the sweep never
/// deletes a directory it cannot safely prove is stale.  Those directories
/// will be reclaimed on a future run once the OS has recycled the PID.
#[cfg(unix)]
fn pid_is_alive(pid: u32) -> bool {
    // SAFETY: kill(2) is async-signal-safe and has no preconditions beyond a
    // valid pid_t value.  Signal 0 never delivers a signal.
    let ret = unsafe { libc::kill(pid as libc::pid_t, 0) };
    if ret == 0 {
        return true;
    }
    // ESRCH means no such process; any other error (e.g. EPERM) means it exists.
    // Use std::io::Error::last_os_error() to read errno portably across all
    // Unix platforms (Linux, macOS, FreeBSD) rather than a glibc-specific symbol.
    std::io::Error::last_os_error().raw_os_error() != Some(libc::ESRCH)
}

#[cfg(not(unix))]
fn pid_is_alive(_pid: u32) -> bool {
    true
}

/// Move all targets to the OS trash in a single batched operation.
///
/// On Linux, prefer `gio trash` so Elio uses the same Freedesktop/GVfs trash
/// backend as GNOME Files and other desktop-aware tools.  The Rust `trash`
/// crate remains the fallback for systems without GIO, unsupported mounts, or
/// command failures that leave sources untouched.
///
/// Cancellation is checked once before the batch starts.  The batch itself
/// is treated as atomic from Elio's perspective: once the OS call is in
/// flight it cannot be interrupted mid-way.  No intermediate progress
/// results are sent; the UI chip stays at 0/N until the call returns.
fn run_trash_batch(
    request: &TrashRequest,
    cancelled: &AtomicBool,
    cancel_token: &AtomicU64,
) -> (usize, Vec<String>, bool) {
    if cancelled.load(Ordering::Relaxed) || cancel_token.load(Ordering::Relaxed) == request.token {
        return (0, Vec::new(), true);
    }

    let paths: Vec<_> = request.targets.iter().map(|t| t.path.as_path()).collect();
    let total = paths.len();

    #[cfg(all(unix, not(target_os = "macos")))]
    if let Some(result) = trash_as_invoking_user(&paths) {
        return result;
    }

    match trash_with_system_backend(&paths) {
        TrashBatchBackendResult::Completed => {
            #[cfg(target_os = "macos")]
            {
                let origins: Vec<(String, std::path::PathBuf)> = request
                    .targets
                    .iter()
                    .map(|t| (t.name.clone(), t.path.clone()))
                    .collect();
                crate::fs::save_restore_origins(&origins);
            }
            (total, Vec::new(), false)
        }
        TrashBatchBackendResult::Failed { completed, error } => (completed, vec![error], false),
    }
}

#[derive(Debug, Eq, PartialEq)]
enum TrashBatchBackendResult {
    Completed,
    Failed { completed: usize, error: String },
}

#[cfg(all(unix, not(target_os = "macos")))]
fn trash_as_invoking_user(paths: &[&Path]) -> Option<(usize, Vec<String>, bool)> {
    use crate::{
        config::{InvocationContext, invoking_user_context},
        user_fs_helper::Request,
    };

    match invoking_user_context() {
        InvocationContext::Normal => None,
        InvocationContext::ElevatedUnresolved => Some((
            0,
            vec!["Could not resolve invoking user; nothing was trashed".to_string()],
            false,
        )),
        InvocationContext::Elevated(user) => {
            let owned = paths.iter().map(|path| (*path).to_path_buf()).collect();
            let response = super::super::invoking_user_fs::run(&user, &Request::Trash(owned));
            Some(match response {
                Ok(response) => (
                    response.completed,
                    response.error.into_iter().collect(),
                    false,
                ),
                Err(error) => (0, vec![format!("Could not trash items: {error}")], false),
            })
        }
    }
}

#[cfg(unix)]
pub(crate) fn run_user_trash_helper(paths: &[PathBuf]) -> crate::user_fs_helper::Response {
    let refs: Vec<_> = paths.iter().map(PathBuf::as_path).collect();
    match trash_with_system_backend(&refs) {
        TrashBatchBackendResult::Completed => crate::user_fs_helper::Response {
            completed: paths.len(),
            error: None,
        },
        TrashBatchBackendResult::Failed { completed, error } => crate::user_fs_helper::Response {
            completed,
            error: Some(error),
        },
    }
}

fn trash_with_system_backend(paths: &[&Path]) -> TrashBatchBackendResult {
    #[cfg(target_os = "linux")]
    {
        trash_with_gio_first(paths)
    }

    #[cfg(not(target_os = "linux"))]
    match trash_with_crate(paths) {
        Ok(()) => TrashBatchBackendResult::Completed,
        Err(error) => TrashBatchBackendResult::Failed {
            completed: 0,
            error,
        },
    }
}

fn trash_with_crate(paths: &[&Path]) -> Result<(), String> {
    ::trash::delete_all(paths.iter().copied()).map_err(|e| e.to_string())
}

#[cfg(target_os = "linux")]
#[derive(Debug, Eq, PartialEq)]
enum GioTrashCommandResult {
    Completed,
    Unavailable,
    Failed(String),
}

#[cfg(target_os = "linux")]
fn trash_with_gio_first(paths: &[&Path]) -> TrashBatchBackendResult {
    trash_with_gio_runner(paths, run_gio_trash_command, trash_with_crate)
}

#[cfg(target_os = "linux")]
fn trash_with_gio_runner<G, F>(
    paths: &[&Path],
    mut run_gio: G,
    mut run_fallback: F,
) -> TrashBatchBackendResult
where
    G: FnMut(&[&Path]) -> GioTrashCommandResult,
    F: FnMut(&[&Path]) -> Result<(), String>,
{
    for range in gio_trash_chunks(paths) {
        match run_gio(&paths[range.clone()]) {
            GioTrashCommandResult::Completed => {}
            GioTrashCommandResult::Unavailable => {
                return finish_gio_unavailable_fallback(
                    paths,
                    range.start,
                    "gio trash is not available",
                    |p| run_fallback(p),
                );
            }
            GioTrashCommandResult::Failed(error) => {
                return finish_gio_failure_fallback(paths, range, &error, |p| run_fallback(p));
            }
        }
    }

    TrashBatchBackendResult::Completed
}

#[cfg(target_os = "linux")]
fn finish_gio_unavailable_fallback<F>(
    paths: &[&Path],
    remaining_start: usize,
    gio_error: &str,
    mut run_fallback: F,
) -> TrashBatchBackendResult
where
    F: FnMut(&[&Path]) -> Result<(), String>,
{
    finish_with_fallback(
        paths[remaining_start..].to_vec(),
        remaining_start,
        gio_error,
        |p| run_fallback(p),
    )
}

#[cfg(target_os = "linux")]
fn finish_gio_failure_fallback<F>(
    paths: &[&Path],
    failed_range: std::ops::Range<usize>,
    gio_error: &str,
    mut run_fallback: F,
) -> TrashBatchBackendResult
where
    F: FnMut(&[&Path]) -> Result<(), String>,
{
    let mut completed = failed_range.start;
    let mut remaining = Vec::new();

    for path in &paths[failed_range.clone()] {
        if path.exists() {
            remaining.push(*path);
        } else {
            completed += 1;
        }
    }
    remaining.extend_from_slice(&paths[failed_range.end..]);

    finish_with_fallback(remaining, completed, gio_error, |p| run_fallback(p))
}

#[cfg(target_os = "linux")]
fn finish_with_fallback<F>(
    remaining: Vec<&Path>,
    completed: usize,
    gio_error: &str,
    mut run_fallback: F,
) -> TrashBatchBackendResult
where
    F: FnMut(&[&Path]) -> Result<(), String>,
{
    if remaining.is_empty() {
        return TrashBatchBackendResult::Completed;
    }

    run_fallback(&remaining).map_or_else(
        |fallback_error| TrashBatchBackendResult::Failed {
            completed,
            error: format!(
                "Could not trash all items: {gio_error}; fallback failed: {fallback_error}"
            ),
        },
        |()| TrashBatchBackendResult::Completed,
    )
}

#[cfg(target_os = "linux")]
fn run_gio_trash_command(paths: &[&Path]) -> GioTrashCommandResult {
    if paths.is_empty() {
        return GioTrashCommandResult::Completed;
    }

    let output = Command::new("gio")
        .arg("trash")
        .arg("--")
        .args(paths)
        .stdin(Stdio::null())
        .output();

    match output {
        Ok(output) if output.status.success() => GioTrashCommandResult::Completed,
        Ok(output) => GioTrashCommandResult::Failed(format!(
            "gio trash failed{}",
            command_output_message(&output.stderr, &output.stdout)
        )),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            GioTrashCommandResult::Unavailable
        }
        Err(error) => GioTrashCommandResult::Failed(format!("could not start gio trash: {error}")),
    }
}

#[cfg(target_os = "linux")]
fn command_output_message(stderr: &[u8], stdout: &[u8]) -> String {
    let bytes = if stderr.is_empty() { stdout } else { stderr };
    let text = String::from_utf8_lossy(bytes).trim().to_string();
    if text.is_empty() {
        String::new()
    } else {
        format!(": {text}")
    }
}

#[cfg(target_os = "linux")]
fn gio_trash_chunks(paths: &[&Path]) -> Vec<std::ops::Range<usize>> {
    gio_trash_chunks_with_budget(paths, GIO_TRASH_ARG_BUDGET)
}

#[cfg(target_os = "linux")]
fn gio_trash_chunks_with_budget(paths: &[&Path], budget: usize) -> Vec<std::ops::Range<usize>> {
    if paths.is_empty() {
        return Vec::new();
    }

    let budget = budget.max(GIO_TRASH_COMMAND_OVERHEAD + 1);
    let mut ranges = Vec::new();
    let mut start = 0usize;
    let mut used = GIO_TRASH_COMMAND_OVERHEAD;

    for (index, path) in paths.iter().enumerate() {
        let arg_len = gio_trash_arg_len(path);
        if index > start && used + arg_len > budget {
            ranges.push(start..index);
            start = index;
            used = GIO_TRASH_COMMAND_OVERHEAD;
        }
        used += arg_len;
    }

    ranges.push(start..paths.len());
    ranges
}

#[cfg(target_os = "linux")]
fn gio_trash_arg_len(path: &Path) -> usize {
    use std::os::unix::ffi::OsStrExt;

    path.as_os_str().as_bytes().len() + 1
}

/// Send a throttled intermediate progress result for the permanent-delete
/// worker.  Returns `false` if the receiver has been dropped (loop should
/// break).
fn send_trash_progress(
    result_tx: &mpsc::Sender<JobResult>,
    token: u64,
    completed: usize,
    last_progress_at: &mut Option<Instant>,
) -> bool {
    let now = Instant::now();
    let due = last_progress_at.is_none_or(|t| now.duration_since(t) >= PROGRESS_SEND_INTERVAL);
    if due {
        *last_progress_at = Some(now);
        return result_tx
            .send(JobResult::Trash(TrashBuild {
                token,
                completed,
                done: false,
                status: None,
            }))
            .is_ok();
    }
    true
}

#[cfg(test)]
mod tests;
