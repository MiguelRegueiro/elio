#[cfg(unix)]
use super::super::state::{BulkRenameEditorSession, EditorRenameConfirmOverlay};
use super::super::{
    App,
    state::{BulkRenameItem, DirectoryHistoryMode, DirectoryLoadCompletion, PendingDirectoryLoad},
};
use crate::fs::rect_contains;
#[cfg(unix)]
use anyhow::Context;
use anyhow::{Result, bail};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use std::{
    collections::HashSet,
    fs,
    path::{Component, Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};
#[cfg(unix)]
use std::{
    env,
    io::{Read, Write},
    os::unix::{fs::OpenOptionsExt, io::AsRawFd},
};

#[cfg(unix)]
const MAX_EDITOR_RENAME_BYTES: u64 = 1024 * 1024;

impl App {
    #[cfg(unix)]
    pub(in crate::app) fn open_editor_bulk_rename(&mut self) -> Result<()> {
        if self.navigation.in_trash || self.cwd_is_inside_trash_subfolder() {
            return Ok(());
        }

        let selected_paths = self.editor_bulk_rename_targets();
        if selected_paths.is_empty() {
            return Ok(());
        }
        if selected_paths
            .iter()
            .any(|path| self.trash_target_is_inside_trash(path))
        {
            self.status = "Cannot rename items from Trash".to_string();
            return Ok(());
        }

        let root = common_root(&selected_paths);
        let mut rows = Vec::with_capacity(selected_paths.len());
        for path in &selected_paths {
            rows.push(
                path.strip_prefix(&root)
                    .unwrap_or(path)
                    .to_string_lossy()
                    .into_owned(),
            );
        }

        let context = crate::config::invoking_user_context();
        let invoking_user = editor_temp_owner(&context)?;
        let expected_temp_owner = invoking_user.map(|(uid, _)| uid);
        let temp_path = create_temp_file(&rows, invoking_user)?;
        let (program, mut args) = editor_command();
        args.push(temp_path.to_string_lossy().into_owned());

        self.close_transient_overlays();
        self.pending_terminal_task = Some(crate::app::PendingTerminalTask::EditorBulkRename {
            program,
            args,
            session: BulkRenameEditorSession {
                root,
                temp_path,
                expected_temp_owner,
                items: selected_paths
                    .into_iter()
                    .map(bulk_rename_item_from_path)
                    .collect(),
            },
        });
        self.status.clear();
        Ok(())
    }

    #[cfg(not(unix))]
    pub(in crate::app) fn open_editor_bulk_rename(&mut self) -> Result<()> {
        self.status = "Editor batch rename is only supported on Unix-like systems".to_string();
        Ok(())
    }

    #[cfg(unix)]
    pub(crate) fn finish_editor_bulk_rename(
        &mut self,
        session: BulkRenameEditorSession,
        launch_result: std::io::Result<std::process::ExitStatus>,
    ) -> Result<()> {
        let BulkRenameEditorSession {
            root,
            temp_path,
            expected_temp_owner,
            items,
        } = session;

        let result = (|| -> Result<()> {
            match launch_result {
                Ok(status) if status.success() => {}
                Ok(status) => {
                    self.status = format!("Editor exited with {status}");
                    return Ok(());
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    self.status = "Editor not found".to_string();
                    return Ok(());
                }
                Err(error) => {
                    self.status = format!("Could not run editor: {error}");
                    return Ok(());
                }
            }

            let edited = read_editor_rename_file(&temp_path, expected_temp_owner)
                .with_context(|| format!("failed to read {}", temp_path.display()))?;
            let mut new_rows: Vec<String> = edited.lines().map(str::to_owned).collect();
            if edited.ends_with('\n') && new_rows.last().is_some_and(String::is_empty) {
                new_rows.pop();
            }

            if new_rows.len() != items.len() {
                self.status = format!(
                    "Editor rename aborted: expected {} line{}, got {}",
                    items.len(),
                    if items.len() == 1 { "" } else { "s" },
                    new_rows.len()
                );
                return Ok(());
            }

            let original_rows: Vec<String> = items
                .iter()
                .map(|item| {
                    item.path
                        .strip_prefix(&root)
                        .unwrap_or(&item.path)
                        .to_string_lossy()
                        .into_owned()
                })
                .collect();
            if new_rows == original_rows {
                self.status = "No files renamed".to_string();
                return Ok(());
            }

            match build_rename_plan(&items, &new_rows, Some(&root)) {
                Ok(plan) if plan.is_empty() => {
                    self.status = "No files renamed".to_string();
                }
                Ok(_) => {
                    self.overlays.editor_rename_confirm = Some(EditorRenameConfirmOverlay {
                        items,
                        new_names: new_rows,
                        root,
                        scroll: 0,
                        confirmed: true,
                    });
                    self.status.clear();
                }
                Err(errors) => {
                    self.status = editor_validation_status(&errors);
                }
            }
            Ok(())
        })();

        let _ = fs::remove_file(&temp_path);
        result
    }

    #[cfg(unix)]
    fn editor_bulk_rename_targets(&self) -> Vec<PathBuf> {
        if !self.navigation.selected_paths.is_empty() {
            return self.selected_paths_in_selection_order();
        }
        self.selected_entry()
            .map(|entry| vec![entry.path.clone()])
            .unwrap_or_default()
    }

    #[cfg(unix)]
    fn close_transient_overlays(&mut self) {
        self.overlays.help = false;
        self.overlays.search = None;
        self.overlays.create = None;
        self.overlays.rename = None;
        self.overlays.trash = None;
        self.overlays.restore = None;
        self.overlays.bulk_rename = None;
        self.overlays.editor_rename_confirm = None;
    }

    pub fn editor_rename_confirm_is_open(&self) -> bool {
        self.overlays.editor_rename_confirm.is_some()
    }

    pub fn editor_rename_confirm_count(&self) -> usize {
        self.overlays
            .editor_rename_confirm
            .as_ref()
            .map_or(0, |overlay| overlay.items.len())
    }

    pub fn editor_rename_confirm_scroll(&self) -> usize {
        self.overlays
            .editor_rename_confirm
            .as_ref()
            .map_or(0, |overlay| overlay.scroll)
    }

    pub fn editor_rename_confirm_row(&self, index: usize) -> Option<(String, String)> {
        let overlay = self.overlays.editor_rename_confirm.as_ref()?;
        let item = overlay.items.get(index)?;
        let old = item
            .path
            .strip_prefix(&overlay.root)
            .unwrap_or(&item.path)
            .to_string_lossy()
            .into_owned();
        let new = overlay.new_names.get(index)?.clone();
        Some((old, new))
    }

    pub fn editor_rename_confirm_title(&self) -> String {
        match self.editor_rename_confirm_count() {
            1 => "Confirm rename?".to_string(),
            count => format!("Confirm {count} renames?"),
        }
    }

    pub fn editor_rename_confirmed(&self) -> bool {
        self.overlays
            .editor_rename_confirm
            .as_ref()
            .is_some_and(|overlay| overlay.confirmed)
    }

    pub(in crate::app) fn cancel_editor_rename_confirm(&mut self) {
        self.overlays.editor_rename_confirm = None;
        self.status = "Editor rename cancelled".to_string();
    }

    pub(in crate::app) fn scroll_editor_rename_confirm(&mut self, delta: isize) {
        if let Some(overlay) = &mut self.overlays.editor_rename_confirm {
            let max_scroll = overlay.items.len().saturating_sub(1);
            overlay.scroll = overlay.scroll.saturating_add_signed(delta).min(max_scroll);
        }
    }

    pub(in crate::app) fn confirm_editor_rename(&mut self) -> Result<()> {
        let Some(overlay) = &self.overlays.editor_rename_confirm else {
            return Ok(());
        };
        let root = overlay.root.clone();
        let plan = match build_rename_plan(&overlay.items, &overlay.new_names, Some(&root)) {
            Ok(plan) => plan,
            Err(errors) => {
                self.status = editor_validation_status(&errors);
                return Ok(());
            }
        };
        let changed_old_paths: Vec<PathBuf> = plan.iter().map(|op| op.old_path.clone()).collect();
        let reload_cwd = self
            .current_directory_escape_for_paths(&changed_old_paths)
            .unwrap_or_else(|| self.navigation.cwd.clone());

        let applied = match apply_rename_ops(&plan) {
            Ok(applied) => applied,
            Err(error) => {
                self.status = error.to_string();
                return Ok(());
            }
        };
        let last_new_path = applied.last_new_path.clone();
        let status = rename_status(&plan, &applied);
        let duplicate_rename_pairs = plan
            .iter()
            .map(|op| (op.old_path.clone(), op.new_path.clone()))
            .collect::<Vec<_>>();

        self.overlays.editor_rename_confirm = None;
        self.navigation.selected_paths.clear();
        self.apply_duplicate_rename_pairs(duplicate_rename_pairs);
        self.queue_directory_load(PendingDirectoryLoad {
            token: 0,
            target_cwd: reload_cwd,
            previous_cwd: self.navigation.cwd.clone(),
            previous_selected_path: None,
            previous_selection_name: None,
            reselect_path: last_new_path,
            history_mode: DirectoryHistoryMode::None,
            refresh_search: false,
            completion: DirectoryLoadCompletion::Status(status),
        })?;
        Ok(())
    }

    pub(in crate::app) fn handle_editor_rename_confirm_key(&mut self, key: KeyEvent) -> Result<()> {
        if key.modifiers.contains(KeyModifiers::CONTROL) && matches!(key.code, KeyCode::Char('c')) {
            self.cancel_editor_rename_confirm();
            return Ok(());
        }

        match key.code {
            KeyCode::Esc if key.modifiers == KeyModifiers::NONE => {
                self.cancel_editor_rename_confirm();
            }
            KeyCode::Enter if key.modifiers == KeyModifiers::NONE => {
                if self.editor_rename_confirmed() {
                    self.confirm_editor_rename()?;
                } else {
                    self.cancel_editor_rename_confirm();
                }
            }
            KeyCode::Left | KeyCode::Char('h') if key.modifiers == KeyModifiers::NONE => {
                if let Some(overlay) = &mut self.overlays.editor_rename_confirm {
                    overlay.confirmed = true;
                }
            }
            KeyCode::Right | KeyCode::Char('l') if key.modifiers == KeyModifiers::NONE => {
                if let Some(overlay) = &mut self.overlays.editor_rename_confirm {
                    overlay.confirmed = false;
                }
            }
            KeyCode::Tab if key.modifiers == KeyModifiers::NONE => {
                if let Some(overlay) = &mut self.overlays.editor_rename_confirm {
                    overlay.confirmed = !overlay.confirmed;
                }
            }
            KeyCode::Up | KeyCode::Char('k') if key.modifiers == KeyModifiers::NONE => {
                self.scroll_editor_rename_confirm(-1);
            }
            KeyCode::Down | KeyCode::Char('j') if key.modifiers == KeyModifiers::NONE => {
                self.scroll_editor_rename_confirm(1);
            }
            KeyCode::PageUp if key.modifiers == KeyModifiers::NONE => {
                self.scroll_editor_rename_confirm(-10);
            }
            KeyCode::PageDown if key.modifiers == KeyModifiers::NONE => {
                self.scroll_editor_rename_confirm(10);
            }
            _ => {}
        }
        Ok(())
    }

    pub(in crate::app) fn handle_editor_rename_confirm_mouse(
        &mut self,
        mouse: MouseEvent,
    ) -> Result<()> {
        let inside = self
            .input
            .frame_state
            .rename_panel
            .is_some_and(|panel| rect_contains(panel, mouse.column, mouse.row));
        match mouse.kind {
            MouseEventKind::Down(MouseButton::Left) if !inside => {
                self.cancel_editor_rename_confirm();
            }
            MouseEventKind::Down(MouseButton::Left)
                if self
                    .input
                    .frame_state
                    .editor_rename_confirm_btn
                    .is_some_and(|rect| rect_contains(rect, mouse.column, mouse.row)) =>
            {
                self.confirm_editor_rename()?;
            }
            MouseEventKind::Down(MouseButton::Left)
                if self
                    .input
                    .frame_state
                    .editor_rename_cancel_btn
                    .is_some_and(|rect| rect_contains(rect, mouse.column, mouse.row)) =>
            {
                self.cancel_editor_rename_confirm();
            }
            MouseEventKind::ScrollUp if inside => {
                self.scroll_editor_rename_confirm(-1);
            }
            MouseEventKind::ScrollDown if inside => {
                self.scroll_editor_rename_confirm(1);
            }
            _ => {}
        }
        Ok(())
    }
}

pub(in crate::app::create) fn confirm_bulk_rename_overlay(app: &mut App) -> Result<()> {
    let Some(r) = &app.overlays.bulk_rename else {
        return Ok(());
    };

    let root = r.root.clone();
    let plan = build_rename_plan(&r.items, &r.new_names, root.as_deref());
    if let Err(errors) = plan {
        if let Some(err_line) = errors.iter().position(Option::is_some)
            && let Some(r) = &mut app.overlays.bulk_rename
        {
            r.line_errors = errors;
            r.cursor_line = err_line;
            r.cursor_col = r.cursor_col.min(r.new_names[err_line].chars().count());
            r.preferred_col = r.cursor_col;
        }
        return Ok(());
    }

    let ops = plan.expect("rename plan was checked");
    let changed_old_paths: Vec<PathBuf> = ops.iter().map(|op| op.old_path.clone()).collect();
    let reload_cwd = app
        .current_directory_escape_for_paths(&changed_old_paths)
        .unwrap_or_else(|| app.navigation.cwd.clone());

    let applied = match apply_rename_ops(&ops) {
        Ok(applied) => applied,
        Err(error) => {
            app.status = error.to_string();
            return Ok(());
        }
    };
    let last_new_path = applied.last_new_path.clone();
    let status = rename_status(&ops, &applied);
    let duplicate_rename_pairs = ops
        .iter()
        .map(|op| (op.old_path.clone(), op.new_path.clone()))
        .collect::<Vec<_>>();

    app.overlays.bulk_rename = None;
    app.navigation.selected_paths.clear();
    app.apply_duplicate_rename_pairs(duplicate_rename_pairs);

    app.queue_directory_load(PendingDirectoryLoad {
        token: 0,
        target_cwd: reload_cwd,
        previous_cwd: app.navigation.cwd.clone(),
        previous_selected_path: None,
        previous_selection_name: None,
        reselect_path: last_new_path,
        history_mode: DirectoryHistoryMode::None,
        refresh_search: false,
        completion: DirectoryLoadCompletion::Status(status),
    })?;
    Ok(())
}

fn editor_validation_status(errors: &[Option<String>]) -> String {
    errors
        .iter()
        .enumerate()
        .find_map(|(index, error)| {
            error
                .as_ref()
                .map(|error| format!("Editor rename aborted: line {}: {}", index + 1, error))
        })
        .unwrap_or_else(|| "Editor rename aborted".to_string())
}

#[derive(Clone, Debug)]
struct RenameOp {
    old_path: PathBuf,
    original_label: String,
    new_label: String,
    new_path: PathBuf,
}

#[derive(Debug, Default)]
struct AppliedRenames {
    renamed: usize,
    last_new_path: Option<PathBuf>,
}

struct StagedRename<'a> {
    op: &'a RenameOp,
    temp_path: PathBuf,
    temp_dir: PathBuf,
}

