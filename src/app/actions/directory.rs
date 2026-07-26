use super::*;

const SIDEBAR_REFRESH_INTERVAL: Duration = Duration::from_secs(2);

impl App {
    pub fn reload(&mut self) -> Result<()> {
        self.queue_directory_reload(false)
    }

    pub(crate) fn process_sidebar_refresh(&mut self) -> bool {
        if self.navigation.last_sidebar_refresh_at.elapsed() < SIDEBAR_REFRESH_INTERVAL {
            return false;
        }
        self.navigation.last_sidebar_refresh_at = Instant::now();

        let sidebar = crate::fs::build_sidebar_rows();
        if sidebar == self.navigation.sidebar {
            return false;
        }

        self.navigation.sidebar = sidebar;
        true
    }

    pub fn process_auto_reload(&mut self) -> Result<bool> {
        while let Ok(event) = self.navigation.directory_runtime.watch_rx.try_recv() {
            match event {
                crate::fs::DirectoryWatchEvent::Changed(paths)
                    if !crate::fs::event_affects_visible_entries(
                        &paths,
                        self.effective_show_hidden(),
                    ) => {}
                _ => {
                    self.navigation.directory_runtime.pending_reload_at =
                        Some(Instant::now() + crate::fs::directory_watch_debounce());
                }
            }
        }

        if let Some(deadline) = self.navigation.directory_runtime.pending_reload_at {
            if Instant::now() < deadline {
                return Ok(false);
            }
            self.navigation.directory_runtime.pending_reload_at = None;
            return self.reload_if_directory_changed();
        }

        if !self.navigation.directory_runtime.use_polling_reload {
            return Ok(false);
        }

        if self.preview.state.deferred_refresh_at.is_some() || self.browser_wheel_burst_active() {
            return Ok(false);
        }

        if self
            .navigation
            .directory_runtime
            .pending_fingerprint_scan
            .is_some()
        {
            return Ok(false);
        }

        if self
            .navigation
            .directory_runtime
            .last_auto_reload_at
            .elapsed()
            < self.polling_reload_interval()
        {
            return Ok(false);
        }
        self.navigation.directory_runtime.last_auto_reload_at = Instant::now();
        self.queue_directory_fingerprint_scan()
    }

    pub(in crate::app) fn queue_directory_load(
        &mut self,
        mut load: PendingDirectoryLoad,
    ) -> Result<()> {
        self.navigation.directory_runtime.pending_fingerprint_scan = None;
        self.jobs.scheduler.cancel_directory_fingerprints();
        self.jobs.scheduler.cancel_directory_stats();
        self.preview.state.directory_stats_ready_at = None;
        self.jobs.directory_token = self.jobs.directory_token.wrapping_add(1);
        load.token = self.jobs.directory_token;
        let request = jobs::DirectoryRequest {
            token: load.token,
            cwd: load.target_cwd.clone(),
            show_hidden: self.effective_show_hidden_for(&load.target_cwd),
            sort_mode: self.navigation.sort_mode,
        };
        if !self.jobs.scheduler.submit_directory(request) {
            bail!("Directory worker unavailable");
        }
        self.navigation.directory_runtime.pending_load = Some(load);
        Ok(())
    }

    pub(in crate::app) fn queue_directory_reload(&mut self, refresh_search: bool) -> Result<()> {
        self.queue_directory_load(PendingDirectoryLoad {
            token: 0,
            target_cwd: self.navigation.cwd.clone(),
            previous_cwd: self.navigation.cwd.clone(),
            previous_selected_path: self.selected_entry().map(|entry| entry.path.clone()),
            previous_selection_name: self.selected_entry().map(|entry| entry.name.clone()),
            reselect_path: None,
            history_mode: DirectoryHistoryMode::None,
            refresh_search,
            completion: DirectoryLoadCompletion::Keep,
        })
    }

