mod draw;
mod input;
mod kitty_dnd;
mod session_files;
mod terminal;

use self::{
    draw::draw_terminal_frame,
    input::{RuntimeInputEvent, RuntimeInputReader},
    session_files::{write_chooser_file_if_requested, write_cwd_file_if_requested},
    terminal::{
        AppTerminal, Drainer, init_terminal, restore_terminal, resume_terminal, suspend_terminal,
    },
};
use crate::{
    RunOptions, RunOutcome,
    app::{App, ChooserExit, PendingTerminalTask},
    config, path_display, shell, ui, zoxide,
};
#[cfg(unix)]
use crate::{app::ClipOp, core::EntryKind};
use anyhow::Result;
use crossterm::{
    cursor::SetCursorStyle,
    event::{self, Event, MouseEvent, MouseEventKind},
    execute,
};
use std::{
    io::Write,
    path::{Path, PathBuf},
    process::Command,
    time::{Duration, Instant},
};

const IDLE_POLL_INTERVAL: Duration = Duration::from_millis(100);
const ACTIVE_SCROLL_POLL_INTERVAL: Duration = Duration::from_millis(12);
const WINDOWS_TERMINAL_ACTIVE_POLL_INTERVAL: Duration = Duration::from_millis(24);
const RELATIVE_TIME_REFRESH_INTERVAL: Duration = Duration::from_secs(1);

#[derive(Debug)]
struct AppExit {
    final_cwd: Option<PathBuf>,
    chooser: Option<ChooserExit>,
}

#[derive(Debug, Default)]
#[cfg(unix)]
struct PendingDragOut {
    active: bool,
    uri_list: Vec<u8>,
}

#[cfg(unix)]
impl PendingDragOut {
    fn reset(&mut self) {
        self.active = false;
        self.uri_list.clear();
    }
}

#[derive(Debug, Default)]
#[cfg(unix)]
struct PendingDropIn {
    op: Option<ClipOp>,
}

#[cfg(unix)]
struct DragIconLabel {
    icon: String,
    text: String,
    icon_color: ratatui::style::Color,
}

#[cfg(unix)]
impl PendingDropIn {
    fn set(&mut self, op: ClipOp) {
        self.op = Some(op);
    }

    fn take(&mut self) -> Option<ClipOp> {
        self.op.take()
    }

    fn reset(&mut self) {
        self.op = None;
    }
}

pub(crate) fn run_with_startup_state(
    options: RunOptions,
    start_focus: Option<PathBuf>,
    reveal_hidden_start_focus: bool,
    chooser_file: Option<PathBuf>,
) -> Result<RunOutcome> {
    let RunOptions {
        start_dir,
        cwd_file,
    } = options;
    config::initialize();
    ui::theme::initialize();
    let (mut terminal, drainer, kitty_dnd) = init_terminal()?;
    let result = run_app(
        &mut terminal,
        &drainer,
        &kitty_dnd,
        start_dir,
        start_focus,
        reveal_hidden_start_focus,
        chooser_file.is_some(),
    );
    restore_terminal(&mut terminal, &drainer, &kitty_dnd)?;
    let app_exit = result?;
    if let Some(final_cwd) = app_exit.final_cwd {
        write_cwd_file_if_requested(cwd_file.as_deref(), &final_cwd)?;
    }
    match app_exit.chooser {
        Some(ChooserExit::Confirmed(paths)) => {
            write_chooser_file_if_requested(chooser_file.as_deref(), &paths)?;
            Ok(RunOutcome::Success)
        }
        Some(ChooserExit::Cancelled) => Ok(RunOutcome::Cancelled),
        None => Ok(RunOutcome::Success),
    }
}

fn run_blocking_in_terminal(program: &str, args: &[String]) {
    let _ = Command::new(program).args(args).status();
}

#[cfg(unix)]
fn run_blocking_in_terminal_result(
    program: &str,
    args: &[String],
) -> std::io::Result<std::process::ExitStatus> {
    Command::new(program).args(args).status()
}

fn refresh_after_shell(app: &mut App, cwd: &Path) {
    let cwd_label = path_display::user_facing(cwd);
    match cwd.try_exists() {
        Ok(true) => {
            if let Err(error) = app.reload() {
                app.report_runtime_error("Shell refresh failed", &error);
            }
        }
        Ok(false) => app.set_status_message(format!(
            "Current folder was removed while shell was open: {}",
            cwd_label
        )),
        Err(error) => app.set_status_message(format!(
            "Could not refresh {cwd_label} after shell: {error}"
        )),
    }
}