fn build_rename_plan(
    items: &[BulkRenameItem],
    new_names: &[String],
    root: Option<&Path>,
) -> std::result::Result<Vec<RenameOp>, Vec<Option<String>>> {
    let count = items.len();
    let mut errors = vec![None; count];
    if new_names.len() != count {
        if !errors.is_empty() {
            errors[0] = Some(format!(
                "Expected {} name{}, got {}",
                count,
                if count == 1 { "" } else { "s" },
                new_names.len()
            ));
        }
        return Err(errors);
    }

    let renaming_paths: HashSet<&Path> = items.iter().map(|item| item.path.as_path()).collect();
    let mut seen_new_paths = HashSet::new();
    let mut first_error = None;

    for (index, (item, new_name)) in items.iter().zip(new_names.iter()).enumerate() {
        let new_name = normalized_new_name(new_name, root);
        let target = match target_path(item, new_name, root) {
            Ok(path) => path,
            Err(message) => {
                errors[index] = Some(message);
                first_error.get_or_insert(index);
                continue;
            }
        };

        let err = if !seen_new_paths.insert(target.clone()) {
            Some(format!("\"{}\" appears more than once", new_name))
        } else if item.is_dir && target.starts_with(&item.path) && target != item.path {
            Some("Cannot move a folder inside itself".to_string())
        } else if target.parent().is_some_and(|parent| !parent.is_dir()) {
            Some("Destination folder does not exist".to_string())
        } else if target.exists() && !renaming_paths.contains(target.as_path()) {
            Some(format!("\"{}\" already exists", new_name))
        } else {
            None
        };

        if let Some(message) = err {
            errors[index] = Some(message);
            first_error.get_or_insert(index);
        }
    }

    if first_error.is_some() {
        return Err(errors);
    }

    Ok(items
        .iter()
        .zip(new_names.iter())
        .filter_map(|(item, new_name)| {
            let new_name = normalized_new_name(new_name, root);
            let new_path = target_path(item, new_name, root).ok()?;
            (new_path != item.path).then(|| RenameOp {
                old_path: item.path.clone(),
                original_label: display_label(item, root),
                new_label: new_name.to_string(),
                new_path,
            })
        })
        .collect())
}