    pub(in crate::app) fn queue_directory_escape_for_paths(
        &mut self,
        paths: &[PathBuf],
    ) -> Result<PathBuf> {
        let target_cwd = self
            .current_directory_escape_for_paths(paths)
            .unwrap_or_else(|| self.navigation.cwd.clone());

        if target_cwd != self.navigation.cwd {
            self.queue_directory_load(PendingDirectoryLoad {
                token: 0,
                target_cwd: target_cwd.clone(),
                previous_cwd: self.navigation.cwd.clone(),
                previous_selected_path: self.selected_entry().map(|entry| entry.path.clone()),
                previous_selection_name: None,
                reselect_path: None,
                history_mode: DirectoryHistoryMode::None,
                refresh_search: false,
                completion: DirectoryLoadCompletion::Keep,
            })?;
        }

        Ok(target_cwd)
    }

    pub(in crate::app) fn apply_directory_snapshot(
        &mut self,
        load: PendingDirectoryLoad,
        snapshot: crate::fs::DirectorySnapshot,
    ) {
        let should_refresh_open_search = self.overlays.search.is_some();
        self.invalidate_search_index_for_directory_snapshot(&load.target_cwd);
        self.navigation.directory_runtime.pending_fingerprint_scan = None;
        let cwd_changed = load.target_cwd != self.navigation.cwd;
        let remembered_view = self.remembered_view_for(&load.target_cwd);
        if cwd_changed {
            self.clear_local_filter_for_directory_change();
        }
        self.navigation.cwd = load.target_cwd.clone();
        self.navigation.in_trash = Self::path_is_trash(&self.navigation.cwd);
        self.navigation.unfiltered_entries = snapshot.entries;
        self.apply_local_filter();
        self.navigation.sidebar = crate::fs::build_sidebar_rows();
        self.navigation.last_sidebar_refresh_at = Instant::now();
        self.navigation.directory_runtime.fingerprint = snapshot.fingerprint;
        self.navigation.directory_runtime.last_auto_reload_at = Instant::now();
        self.navigation.directory_count_viewport = None;
        self.navigation.directory_item_count_ready_at = None;

        self.navigation.selected = if let Some(path) = &load.reselect_path {
            self.navigation
                .entries
                .iter()
                .position(|entry| entry.path == *path)
                .unwrap_or(0)
        } else if let Some(path) = remembered_view
            .as_ref()
            .and_then(|view| view.selected_path.as_ref())
        {
            self.navigation
                .entries
                .iter()
                .position(|entry| entry.path == *path)
                .unwrap_or(0)
        } else if let Some(name) = &load.previous_selection_name {
            self.navigation
                .entries
                .iter()
                .position(|entry| entry.name == *name)
                .unwrap_or(0)
        } else {
            0
        };
        self.navigation.scroll_row = remembered_view.map_or(0, |view| view.scroll_row);
        self.input.last_selection_change_at = Instant::now();
        self.preview.image.selection_activation_delay = std::time::Duration::ZERO;
        self.clamp_selection();
        self.sync_scroll();
        self.remember_current_directory_view();
        self.refresh_preview();
        self.clear_wheel_scroll();

        if cwd_changed {
            self.reset_directory_watch();
        }
        self.refresh_git_branch();

        match load.history_mode {
            DirectoryHistoryMode::None => {}
            DirectoryHistoryMode::PushCurrent => {
                self.navigation.navigation_history.back.push(HistoryEntry {
                    cwd: load.previous_cwd,
                    selected_path: load.previous_selected_path,
                });
                self.navigation.navigation_history.forward.clear();
            }
            DirectoryHistoryMode::GoBack => {
                if !self.navigation.navigation_history.back.is_empty() {
                    self.navigation.navigation_history.back.pop();
                }
                self.navigation
                    .navigation_history
                    .forward
                    .push(HistoryEntry {
                        cwd: load.previous_cwd,
                        selected_path: load.previous_selected_path,
                    });
            }
            DirectoryHistoryMode::GoForward => {
                if !self.navigation.navigation_history.forward.is_empty() {
                    self.navigation.navigation_history.forward.pop();
                }
                self.navigation.navigation_history.back.push(HistoryEntry {
                    cwd: load.previous_cwd,
                    selected_path: load.previous_selected_path,
                });
            }
        }

        if load.refresh_search || should_refresh_open_search {
            self.refresh_search_after_directory_reload();
        }

        match load.completion {
            DirectoryLoadCompletion::Keep => {}
            DirectoryLoadCompletion::Clear => self.status.clear(),
            DirectoryLoadCompletion::Status(status) => self.status = status,
        }

        if self.preview.exit_fullscreen_after_directory_load {
            self.clear_fullscreen_preview();
        }
    }

