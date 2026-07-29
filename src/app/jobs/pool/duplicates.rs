use super::*;
use std::{
    path::PathBuf,
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
};

pub(in crate::app::jobs) struct DuplicatePool {
    shared: Arc<DuplicateShared>,
    workers: Vec<thread::JoinHandle<()>>,
}

struct DuplicateShared {
    state: Mutex<DuplicateState>,
    available: Condvar,
}

struct DuplicateState {
    pending: Option<DuplicateScanRequest>,
    pending_key: Option<DuplicateJobKey>,
    active: Option<ActiveDuplicateJob>,
    closed: bool,
}

#[derive(Clone, Debug)]
struct ActiveDuplicateJob {
    key: DuplicateJobKey,
    canceled: Arc<AtomicBool>,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(in crate::app::jobs) struct DuplicateJobKey {
    pub(in crate::app::jobs) cwd: PathBuf,
    pub(in crate::app::jobs) show_hidden: bool,
    pub(in crate::app::jobs) fingerprint: crate::fs::DirectoryFingerprint,
}

impl DuplicatePool {
    pub(in crate::app::jobs) fn new(
        worker_count: usize,
        result_tx: mpsc::Sender<JobResult>,
    ) -> Self {
        let shared = Arc::new(DuplicateShared {
            state: Mutex::new(DuplicateState {
                pending: None,
                pending_key: None,
                active: None,
                closed: false,
            }),
            available: Condvar::new(),
        });
        let mut workers = Vec::with_capacity(worker_count);
        for _ in 0..worker_count {
            let shared = Arc::clone(&shared);
            let result_tx = result_tx.clone();
            workers.push(thread::spawn(move || {
                while let Some((request, canceled)) = DuplicateShared::pop(&shared) {
                    let key = DuplicateJobKey::from_request(&request);
                    let progress_token = request.token;
                    let progress_cwd = request.cwd.clone();
                    let progress_show_hidden = request.show_hidden;
                    let progress_fingerprint = request.fingerprint;
                    let mut progress_send_failed = false;
                    let result = crate::fs::duplicates::scan_duplicates_streaming(
                        &request.cwd,
                        request.show_hidden,
                        || canceled.load(Ordering::Relaxed),
                        |batch| {
                            if result_tx
                                .send(JobResult::DuplicateScanBatch(DuplicateScanBatchBuild {
                                    token: progress_token,
                                    cwd: progress_cwd.clone(),
                                    show_hidden: progress_show_hidden,
                                    fingerprint: progress_fingerprint,
                                    batch,
                                }))
                                .is_err()
                            {
                                progress_send_failed = true;
                                return false;
                            }
                            true
                        },
                    )
                    .map_err(|error| error.to_string());
                    DuplicateShared::finish(&shared, &key);
                    if progress_send_failed || canceled.load(Ordering::Relaxed) {
                        continue;
                    }
                    if result_tx
                        .send(JobResult::DuplicateScan(DuplicateScanBuild {
                            token: request.token,
                            cwd: request.cwd,
                            show_hidden: request.show_hidden,
                            fingerprint: request.fingerprint,
                            result,
                        }))
                        .is_err()
                    {
                        break;
                    }
                }
            }));
        }
        Self { shared, workers }
    }

    pub(in crate::app::jobs) fn submit(&self, request: DuplicateScanRequest) -> bool {
        let key = DuplicateJobKey::from_request(&request);
        let mut state = lock_unpoison(&self.shared.state);
        if state.closed {
            return false;
        }
        state.pending = Some(request);
        state.pending_key = Some(key);
        if let Some(active) = &state.active {
            active.canceled.store(true, Ordering::Relaxed);
        }
        self.shared.available.notify_one();
        true
    }

    pub(in crate::app::jobs) fn cancel_all(&self) {
        let mut state = lock_unpoison(&self.shared.state);
        state.pending = None;
        state.pending_key = None;
        if let Some(active) = &state.active {
            active.canceled.store(true, Ordering::Relaxed);
        }
    }

    pub(in crate::app::jobs) fn has_pending_work(&self) -> bool {
        let state = lock_unpoison(&self.shared.state);
        state.pending.is_some() || state.active.is_some()
    }
}

impl Drop for DuplicatePool {
    fn drop(&mut self) {
        {
            let mut state = lock_unpoison(&self.shared.state);
            state.closed = true;
            state.pending = None;
            state.pending_key = None;
            if let Some(active) = &state.active {
                active.canceled.store(true, Ordering::Relaxed);
            }
        }
        self.shared.available.notify_all();
        for worker in self.workers.drain(..) {
            let _ = worker.join();
        }
    }
}

impl DuplicateShared {
    fn pop(shared: &Arc<Self>) -> Option<(DuplicateScanRequest, Arc<AtomicBool>)> {
        let mut state = lock_unpoison(&shared.state);
        loop {
            if state.closed {
                return None;
            }
            if state.active.is_none()
                && let Some(request) = state.pending.take()
            {
                let key = state.pending_key.take().expect("pending key should exist");
                let canceled = Arc::new(AtomicBool::new(false));
                state.active = Some(ActiveDuplicateJob {
                    key,
                    canceled: Arc::clone(&canceled),
                });
                return Some((request, canceled));
            }
            state = wait_unpoison(&shared.available, state);
        }
    }

    fn finish(shared: &Arc<Self>, key: &DuplicateJobKey) {
        let mut state = lock_unpoison(&shared.state);
        if state
            .active
            .as_ref()
            .is_some_and(|active| &active.key == key)
        {
            state.active = None;
            shared.available.notify_one();
        }
    }
}

impl DuplicateJobKey {
    fn from_request(request: &DuplicateScanRequest) -> Self {
        Self {
            cwd: request.cwd.clone(),
            show_hidden: request.show_hidden,
            fingerprint: request.fingerprint,
        }
    }
}