fn normalized_new_name<'a>(new_name: &'a str, root: Option<&Path>) -> &'a str {
    if root.is_some() {
        new_name
    } else {
        new_name.trim()
    }
}

fn target_path(
    item: &BulkRenameItem,
    new_name: &str,
    root: Option<&Path>,
) -> std::result::Result<PathBuf, String> {
    if new_name.is_empty() {
        return Err("Name cannot be empty".to_string());
    }

    if let Some(root) = root {
        validate_relative_path(new_name)?;
        return Ok(root.join(new_name));
    }

    if new_name.contains('/') {
        return Err("Name cannot contain /".to_string());
    }
    Ok(renamed_path(&item.path, new_name))
}

fn validate_relative_path(value: &str) -> std::result::Result<(), String> {
    let path = Path::new(value);
    if path.is_absolute() {
        return Err("Path must be relative".to_string());
    }
    let mut saw_component = false;
    for component in path.components() {
        match component {
            Component::Normal(part) if !part.is_empty() => saw_component = true,
            _ => return Err("Path cannot contain . or ..".to_string()),
        }
    }
    if saw_component {
        Ok(())
    } else {
        Err("Name cannot be empty".to_string())
    }
}

fn apply_rename_ops(ops: &[RenameOp]) -> Result<AppliedRenames> {
    if ops.is_empty() {
        return Ok(AppliedRenames::default());
    }

    let mut staged = Vec::with_capacity(ops.len());
    for (index, op) in ops.iter().enumerate() {
        let temp_dir = unique_temp_sibling_dir(&op.old_path, index)?;
        let temp_path = temp_dir.join(path_name(&op.old_path));
        if let Err(error) = fs::rename(&op.old_path, &temp_path) {
            rollback_staged(&staged);
            let _ = fs::remove_dir(&temp_dir);
            bail!("Could not rename \"{}\": {error}", op.original_label);
        }
        staged.push(StagedRename {
            op,
            temp_path,
            temp_dir,
        });
    }

    let mut applied = AppliedRenames::default();
    let mut applied_ops: Vec<&RenameOp> = Vec::with_capacity(ops.len());
    for staged_rename in &staged {
        let op = staged_rename.op;
        if let Err(error) = fs::rename(&staged_rename.temp_path, &op.new_path) {
            rollback_applied(&applied_ops);
            rollback_staged_remaining(&staged, applied_ops.len());
            bail!("Could not rename \"{}\": {error}", op.original_label);
        }
        let _ = fs::remove_dir(&staged_rename.temp_dir);
        applied_ops.push(op);
        applied.renamed += 1;
        applied.last_new_path = Some(op.new_path.clone());
    }
    Ok(applied)
}