    fn invalidate_search_index_for_directory_snapshot(&mut self, cwd: &Path) {
        if self
            .jobs
            .search_cache
            .as_ref()
            .is_some_and(|cache| cache.cwd == cwd)
        {
            self.jobs.search_cache = None;
        }

        self.jobs.search_loading = false;
        self.jobs.search_token = self.jobs.search_token.wrapping_add(1);
        self.jobs.scheduler.cancel_search();
    }

    fn remembered_view_for(&self, cwd: &Path) -> Option<DirectoryViewMemory> {
        self.navigation.directory_view_memory.get(cwd).cloned()
    }

    pub(in crate::app) fn remember_current_directory_view(&mut self) {
        self.navigation.directory_view_memory.insert(
            self.navigation.cwd.clone(),
            DirectoryViewMemory {
                selected_path: self.selected_entry().map(|entry| entry.path.clone()),
                scroll_row: self.navigation.scroll_row,
            },
        );
    }

    pub(in crate::app) fn set_dir(&mut self, path: PathBuf) -> Result<()> {
        self.set_dir_transition(
            path,
            DirectoryHistoryMode::PushCurrent,
            None,
            DirectoryLoadCompletion::Clear,
        )
    }

    pub(in crate::app) fn set_dir_transition(
        &mut self,
        path: PathBuf,
        history_mode: DirectoryHistoryMode,
        reselect_path: Option<PathBuf>,
        completion: DirectoryLoadCompletion,
    ) -> Result<()> {
        let metadata = std::fs::metadata(&path).map_err(|error| {
            anyhow!(
                "Cannot open {}: {}",
                crate::path_display::user_facing(&path),
                crate::fs::describe_io_error(&error)
            )
        })?;
        if !metadata.is_dir() {
            bail!(
                "{} is not a directory",
                crate::path_display::user_facing(&path)
            );
        }
        let normalized = path.canonicalize().unwrap_or(path);
        if normalized == self.navigation.cwd
            && self.navigation.directory_runtime.pending_load.is_none()
        {
            if let Some(path) = reselect_path.as_ref()
                && self.reselect_visible_entry(path)
            {
                self.apply_directory_completion(completion);
                return Ok(());
            }
            self.status = format!(
                "Already in {}",
                crate::path_display::user_facing(&self.navigation.cwd)
            );
            return Ok(());
        }
        if self
            .navigation
            .directory_runtime
            .pending_load
            .as_ref()
            .is_some_and(|load| load.target_cwd == normalized)
        {
            if let Some(load) = self.navigation.directory_runtime.pending_load.as_mut() {
                if let Some(path) = reselect_path {
                    load.reselect_path = Some(path);
                }
                load.completion = completion;
            }
            self.status = format!(
                "Already opening {}",
                crate::path_display::user_facing(&normalized)
            );
            return Ok(());
        }

        let reselect_path = reselect_path.or_else(|| {
            self.remembered_view_for(&normalized)
                .and_then(|view| view.selected_path)
        });
        self.queue_directory_load(PendingDirectoryLoad {
            token: 0,
            target_cwd: normalized,
            previous_cwd: self.navigation.cwd.clone(),
            previous_selected_path: self.selected_entry().map(|entry| entry.path.clone()),
            previous_selection_name: None,
            reselect_path,
            history_mode,
            refresh_search: false,
            completion,
        })
    }