fn apply_zoxide_query_result(app: &mut App, result: zoxide::QueryResult) {
    match result {
        zoxide::QueryResult::Selected(path) => app.open_zoxide_selection(path),
        zoxide::QueryResult::Cancelled => {}
        zoxide::QueryResult::NotFound => app.set_status_message("zoxide not found"),
        zoxide::QueryResult::PickerNotFound => app.set_status_message("fzf not found"),
        zoxide::QueryResult::Empty => app.set_status_message("No zoxide directory history found"),
        zoxide::QueryResult::OnlyCurrentDirectory => {
            app.set_status_message("Zoxide history only contains the current directory")
        }
        zoxide::QueryResult::LaunchFailed => app.set_status_message("Could not run zoxide"),
    }
}

fn run_app(
    terminal: &mut AppTerminal,
    drainer: &Drainer,
    kitty_dnd: &kitty_dnd::KittyDndRuntime,
    cwd: Option<PathBuf>,
    start_focus: Option<PathBuf>,
    reveal_hidden_start_focus: bool,
    chooser_enabled: bool,
) -> Result<AppExit> {
    #[cfg(unix)]
    if kitty_dnd.is_enabled() {
        kitty_dnd::prewarm_drag_image_renderer();
    }

    let mut app = match cwd {
        Some(cwd) => App::new_at_startup(cwd, start_focus, reveal_hidden_start_focus)?,
        None => App::new()?,
    };
    if chooser_enabled {
        app.enable_chooser_mode();
    }
    app.refresh_git_branch();

    // Enable terminal image previews. Detection handles the current policy:
    // Kitty, Ghostty, Warp, WezTerm, iTerm2, and Konsole auto-enable supported
    // image protocols;
    // ELIO_IMAGE_PREVIEWS=1 force-enables Kitty graphics on otherwise unrecognized terminals.
    // All image bytes are routed through terminal.backend_mut() so they never bypass
    // crossterm and cannot corrupt mouse reporting.
    app.enable_terminal_image_previews();

    let input_reader = match RuntimeInputReader::new(kitty_dnd.is_enabled()) {
        Ok(reader) => reader,
        Err(error) => {
            if kitty_dnd.is_enabled() {
                terminal
                    .backend_mut()
                    .write_all(kitty_dnd::disable_sequence().as_bytes())?;
                terminal.backend_mut().flush()?;
                app.set_status_message(format!("Kitty DND disabled: {error}"));
            }
            RuntimeInputReader::Crossterm
        }
    };
    let mut dirty = true;
    let mut search_cursor_active = false;
    let mut terminal_focused = true;
    let mut last_relative_time_refresh_at = Instant::now();
    #[cfg(unix)]
    let mut pending_drag_out = PendingDragOut::default();
    #[cfg(unix)]
    let mut pending_drop_in = PendingDropIn::default();

    loop {
        if app.should_quit {
            break;
        }

        if terminal_focused
            && last_relative_time_refresh_at.elapsed() >= RELATIVE_TIME_REFRESH_INTERVAL
        {
            dirty = true;
            last_relative_time_refresh_at = Instant::now();
        }

        // Background work and drawing run regardless of focus so a still-visible
        // pane stays live. tmux delivers FocusLost to a pane the moment it stops
        // being the active pane (verified: switching panes — not just windows —
        // sends \e[O), and again when another GUI app steals the outer terminal's
        // focus. In a tiled tmux layout that is most of the time, so gating these on
        // focus made elio appear frozen — in-flight previews, directory loads, and
        // filesystem changes never landed until focus returned. Drawing stays cheap
        // when idle because it only runs when `dirty`, which is set solely by real
        // state changes; an unfocused, idle pane sets nothing dirty and never draws.
        // These all run together so the deferred-refresh coordination (see #64) that
        // process_background_jobs shares with the directory timers cannot desync.
        // Only the cosmetic per-second relative-time tick (above) and the poll
        // cadence below stay focus-dependent.
        if app.process_background_jobs() {
            dirty = true;
        }

        if app.process_pdf_preview_timers() {
            dirty = true;
        }

        if app.process_pending_scroll() {
            dirty = true;
        }

        if app.process_preview_refresh_timers() {
            dirty = true;
        }

        if app.process_preview_prefetch_timers() {
            dirty = true;
        }

        if app.process_directory_stats_timer() {
            dirty = true;
        }

        if app.process_directory_item_count_timer() {
            dirty = true;
        }

        if app.process_browser_wheel_timers() {
            dirty = true;
        }

        if app.process_image_preview_timers() {
            dirty = true;
        }

        if app.process_terminal_image_resize_settle_timer() {
            dirty = true;
        }

        if app.process_sidebar_refresh() {
            dirty = true;
        }

        match app.process_auto_reload() {
            Ok(changed) => {
                dirty |= changed;
            }
            Err(error) => {
                app.report_runtime_error("Auto-reload failed", &error);
                dirty = true;
            }
        }

        if dirty {
            dirty = draw_terminal_frame(terminal, &mut app)?;
        }

        let wants_search_cursor = app.search_is_open()
            || app.local_filter_is_editing()
            || app.create_is_open()
            || app.rename_is_open()
            || app.bulk_rename_is_open()
            || app.archive_create_is_open()
            || app.archive_password_is_open();
        if wants_search_cursor != search_cursor_active {
            if wants_search_cursor {
                terminal.show_cursor()?;
            } else {
                terminal.hide_cursor()?;
            }
            execute!(
                terminal.backend_mut(),
                if wants_search_cursor {
                    SetCursorStyle::SteadyBar
                } else {
                    SetCursorStyle::DefaultUserShape
                }
            )?;
            search_cursor_active = wants_search_cursor;
        }

        let base_poll_interval = if !terminal_focused {
            IDLE_POLL_INTERVAL
        } else if app.has_pending_scroll()
            || app.has_pending_auto_reload()
            || app.has_pending_background_work()
        {
            if app.is_windows_terminal() {
                WINDOWS_TERMINAL_ACTIVE_POLL_INTERVAL
            } else {
                ACTIVE_SCROLL_POLL_INTERVAL
            }
        } else {
            IDLE_POLL_INTERVAL
        };
        let poll_interval = event_poll_interval(
            base_poll_interval,
            terminal_focused,
            [
                app.pending_pdf_preview_timer(),
                app.pending_image_preview_timer(),
                app.pending_terminal_image_resize_settle_timer(),
                app.pending_preview_refresh_timer(),
                app.pending_preview_prefetch_timer(),
                app.pending_directory_stats_timer(),
                app.pending_directory_item_count_timer(),
                app.pending_browser_wheel_timer(),
            ],
        );

        if let Some(first_input) = read_runtime_input(&input_reader, poll_interval)? {
            // Batch all immediately-available events into one render cycle.
            // This prevents lag when events (especially scroll events from high-frequency
            // terminals) arrive faster than the app can render: instead of one render per
            // event we accumulate all queued events first and render the final state once.
            let mut next_input = Some(first_input);
            loop {
                let input = match next_input.take() {
                    Some(input) => input,
                    None => match try_read_runtime_input(&input_reader)? {
                        Some(input) => input,
                        None => break,
                    },
                };
                let input = coalesce_resize_inputs(&input_reader, input, &mut next_input)?;
                #[cfg(unix)]
                let input_result = handle_runtime_input(
                    terminal,
                    &mut app,
                    input,
                    &mut pending_drag_out,
                    &mut pending_drop_in,
                )?;
                #[cfg(not(unix))]
                let input_result = handle_runtime_input(input)?;
                let Some(event) = input_result else {
                    dirty = true;
                    next_input = try_read_runtime_input(&input_reader)?;
                    continue;
                };
                if std::env::var_os("ELIO_LOG_MOUSE").is_some()
                    && let Event::Mouse(m) = &event
                {
                    let _ = std::fs::OpenOptions::new()
                        .create(true)
                        .append(true)
                        .open(std::env::temp_dir().join("elio-mouse.log"))
                        .and_then(|mut f| {
                            writeln!(f, "{:?} col={} row={}", m.kind, m.column, m.row)
                        });
                }
                if matches!(event, Event::FocusLost) {
                    terminal_focused = false;
                } else if matches!(event, Event::FocusGained) {
                    terminal_focused = true;
                    app.handle_terminal_image_focus_gained();
                    dirty = true;
                } else if matches!(event, Event::Resize(_, _)) {
                    app.handle_terminal_image_resize();
                    dirty = true;
                } else {
                    // Input doubles as an implicit FocusGained; see
                    // event_implies_terminal_focus for why the real one can be dropped.
                    if !terminal_focused && event_implies_terminal_focus(&event) {
                        terminal_focused = true;
                        app.handle_terminal_image_focus_gained();
                        dirty = true;
                    }
                    // Mouse move events only update the hover/target state — nothing
                    // visual changes, so they don't need a re-render. Skipping dirty here
                    // avoids the constant re-render storm that ?1003h (any-event tracking)
                    // causes in terminals like Alacritty, Ghostty, and Gnome Terminal.
                    let needs_render = !matches!(
                        event,
                        Event::Mouse(MouseEvent {
                            kind: MouseEventKind::Moved,
                            ..
                        })
                    );
                    let _ = app.handle_event(event);
                    if needs_render && terminal_focused {
                        dirty = true;
                    }
                };
                next_input = try_read_runtime_input(&input_reader)?;
            }

            if app.should_quit {
                break;
            }

            // A terminal task (e.g. nvim from Open With, or zoxide) needs the real terminal.
            // Suspend the TUI, run the task blocking, then restore.
            if let Some(task) = app.pending_terminal_task.take() {
                let zoxide_result = match task {
                    PendingTerminalTask::Command { program, args } => {
                        pause_runtime_input(&input_reader, true);
                        suspend_terminal(terminal, drainer, true, kitty_dnd)?;
                        run_blocking_in_terminal(&program, &args);
                        resume_terminal(terminal, drainer, kitty_dnd)?;
                        app.invalidate_terminal_image_overlay_after_terminal_task();
                        pause_runtime_input(&input_reader, false);
                        None
                    }
                    PendingTerminalTask::Commands(commands) => {
                        pause_runtime_input(&input_reader, true);
                        suspend_terminal(terminal, drainer, true, kitty_dnd)?;
                        for (program, args) in commands {
                            run_blocking_in_terminal(&program, &args);
                        }
                        resume_terminal(terminal, drainer, kitty_dnd)?;
                        app.invalidate_terminal_image_overlay_after_terminal_task();
                        pause_runtime_input(&input_reader, false);
                        None
                    }
                    #[cfg(unix)]
                    PendingTerminalTask::EditorBulkRename {
                        program,
                        args,
                        session,
                    } => {
                        pause_runtime_input(&input_reader, true);
                        suspend_terminal(terminal, drainer, true, kitty_dnd)?;
                        let result = run_blocking_in_terminal_result(&program, &args);
                        resume_terminal(terminal, drainer, kitty_dnd)?;
                        app.invalidate_terminal_image_overlay_after_terminal_task();
                        pause_runtime_input(&input_reader, false);
                        if let Err(error) = app.finish_editor_bulk_rename(session, result) {
                            app.report_runtime_error("Editor rename failed", &error);
                        }
                        None
                    }
                    PendingTerminalTask::Shell { cwd } => {
                        pause_runtime_input(&input_reader, true);
                        suspend_terminal(terminal, drainer, true, kitty_dnd)?;
                        let shell_result = shell::run_in_current_terminal(&cwd);
                        resume_terminal(terminal, drainer, kitty_dnd)?;
                        app.invalidate_terminal_image_overlay_after_terminal_task();
                        pause_runtime_input(&input_reader, false);
                        match shell_result {
                            Ok(()) => refresh_after_shell(&mut app, &cwd),
                            Err(error) => app.set_status_message(error),
                        }
                        None
                    }
                    PendingTerminalTask::Zoxide => {
                        let cwd = app.navigation.cwd.clone();
                        if let Some(result) = zoxide::preflight(&cwd) {
                            Some(result)
                        } else {
                            pause_runtime_input(&input_reader, true);
                            suspend_terminal(terminal, drainer, false, kitty_dnd)?;
                            let result = zoxide::run_query_in_terminal(&cwd);
                            resume_terminal(terminal, drainer, kitty_dnd)?;
                            app.invalidate_terminal_image_overlay_after_terminal_task();
                            pause_runtime_input(&input_reader, false);
                            Some(result)
                        }
                    }
                };
                if let Some(result) = zoxide_result {
                    apply_zoxide_query_result(&mut app, result);
                }
                dirty = true;
            }
        }
    }

    let final_cwd = app
        .should_change_directory_on_quit
        .then(|| app.navigation.cwd.clone());
    let chooser = app.take_chooser_exit();
    app.queue_forced_iterm_preview_erase();
    let mut overlay_bytes = app.clear_preview_overlay()?;
    overlay_bytes.extend(app.iterm_pre_draw_erase());
    if !overlay_bytes.is_empty() {
        terminal.backend_mut().write_all(&overlay_bytes)?;
        terminal.backend_mut().flush()?;
    }
    Ok(AppExit { final_cwd, chooser })
}