fn rollback_staged(staged: &[StagedRename<'_>]) {
    for staged_rename in staged.iter().rev() {
        let _ = fs::rename(&staged_rename.temp_path, &staged_rename.op.old_path);
        let _ = fs::remove_dir(&staged_rename.temp_dir);
    }
}

fn rollback_staged_remaining(staged: &[StagedRename<'_>], start: usize) {
    for staged_rename in staged.iter().skip(start).rev() {
        let _ = fs::rename(&staged_rename.temp_path, &staged_rename.op.old_path);
        let _ = fs::remove_dir(&staged_rename.temp_dir);
    }
}

fn rollback_applied(applied: &[&RenameOp]) {
    for op in applied.iter().rev() {
        let _ = fs::rename(&op.new_path, &op.old_path);
    }
}

fn unique_temp_sibling_dir(path: &Path, index: usize) -> Result<PathBuf> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    for attempt in 0..1000usize {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let candidate = parent.join(format!(
            ".elio-rename-{}-{now}-{index}-{attempt}",
            std::process::id()
        ));
        match fs::create_dir(&candidate) {
            Ok(()) => {
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    let _ = fs::set_permissions(&candidate, fs::Permissions::from_mode(0o700));
                }
                return Ok(candidate);
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error.into()),
        }
    }
    bail!(
        "Could not create temporary rename path for {}",
        path.display()
    )
}

