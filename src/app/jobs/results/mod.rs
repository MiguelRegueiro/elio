use super::*;
use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};

const JOB_RESULT_APPLY_MAX_PER_TICK: usize = 12;
const JOB_RESULT_APPLY_TIME_BUDGET: Duration = Duration::from_millis(2);

impl App {
    fn refresh_static_image_preloads_for_cached_preview_visual(
        &mut self,
        build_entry: &Entry,
        build_variant: &preview::PreviewRequestOptions,
        build_visual_kind: Option<preview::PreviewVisualKind>,
        is_current_entry: bool,
    ) {
        let Some(build_visual_kind) = build_visual_kind else {
            return;
        };

        let should_refresh = match build_visual_kind {
            preview::PreviewVisualKind::PageImage => {
                is_current_entry
                    || self.refreshes_image_preloads_for_nearby_comic_entry_preview(
                        build_entry,
                        build_variant,
                    )
                    || self.refreshes_image_preloads_for_nearby_epub_entry_preview(
                        build_entry,
                        build_variant,
                    )
            }
            preview::PreviewVisualKind::Cover => {
                is_current_entry
                    || self.refreshes_image_preloads_for_nearby_audio_preview(
                        build_entry,
                        build_variant,
                    )
            }
        };
        if should_refresh {
            self.refresh_static_image_preloads();
        }
    }

