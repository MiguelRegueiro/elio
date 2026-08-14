#[cfg(unix)]
use std::os::unix::process::CommandExt;
#[cfg(test)]
use std::{cell::RefCell, path::PathBuf};
use std::{
    io,
    path::Path,
    process::{Command, Stdio},
    time::Duration,
};

const DETACHED_COMMAND_DEADLINE: Duration = Duration::from_millis(250);
const DETACHED_COMMAND_POLL: Duration = Duration::from_millis(10);

#[cfg(test)]
thread_local! {
    static TEST_OPEN_IN_SYSTEM_CAPTURE: RefCell<Option<PathBuf>> = const { RefCell::new(None) };
}

pub(crate) fn open_in_system(target: &Path) -> Result<(), String> {
    #[cfg(test)]
    if let Some(capture) = TEST_OPEN_IN_SYSTEM_CAPTURE.with(|slot| slot.borrow().clone()) {
        use std::{fs::OpenOptions, io::Write};

        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&capture)
            .map_err(|e| e.to_string())?;
        if file.metadata().map_err(|e| e.to_string())?.len() > 0 {
            writeln!(file).map_err(|e| e.to_string())?;
        }
        write!(file, "{}", target.display()).map_err(|e| e.to_string())?;
        return Ok(());
    }

    #[cfg(target_os = "macos")]
    {
        detached_open("open", &[], target).map_err(|e| format!("open: {e}"))
    }

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const DETACHED_PROCESS: u32 = 0x00000008;
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x00000200;

        Command::new("cmd")
            .args(["/c", "start", ""])
            .arg(target)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP)
            .spawn()
            .map(|_| ())
            .map_err(|e| format!("cmd: {e}"))
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        open_unix_preferring_gio(target)
    }
}

#[cfg(test)]
pub(crate) fn set_open_in_system_capture_for_test(path: Option<PathBuf>) {
    TEST_OPEN_IN_SYSTEM_CAPTURE.with(|slot| *slot.borrow_mut() = path);
}

#[cfg(all(unix, not(target_os = "macos")))]
fn open_with_unix_backends(target: &Path, backends: &[(&str, &[&str])]) -> Result<(), String> {
    for &(program, args) in backends {
        match detached_open(program, args, target) {
            Ok(()) => return Ok(()),
            Err(e) if e.kind() == io::ErrorKind::NotFound => continue,
            Err(e) => return Err(format!("{program}: {e}")),
        }
    }
    Err(String::from("No desktop opener available in this session"))
}

#[cfg(all(unix, not(target_os = "macos")))]
fn open_unix_preferring_gio(target: &Path) -> Result<(), String> {
    open_unix_preferring_gio_impl(target, "gio", "xdg-open")
}

#[cfg(all(unix, not(target_os = "macos")))]
fn open_unix_preferring_gio_impl(target: &Path, gio: &str, xdg_open: &str) -> Result<(), String> {
    // gio uses GLib MIME detection, which is more consistent with desktop
    // defaults for extension- and name-based MIME matches than the xdg-open path
    // in some sessions. Use a 250ms bounded wait so gio's synchronous failures
    // can fall back. Longer-running portal startup is detached to keep opening
    // responsive; late failures after that point cannot fall back.
    open_unix_preferring_gio_with(
        gio,
        || gio_open(gio, target),
        || open_with_unix_backends(target, &[(xdg_open, &[][..])]),
    )
}

#[cfg(all(unix, not(target_os = "macos")))]
fn open_unix_preferring_gio_with(
    gio: &str,
    gio_open: impl FnOnce() -> io::Result<()>,
    fallback_open: impl FnOnce() -> Result<(), String>,
) -> Result<(), String> {
    match gio_open() {
        Ok(()) => return Ok(()),
        Err(e)
            if matches!(
                e.kind(),
                io::ErrorKind::NotFound
                    | io::ErrorKind::Other
                    | io::ErrorKind::PermissionDenied
                    | io::ErrorKind::Interrupted
            ) => {}
        Err(e) => return Err(format!("{gio}: {e}")),
    }
    fallback_open()
}