fn rename_status(ops: &[RenameOp], applied: &AppliedRenames) -> String {
    match applied.renamed {
        0 => "No files renamed".to_string(),
        1 => {
            let op = ops.first().expect("single rename op should exist");
            format!("Renamed \"{}\" → \"{}\"", op.original_label, op.new_label)
        }
        n => format!("Renamed {} items", n),
    }
}

fn display_label(item: &BulkRenameItem, root: Option<&Path>) -> String {
    root.and_then(|root| item.path.strip_prefix(root).ok())
        .unwrap_or_else(|| Path::new(&item.original_name))
        .to_string_lossy()
        .into_owned()
}

#[cfg(unix)]
fn bulk_rename_item_from_path(path: PathBuf) -> BulkRenameItem {
    let original_name = path_name(&path);
    let is_dir = path.is_dir();
    BulkRenameItem {
        path,
        original_name,
        is_dir,
    }
}

fn path_name(path: &Path) -> String {
    path.file_name()
        .and_then(|name| name.to_str())
        .map(str::to_owned)
        .unwrap_or_else(|| path.display().to_string())
}

fn renamed_path(path: &Path, new_name: &str) -> PathBuf {
    path.parent()
        .map(|parent| parent.join(new_name))
        .unwrap_or_else(|| PathBuf::from(new_name))
}