    pub fn process_background_jobs(&mut self) -> bool {
        let mut dirty = false;
        let started_at = Instant::now();
        let mut processed = 0usize;

        while processed < JOB_RESULT_APPLY_MAX_PER_TICK
            && started_at.elapsed() < JOB_RESULT_APPLY_TIME_BUDGET
        {
            let Ok(job) = self.jobs.scheduler.try_recv() else {
                break;
            };
            processed += 1;
            match job {
                JobResult::Directory(build) => {
                    let Some(load) = self.navigation.directory_runtime.pending_load.clone() else {
                        continue;
                    };
                    if build.token != self.jobs.directory_token
                        || build.token != load.token
                        || build.cwd != load.target_cwd
                    {
                        continue;
                    }

                    self.navigation.directory_runtime.pending_load = None;
                    dirty = true;

                    match build.result {
                        Ok(snapshot) => self.apply_directory_snapshot(load, snapshot),
                        Err(error) => {
                            self.preview.exit_fullscreen_after_directory_load = false;
                            self.exit_fullscreen_preview();
                            self.status = format!("Cannot open {}: {}", build.cwd.display(), error);
                        }
                    }
                }
                JobResult::DirectoryFingerprint(build) => {
                    let Some(scan) = self
                        .navigation
                        .directory_runtime
                        .pending_fingerprint_scan
                        .clone()
                    else {
                        continue;
                    };
                    if build.token != self.jobs.directory_fingerprint_token
                        || build.token != scan.token
                        || build.cwd != scan.cwd
                        || build.show_hidden != scan.show_hidden
                    {
                        continue;
                    }

                    self.navigation.directory_runtime.pending_fingerprint_scan = None;

                    let Ok(fingerprint) = build.result else {
                        continue;
                    };
                    if self.navigation.directory_runtime.pending_load.is_some()
                        || fingerprint == self.navigation.directory_runtime.fingerprint
                    {
                        continue;
                    }
                    if self.queue_directory_reload(true).is_ok() {
                        dirty = true;
                    }
                }
                JobResult::DirectoryItemCount(build) => {
                    self.cache_directory_item_count(
                        build.path.clone(),
                        build.modified,
                        build.show_hidden,
                        build.item_count,
                    );
                    dirty |= self.should_redraw_for_directory_item_count(
                        &build.path,
                        build.modified,
                        build.show_hidden,
                    );
                }
                JobResult::DirectoryStats(build) => {
                    dirty |= self.apply_preview_directory_stats_result(
                        build.token,
                        &build.path,
                        build.result,
                    );
                }
                JobResult::GitStatus(build) => {
                    dirty |= self.apply_git_status_result(build);
                }
                JobResult::PreviewLineCount(build) => {
                    dirty |= self.apply_preview_line_count_result(
                        &build.path,
                        build.size,
                        build.modified,
                        build.total_lines,
                    );
                }
                JobResult::PdfProbe(build) => {
                    dirty |= self.apply_pdf_probe_build(build);
                }
                JobResult::PdfRender(build) => {
                    dirty |= self.apply_pdf_render_build(build);
                }
                JobResult::ImagePrepare(build) => {
                    dirty |= self.apply_image_prepare_build(build);
                }
                JobResult::SearchBatch(build) => {
                    if build.token != self.jobs.search_token
                        || build.cwd != self.navigation.cwd
                        || build.show_hidden != self.effective_show_hidden()
                        || build.fingerprint != self.navigation.directory_runtime.fingerprint
                    {
                        continue;
                    }

                    let mut sync_search_scroll = false;
                    if let Some(search) = &mut self.overlays.search
                        && search.scope == build.scope
                    {
                        search.loading = true;
                        search.error = None;
                        search.stats = build.batch.stats;
                        if !build.batch.candidates.is_empty() {
                            append_streamed_search_candidates(search, build.batch.candidates);
                            sync_search_scroll = true;
                        }
                        dirty = true;
                    }
                    if sync_search_scroll {
                        self.sync_search_scroll();
                    }
                }
                JobResult::Search(build) => {
                    if build.token != self.jobs.search_token
                        || build.cwd != self.navigation.cwd
                        || build.show_hidden != self.effective_show_hidden()
                        || build.fingerprint != self.navigation.directory_runtime.fingerprint
                    {
                        continue;
                    }

                    self.jobs.search_loading = false;
                    dirty = true;

                    match build.result {
                        Ok(index) => {
                            let stats = index.stats;
                            let candidates = Arc::new(index.candidates);
                            self.jobs.search_cache = Some(SearchCache {
                                cwd: build.cwd,
                                scope: build.scope,
                                show_hidden: build.show_hidden,
                                fingerprint: build.fingerprint,
                                candidates: candidates.clone(),
                                stats,
                            });
                            if let Some(search) = &mut self.overlays.search
                                && search.scope == build.scope
                            {
                                search.candidates = candidates;
                                search.cached_matches = HashMap::from([(
                                    String::new(),
                                    crate::app::search::build_base_search_cache_entry(
                                        (0..search.candidates.len()).collect(),
                                    ),
                                )]);
                                search.loading = false;
                                search.error = None;
                                search.stats = stats;
                            }
                            self.refresh_search_matches("");
                        }
                        Err(error) => {
                            self.jobs.search_cache = None;
                            if let Some(search) = &mut self.overlays.search
                                && search.scope == build.scope
                            {
                                search.candidates = Arc::new(Vec::new());
                                search.matches.clear();
                                search.cached_matches = HashMap::from([(
                                    String::new(),
                                    crate::app::search::build_base_search_cache_entry(Vec::new()),
                                )]);
                                search.selected = 0;
                                search.scroll = 0;
                                search.loading = false;
                                search.error = Some(error);
                                search.stats = crate::fs::search::SearchIndexStats::default();
                            }
                        }
                    }
                }
                JobResult::ArchiveCreate(build) => {
                    if build.token != self.jobs.archive_create_token {
                        continue;
                    }
                    if build.done {
                        self.jobs.archive_create_progress = None;
                        self.jobs.archive_create_path = None;
                        let source_cwd = self
                            .jobs
                            .archive_create_source_cwd
                            .take()
                            .unwrap_or_else(|| self.navigation.cwd.clone());
                        let status = build.status.unwrap_or_default();
                        let nav_target = self
                            .navigation
                            .directory_runtime
                            .pending_load
                            .as_ref()
                            .map(|load| load.target_cwd.as_path());
                        let nav_to_source = nav_target == Some(source_cwd.as_path());
                        if nav_to_source
                            || (source_cwd == self.navigation.cwd && nav_target.is_none())
                        {
                            let _ = self.queue_directory_load(PendingDirectoryLoad {
                                token: 0,
                                target_cwd: source_cwd,
                                previous_cwd: self.navigation.cwd.clone(),
                                previous_selected_path: None,
                                previous_selection_name: None,
                                reselect_path: build.output_path,
                                history_mode: DirectoryHistoryMode::None,
                                refresh_search: false,
                                completion: DirectoryLoadCompletion::Status(status),
                            });
                        } else {
                            self.status = status;
                        }
                    } else if let Some(progress) = &mut self.jobs.archive_create_progress {
                        progress.completed = build.completed;
                        progress.total = build.total;
                    }
                    dirty = true;
                }
                JobResult::ArchiveExtract(build) => {
                    if build.token != self.jobs.archive_extract_token {
                        continue;
                    }
                    if build.done {
                        self.jobs.archive_extract_progress = None;
                        if let Some(prompt) = build.password_prompt {
                            if let Some(request) = build.password_request {
                                let error = match prompt {
                                    ArchivePasswordPrompt::Required => None,
                                    ArchivePasswordPrompt::BadPassword => {
                                        Some("Wrong password".to_string())
                                    }
                                };
                                self.jobs.archive_extract_request = Some(request.clone());
                                self.open_archive_password_prompt(request, error);
                            } else {
                                self.status = "Archive requires a password".to_string();
                            }
                            dirty = true;
                            continue;
                        }
                        self.jobs.archive_extract_request = None;
                        let source_cwd = self
                            .jobs
                            .archive_extract_source_cwd
                            .take()
                            .unwrap_or_else(|| self.navigation.cwd.clone());
                        let status = build.status.unwrap_or_default();
                        let nav_target = self
                            .navigation
                            .directory_runtime
                            .pending_load
                            .as_ref()
                            .map(|l| l.target_cwd.as_path());
                        let nav_to_source = nav_target == Some(source_cwd.as_path());
                        if nav_to_source
                            || (source_cwd == self.navigation.cwd && nav_target.is_none())
                        {
                            let _ = self.queue_directory_load(PendingDirectoryLoad {
                                token: 0,
                                target_cwd: source_cwd,
                                previous_cwd: self.navigation.cwd.clone(),
                                previous_selected_path: None,
                                previous_selection_name: None,
                                reselect_path: build.dest_dir,
                                history_mode: DirectoryHistoryMode::None,
                                refresh_search: false,
                                completion: DirectoryLoadCompletion::Status(status),
                            });
                        } else {
                            self.status = status;
                        }
                    } else if let Some(prog) = &mut self.jobs.archive_extract_progress {
                        prog.completed = build.completed;
                        prog.total = build.total;
                    }
                    dirty = true;
                }
                JobResult::Paste(build) => {
                    if build.token != self.jobs.paste_token {
                        continue;
                    }
                    if build.done {
                        let paste_origin =
                            self.jobs.paste_progress.as_ref().map(|p| p.origin.clone());
                        self.jobs.paste_progress = None;
                        let dest_dir = self
                            .jobs
                            .paste_dest_dir
                            .take()
                            .unwrap_or_else(|| self.navigation.cwd.clone());
                        let status = build.status.unwrap_or_default();
                        let next_queued_dest = self
                            .jobs
                            .queued_pastes
                            .front()
                            .map(|queued| queued.dest_dir.as_path());
                        let defer_reload_for_same_dest =
                            next_queued_dest == Some(dest_dir.as_path());
                        // Only reload in-place when the user is still in the
                        // destination directory and not mid-navigation to
                        // somewhere else (which would cancel their navigation).
                        let nav_target = self
                            .navigation
                            .directory_runtime
                            .pending_load
                            .as_ref()
                            .map(|l| l.target_cwd.as_path());
                        let nav_to_dest = nav_target == Some(dest_dir.as_path());
                        if dest_dir == self.navigation.cwd
                            && (nav_target.is_none() || nav_to_dest)
                            && !defer_reload_for_same_dest
                        {
                            let reselect_path =
                                if paste_origin == Some(crate::app::state::PasteOrigin::Drop) {
                                    build.destination_paths.first().cloned()
                                } else {
                                    None
                                };
                            let _ = self.queue_directory_load(PendingDirectoryLoad {
                                token: 0,
                                target_cwd: dest_dir,
                                previous_cwd: self.navigation.cwd.clone(),
                                previous_selected_path: None,
                                previous_selection_name: None,
                                reselect_path,
                                history_mode: DirectoryHistoryMode::None,
                                refresh_search: false,
                                completion: DirectoryLoadCompletion::Status(status),
                            });
                        } else {
                            // User navigated away — just surface the status.
                            // Navigation will load dest_dir fresh if they return.
                            // If another queued paste targets this same
                            // directory, defer the reload until that queued
                            // paste finishes to avoid showing a mid-queue
                            // snapshot.
                            self.status = status;
                        }
                        self.start_next_queued_paste();
                    } else if let Some(prog) = &mut self.jobs.paste_progress {
                        prog.completed = build.completed;
                    }
                    dirty = true;
                }
                JobResult::Trash(build) => {
                    if build.token != self.jobs.trash_token {
                        continue;
                    }
                    if build.done {
                        // Only reposition the cursor when every target was
                        // actually removed.  Cancelled or partially-failed
                        // operations leave some entries intact, so using the
                        // pre-computed survivor path would move the cursor
                        // away from entries that are still present.
                        let next_selection = self.jobs.trash_progress.take().and_then(|p| {
                            (build.completed == p.total)
                                .then_some(p.next_selection)
                                .flatten()
                        });
                        let source_cwd = self
                            .jobs
                            .trash_source_cwd
                            .take()
                            .unwrap_or_else(|| self.navigation.cwd.clone());
                        let status = build.status.unwrap_or_default();
                        // Only reload in-place when the user is still in the
                        // source directory and not mid-navigation to somewhere
                        // else (which would cancel their navigation).
                        let nav_target = self
                            .navigation
                            .directory_runtime
                            .pending_load
                            .as_ref()
                            .map(|l| l.target_cwd.as_path());
                        let nav_to_source = nav_target == Some(source_cwd.as_path());
                        if nav_to_source
                            || (source_cwd == self.navigation.cwd && nav_target.is_none())
                        {
                            let _ = self.queue_directory_load(PendingDirectoryLoad {
                                token: 0,
                                target_cwd: source_cwd,
                                previous_cwd: self.navigation.cwd.clone(),
                                previous_selected_path: None,
                                previous_selection_name: None,
                                reselect_path: next_selection,
                                history_mode: DirectoryHistoryMode::None,
                                refresh_search: false,
                                completion: DirectoryLoadCompletion::Status(status),
                            });
                        } else {
                            // User navigated away — just surface the status.
                            // Navigation will load source_cwd fresh if they return.
                            self.status = status;
                        }
                    } else if let Some(prog) = &mut self.jobs.trash_progress {
                        prog.completed = build.completed;
                    }
                    dirty = true;
                }
                JobResult::Restore(build) => {
                    if build.token != self.jobs.restore_token {
                        continue;
                    }
                    if build.done {
                        let next_selection = self.jobs.restore_progress.take().and_then(|p| {
                            (build.completed == p.total)
                                .then_some(p.next_selection)
                                .flatten()
                        });
                        let source_cwd = self
                            .jobs
                            .restore_source_cwd
                            .take()
                            .unwrap_or_else(|| self.navigation.cwd.clone());
                        let status = build.status.unwrap_or_default();
                        let nav_target = self
                            .navigation
                            .directory_runtime
                            .pending_load
                            .as_ref()
                            .map(|l| l.target_cwd.as_path());
                        let nav_to_source = nav_target == Some(source_cwd.as_path());
                        if nav_to_source
                            || (source_cwd == self.navigation.cwd && nav_target.is_none())
                        {
                            let _ = self.queue_directory_load(PendingDirectoryLoad {
                                token: 0,
                                target_cwd: source_cwd,
                                previous_cwd: self.navigation.cwd.clone(),
                                previous_selected_path: None,
                                previous_selection_name: None,
                                reselect_path: next_selection,
                                history_mode: DirectoryHistoryMode::None,
                                refresh_search: false,
                                completion: DirectoryLoadCompletion::Status(status),
                            });
                        } else {
                            self.status = status;
                        }
                    } else if let Some(prog) = &mut self.jobs.restore_progress {
                        prog.completed = build.completed;
                    }
                    dirty = true;
                }
                JobResult::Preview(build) => {
                    self.cache_preview_result_with_limits(
                        &build.entry,
                        &build.variant,
                        build.code_line_limit,
                        build.code_render_limit,
                        build.ffmpeg_available,
                        &build.result,
                    );
                    let build_is_comic = build.result.kind == preview::PreviewKind::Comic;
                    let build_is_epub_section = matches!(
                        build.variant,
                        preview::PreviewRequestOptions::EpubSection(_)
                    );
                    let build_visual_kind = build
                        .result
                        .preview_visual
                        .as_ref()
                        .map(|visual| visual.kind);
                    let is_current_entry = self
                        .selected_entry()
                        .map(|entry| {
                            entry.path == build.entry.path
                                && entry.modified == build.entry.modified
                                && entry.size == build.entry.size
                        })
                        .unwrap_or(false);
                    let is_current_variant =
                        build.variant == self.current_preview_request_options();
                    if build.token != self.preview.state.token
                        || !is_current_entry
                        || !is_current_variant
                        || build.code_line_limit
                            != self.preview_code_line_limit_for_entry(&build.entry)
                    {
                        // For comic results that match the current entry and variant but
                        // arrived with a stale token, apply them immediately if we are
                        // still showing a placeholder (no preview at all).  The rendered
                        // page list is deterministic for a given path + page index, so a
                        // token skew does not indicate wrong content.  This rescues the
                        // common race where a rapid-nav `refresh_preview()` bumps the
                        // token after the job was already submitted, and the result
                        // arrives before the replacement job finishes — leaving the
                        // placeholder on-screen even though a valid result is available.
                        let can_rescue_stale_comic = build_is_comic
                            && is_current_entry
                            && is_current_variant
                            && build.code_line_limit
                                == self.preview_code_line_limit_for_entry(&build.entry)
                            && matches!(
                                &self.preview.state.load_state,
                                Some(PreviewLoadState::Placeholder(p) | PreviewLoadState::Refreshing(p))
                                    if p == &build.entry.path
                            );
                        if !can_rescue_stale_comic {
                            self.refresh_static_image_preloads_for_cached_preview_visual(
                                &build.entry,
                                &build.variant,
                                build_visual_kind,
                                is_current_entry,
                            );
                            // Clear the in-flight flag if this stale drop belongs to our
                            // outstanding extension job (prevents stuck state when the
                            // user navigates away mid-extension).
                            if self.preview.state.incremental_render_path.as_deref()
                                == Some(build.entry.path.as_path())
                            {
                                self.preview.state.incremental_render_in_flight = false;
                                self.preview.state.incremental_render_path = None;
                            }
                            self.preview.state.metrics.stale_results_dropped += 1;
                            continue;
                        }
                    }

                    // Detect whether this is a complete extension result arriving for a
                    // currently-displayed partial preview.  If so, replace the content
                    // WITHOUT resetting scroll so there are no visual artifacts.
                    let is_extension_result = build.result.incremental_render_limit.is_none()
                        && self.preview.state.content.is_incrementally_partial()
                        && is_current_entry
                        && is_current_variant;

                    self.preview.state.content = build.result;
                    if self.preview.state.content.kind != preview::PreviewKind::Directory {
                        self.clear_preview_directory_stats();
                    }
                    self.preview.state.load_state = None;
                    self.apply_current_comic_preview_metadata();
                    self.apply_current_epub_preview_metadata();
                    self.sync_current_preview_line_count();

                    if is_extension_result {
                        // Extension result: preserve scroll, just clamp if needed.
                        self.preview.state.incremental_render_in_flight = false;
                        self.preview.state.incremental_render_path = None;
                        self.sync_preview_scroll();
                    } else {
                        // Normal first-time render: reset scroll.
                        self.preview.state.scroll = 0;
                        self.preview.state.horizontal_scroll = 0;
                        self.sync_preview_scroll();
                    }

                    if build_visual_kind.is_some() {
                        self.refresh_static_image_preloads();
                    }
                    if build_is_comic || build_is_epub_section || is_current_entry {
                        self.prefetch_nearby_audio_previews();
                        self.schedule_preview_prefetch();
                    }
                    self.preview.state.metrics.applied_results += 1;
                    dirty = true;
                }
            }
        }

        if (processed == JOB_RESULT_APPLY_MAX_PER_TICK
            || (processed > 0 && started_at.elapsed() >= JOB_RESULT_APPLY_TIME_BUDGET))
            && let Ok(job) = self.jobs.scheduler.try_recv()
        {
            self.jobs.scheduler.defer_result(job);
        }

        dirty
    }
}