/// Whether receiving `event` implies the terminal is focused. Key, mouse, and
/// paste events can only originate from a focused terminal, so they double as an
/// implicit FocusGained — the recovery path for terminals (notably on
/// Wayland/Hyprland) that drop the real FocusGained after a spawned GUI app
/// returns focus. Focus and resize events carry no such implication.
fn event_implies_terminal_focus(event: &Event) -> bool {
    matches!(event, Event::Key(_) | Event::Mouse(_) | Event::Paste(_))
}

fn read_runtime_input(
    reader: &RuntimeInputReader,
    timeout: Duration,
) -> Result<Option<RuntimeInputEvent>> {
    match reader {
        RuntimeInputReader::Crossterm => {
            if event::poll(timeout)? {
                Ok(Some(RuntimeInputEvent::Terminal(event::read()?)))
            } else {
                Ok(None)
            }
        }
        #[cfg(unix)]
        RuntimeInputReader::Custom(reader) => Ok(reader.recv_timeout(timeout)?),
    }
}

fn try_read_runtime_input(reader: &RuntimeInputReader) -> Result<Option<RuntimeInputEvent>> {
    match reader {
        RuntimeInputReader::Crossterm => {
            if event::poll(Duration::ZERO)? {
                Ok(Some(RuntimeInputEvent::Terminal(event::read()?)))
            } else {
                Ok(None)
            }
        }
        #[cfg(unix)]
        RuntimeInputReader::Custom(reader) => Ok(reader.try_recv()?),
    }
}