#[cfg(any(test, unix))]
fn common_root(paths: &[PathBuf]) -> PathBuf {
    let mut components: Vec<_> = paths
        .first()
        .and_then(|path| path.parent())
        .map(|path| path.components().collect::<Vec<_>>())
        .unwrap_or_default();

    for path in paths.iter().skip(1) {
        let parent_components: Vec<_> = path
            .parent()
            .map(|parent| parent.components().collect())
            .unwrap_or_default();
        let common_len = components
            .iter()
            .zip(parent_components.iter())
            .take_while(|(a, b)| a == b)
            .count();
        components.truncate(common_len);
    }

    let mut root = PathBuf::new();
    for component in components {
        root.push(component.as_os_str());
    }
    if root.as_os_str().is_empty() {
        PathBuf::from(".")
    } else {
        root
    }
}

#[cfg(unix)]
fn editor_temp_owner(
    context: &crate::config::InvocationContext,
) -> Result<Option<(libc::uid_t, libc::gid_t)>> {
    match context {
        crate::config::InvocationContext::Normal
        | crate::config::InvocationContext::RootSession => Ok(None),
        crate::config::InvocationContext::Elevated(user) => Ok(Some((user.uid, user.gid))),
        crate::config::InvocationContext::ElevatedUnresolved => {
            bail!("could not resolve invoking user")
        }
    }
}

#[cfg(unix)]
fn create_temp_file(
    rows: &[String],
    invoking_user: Option<(libc::uid_t, libc::gid_t)>,
) -> Result<PathBuf> {
    let base = if invoking_user.is_some() {
        PathBuf::from("/tmp")
    } else {
        env::temp_dir()
    };
    for attempt in 0..1000u32 {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let path = base.join(format!(
            "elio-bulk-rename-{}-{now}-{attempt}.txt",
            std::process::id()
        ));
        let mut options = fs::OpenOptions::new();
        options.write(true).create_new(true);
        if invoking_user.is_some() {
            options.mode(0o600);
        }
        match options.open(&path) {
            Ok(mut file) => {
                let result = (|| -> Result<()> {
                    file.write_all(rows.join("\n").as_bytes())?;
                    file.write_all(b"\n")?;
                    if let Some((uid, gid)) = invoking_user {
                        if unsafe { libc::fchmod(file.as_raw_fd(), 0o600) } != 0 {
                            return Err(std::io::Error::last_os_error().into());
                        }
                        if unsafe { libc::fchown(file.as_raw_fd(), uid, gid) } != 0 {
                            return Err(std::io::Error::last_os_error().into());
                        }
                    }
                    Ok(())
                })();
                if let Err(error) = result {
                    drop(file);
                    let _ = fs::remove_file(&path);
                    return Err(error);
                }
                return Ok(path);
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error.into()),
        }
    }
    bail!("Could not create temporary rename file")
}

