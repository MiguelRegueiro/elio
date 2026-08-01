use super::{ImageProtocol, TerminalIdentity};
use std::{env, fs, path::Path, process::Command};

pub(super) fn pdf_preview_tools_available() -> bool {
    command_exists("pdfinfo") && command_exists("pdftocairo")
}

pub(in crate::app) fn detect_terminal_identity() -> TerminalIdentity {
    detect_terminal_identity_with(
        real_env_lookup,
        query_tmux_client_termname,
        query_tmux_client_env,
        query_tmux_env,
    )
}

/// Inner detection logic with injectable lookups so tests can drive the tmux
/// fallback without spawning tmux. Direct process env usually wins, with one
/// exception: identities that, inside tmux, can only have come from an
/// *inherited marker* env var — `KITTY_WINDOW_ID` (Kitty) and `ALACRITTY_SOCKET`
/// (Alacritty). tmux passes the environment the *server* was started in to every
/// pane, so a server first launched from Kitty or Alacritty mislabels a later
/// Ghostty/Foot/… client (for Alacritty this silently disables image previews,
/// since Alacritty supports none). When such a marker identity is seen inside
/// tmux the live attached client is authoritative, so we consult (in order) the
/// live tmux client `TERM`, the live tmux client environment, the tmux session
/// env, and the tmux server global env, keeping the marker identity only if none
/// of those resolve a terminal.
fn detect_terminal_identity_with(
    env_lookup: impl Fn(&str) -> Option<String>,
    tmux_client_term: impl Fn() -> Option<String>,
    tmux_client_env_lookup: impl Fn(&str) -> Option<String>,
    tmux_env_lookup: impl Fn(&str) -> Option<String>,
) -> TerminalIdentity {
    let identity = classify_from_env(&env_lookup);
    let in_tmux = env_lookup("TMUX").is_some();

    // Inside tmux, `TERM`/`TERM_PROGRAM` are masked to tmux's own values, so a
    // Kitty or Alacritty identity here must have come from a leaked marker var
    // and cannot be trusted — defer it to the live client below. Any other
    // direct identity (e.g. a forwarded `TERM_PROGRAM=ghostty`) is authoritative.
    let marker_identity_in_tmux = in_tmux
        && matches!(
            identity,
            TerminalIdentity::Kitty | TerminalIdentity::Alacritty
        );
    if identity != TerminalIdentity::Other && !marker_identity_in_tmux {
        return identity;
    }
    if !in_tmux {
        return identity;
    }

    if let Some(term) = tmux_client_term()
        && let Some(id) = classify_tmux_client_termname(&term)
    {
        return id;
    }

    if let Some(id) = classify_supported_tmux_env(&tmux_client_env_lookup) {
        return id;
    }

    if let Some(id) = classify_supported_tmux_env(&tmux_env_lookup) {
        return id;
    }

    // No authoritative tmux signal: keep whatever the direct env produced
    // (`Other`, or a genuine Kitty/Alacritty that the client simply didn't
    // confirm — e.g. real Alacritty, which exposes no graphics-capable client).
    identity
}

fn classify_from_env(env_lookup: &impl Fn(&str) -> Option<String>) -> TerminalIdentity {
    let term = env_lookup("TERM").unwrap_or_default().to_ascii_lowercase();
    let term_program = env_lookup("TERM_PROGRAM")
        .unwrap_or_default()
        .to_ascii_lowercase();
    let kitty_window_id = env_lookup("KITTY_WINDOW_ID").is_some();
    let konsole_dbus = env_lookup("KONSOLE_DBUS_SESSION").is_some()
        || env_lookup("KONSOLE_DBUS_SERVICE").is_some()
        || env_lookup("KONSOLE_DBUS_WINDOW").is_some();

    if term.contains("xterm-kitty") || term_program == "kitty" {
        TerminalIdentity::Kitty
    } else if term.contains("ghostty") || term_program == "ghostty" {
        TerminalIdentity::Ghostty
    } else if term.contains("wezterm") || term_program == "wezterm" {
        TerminalIdentity::WezTerm
    } else if term_program.contains("warp") || env_lookup("WARP_SESSION_ID").is_some() {
        TerminalIdentity::Warp
    } else if term_program == "iterm.app" {
        TerminalIdentity::ITerm2
    } else if term.contains("alacritty")
        || term_program.contains("alacritty")
        || env_lookup("ALACRITTY_SOCKET").is_some()
    {
        TerminalIdentity::Alacritty
    } else if konsole_dbus {
        // Konsole exports D-Bus identifiers into child shells so scripts can
        // address the current window/session.
        TerminalIdentity::Konsole
    } else if kitty_window_id {
        TerminalIdentity::Kitty
    } else if term == "foot" || term == "foot-extra" {
        // Foot sets TERM=foot or TERM=foot-extra and supports Sixel natively.
        TerminalIdentity::Foot
    } else if env_lookup("WT_SESSION").is_some() {
        // WT_SESSION is a GUID set by Windows Terminal in every shell it hosts.
        // WT_PROFILE_ID is also available but WT_SESSION is the canonical marker.
        TerminalIdentity::WindowsTerminal
    } else {
        TerminalIdentity::Other
    }
}