fn pause_runtime_input(reader: &RuntimeInputReader, paused: bool) {
    #[cfg(unix)]
    if let RuntimeInputReader::Custom(reader) = reader {
        reader.set_paused(paused);
    }
    #[cfg(not(unix))]
    let _ = (reader, paused);
}

fn coalesce_resize_inputs(
    reader: &RuntimeInputReader,
    input: RuntimeInputEvent,
    next_input: &mut Option<RuntimeInputEvent>,
) -> Result<RuntimeInputEvent> {
    let RuntimeInputEvent::Terminal(Event::Resize(mut width, mut height)) = input else {
        return Ok(input);
    };

    loop {
        let candidate = if let Some(input) = next_input.take() {
            Some(input)
        } else {
            try_read_runtime_input(reader)?
        };
        match candidate {
            Some(RuntimeInputEvent::Terminal(Event::Resize(w, h))) => {
                width = w;
                height = h;
            }
            Some(other) => {
                *next_input = Some(other);
                break;
            }
            None => break,
        }
    }

    Ok(RuntimeInputEvent::Terminal(Event::Resize(width, height)))
}

#[cfg(unix)]
fn handle_runtime_input(
    terminal: &mut AppTerminal,
    app: &mut App,
    input: RuntimeInputEvent,
    pending_drag_out: &mut PendingDragOut,
    pending_drop_in: &mut PendingDropIn,
) -> Result<Option<Event>> {
    match input {
        RuntimeInputEvent::Terminal(event) => Ok(Some(event)),
        RuntimeInputEvent::KittyDnd(event) => {
            handle_kitty_dnd_event(terminal, app, event, pending_drag_out, pending_drop_in)?;
            Ok(None)
        }
    }
}