#[cfg(unix)]
fn read_editor_rename_file(path: &Path, expected_owner: Option<libc::uid_t>) -> Result<String> {
    let Some(expected_owner) = expected_owner else {
        return Ok(fs::read_to_string(path)?);
    };

    use std::os::unix::fs::MetadataExt;

    let file = fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_CLOEXEC | libc::O_NOFOLLOW)
        .open(path)?;
    let metadata = file.metadata()?;
    if !metadata.is_file() {
        bail!("edited rename document is not a regular file");
    }
    if metadata.uid() != expected_owner {
        bail!("edited rename document has unexpected owner");
    }
    if metadata.mode() & 0o022 != 0 {
        bail!("edited rename document is writable by another user");
    }
    if metadata.len() > MAX_EDITOR_RENAME_BYTES {
        bail!("edited rename document is too large");
    }

    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    file.take(MAX_EDITOR_RENAME_BYTES + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_EDITOR_RENAME_BYTES {
        bail!("edited rename document is too large");
    }
    String::from_utf8(bytes).context("edited rename document is not valid UTF-8")
}

#[cfg(unix)]
fn editor_command() -> (String, Vec<String>) {
    for key in ["VISUAL", "EDITOR"] {
        if let Some(value) =
            crate::config::invoking_user_env_var(key).and_then(|value| value.into_string().ok())
        {
            let tokens = crate::app::open_rules::tokenize_command(&value);
            if let Some((program, args)) = split_program_args(tokens) {
                return (program, args);
            }
        }
    }
    ("vi".to_string(), Vec::new())
}

