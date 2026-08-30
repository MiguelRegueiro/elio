mod support;

use std::fs;
use support::{elio, temp_path};

#[test]
fn version_prints_package_version() {
    let output = elio()
        .arg("--version")
        .output()
        .expect("failed to run elio --version");

    assert!(output.status.success());
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        format!("elio {}\n", env!("CARGO_PKG_VERSION"))
    );
    assert!(output.stderr.is_empty());
}

#[test]
fn help_prints_usage() {
    let output = elio()
        .arg("--help")
        .output()
        .expect("failed to run elio --help");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Usage: elio [OPTIONS] [PATH]"));
    assert!(stdout.contains("Arguments:"));
    assert!(
        stdout.contains("[PATH]  Start in a directory, or focus a file in its parent directory")
    );
    assert!(
        stdout.contains(
            "--chooser-file <FILE>     Write selected paths to FILE; use \"-\" for stdout"
        )
    );
    assert!(stdout.contains("--config <FILE>           Load configuration from FILE"));
    assert!(stdout.contains("--cwd-file <FILE>         Write the final directory to FILE on exit"));
    assert!(stdout.contains("--theme <FILE>            Load a theme from FILE"));
    assert!(stdout.contains("-h, --help                    Print help"));
    assert!(stdout.contains("-V, --version                 Print version"));
    assert!(stdout.contains("Shell integration:"));
    assert!(stdout.contains("elio shell init <SHELL>       Print shell integration code"));
    assert!(
        stdout.contains(
            "elio shell install [SHELL]    Install integration; detect shell when omitted"
        )
    );
    assert!(
        stdout.contains(
            "elio shell uninstall [SHELL]  Remove integration; detect shell when omitted"
        )
    );
    assert!(stdout.contains("Supported shells: bash, zsh, fish, nu"));
    assert!(stdout.contains("CLI documentation: https://elio-fm.github.io/docs/cli/"));
    assert!(!stdout.contains(env!("CARGO_PKG_VERSION")));
    assert!(!stdout.contains("\x1b["));
    assert!(output.stderr.is_empty());
}

#[test]
fn mistyped_version_flag_exits_with_suggestion() {
    let output = elio().arg("--v").output().expect("failed to run elio --v");

    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("error: unexpected argument '--v' found"));
    assert!(stderr.contains("tip: a similar argument exists: '--version'"));
}

#[test]
fn mistyped_chooser_file_flag_exits_with_suggestion() {
    let output = elio()
        .arg("--chooser")
        .output()
        .expect("failed to run elio --chooser");

    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("error: unexpected argument '--chooser' found"));
    assert!(stderr.contains("tip: a similar argument exists: '--chooser-file'"));
}

#[test]
fn mistyped_config_flag_exits_with_suggestion() {
    let output = elio()
        .arg("--conf")
        .output()
        .expect("failed to run elio --conf");

    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("error: unexpected argument '--conf' found"));
    assert!(stderr.contains("tip: a similar argument exists: '--config'"));
}

#[test]
fn mistyped_theme_flag_exits_with_suggestion() {
    let output = elio()
        .arg("--them")
        .output()
        .expect("failed to run elio --them");

    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("error: unexpected argument '--them' found"));
    assert!(stderr.contains("tip: a similar argument exists: '--theme'"));
}

#[test]
fn config_and_theme_do_not_have_short_flags() {
    for flag in ["-c", "-t"] {
        let output = elio()
            .arg(flag)
            .output()
            .unwrap_or_else(|error| panic!("failed to run elio {flag}: {error}"));

        assert!(!output.status.success());
        assert!(output.stdout.is_empty());
        assert!(
            String::from_utf8_lossy(&output.stderr)
                .contains(&format!("error: unexpected argument '{flag}' found"))
        );
    }
}

#[test]
fn config_requires_value() {
    let output = elio()
        .arg("--config")
        .output()
        .expect("failed to run elio --config");

    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("error: expected a file path after '--config'")
    );
}

#[test]
fn config_equals_requires_value() {
    let output = elio()
        .arg("--config=")
        .output()
        .expect("failed to run elio --config=");

    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("error: expected a file path after '--config'")
    );
}

#[test]
fn theme_requires_value() {
    let output = elio()
        .arg("--theme")
        .output()
        .expect("failed to run elio --theme");

    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("error: expected a file path after '--theme'")
    );
}

#[test]
fn theme_equals_requires_value() {
    let output = elio()
        .arg("--theme=")
        .output()
        .expect("failed to run elio --theme=");

    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("error: expected a file path after '--theme'")
    );
}

#[test]
fn duplicate_config_is_rejected() {
    let output = elio()
        .args(["--config", "first.toml", "--config=second.toml"])
        .output()
        .expect("failed to run elio with duplicate --config");

    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("error: '--config' cannot be used more than once")
    );
}

#[test]
fn duplicate_theme_is_rejected() {
    let output = elio()
        .args(["--theme", "first.toml", "--theme=second.toml"])
        .output()
        .expect("failed to run elio with duplicate --theme");

    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("error: '--theme' cannot be used more than once")
    );
}

#[test]
fn missing_explicit_config_fails_before_terminal_startup() {
    let path = temp_path("missing-explicit-config").join("config.toml");
    let output = elio()
        .arg(format!("--config={}", path.display()))
        .output()
        .expect("failed to run elio with missing explicit config");

    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&output.stderr).contains(&format!(
        "elio: failed to read config from {}",
        path.display()
    )));
}