#[cfg(all(unix, not(target_os = "macos")))]
fn gio_open(program: &str, target: &Path) -> io::Result<()> {
    use std::time::Duration;
    const DEADLINE: Duration = Duration::from_millis(250);
    const POLL: Duration = Duration::from_millis(10);

    gio_open_with_deadline(program, target, DEADLINE, POLL)
}

#[cfg(all(unix, not(target_os = "macos")))]
fn gio_open_with_deadline(
    program: &str,
    target: &Path,
    deadline_duration: std::time::Duration,
    poll: std::time::Duration,
) -> io::Result<()> {
    let mut command = Command::new(program);
    command.arg("open").arg(target);
    spawn_with_deadline(command, deadline_duration, poll)
}

#[cfg(all(unix, not(target_os = "macos")))]
fn spawn_with_deadline(
    mut command: Command,
    deadline_duration: std::time::Duration,
    poll: std::time::Duration,
) -> io::Result<()> {
    use std::time::Instant;

    prepare_external_open_command(&mut command)?;
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .process_group(0)
        .spawn()?;

    let deadline = Instant::now() + deadline_duration;
    while Instant::now() < deadline {
        match child.try_wait()? {
            Some(s) if s.success() => return Ok(()),
            Some(s) => return Err(io::Error::other(format!("process exited with {s}"))),
            None => std::thread::sleep(poll),
        }
    }
    // Still running past the deadline: detach it to keep opening responsive.
    // Reap the child in the background to avoid a zombie.
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}

#[cfg(unix)]
pub(crate) fn detached_open(program: &str, args: &[&str], target: &Path) -> io::Result<()> {
    let mut command = Command::new(program);
    command.args(args);
    command.arg(target);

    #[cfg(target_os = "macos")]
    if program == "open" {
        return status_spawn(&mut command);
    }

    detached_spawn_with_deadline(
        &mut command,
        DETACHED_COMMAND_DEADLINE,
        DETACHED_COMMAND_POLL,
    )
}

/// Spawns `program` with the given `args` detached from the terminal.
/// Unlike [`detached_open`], the target path is not appended; it must
/// already be present in `args` (as produced by the Exec= expansion).
pub(crate) fn detached_open_command(program: &str, args: &[String]) -> io::Result<()> {
    let mut command = Command::new(program);
    command.args(args);

    #[cfg(target_os = "macos")]
    if program == "open" {
        return status_spawn(&mut command);
    }

    detached_spawn_with_deadline(
        &mut command,
        DETACHED_COMMAND_DEADLINE,
        DETACHED_COMMAND_POLL,
    )
}

fn detached_spawn_with_deadline(
    command: &mut Command,
    deadline_duration: Duration,
    poll: Duration,
) -> io::Result<()> {
    use std::time::Instant;

    prepare_external_open_command(command)?;
    prepare_detached_command(command);
    let mut child = command.spawn()?;

    let deadline = Instant::now() + deadline_duration;
    while Instant::now() < deadline {
        match child.try_wait()? {
            Some(status) if status.success() => return Ok(()),
            Some(status) => {
                return Err(io::Error::other(format!("process exited with {status}")));
            }
            None => std::thread::sleep(poll),
        }
    }

    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}

fn prepare_detached_command(command: &mut Command) {
    command.stdin(Stdio::null());
    command.stdout(Stdio::null());
    command.stderr(Stdio::null());
    #[cfg(unix)]
    command.process_group(0);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const DETACHED_PROCESS: u32 = 0x00000008;
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x00000200;
        command.creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP);
    }
}

#[cfg(unix)]
fn prepare_external_open_command(command: &mut Command) -> io::Result<()> {
    crate::invoking_user_command::prepare_external(command, None)
}

#[cfg(not(unix))]
fn prepare_external_open_command(_command: &mut Command) -> io::Result<()> {
    Ok(())
}