#[cfg(unix)]
fn split_program_args(tokens: Vec<String>) -> Option<(String, Vec<String>)> {
    let mut tokens = tokens.into_iter();
    let program = tokens.next()?;
    Some((program, tokens.collect()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(path: &Path, is_dir: bool) -> BulkRenameItem {
        BulkRenameItem {
            path: path.to_path_buf(),
            original_name: path_name(path),
            is_dir,
        }
    }

    #[cfg(unix)]
    #[test]
    fn unresolved_elevation_cannot_prepare_editor_document() {
        assert!(editor_temp_owner(&crate::config::InvocationContext::ElevatedUnresolved).is_err());
        assert!(
            editor_temp_owner(&crate::config::InvocationContext::Normal)
                .expect("normal session should be accepted")
                .is_none()
        );
        assert!(
            editor_temp_owner(&crate::config::InvocationContext::RootSession)
                .expect("root session should be accepted")
                .is_none()
        );
    }

    #[cfg(unix)]
    #[test]
    fn invoking_user_editor_document_is_private_and_readable() {
        use std::os::unix::fs::MetadataExt;

        let uid = unsafe { libc::getuid() };
        let gid = unsafe { libc::getgid() };
        let path = create_temp_file(&["alpha.txt".to_string()], Some((uid, gid)))
            .expect("failed to create invoking-user editor document");
        let metadata = fs::metadata(&path).expect("failed to stat editor document");

        assert!(path.is_absolute());
        assert_eq!(path.parent(), Some(Path::new("/tmp")));
        assert_eq!(metadata.uid(), uid);
        assert_eq!(metadata.gid(), gid);
        assert_eq!(metadata.mode() & 0o777, 0o600);
        assert_eq!(
            read_editor_rename_file(&path, Some(uid)).expect("failed to read editor document"),
            "alpha.txt\n"
        );
        fs::remove_file(path).expect("failed to remove editor document");
    }

    #[cfg(unix)]
    #[test]
    fn invoking_user_editor_document_accepts_atomic_save_replacement() {
        let uid = unsafe { libc::getuid() };
        let gid = unsafe { libc::getgid() };
        let path = create_temp_file(&["alpha.txt".to_string()], Some((uid, gid)))
            .expect("failed to create invoking-user editor document");
        let replacement = path.with_extension("replacement");
        fs::write(&replacement, "renamed.txt\n").expect("failed to write replacement document");
        fs::rename(&replacement, &path).expect("failed to replace editor document");

        assert_eq!(
            read_editor_rename_file(&path, Some(uid)).expect("failed to read replacement document"),
            "renamed.txt\n"
        );
        fs::remove_file(path).expect("failed to remove editor document");
    }

    #[cfg(unix)]
    #[test]
    fn invoking_user_editor_document_rejects_symlinks_and_oversized_files() {
        use std::os::unix::fs::{PermissionsExt, symlink};

        let uid = unsafe { libc::getuid() };
        let gid = unsafe { libc::getgid() };
        let path = create_temp_file(&["alpha.txt".to_string()], Some((uid, gid)))
            .expect("failed to create invoking-user editor document");
        let target = path.with_extension("target");
        fs::write(&target, "renamed.txt\n").expect("failed to write symlink target");
        fs::remove_file(&path).expect("failed to remove editor document");
        symlink(&target, &path).expect("failed to create editor document symlink");
        assert!(read_editor_rename_file(&path, Some(uid)).is_err());
        fs::remove_file(&path).expect("failed to remove editor document symlink");
        fs::remove_file(target).expect("failed to remove symlink target");

        let path = create_temp_file(&["alpha.txt".to_string()], Some((uid, gid)))
            .expect("failed to create invoking-user editor document");
        fs::write(&path, vec![b'a'; MAX_EDITOR_RENAME_BYTES as usize + 1])
            .expect("failed to write oversized editor document");
        assert!(read_editor_rename_file(&path, Some(uid)).is_err());
        fs::remove_file(path).expect("failed to remove oversized editor document");

        let path = create_temp_file(&["alpha.txt".to_string()], Some((uid, gid)))
            .expect("failed to create invoking-user editor document");
        fs::set_permissions(&path, std::fs::Permissions::from_mode(0o622))
            .expect("failed to make editor document world-writable");
        assert!(read_editor_rename_file(&path, Some(uid)).is_err());
        fs::remove_file(path).expect("failed to remove world-writable editor document");
    }

    #[test]
    fn common_root_uses_parent_paths() {
        let paths = vec![
            PathBuf::from("/tmp/root/left/a.txt"),
            PathBuf::from("/tmp/root/right/b.txt"),
        ];
        assert_eq!(common_root(&paths), PathBuf::from("/tmp/root"));
    }

    #[test]
    fn editor_plan_accepts_relative_paths_in_multiple_directories() {
        let root =
            std::env::temp_dir().join(format!("elio-editor-rename-plan-{}", std::process::id()));
        std::fs::create_dir_all(root.join("left")).expect("failed to create left dir");
        std::fs::create_dir_all(root.join("right")).expect("failed to create right dir");
        let items = vec![
            item(&root.join("left/a.txt"), false),
            item(&root.join("right/b.txt"), false),
        ];
        let names = vec![
            "left/renamed.txt".to_string(),
            "right/renamed.txt".to_string(),
        ];
        let plan = build_rename_plan(&items, &names, Some(&root)).expect("plan should build");
        assert_eq!(plan[0].new_path, root.join("left/renamed.txt"));
        assert_eq!(plan[1].new_path, root.join("right/renamed.txt"));
        std::fs::remove_dir_all(root).expect("failed to remove temp root");
    }

    #[test]
    fn editor_plan_rejects_parent_traversal() {
        let root = PathBuf::from("/tmp/root");
        let items = vec![item(&root.join("a.txt"), false)];
        let names = vec!["../outside.txt".to_string()];
        let errors = build_rename_plan(&items, &names, Some(&root)).expect_err("plan should fail");
        assert_eq!(errors[0].as_deref(), Some("Path cannot contain . or .."));
    }

    #[test]
    fn apply_failure_rolls_back_chained_renames_without_overwriting() {
        let root = std::env::temp_dir().join(format!(
            "elio-editor-rename-rollback-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("failed to create temp root");
        let a = root.join("a.txt");
        let b = root.join("b.txt");
        let c = root.join("c.txt");
        std::fs::write(&a, "alpha").expect("failed to write a");
        std::fs::write(&b, "beta").expect("failed to write b");
        std::fs::create_dir(&c).expect("failed to create blocking directory");

        let ops = vec![
            RenameOp {
                old_path: a.clone(),
                original_label: "a.txt".to_string(),
                new_label: "b.txt".to_string(),
                new_path: b.clone(),
            },
            RenameOp {
                old_path: b.clone(),
                original_label: "b.txt".to_string(),
                new_label: "c.txt".to_string(),
                new_path: c.clone(),
            },
        ];

        let error = apply_rename_ops(&ops).expect_err("second apply should fail");
        assert!(error.to_string().contains("Could not rename \"b.txt\""));
        assert_eq!(
            std::fs::read_to_string(&a).expect("a should be restored"),
            "alpha"
        );
        assert_eq!(
            std::fs::read_to_string(&b).expect("b should be restored"),
            "beta"
        );
        assert!(c.is_dir());
        assert!(
            std::fs::read_dir(&root)
                .expect("root should be readable")
                .all(|entry| !entry
                    .expect("entry should be readable")
                    .file_name()
                    .to_string_lossy()
                    .starts_with(".elio-rename-"))
        );

        std::fs::remove_dir_all(root).expect("failed to remove temp root");
    }
}