#[test]
fn missing_explicit_theme_fails_before_terminal_startup() {
    let path = temp_path("missing-explicit-theme").join("theme.toml");
    let output = elio()
        .arg("--theme")
        .arg(&path)
        .output()
        .expect("failed to run elio with missing explicit theme");

    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&output.stderr).contains(&format!(
        "elio: failed to read theme from {}",
        path.display()
    )));
}

#[test]
fn extra_argument_after_version_reports_the_extra_argument() {
    let output = elio()
        .args(["--version", "extra"])
        .output()
        .expect("failed to run elio --version extra");

    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("error: unexpected argument 'extra' found"));
    assert!(!stderr.contains("tip: a similar argument exists"));
}

#[test]
fn missing_path_argument_exits_with_clear_error() {
    let missing = temp_path("missing");

    let output = elio()
        .arg(missing.to_str().expect("temp path should be valid utf-8"))
        .output()
        .expect("failed to run elio with missing directory");

    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert_eq!(
        String::from_utf8_lossy(&output.stderr),
        format!(
            "Cannot open \"{}\": no such file or directory\n",
            missing.display()
        )
    );
}

#[test]
fn more_than_one_path_is_rejected() {
    let first = temp_path("dir-one");
    let second = temp_path("dir-two");
    fs::create_dir_all(&first).expect("first temp directory should be created");
    fs::create_dir_all(&second).expect("second temp directory should be created");

    let output = elio()
        .arg(&first)
        .arg(&second)
        .output()
        .expect("failed to run elio with two directory arguments");

    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&output.stderr).contains("error: unexpected argument"));

    fs::remove_dir_all(first).expect("first temp directory should be removed");
    fs::remove_dir_all(second).expect("second temp directory should be removed");
}

#[cfg(target_os = "linux")]
#[test]
fn chooser_stdout_pipe_receives_only_selection() {
    use std::{
        fs::File,
        io::{Read, Write},
        os::{fd::FromRawFd, unix::process::CommandExt},
        process::Stdio,
        thread,
        time::{Duration, Instant},
    };

    let root = temp_path("chooser-stdout-pipe");
    fs::create_dir_all(&root).expect("temp directory should be created");
    let selected = root.join("picked.txt");
    fs::write(&selected, "picked").expect("selected file should be written");

    let mut master = 0;
    let mut slave = 0;
    let mut stdout_pipe = [0; 2];
    unsafe {
        assert_eq!(
            libc::openpty(
                &mut master,
                &mut slave,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            ),
            0
        );
        assert_eq!(libc::pipe(stdout_pipe.as_mut_ptr()), 0);
    }

    let tty_for_child = unsafe { libc::dup(slave) };
    assert!(tty_for_child >= 0);

    let slave_file = unsafe { File::from_raw_fd(slave) };
    let stdout_writer = unsafe { File::from_raw_fd(stdout_pipe[1]) };
    let mut stdout_reader = unsafe { File::from_raw_fd(stdout_pipe[0]) };
    let mut tty_master = unsafe { File::from_raw_fd(master) };

    let mut command = elio();
    command
        .arg("--chooser-file")
        .arg("-")
        .arg(&root)
        .env("TERM", "xterm-256color")
        .stdin(Stdio::from(slave_file))
        .stdout(Stdio::from(stdout_writer))
        .stderr(Stdio::null());

    unsafe {
        command.pre_exec(move || {
            if libc::setsid() == -1 {
                return Err(std::io::Error::last_os_error());
            }
            if libc::ioctl(tty_for_child, libc::TIOCSCTTY as libc::c_ulong, 0) == -1 {
                return Err(std::io::Error::last_os_error());
            }
            libc::close(tty_for_child);
            Ok(())
        });
    }

    let mut child = command.spawn().expect("elio should spawn under a pty");
    unsafe {
        libc::close(tty_for_child);
    }
    thread::sleep(Duration::from_millis(500));
    tty_master
        .write_all(b"\r")
        .expect("enter key should be sent to pty");

    let deadline = Instant::now() + Duration::from_secs(5);
    while child
        .try_wait()
        .expect("child status should be readable")
        .is_none()
    {
        if Instant::now() > deadline {
            child.kill().expect("hung child should be killed");
            panic!("elio did not exit after chooser confirmation");
        }
        thread::sleep(Duration::from_millis(20));
    }

    unsafe {
        let flags = libc::fcntl(stdout_pipe[0], libc::F_GETFL);
        assert!(flags >= 0);
        assert_eq!(
            libc::fcntl(stdout_pipe[0], libc::F_SETFL, flags | libc::O_NONBLOCK),
            0
        );
    }

    let mut stdout = Vec::new();
    loop {
        let mut chunk = [0; 1024];
        match stdout_reader.read(&mut chunk) {
            Ok(0) => break,
            Ok(count) => stdout.extend_from_slice(&chunk[..count]),
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => break,
            Err(error) => panic!("chooser stdout should be readable: {error}"),
        }
    }

    let expected = format!("{}\n", selected.display());
    assert_eq!(String::from_utf8_lossy(&stdout), expected);
    assert!(
        !stdout.contains(&0x1b),
        "stdout contained terminal escape bytes"
    );

    fs::remove_dir_all(root).expect("temp directory should be removed");
}