#[cfg(not(unix))]
fn handle_runtime_input(input: RuntimeInputEvent) -> Result<Option<Event>> {
    match input {
        RuntimeInputEvent::Terminal(event) => Ok(Some(event)),
    }
}

#[cfg(unix)]
fn handle_kitty_dnd_event(
    terminal: &mut AppTerminal,
    app: &mut App,
    event: kitty_dnd::KittyDndEvent,
    pending_drag_out: &mut PendingDragOut,
    pending_drop_in: &mut PendingDropIn,
) -> Result<()> {
    match event {
        kitty_dnd::KittyDndEvent::DropOffer {
            mime_index,
            operation,
            final_drop,
        } => {
            let chosen_op = choose_drop_op(operation);
            let sequence = if pending_drag_out.active && final_drop {
                pending_drag_out.reset();
                pending_drop_in.reset();
                app.clear_drag_state();
                format!(
                    "{}{}",
                    kitty_dnd::finish_drop_sequence(kitty_dnd::DropFinish::Reject),
                    kitty_dnd::cancel_drag_sequence()
                )
            } else if pending_drag_out.active {
                pending_drop_in.reset();
                kitty_dnd::reject_drop_sequence().to_string()
            } else if final_drop {
                pending_drop_in.set(chosen_op);
                kitty_dnd::request_drop_data_sequence(mime_index)
            } else {
                pending_drop_in.set(chosen_op);
                kitty_dnd::accept_drop_sequence(drop_op_to_dnd_operation(chosen_op))
            };
            terminal.backend_mut().write_all(sequence.as_bytes())?;
            terminal.backend_mut().flush()?;
        }
        kitty_dnd::KittyDndEvent::DropData {
            paths,
            unsupported_schemes,
            ..
        } => {
            let was_own_drag = pending_drag_out.active;
            let op = pending_drop_in.take();
            let mut finish = kitty_dnd::DropFinish::Reject;
            if was_own_drag {
                pending_drag_out.reset();
                app.clear_drag_state();
            } else if !unsupported_schemes.is_empty() {
                app.set_status_message(unsupported_drop_scheme_status(&unsupported_schemes));
            } else if let Some(op) = op {
                if app.drop_external_paths(paths, op)? {
                    finish = clip_op_to_drop_finish(op);
                }
            } else {
                app.set_status_message("Drop was not negotiated");
            }
            terminal
                .backend_mut()
                .write_all(kitty_dnd::finish_drop_sequence(finish).as_bytes())?;
            if was_own_drag {
                terminal
                    .backend_mut()
                    .write_all(kitty_dnd::cancel_drag_sequence().as_bytes())?;
            }
            terminal.backend_mut().flush()?;
        }
        kitty_dnd::KittyDndEvent::DropLeave => {}
        kitty_dnd::KittyDndEvent::DropDataError {
            mime_index: _,
            message,
        } => {
            let was_own_drag = pending_drag_out.active;
            if pending_drag_out.active {
                pending_drag_out.reset();
                app.clear_drag_state();
            }
            terminal.backend_mut().write_all(
                kitty_dnd::finish_drop_sequence(kitty_dnd::DropFinish::Reject).as_bytes(),
            )?;
            if was_own_drag {
                terminal
                    .backend_mut()
                    .write_all(kitty_dnd::cancel_drag_sequence().as_bytes())?;
            }
            if !message.is_empty() {
                app.set_status_message(format!("Drop failed: {message}"));
            }
            terminal.backend_mut().flush()?;
        }
        kitty_dnd::KittyDndEvent::DropUnsupported { final_drop } => {
            pending_drop_in.reset();
            let sequence = if final_drop {
                kitty_dnd::finish_drop_sequence(kitty_dnd::DropFinish::Reject)
            } else {
                kitty_dnd::reject_drop_sequence()
            };
            terminal.backend_mut().write_all(sequence.as_bytes())?;
            terminal.backend_mut().flush()?;
        }
        kitty_dnd::KittyDndEvent::DragOffer { x, y } => {
            if pending_drag_out.active {
                return Ok(());
            }
            let paths = app.take_drag_export_paths_at(x, y);
            let uri_list = kitty_dnd::uri_list_payload(&paths);
            if uri_list.is_empty() {
                pending_drag_out.reset();
                app.clear_drag_candidate();
                terminal
                    .backend_mut()
                    .write_all(kitty_dnd::cancel_drag_sequence().as_bytes())?;
                terminal.backend_mut().flush()?;
                return Ok(());
            }

            let label = drag_icon_label(app, &paths);
            let mut sequence = kitty_dnd::agree_drag_sequence(kitty_dnd::DndOperation::Either);
            sequence.push_str(&kitty_dnd::present_drag_data_sequence(0, &uri_list));
            sequence.push_str(&drag_icon_sequence(&label));
            sequence.push_str(kitty_dnd::start_drag_sequence());
            terminal.backend_mut().write_all(sequence.as_bytes())?;
            terminal.backend_mut().flush()?;

            pending_drag_out.active = true;
            pending_drag_out.uri_list = uri_list;
        }
        kitty_dnd::KittyDndEvent::DragDataRequested { mime_index } => {
            let sequence = if pending_drag_out.active && mime_index == 0 {
                kitty_dnd::send_drag_data_sequence(mime_index, &pending_drag_out.uri_list)
            } else {
                kitty_dnd::drag_data_error_sequence(mime_index, "ENOENT")
            };
            terminal.backend_mut().write_all(sequence.as_bytes())?;
            terminal.backend_mut().flush()?;
        }
        kitty_dnd::KittyDndEvent::DragStarted
        | kitty_dnd::KittyDndEvent::DragAccepted { mime_index: _ }
        | kitty_dnd::KittyDndEvent::DragActionChanged { operation: _ }
        | kitty_dnd::KittyDndEvent::DragDropped => {}
        kitty_dnd::KittyDndEvent::DragEnded { cancelled: _ } => {
            pending_drag_out.reset();
            app.clear_drag_state();
        }
        kitty_dnd::KittyDndEvent::DragError { message } => {
            pending_drag_out.reset();
            app.clear_drag_state();
            if !message.is_empty() {
                app.set_status_message(format!("Kitty DND drag failed: {message}"));
            }
        }
    }
    Ok(())
}