fn classify_tmux_client_termname(term: &str) -> Option<TerminalIdentity> {
    let term = term.to_ascii_lowercase();
    if term.contains("xterm-kitty") {
        Some(TerminalIdentity::Kitty)
    } else if term.contains("ghostty") {
        Some(TerminalIdentity::Ghostty)
    } else if term == "foot" || term == "foot-extra" {
        Some(TerminalIdentity::Foot)
    } else {
        None
    }
}

fn classify_tmux_recovered_identity(
    term: &str,
    term_program: &str,
    kitty_window_id_set: bool,
    warp_session_id_set: bool,
    konsole_dbus_set: bool,
    windows_terminal_session_set: bool,
) -> Option<TerminalIdentity> {
    let term = term.to_ascii_lowercase();
    let term_program = term_program.to_ascii_lowercase();
    if term.contains("xterm-kitty") || term_program == "kitty" {
        Some(TerminalIdentity::Kitty)
    } else if term.contains("ghostty") || term_program == "ghostty" {
        Some(TerminalIdentity::Ghostty)
    } else if term.contains("wezterm") || term_program == "wezterm" {
        Some(TerminalIdentity::WezTerm)
    } else if term_program.contains("warp") || warp_session_id_set {
        Some(TerminalIdentity::Warp)
    } else if term_program == "iterm.app" {
        Some(TerminalIdentity::ITerm2)
    } else if konsole_dbus_set {
        Some(TerminalIdentity::Konsole)
    } else if term == "foot" || term == "foot-extra" {
        Some(TerminalIdentity::Foot)
    } else if windows_terminal_session_set {
        Some(TerminalIdentity::WindowsTerminal)
    } else if kitty_window_id_set {
        Some(TerminalIdentity::Kitty)
    } else {
        None
    }
}

fn classify_supported_tmux_env(
    env_lookup: &impl Fn(&str) -> Option<String>,
) -> Option<TerminalIdentity> {
    classify_tmux_recovered_identity(
        &env_lookup("TERM").unwrap_or_default(),
        &env_lookup("TERM_PROGRAM").unwrap_or_default(),
        env_lookup("KITTY_WINDOW_ID").is_some(),
        env_lookup("WARP_SESSION_ID").is_some(),
        env_lookup("KONSOLE_DBUS_SESSION").is_some()
            || env_lookup("KONSOLE_DBUS_SERVICE").is_some()
            || env_lookup("KONSOLE_DBUS_WINDOW").is_some(),
        env_lookup("WT_SESSION").is_some(),
    )
}

fn real_env_lookup(name: &str) -> Option<String> {
    // `to_string_lossy` keeps presence-only semantics for non-UTF-8 values:
    // they still produce `Some(_)` rather than collapsing to `None` like
    // `env::var` would.
    env::var_os(name).map(|v| v.to_string_lossy().into_owned())
}