#[cfg(target_os = "macos")]
fn status_spawn(command: &mut Command) -> io::Result<()> {
    prepare_external_open_command(command)?;
    command.stdin(Stdio::null());
    command.stdout(Stdio::null());
    command.stderr(Stdio::null());
    let status = command.status()?;
    if status.success() {
        Ok(())
    } else {
        Err(io::Error::other(format!("process exited with {status}")))
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    #[cfg(all(unix, not(target_os = "macos")))]
    use std::path::Path;
    use std::{
        fs,
        path::PathBuf,
        time::{SystemTime, UNIX_EPOCH},
    };

    fn temp_path(label: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after unix epoch")
            .as_nanos();
        std::env::temp_dir().join(format!("elio-{label}-{unique}"))
    }

    /// Wraps `s` in single quotes, escaping any embedded single quotes so the
    /// result is safe to embed in a POSIX shell command string even when `s`
    /// contains apostrophes (e.g. a TMPDIR like `/tmp/user's tmp`).
    ///
    /// Strategy: end the current single-quoted span, emit `'\''`, then re-open.
    /// `foo'bar` -> `'foo'\''bar'`
    #[cfg(unix)]
    fn shell_quote(s: &str) -> String {
        format!("'{}'", s.replace('\'', r"'\''"))
    }

    #[test]
    #[cfg(unix)]
    fn detached_open_moves_child_into_its_own_process_group() {
        let root = temp_path("detached-open");
        fs::create_dir_all(&root).expect("failed to create temp root");

        let capture = root.join("capture.txt");
        // Use /bin/sh -c with the capture path interpolated directly into the
        // command string.  Passing it via $1 relies on how the target shell
        // (e.g. FreeBSD sh) handles the positional-parameter slot after "-c cmd",
        // which varies across implementations.  The path comes from temp_path()
        // and contains only alphanumerics, hyphens, and slashes: safe to
        // single-quote.  The target arg that detached_open appends becomes $0
        // (the script name) and is harmlessly ignored.
        let capture_str = capture
            .to_str()
            .expect("capture path should be valid utf-8");
        let cmd = format!(
            "pgid=$(ps -o pgid= -p $$ | tr -d ' '); printf '%s %s\\n' \"$$\" \"$pgid\" > {}",
            shell_quote(capture_str)
        );
        detached_open("/bin/sh", &["-c", &cmd], &root).expect("failed to spawn fake opener");

        // Wait for non-empty content; the shell's `>` redirect creates the
        // file before printf writes to it, so existence alone is not enough.
        let mut capture_text = String::new();
        for _ in 0..300 {
            match fs::read_to_string(&capture) {
                Ok(s) if !s.is_empty() => {
                    capture_text = s;
                    break;
                }
                _ => {}
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }

        let mut parts = capture_text.split_whitespace();
        let pid = parts
            .next()
            .expect("capture should contain pid")
            .parse::<i32>()
            .expect("pid should be numeric");
        let pgid = parts
            .next()
            .expect("capture should contain pgid")
            .parse::<i32>()
            .expect("pgid should be numeric");

        assert_eq!(pgid, pid);
        assert_ne!(pgid, unsafe { libc::getpgrp() });

        fs::remove_dir_all(root).expect("failed to remove temp root");
    }

    #[test]
    #[cfg(all(unix, not(target_os = "macos")))]
    fn detached_open_command_reports_immediate_nonzero_exit() {
        let mut command = Command::new("/bin/sh");
        command.arg("-c").arg("exit 7");

        let result = detached_spawn_with_deadline(
            &mut command,
            std::time::Duration::from_secs(5),
            std::time::Duration::from_millis(10),
        );

        let error = result.expect_err("immediate nonzero exit should be reported");
        assert!(
            error.to_string().contains("process exited with"),
            "error should report child exit status, got: {error}"
        );
    }

    #[test]
    #[cfg(all(unix, not(target_os = "macos")))]
    fn detached_open_command_detaches_when_deadline_expires() {
        let mut command = Command::new("/bin/sh");
        command.arg("-c").arg("sleep 1");

        let result = detached_spawn_with_deadline(
            &mut command,
            std::time::Duration::ZERO,
            std::time::Duration::from_millis(10),
        );

        assert!(
            result.is_ok(),
            "deadline expiry should detach the still-running child: {result:?}"
        );
    }

    #[test]
    #[cfg(all(unix, not(target_os = "macos")))]
    fn open_with_unix_backends_uses_first_available_backend() {
        let root = temp_path("open-backends-first");
        fs::create_dir_all(&root).expect("failed to create temp root");

        let capture = root.join("capture.txt");
        let capture_str = capture
            .to_str()
            .expect("capture path should be valid utf-8");
        let cmd = format!("printf 'xdg-open' > {}", shell_quote(capture_str));

        let result = open_with_unix_backends(
            &capture,
            &[
                ("/bin/sh", &["-c", &cmd][..]),
                ("this-program-does-not-exist-elio", &[][..]),
            ],
        );

        assert!(result.is_ok(), "expected Ok, got {result:?}");

        // Wait for the script to finish writing. The shell redirect `>` creates
        // the file (empty) before printf writes to it, so wait for non-empty
        // content to avoid a TOCTOU race on slow CI.
        for _ in 0..300 {
            match fs::read_to_string(&capture) {
                Ok(s) if !s.is_empty() => break,
                _ => {}
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }

        let recorded = fs::read_to_string(&capture).expect("capture should exist");
        assert_eq!(recorded.trim(), "xdg-open");

        fs::remove_dir_all(root).expect("failed to remove temp root");
    }

    #[test]
    #[cfg(all(unix, not(target_os = "macos")))]
    fn open_with_unix_backends_skips_missing_backend_and_tries_next() {
        let root = temp_path("open-backends-fallback");
        fs::create_dir_all(&root).expect("failed to create temp root");

        let capture = root.join("capture.txt");

        // Use /bin/sh -c with the capture path baked into the command string.
        // Passing it via $1 relies on how each sh implementation populates
        // positional parameters after "-c cmd", behavior that differs between
        // Linux dash/bash and FreeBSD sh.  The path comes from temp_path() and
        // contains only alphanumerics, hyphens, and slashes: safe to
        // single-quote.
        let capture_str = capture
            .to_str()
            .expect("capture path should be valid utf-8");
        let cmd = format!("printf 'gio' > {}", shell_quote(capture_str));
        let result = open_with_unix_backends(
            &capture,
            &[
                ("this-program-does-not-exist-elio", &[][..]),
                ("/bin/sh", &["-c", &cmd][..]),
            ],
        );

        assert!(result.is_ok(), "expected Ok after fallback, got {result:?}");

        // Wait for non-empty content; the shell's `>` redirect creates the
        // file before printf writes to it, so existence alone is not enough.
        let mut recorded = String::new();
        for _ in 0..300 {
            match fs::read_to_string(&capture) {
                Ok(s) if !s.is_empty() => {
                    recorded = s;
                    break;
                }
                _ => {}
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }

        assert_eq!(recorded.trim(), "gio");

        fs::remove_dir_all(root).expect("failed to remove temp root");
    }

    #[test]
    #[cfg(all(unix, not(target_os = "macos")))]
    fn open_with_unix_backends_returns_session_error_when_all_missing() {
        let result = open_with_unix_backends(
            Path::new("/tmp/anything"),
            &[
                ("this-program-does-not-exist-elio-a", &[][..]),
                ("this-program-does-not-exist-elio-b", &[][..]),
            ],
        );

        assert_eq!(
            result.unwrap_err(),
            "No desktop opener available in this session"
        );
    }

    #[test]
    #[cfg(all(unix, not(target_os = "macos")))]
    fn open_with_unix_backends_propagates_non_notfound_errors_immediately() {
        use std::os::unix::fs::PermissionsExt;

        let root = temp_path("open-backends-permerror");
        fs::create_dir_all(&root).expect("failed to create temp root");

        // A file that exists but is not executable; spawn returns PermissionDenied.
        let not_executable = root.join("not-executable");
        fs::write(&not_executable, "#!/bin/sh\n").expect("failed to write file");
        let mut perms = fs::metadata(&not_executable).unwrap().permissions();
        perms.set_mode(0o644);
        fs::set_permissions(&not_executable, perms).unwrap();

        let script = root.join("should-not-run");
        fs::write(&script, "#!/bin/sh\n").expect("failed to write script");

        let result = open_with_unix_backends(
            Path::new("/tmp/anything"),
            &[
                (not_executable.to_str().unwrap(), &[][..]),
                (script.to_str().unwrap(), &[][..]),
            ],
        );

        let err = result.unwrap_err();
        assert!(
            err.contains("not-executable"),
            "error should name the failing backend, got: {err}"
        );
        // The second backend should never have been tried.
        assert!(
            !err.contains("should-not-run"),
            "second backend should not appear in error, got: {err}"
        );

        fs::remove_dir_all(root).expect("failed to remove temp root");
    }

    #[test]
    #[cfg(all(unix, not(target_os = "macos")))]
    fn open_unix_preferring_gio_uses_gio_when_available() {
        use std::cell::Cell;

        let fallback_called = Cell::new(false);
        let result = open_unix_preferring_gio_with(
            "gio",
            || Ok(()),
            || {
                fallback_called.set(true);
                Ok(())
            },
        );

        assert!(result.is_ok(), "expected Ok, got {result:?}");
        assert!(!fallback_called.get(), "fallback should not run");
    }

    #[test]
    #[cfg(all(unix, not(target_os = "macos")))]
    fn open_unix_preferring_gio_falls_back_when_gio_missing() {
        use std::cell::Cell;

        let fallback_called = Cell::new(false);
        let result = open_unix_preferring_gio_with(
            "gio",
            || Err(io::Error::new(io::ErrorKind::NotFound, "missing")),
            || {
                fallback_called.set(true);
                Ok(())
            },
        );

        assert!(result.is_ok(), "expected Ok after fallback, got {result:?}");
        assert!(fallback_called.get(), "fallback should run");
    }

    #[test]
    #[cfg(all(unix, not(target_os = "macos")))]
    fn open_unix_preferring_gio_falls_back_when_gio_exits_nonzero() {
        use std::cell::Cell;

        let fallback_called = Cell::new(false);
        let result = open_unix_preferring_gio_with(
            "gio",
            || Err(io::Error::other("process exited with exit status: 1")),
            || {
                fallback_called.set(true);
                Ok(())
            },
        );

        assert!(result.is_ok(), "expected Ok after fallback, got {result:?}");
        assert!(fallback_called.get(), "fallback should run");
    }

    #[test]
    #[cfg(all(unix, not(target_os = "macos")))]
    fn gio_open_detaches_when_deadline_expires() {
        let root = temp_path("open-gio-timeout-detach");
        fs::create_dir_all(&root).expect("failed to create temp root");

        let capture = root.join("capture.txt");
        let capture_str = capture.to_str().unwrap();

        // Use a stable shell binary instead of a freshly written fake
        // executable. This test is about deadline/detach behavior, not opener
        // discovery, and immediate write→exec of test scripts can hit ETXTBSY
        // on some Linux filesystems.
        let cmd = format!("sleep 1; printf 'gio' > {}", shell_quote(capture_str));
        let mut command = Command::new("/bin/sh");
        command.arg("-c").arg(cmd);

        let started = std::time::Instant::now();
        let result = spawn_with_deadline(
            command,
            std::time::Duration::from_millis(50),
            std::time::Duration::from_millis(5),
        );
        let elapsed = started.elapsed();

        assert!(
            result.is_ok(),
            "expected Ok via detach path, got {result:?}"
        );
        assert!(
            elapsed < std::time::Duration::from_millis(500),
            "should return before the opener finishes its 1s sleep, took {elapsed:?}"
        );

        // Reaper thread is still waiting on the opener; poll for the eventual write
        // to confirm the child was detached (not killed) and ran to completion.
        let mut recorded = String::new();
        for _ in 0..300 {
            match fs::read_to_string(&capture) {
                Ok(s) if !s.is_empty() => {
                    recorded = s;
                    break;
                }
                _ => {}
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert_eq!(recorded.trim(), "gio");

        fs::remove_dir_all(root).expect("failed to remove temp root");
    }
}