#[cfg(unix)]
fn unsupported_drop_scheme_status(schemes: &[String]) -> String {
    match schemes {
        [] => "Drop contains no local files".to_string(),
        [scheme] => format!("Unsupported drop URI scheme: {scheme}"),
        schemes => format!("Unsupported drop URI schemes: {}", schemes.join(", ")),
    }
}

#[cfg(unix)]
fn choose_drop_op(operation: kitty_dnd::DndOperation) -> ClipOp {
    match operation {
        kitty_dnd::DndOperation::Copy => ClipOp::Yank,
        kitty_dnd::DndOperation::Move | kitty_dnd::DndOperation::Either => ClipOp::Cut,
    }
}

#[cfg(unix)]
fn drop_op_to_dnd_operation(op: ClipOp) -> kitty_dnd::DndOperation {
    match op {
        ClipOp::Yank => kitty_dnd::DndOperation::Copy,
        ClipOp::Cut => kitty_dnd::DndOperation::Move,
    }
}

#[cfg(unix)]
fn clip_op_to_drop_finish(op: ClipOp) -> kitty_dnd::DropFinish {
    match op {
        ClipOp::Yank => kitty_dnd::DropFinish::Copy,
        ClipOp::Cut => kitty_dnd::DropFinish::Move,
    }
}

#[cfg(unix)]
fn drag_icon_sequence(label: &DragIconLabel) -> String {
    let palette = ui::theme::palette();
    if let Some(image) = kitty_dnd::render_drag_image(
        &label.icon,
        &label.text,
        label.icon_color,
        palette.elevated,
        palette.text,
    ) {
        return kitty_dnd::present_drag_icon_png_sequence(image.width, image.height, &image.png);
    }

    kitty_dnd::present_drag_icon_sequence(&label.as_text())
}

