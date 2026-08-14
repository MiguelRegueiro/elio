use super::*;
use std::{
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};

/// Minimum time between intermediate progress results sent to the UI.
const PROGRESS_SEND_INTERVAL: Duration = Duration::from_millis(80);

pub(in crate::app::jobs) struct RestorePool {
    shared: Arc<RestoreShared>,
    workers: Vec<thread::JoinHandle<()>>,
}

struct RestoreShared {
    state: Mutex<RestoreState>,
    available: Condvar,
    cancelled: AtomicBool,
    cancel_token: AtomicU64,
}

struct RestoreState {
    pending: Option<RestoreRequest>,
    active: bool,
    closed: bool,
}

impl RestorePool {
    pub(in crate::app::jobs) fn new(result_tx: mpsc::Sender<JobResult>) -> Self {
        let shared = Arc::new(RestoreShared {
            state: Mutex::new(RestoreState {
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
            while let Some(request) = RestoreShared::pop(&shared_worker) {
                RestoreShared::set_active(&shared_worker, true);
                let (completed, errors, stopped_early) = run_restore(
                    &request,
                    &result_tx,
                    &shared_worker.cancelled,
                    &shared_worker.cancel_token,
                );
                RestoreShared::set_active(&shared_worker, false);

                let total = request.targets.len();
                let single_name = (total == 1).then(|| request.targets[0].name.as_str());
                let status = if stopped_early {
                    match completed {
                        0 => "Restore cancelled".to_string(),
                        1 => "Restore cancelled — Restored 1 item".to_string(),
                        n => format!("Restore cancelled — Restored {n} items"),
                    }
                } else if errors.is_empty() {
                    match (completed, single_name) {
                        (0, _) => "Nothing was restored".to_string(),
                        (1, Some(name)) => format!("Restored \"{name}\""),
                        (n, _) => format!("Restored {n} items"),
                    }
                } else if completed == 0 {
                    if errors.len() == 1 {
                        errors[0].clone()
                    } else {
                        format!("{} errors — first: {}", errors.len(), errors[0])
                    }
                } else {
                    format!(
                        "Restored {completed} item(s); {} error(s) — first: {}",
                        errors.len(),
                        errors[0]
                    )
                };

                if result_tx
                    .send(JobResult::Restore(RestoreBuild {
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

    pub(in crate::app::jobs) fn submit(&self, request: RestoreRequest) -> bool {
        let mut state = lock_unpoison(&self.shared.state);
        if state.closed {
            return false;
        }
        state.pending = Some(request);
        self.shared.available.notify_one();
        true
    }

    /// Signal the worker to stop after the current item if it is processing
    /// the restore request with the given token.  A concurrent or future
    /// request with a different token is unaffected.
    pub(in crate::app::jobs) fn cancel_restore(&self, token: u64) {
        self.shared.cancel_token.store(token, Ordering::Relaxed);
    }

    pub(in crate::app::jobs) fn has_pending_work(&self) -> bool {
        let state = lock_unpoison(&self.shared.state);
        state.pending.is_some() || state.active
    }
}

impl Drop for RestorePool {
    fn drop(&mut self) {
        {
            let mut state = lock_unpoison(&self.shared.state);
            state.closed = true;
            // Finish any queued restore before exiting — same rationale as
            // TrashPool: partially-restored batches are confusing.
        }
        self.shared.available.notify_all();
        for worker in self.workers.drain(..) {
            let _ = worker.join();
        }
    }
}

impl RestoreShared {
    fn pop(shared: &Arc<Self>) -> Option<RestoreRequest> {
        let mut state = lock_unpoison(&shared.state);
        loop {
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

/// Restore each target one at a time with per-item throttled progress and
/// cancellation support between items.
fn run_restore(
    request: &RestoreRequest,
    result_tx: &mpsc::Sender<JobResult>,
    cancelled: &AtomicBool,
    cancel_token: &AtomicU64,
) -> (usize, Vec<String>, bool) {
    let mut completed = 0usize;
    let mut errors: Vec<String> = Vec::new();
    let mut stopped_early = false;
    let mut last_progress_at: Option<Instant> = None;
    #[cfg(target_os = "macos")]
    let mut restored_names: Vec<&str> = Vec::new();

    for target in &request.targets {
        if cancelled.load(Ordering::Relaxed)
            || cancel_token.load(Ordering::Relaxed) == request.token
        {
            stopped_early = true;
            break;
        }

        let restore_result = restore_as_invoking_user(&target.path);
        match restore_result {
            Ok(_) => {
                completed += 1;
                #[cfg(target_os = "macos")]
                restored_names.push(target.name.as_str());
            }
            Err(e) => {
                errors.push(format!("Could not restore \"{}\": {e}", target.name));
            }
        }

        if !send_restore_progress(result_tx, request.token, completed, &mut last_progress_at) {
            break;
        }
    }

    #[cfg(target_os = "macos")]
    if !restored_names.is_empty() {
        crate::fs::remove_restore_origins(&restored_names);
    }

    (completed, errors, stopped_early)
}

fn restore_as_invoking_user(path: &std::path::Path) -> anyhow::Result<()> {
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        use crate::{
            config::{InvocationContext, invoking_user_context},
            user_fs_helper::Request,
        };
        match invoking_user_context() {
            InvocationContext::Normal | InvocationContext::RootSession => {}
            InvocationContext::ElevatedUnresolved => {
                anyhow::bail!("could not resolve invoking user; nothing was restored")
            }
            InvocationContext::Elevated(user) => {
                let response = super::super::invoking_user_fs::run(
                    &user,
                    &Request::Restore(path.to_path_buf()),
                )
                .map_err(anyhow::Error::msg)?;
                if let Some(error) = response.error {
                    anyhow::bail!(error);
                }
                anyhow::ensure!(response.completed == 1, "helper did not restore the item");
                return Ok(());
            }
        }
    }
    crate::fs::restore_trash_item(path)
}

/// Send a throttled intermediate progress result.  Returns `false` if the
/// receiver has been dropped (loop should break).
fn send_restore_progress(
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
            .send(JobResult::Restore(RestoreBuild {
                token,
                completed,
                done: false,
                status: None,
            }))
            .is_ok();
    }
    true
}