/// Live `TERM` of the currently attached tmux client. Any failure (missing
/// tmux binary, detached session, non-zero exit, non-UTF-8 output) is silent
/// and returns `None` so the next fallback layer can try.
fn query_tmux_client_termname() -> Option<String> {
    let output = Command::new("tmux")
        .args(["display-message", "-p", "#{client_termname}"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8(output.stdout).ok()?;
    let trimmed = stdout.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn query_tmux_client_env(name: &str) -> Option<String> {
    let pid = query_tmux_client_pid()?;
    read_process_env(pid, name)
}

fn query_tmux_client_pid() -> Option<u32> {
    let output = Command::new("tmux")
        .args(["display-message", "-p", "#{client_pid}"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8(output.stdout).ok()?;
    stdout.trim().parse().ok()
}

#[cfg(target_os = "linux")]
fn read_process_env(pid: u32, name: &str) -> Option<String> {
    let environ = fs::read(format!("/proc/{pid}/environ")).ok()?;
    parse_proc_environ(&environ, name)
}

#[cfg(not(target_os = "linux"))]
fn read_process_env(_: u32, _: &str) -> Option<String> {
    None
}

#[cfg(any(target_os = "linux", test))]
fn parse_proc_environ(environ: &[u8], name: &str) -> Option<String> {
    let mut prefix = name.as_bytes().to_vec();
    prefix.push(b'=');
    environ.split(|&byte| byte == 0).find_map(|entry| {
        entry
            .strip_prefix(prefix.as_slice())
            .map(|value| String::from_utf8_lossy(value).into_owned())
    })
}

/// Look up `name` in the tmux session environment, then in the server-global
/// environment. Each `tmux show-environment` invocation is fallible and
/// silent: any error or unset (`-NAME`) line yields `None` so the caller can
/// fall through to the next layer.
fn query_tmux_env(name: &str) -> Option<String> {
    show_environment_value(&[], name).or_else(|| show_environment_value(&["-g"], name))
}

fn show_environment_value(extra_args: &[&str], name: &str) -> Option<String> {
    let output = Command::new("tmux")
        .arg("show-environment")
        .args(extra_args)
        .arg(name)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8(output.stdout).ok()?;
    parse_show_environment_line(&stdout, name)
}

fn parse_show_environment_line(stdout: &str, name: &str) -> Option<String> {
    let prefix = format!("{name}=");
    let unset = format!("-{name}");
    for line in stdout.lines() {
        let line = line.trim_end_matches('\r');
        if line == unset {
            return None;
        }
        if let Some(value) = line.strip_prefix(&prefix) {
            return Some(value.to_string());
        }
    }
    None
}

pub(in crate::app) fn select_image_protocol(
    identity: TerminalIdentity,
    image_previews_override: bool,
) -> ImageProtocol {
    select_image_protocol_with_zellij(
        identity,
        image_previews_override,
        env::var_os("ZELLIJ").is_some(),
    )
}

fn select_image_protocol_with_zellij(
    identity: TerminalIdentity,
    image_previews_override: bool,
    in_zellij: bool,
) -> ImageProtocol {
    match identity {
        // Zellij currently implements Kitty graphics, but not Unicode placeholders.
        // If Zellij later adds placeholder support, remove this block to let
        // Kitty/Ghostty inside Zellij fall through to `KittyGraphics`.
        TerminalIdentity::Kitty | TerminalIdentity::Ghostty if in_zellij => {
            ImageProtocol::KittyDirectGraphics
        }
        // Use Kitty direct placement inside Zellij since it does not support iTerm inline.
        TerminalIdentity::WezTerm if in_zellij => ImageProtocol::KittyDirectGraphics,

        TerminalIdentity::Kitty => ImageProtocol::KittyGraphics,
        TerminalIdentity::Ghostty => ImageProtocol::KittyGraphics,
        // Warp supports direct Kitty graphics, but not Unicode placeholders.
        // Use Kitty direct placement instead of elio's `KittyGraphics` path.
        TerminalIdentity::Warp => ImageProtocol::KittyDirectGraphics,
        TerminalIdentity::Konsole => ImageProtocol::KittyDirectGraphics,
        TerminalIdentity::WezTerm | TerminalIdentity::ITerm2 => ImageProtocol::ItermInline,
        // Foot and Windows Terminal ≥ 1.22 both support Sixel graphics.
        TerminalIdentity::Foot | TerminalIdentity::WindowsTerminal => ImageProtocol::Sixel,
        // ELIO_IMAGE_PREVIEWS=1 force-enables KittyGraphics on unrecognised terminals
        // for testing. Alacritty is excluded — it does not support image protocols.
        TerminalIdentity::Other if image_previews_override => ImageProtocol::KittyGraphics,
        TerminalIdentity::Alacritty | TerminalIdentity::Other => ImageProtocol::None,
    }
}

pub(in crate::app) fn command_exists(program: &str) -> bool {
    if program.is_empty() {
        return false;
    }

    let program_path = Path::new(program);
    if program_path.components().count() > 1 {
        return executable_file_exists(program_path)
            || cfg!(windows) && executable_file_exists(&program_path.with_extension("exe"));
    }

    env::var_os("PATH").is_some_and(|paths| {
        env::split_paths(&paths).any(|dir| {
            let candidate = dir.join(program);
            executable_file_exists(&candidate)
                || cfg!(windows) && executable_file_exists(&candidate.with_extension("exe"))
        })
    })
}

fn executable_file_exists(path: &Path) -> bool {
    let Ok(metadata) = fs::metadata(path) else {
        return false;
    };
    if !metadata.is_file() {
        return false;
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        metadata.permissions().mode() & 0o111 != 0
    }

    #[cfg(not(unix))]
    {
        true
    }
}

#[cfg(test)]
mod tests;