#[cfg(unix)]
impl DragIconLabel {
    fn as_text(&self) -> String {
        format!("{} {}", self.icon, self.text)
    }
}

#[cfg(unix)]
fn drag_icon_label(app: &App, paths: &[PathBuf]) -> DragIconLabel {
    drag_icon_label_with(paths, |path| drag_icon_for_path(app, path))
}

#[cfg(unix)]
fn drag_icon_label_with<F>(paths: &[PathBuf], mut icon_for_path: F) -> DragIconLabel
where
    F: FnMut(&Path) -> (String, ratatui::style::Color),
{
    const MAX_LABEL_CHARS: usize = 32;

    let (icon, text) = match paths {
        [path] => {
            let (icon, icon_color) = icon_for_path(path);
            let text = path
                .file_name()
                .map(|name| crate::app::sanitize_terminal_text(&name.to_string_lossy()))
                .filter(|name| !name.is_empty())
                .unwrap_or_else(|| "1 item".to_string());
            return DragIconLabel {
                icon,
                text: truncate_drag_label_text(&text, MAX_LABEL_CHARS),
                icon_color,
            };
        }
        paths => (
            drag_icon_for_many(paths, &mut icon_for_path),
            format!("{} items", paths.len()),
        ),
    };
    let text = truncate_drag_label_text(&text, MAX_LABEL_CHARS);
    DragIconLabel {
        icon: icon.0,
        text,
        icon_color: icon.1,
    }
}

#[cfg(unix)]
fn drag_icon_for_path(app: &App, path: &Path) -> (String, ratatui::style::Color) {
    if let Some(entry) = app
        .navigation
        .entries
        .iter()
        .find(|entry| entry.path == path)
    {
        let appearance = ui::theme::resolve_browser_entry(entry);
        return (appearance.icon.to_string(), appearance.color);
    }

    let appearance = ui::theme::resolve_path(path, drag_entry_kind(path));
    (appearance.icon.to_string(), appearance.color)
}

#[cfg(unix)]
fn drag_entry_kind(path: &Path) -> EntryKind {
    match std::fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_dir() => EntryKind::Directory,
        _ => EntryKind::File,
    }
}

#[cfg(unix)]
fn drag_icon_for_many<F>(
    paths: &[PathBuf],
    icon_for_path: &mut F,
) -> (String, ratatui::style::Color)
where
    F: FnMut(&Path) -> (String, ratatui::style::Color),
{
    const MULTIPLE_FOLDERS_ICON: &str = "󰉓";
    const MULTIPLE_FILES_ICON: &str = "";

    let Some((first, rest)) = paths.split_first() else {
        let appearance = ui::theme::resolve_path(Path::new("item"), EntryKind::File);
        return (MULTIPLE_FILES_ICON.to_string(), appearance.color);
    };

    let first_kind = drag_entry_kind(first);
    let (first_icon, first_color) = icon_for_path(first);
    let mut all_same_icon = true;
    let mut all_directories = first_kind == EntryKind::Directory;

    for path in rest {
        let kind = drag_entry_kind(path);
        let (icon, _) = icon_for_path(path);
        all_same_icon &= icon == first_icon;
        all_directories &= kind == EntryKind::Directory;
    }

    if all_same_icon {
        (first_icon, first_color)
    } else if all_directories {
        let appearance = ui::theme::resolve_path(Path::new("folder"), EntryKind::Directory);
        (MULTIPLE_FOLDERS_ICON.to_string(), appearance.color)
    } else {
        let appearance = ui::theme::resolve_path(Path::new("item"), EntryKind::File);
        (MULTIPLE_FILES_ICON.to_string(), appearance.color)
    }
}

#[cfg(unix)]
fn truncate_drag_label_text(label: &str, max_chars: usize) -> String {
    if label.chars().count() <= max_chars {
        return label.to_string();
    }
    if max_chars <= 3 {
        return ".".repeat(max_chars);
    }

    let mut truncated: String = label.chars().take(max_chars - 3).collect();
    truncated.push_str("...");
    truncated
}

fn event_poll_interval<I>(
    base_poll_interval: Duration,
    terminal_focused: bool,
    timers: I,
) -> Duration
where
    I: IntoIterator<Item = Option<Duration>>,
{
    if !terminal_focused {
        return base_poll_interval;
    }

    timers
        .into_iter()
        .flatten()
        .min()
        .map(|delay| delay.min(base_poll_interval))
        .unwrap_or(base_poll_interval)
}

#[cfg(test)]
mod tests {
    use super::{
        ACTIVE_SCROLL_POLL_INTERVAL, IDLE_POLL_INTERVAL, event_implies_terminal_focus,
        event_poll_interval,
    };
    #[cfg(unix)]
    use super::{drag_icon_label_with, unsupported_drop_scheme_status};
    use crossterm::event::Event;
    use std::time::Duration;
    #[cfg(unix)]
    use std::{
        fs,
        path::PathBuf,
        time::{SystemTime, UNIX_EPOCH},
    };