    fn reselect_visible_entry(&mut self, path: &Path) -> bool {
        let Some(index) = self
            .navigation
            .entries
            .iter()
            .position(|entry| entry.path == path)
        else {
            return false;
        };
        self.set_selected(index);
        self.clear_wheel_scroll();
        true
    }

    fn apply_directory_completion(&mut self, completion: DirectoryLoadCompletion) {
        match completion {
            DirectoryLoadCompletion::Keep => {}
            DirectoryLoadCompletion::Clear => self.status.clear(),
            DirectoryLoadCompletion::Status(status) => self.status = status,
        }
    }

    pub(in crate::app) fn go_parent(&mut self) -> Result<()> {
        let current = self.navigation.cwd.clone();
        let Some(parent) = self.navigation.cwd.parent() else {
            self.status = "Already at filesystem root".to_string();
            return Ok(());
        };
        self.set_dir_transition(
            parent.to_path_buf(),
            DirectoryHistoryMode::PushCurrent,
            Some(current),
            DirectoryLoadCompletion::Clear,
        )
    }

    pub(in crate::app) fn reset_directory_watch(&mut self) {
        self.navigation.directory_runtime.watch = None;
        self.navigation.directory_runtime.pending_reload_at = None;
        self.navigation.directory_runtime.pending_fingerprint_scan = None;
        self.jobs.scheduler.cancel_directory_fingerprints();
        while self
            .navigation
            .directory_runtime
            .watch_rx
            .try_recv()
            .is_ok()
        {}

        match crate::fs::start_directory_watcher(
            &self.navigation.cwd,
            &self.navigation.directory_runtime.watch_tx,
        ) {
            Ok(watcher) => {
                self.navigation.directory_runtime.watch = Some(watcher);
                self.navigation.directory_runtime.use_polling_reload = false;
            }
            Err(_) => {
                self.navigation.directory_runtime.use_polling_reload = true;
            }
        }
    }

    fn reload_if_directory_changed(&mut self) -> Result<bool> {
        if self.navigation.directory_runtime.pending_load.is_some()
            || self
                .navigation
                .directory_runtime
                .pending_fingerprint_scan
                .is_some()
        {
            return Ok(false);
        }
        let show_hidden = self.effective_show_hidden();
        self.jobs.directory_fingerprint_token =
            self.jobs.directory_fingerprint_token.wrapping_add(1);
        let token = self.jobs.directory_fingerprint_token;
        let cwd = self.navigation.cwd.clone();
        if !self
            .jobs
            .scheduler
            .submit_directory_fingerprint(jobs::DirectoryFingerprintRequest {
                token,
                cwd: cwd.clone(),
                show_hidden,
            })
        {
            return Ok(false);
        }
        self.navigation.directory_runtime.pending_fingerprint_scan =
            Some(PendingDirectoryFingerprintScan {
                token,
                cwd,
                show_hidden,
            });
        Ok(false)
    }

    fn queue_directory_fingerprint_scan(&mut self) -> Result<bool> {
        self.reload_if_directory_changed()
    }

    fn polling_reload_interval(&self) -> Duration {
        match self.navigation.entries.len() {
            0..=255 => AUTO_RELOAD_INTERVAL_SMALL,
            256..=2047 => AUTO_RELOAD_INTERVAL_MEDIUM,
            _ => AUTO_RELOAD_INTERVAL_LARGE,
        }
    }

    fn refresh_search_after_directory_reload(&mut self) {
        let Some(scope) = self.overlays.search.as_ref().map(|search| search.scope) else {
            return;
        };

        if let Some(search) = &mut self.overlays.search {
            search.candidates = Arc::new(Vec::new());
            search.matches.clear();
            search.cached_matches = HashMap::from([(
                String::new(),
                super::super::search::build_base_search_cache_entry(Vec::new()),
            )]);
            search.selected = 0;
            search.scroll = 0;
            search.loading = true;
            search.error = None;
            search.stats = crate::fs::search::SearchIndexStats::default();
        }
        self.prewarm_search_index(scope);
    }
}