fn append_streamed_search_candidates(
    search: &mut SearchOverlay,
    candidates: Vec<crate::fs::search::SearchCandidate>,
) {
    let start = search.candidates.len();
    let end = start + candidates.len();
    Arc::make_mut(&mut search.candidates).extend(candidates);

    append_empty_query_search_cache(search, start, end);

    let query = search.query.clone();
    let query_key = crate::app::search::search_cache_key(&query);
    search
        .cached_matches
        .retain(|cached_query, _| cached_query.is_empty() || cached_query == &query_key);

    if query_key.is_empty() {
        if let Some(entry) = search.cached_matches.get("") {
            search.matches = entry.matches.clone();
        }
    } else {
        update_streamed_query_search_cache(search, &query, &query_key, start, end);
    }

    clamp_search_selection(search);
}

fn append_empty_query_search_cache(search: &mut SearchOverlay, start: usize, end: usize) {
    let base = search
        .cached_matches
        .entry(String::new())
        .or_insert_with(|| crate::app::search::build_base_search_cache_entry((0..start).collect()));
    for index in start..end {
        base.pool.push(index);
        if base.matches.len() < SEARCH_MATCH_LIMIT {
            base.matches.push(index);
        }
    }
}

fn update_streamed_query_search_cache(
    search: &mut SearchOverlay,
    query: &str,
    query_key: &str,
    start: usize,
    end: usize,
) {
    let Some(existing) = search.cached_matches.remove(query_key) else {
        let result = crate::fs::search::filter_candidates_in(
            &search.candidates,
            0..end,
            query,
            SEARCH_MATCH_LIMIT,
        );
        search.matches = result.matches.clone();
        search.cached_matches.insert(
            query_key.to_string(),
            crate::app::search::build_search_cache_entry(result.pool, result.matches),
        );
        return;
    };

    let new_result = crate::fs::search::filter_candidates_in(
        &search.candidates,
        start..end,
        query,
        SEARCH_MATCH_LIMIT,
    );
    let mut pool = existing.pool;
    pool.extend(new_result.pool.iter().copied());

    let rerank_pool = existing
        .matches
        .iter()
        .copied()
        .chain(new_result.pool.iter().copied());
    let matches = crate::fs::search::filter_candidates_in(
        &search.candidates,
        rerank_pool,
        query,
        SEARCH_MATCH_LIMIT,
    )
    .matches;

    search.matches = matches.clone();
    search.cached_matches.insert(
        query_key.to_string(),
        crate::app::search::build_search_cache_entry(pool, matches),
    );
}

fn clamp_search_selection(search: &mut SearchOverlay) {
    if search.matches.is_empty() {
        search.selected = 0;
        search.scroll = 0;
        return;
    }
    search.selected = search.selected.min(search.matches.len().saturating_sub(1));
}

#[cfg(test)]
mod tests;