    #[test]
    fn event_poll_interval_stays_idle_while_terminal_is_unfocused() {
        let interval = event_poll_interval(
            IDLE_POLL_INTERVAL,
            false,
            [
                Some(Duration::from_millis(25)),
                Some(Duration::from_millis(10)),
            ],
        );

        assert_eq!(interval, IDLE_POLL_INTERVAL);
    }

    #[test]
    fn event_poll_interval_uses_pending_timer_when_terminal_is_focused() {
        let delay = Duration::from_millis(25);
        let interval = event_poll_interval(
            ACTIVE_SCROLL_POLL_INTERVAL,
            true,
            [None, Some(delay), Some(Duration::from_millis(50))],
        );

        assert!(interval <= delay);
    }

    #[test]
    fn input_events_imply_terminal_focus() {
        use crossterm::event::{
            KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
        };

        // Key, mouse, and paste events recover focus after a dropped FocusGained.
        assert!(event_implies_terminal_focus(&Event::Key(KeyEvent::new(
            KeyCode::Char('j'),
            KeyModifiers::NONE
        ))));
        assert!(event_implies_terminal_focus(&Event::Mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 0,
            row: 0,
            modifiers: KeyModifiers::NONE,
        })));
        assert!(event_implies_terminal_focus(&Event::Paste(
            "text".to_string()
        )));
    }

    #[test]
    fn focus_and_resize_events_do_not_imply_terminal_focus() {
        // Focus/resize events carry no focus implication and must not trip the
        // recovery path (FocusLost in particular would be misread as focus).
        assert!(!event_implies_terminal_focus(&Event::FocusLost));
        assert!(!event_implies_terminal_focus(&Event::FocusGained));
        assert!(!event_implies_terminal_focus(&Event::Resize(80, 24)));
    }

    #[cfg(unix)]
    #[test]
    fn unsupported_drop_scheme_status_names_one_or_many_schemes() {
        assert_eq!(
            unsupported_drop_scheme_status(&["trash".to_string()]),
            "Unsupported drop URI scheme: trash"
        );
        assert_eq!(
            unsupported_drop_scheme_status(&["trash".to_string(), "smb".to_string()]),
            "Unsupported drop URI schemes: trash, smb"
        );
    }

    #[cfg(unix)]
    #[test]
    fn drag_icon_label_uses_name_for_single_item_and_count_for_many() {
        assert_eq!(
            drag_icon_label_with(&[PathBuf::from("/tmp/report.pdf")], |_| (
                "󰈙".to_string(),
                ratatui::style::Color::White,
            ))
            .as_text(),
            "󰈙 report.pdf"
        );
        assert_eq!(
            drag_icon_label_with(&[PathBuf::from("/tmp/a"), PathBuf::from("/tmp/b")], |_| (
                "󰈔".to_string(),
                ratatui::style::Color::White,
            ))
            .as_text(),
            "󰈔 2 items"
        );
    }

    #[cfg(unix)]
    #[test]
    fn drag_icon_label_uses_multi_item_icon_for_mixed_selection() {
        let paths = [PathBuf::from("/tmp/a"), PathBuf::from("/tmp/b")];

        assert_eq!(
            drag_icon_label_with(&paths, |path| (
                if path.ends_with("a") { "󰉋" } else { "󰈔" }.to_string(),
                ratatui::style::Color::White,
            ))
            .as_text(),
            " 2 items"
        );
    }

    #[cfg(unix)]
    #[test]
    fn drag_icon_label_uses_multi_folder_icon_for_different_folder_icons() {
        let root = std::env::temp_dir().join(format!(
            "elio-drag-icons-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        let first = root.join("src");
        let second = root.join("docs");
        fs::create_dir_all(&first).unwrap();
        fs::create_dir_all(&second).unwrap();

        let label = drag_icon_label_with(&[first, second], |path| {
            let icon = if path.ends_with("src") {
                "󰉋"
            } else {
                "󰉓"
            };
            (icon.to_string(), ratatui::style::Color::White)
        })
        .as_text();
        fs::remove_dir_all(root).unwrap();

        assert_eq!(label, "󰉓 2 items");
    }

    #[cfg(unix)]
    #[test]
    fn drag_icon_label_truncates_long_names() {
        assert_eq!(
            drag_icon_label_with(
                &[PathBuf::from(
                    "/tmp/abcdefghijklmnopqrstuvwxyz0123456789.txt"
                )],
                |_| ("󰈔".to_string(), ratatui::style::Color::White)
            )
            .as_text(),
            "󰈔 abcdefghijklmnopqrstuvwxyz012..."
        );
    }
}
