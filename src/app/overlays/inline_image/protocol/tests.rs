use super::*;
use std::{
    ffi::OsString,
    sync::{Mutex, OnceLock},
};

#[cfg(unix)]
use std::{
    fs,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

#[cfg(unix)]
fn temp_root(label: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time should be after unix epoch")
        .as_nanos();
    std::env::temp_dir().join(format!("elio-inline-image-{label}-{unique}"))
}

fn terminal_env_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

struct TerminalEnvGuard {
    saved: Vec<(&'static str, Option<OsString>)>,
}

impl TerminalEnvGuard {
    fn isolate() -> Self {
        const VARS: &[&str] = &[
            "TERM",
            "TERM_PROGRAM",
            "KITTY_WINDOW_ID",
            "WARP_SESSION_ID",
            "ALACRITTY_SOCKET",
            "WT_SESSION",
            "KONSOLE_DBUS_SESSION",
            "KONSOLE_DBUS_SERVICE",
            "KONSOLE_DBUS_WINDOW",
            "TMUX",
            "ZELLIJ",
        ];

        let saved = VARS
            .iter()
            .map(|name| (*name, env::var_os(name)))
            .collect::<Vec<_>>();
        for name in VARS {
            unsafe {
                env::remove_var(name);
            }
        }

        Self { saved }
    }
}

impl Drop for TerminalEnvGuard {
    fn drop(&mut self) {
        for (name, value) in &self.saved {
            if let Some(value) = value {
                unsafe {
                    env::set_var(name, value);
                }
            } else {
                unsafe {
                    env::remove_var(name);
                }
            }
        }
    }
}

#[test]
fn detect_terminal_identity_recognizes_iterm2_term_program() {
    let _lock = terminal_env_lock();
    let _guard = TerminalEnvGuard::isolate();

    unsafe {
        env::set_var("TERM_PROGRAM", "iTerm.app");
    }

    assert_eq!(detect_terminal_identity(), TerminalIdentity::ITerm2);
}

#[test]
fn select_image_protocol_kitty_always_enabled() {
    assert_eq!(
        select_image_protocol(TerminalIdentity::Kitty, false),
        ImageProtocol::KittyGraphics
    );
    assert_eq!(
        select_image_protocol(TerminalIdentity::Kitty, true),
        ImageProtocol::KittyGraphics
    );
}

#[test]
fn select_image_protocol_ghostty_always_enabled() {
    assert_eq!(
        select_image_protocol(TerminalIdentity::Ghostty, false),
        ImageProtocol::KittyGraphics
    );
    assert_eq!(
        select_image_protocol(TerminalIdentity::Ghostty, true),
        ImageProtocol::KittyGraphics
    );
}

#[test]
fn select_image_protocol_uses_direct_kitty_graphics_inside_zellij() {
    // Mirrors the Zellij compatibility gate in protocol.rs. If Zellij later
    // supports Kitty Unicode placeholders, this is the test pair to revisit
    // before removing the direct-path override.
    assert_eq!(
        select_image_protocol_with_zellij(TerminalIdentity::Kitty, false, true),
        ImageProtocol::KittyDirectGraphics
    );
    assert_eq!(
        select_image_protocol_with_zellij(TerminalIdentity::Ghostty, false, true),
        ImageProtocol::KittyDirectGraphics
    );
}

#[test]
fn select_image_protocol_keeps_normal_kitty_graphics_outside_zellij() {
    assert_eq!(
        select_image_protocol_with_zellij(TerminalIdentity::Kitty, false, false),
        ImageProtocol::KittyGraphics
    );
    assert_eq!(
        select_image_protocol_with_zellij(TerminalIdentity::Ghostty, false, false),
        ImageProtocol::KittyGraphics
    );
}

#[test]
fn select_image_protocol_wezterm_always_enabled() {
    assert_eq!(
        select_image_protocol(TerminalIdentity::WezTerm, false),
        ImageProtocol::ItermInline
    );
    assert_eq!(
        select_image_protocol(TerminalIdentity::WezTerm, true),
        ImageProtocol::ItermInline
    );
}

#[test]
fn select_image_protocol_iterm2_always_enabled() {
    assert_eq!(
        select_image_protocol(TerminalIdentity::ITerm2, false),
        ImageProtocol::ItermInline
    );
    assert_eq!(
        select_image_protocol(TerminalIdentity::ITerm2, true),
        ImageProtocol::ItermInline
    );
}

#[test]
fn detect_terminal_identity_recognizes_konsole_dbus_session() {
    let _lock = terminal_env_lock();
    let _guard = TerminalEnvGuard::isolate();

    unsafe {
        env::set_var("KONSOLE_DBUS_SESSION", "/Sessions/7");
    }

    assert_eq!(detect_terminal_identity(), TerminalIdentity::Konsole);
}

#[test]
fn detect_terminal_identity_recognizes_konsole_dbus_service() {
    let _lock = terminal_env_lock();
    let _guard = TerminalEnvGuard::isolate();

    unsafe {
        env::set_var("KONSOLE_DBUS_SERVICE", "org.kde.konsole-12345");
    }

    assert_eq!(detect_terminal_identity(), TerminalIdentity::Konsole);
}

#[test]
fn konsole_dbus_takes_precedence_over_inherited_kitty_window_id() {
    let _lock = terminal_env_lock();
    let _guard = TerminalEnvGuard::isolate();

    unsafe {
        env::set_var("KITTY_WINDOW_ID", "1");
        env::set_var("KONSOLE_DBUS_SESSION", "/Sessions/9");
    }

    assert_eq!(detect_terminal_identity(), TerminalIdentity::Konsole);
}

#[test]
fn select_image_protocol_konsole_always_enabled() {
    assert_eq!(
        select_image_protocol(TerminalIdentity::Konsole, false),
        ImageProtocol::KittyDirectGraphics
    );
    assert_eq!(
        select_image_protocol(TerminalIdentity::Konsole, true),
        ImageProtocol::KittyDirectGraphics
    );
}

#[test]
fn wezterm_takes_precedence_over_inherited_konsole_markers() {
    let _lock = terminal_env_lock();
    let _guard = TerminalEnvGuard::isolate();

    unsafe {
        env::set_var("TERM_PROGRAM", "WezTerm");
        env::set_var("KONSOLE_DBUS_SESSION", "/Sessions/9");
    }

    assert_eq!(detect_terminal_identity(), TerminalIdentity::WezTerm);
}

#[test]
fn select_image_protocol_warp_always_enabled() {
    assert_eq!(
        select_image_protocol(TerminalIdentity::Warp, false),
        ImageProtocol::KittyDirectGraphics
    );
    assert_eq!(
        select_image_protocol(TerminalIdentity::Warp, true),
        ImageProtocol::KittyDirectGraphics
    );
}

#[test]
fn warp_markers_take_precedence_over_inherited_kitty_window_id() {
    let _lock = terminal_env_lock();
    let _guard = TerminalEnvGuard::isolate();

    unsafe {
        env::set_var("KITTY_WINDOW_ID", "1");
        env::set_var("WARP_SESSION_ID", "123");
    }

    assert_eq!(detect_terminal_identity(), TerminalIdentity::Warp);
}

#[test]
fn select_image_protocol_alacritty_disabled_and_other_override_enabled() {
    assert_eq!(
        select_image_protocol(TerminalIdentity::Alacritty, true),
        ImageProtocol::None
    );
    assert_eq!(
        select_image_protocol(TerminalIdentity::Other, false),
        ImageProtocol::None
    );
    assert_eq!(
        select_image_protocol(TerminalIdentity::Other, true),
        ImageProtocol::KittyGraphics
    );
}

#[test]
fn detect_terminal_identity_recognizes_foot_term() {
    let _lock = terminal_env_lock();
    let _guard = TerminalEnvGuard::isolate();

    unsafe {
        env::set_var("TERM", "foot");
    }

    assert_eq!(detect_terminal_identity(), TerminalIdentity::Foot);
}

#[test]
fn detect_terminal_identity_recognizes_foot_extra_term() {
    let _lock = terminal_env_lock();
    let _guard = TerminalEnvGuard::isolate();

    unsafe {
        env::set_var("TERM", "foot-extra");
    }

    assert_eq!(detect_terminal_identity(), TerminalIdentity::Foot);
}

#[test]
fn select_image_protocol_foot_uses_sixel() {
    assert_eq!(
        select_image_protocol(TerminalIdentity::Foot, false),
        ImageProtocol::Sixel
    );
    assert_eq!(
        select_image_protocol(TerminalIdentity::Foot, true),
        ImageProtocol::Sixel
    );
}

#[test]
fn detect_terminal_identity_recognizes_windows_terminal_wt_session() {
    let _lock = terminal_env_lock();
    let _guard = TerminalEnvGuard::isolate();

    unsafe {
        env::set_var("WT_SESSION", "00000000-0000-0000-0000-000000000001");
    }

    assert_eq!(
        detect_terminal_identity(),
        TerminalIdentity::WindowsTerminal
    );
}

#[test]
fn select_image_protocol_windows_terminal_uses_sixel() {
    assert_eq!(
        select_image_protocol(TerminalIdentity::WindowsTerminal, false),
        ImageProtocol::Sixel
    );
    assert_eq!(
        select_image_protocol(TerminalIdentity::WindowsTerminal, true),
        ImageProtocol::Sixel
    );
}

#[test]
fn windows_terminal_takes_precedence_over_other_fallback() {
    let _lock = terminal_env_lock();
    let _guard = TerminalEnvGuard::isolate();

    // WT_SESSION present, no other terminal markers → WindowsTerminal
    unsafe {
        env::set_var("WT_SESSION", "some-guid");
    }

    assert_eq!(
        detect_terminal_identity(),
        TerminalIdentity::WindowsTerminal
    );
}

#[cfg(unix)]
#[test]
fn command_exists_checks_direct_executable_paths_without_shelling_out() {
    use std::os::unix::fs::PermissionsExt;

    let root = temp_root("command-exists-direct-path");
    fs::create_dir_all(&root).expect("failed to create temp root");

    let executable = root.join("demo-tool");
    fs::write(&executable, b"#!/bin/sh\nexit 0\n").expect("failed to write test executable");

    let mut permissions = fs::metadata(&executable)
        .expect("test executable metadata should exist")
        .permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&executable, permissions).expect("failed to mark test executable");

    assert!(command_exists(
        executable.to_str().expect("path should be valid utf-8")
    ));

    let not_executable = root.join("demo-data");
    fs::write(&not_executable, b"plain data").expect("failed to write plain file");
    assert!(!command_exists(
        not_executable.to_str().expect("path should be valid utf-8")
    ));
}

fn no_tmux_client_term() -> Option<String> {
    None
}

fn no_tmux_env_lookup(_: &str) -> Option<String> {
    None
}

fn tmux_set_only(env: &str) -> impl Fn(&str) -> Option<String> + '_ {
    move |name: &str| {
        if name == "TMUX" {
            Some(env.to_string())
        } else {
            None
        }
    }
}

#[test]
fn detect_terminal_identity_inside_tmux_uses_client_termname_for_kitty() {
    let id = detect_terminal_identity_with(
        tmux_set_only("/tmp/tmux-1000/default,123,4"),
        || Some("xterm-kitty".to_string()),
        no_tmux_env_lookup,
        no_tmux_env_lookup,
    );
    assert_eq!(id, TerminalIdentity::Kitty);
}

#[test]
fn detect_terminal_identity_inside_tmux_uses_client_termname_for_ghostty() {
    let id = detect_terminal_identity_with(
        tmux_set_only("/tmp/tmux-1000/default,123,4"),
        || Some("xterm-ghostty".to_string()),
        no_tmux_env_lookup,
        no_tmux_env_lookup,
    );
    assert_eq!(id, TerminalIdentity::Ghostty);
}

#[test]
fn detect_terminal_identity_inside_tmux_uses_client_termname_for_foot() {
    let id = detect_terminal_identity_with(
        tmux_set_only("/tmp/tmux-1000/default,123,4"),
        || Some("foot".to_string()),
        no_tmux_env_lookup,
        no_tmux_env_lookup,
    );
    assert_eq!(id, TerminalIdentity::Foot);
}

#[test]
fn detect_terminal_identity_inside_tmux_uses_client_termname_for_foot_extra() {
    let id = detect_terminal_identity_with(
        tmux_set_only("/tmp/tmux-1000/default,123,4"),
        || Some("foot-extra".to_string()),
        no_tmux_env_lookup,
        no_tmux_env_lookup,
    );
    assert_eq!(id, TerminalIdentity::Foot);
}

#[test]
fn detect_terminal_identity_inside_tmux_falls_back_to_session_env_kitty_window_id_presence() {
    // Empty value still counts as Kitty: tmux records `KITTY_WINDOW_ID=` for
    // present-but-empty vars, which must mirror `env::var_os(...).is_some()`.
    let id = detect_terminal_identity_with(
        tmux_set_only("/tmp/tmux-1000/default,123,4"),
        no_tmux_client_term,
        no_tmux_env_lookup,
        |name| {
            if name == "KITTY_WINDOW_ID" {
                Some(String::new())
            } else {
                None
            }
        },
    );
    assert_eq!(id, TerminalIdentity::Kitty);
}

#[test]
fn detect_terminal_identity_inside_tmux_falls_back_to_session_env_term_program_ghostty() {
    let id = detect_terminal_identity_with(
        tmux_set_only("/tmp/tmux-1000/default,123,4"),
        no_tmux_client_term,
        no_tmux_env_lookup,
        |name| {
            if name == "TERM_PROGRAM" {
                Some("ghostty".to_string())
            } else {
                None
            }
        },
    );
    assert_eq!(id, TerminalIdentity::Ghostty);
}

#[test]
fn detect_terminal_identity_inside_tmux_falls_back_to_session_env_term_program_wezterm() {
    let id = detect_terminal_identity_with(
        tmux_set_only("/tmp/tmux-1000/default,123,4"),
        no_tmux_client_term,
        no_tmux_env_lookup,
        |name| {
            if name == "TERM_PROGRAM" {
                Some("WezTerm".to_string())
            } else {
                None
            }
        },
    );
    assert_eq!(id, TerminalIdentity::WezTerm);
}

#[test]
fn detect_terminal_identity_inside_tmux_falls_back_to_session_env_term_program_iterm2() {
    let id = detect_terminal_identity_with(
        tmux_set_only("/tmp/tmux-1000/default,123,4"),
        no_tmux_client_term,
        no_tmux_env_lookup,
        |name| {
            if name == "TERM_PROGRAM" {
                Some("iTerm.app".to_string())
            } else {
                None
            }
        },
    );
    assert_eq!(id, TerminalIdentity::ITerm2);
}

#[test]
fn detect_terminal_identity_inside_tmux_falls_back_to_session_env_warp() {
    let id = detect_terminal_identity_with(
        tmux_set_only("/tmp/tmux-1000/default,123,4"),
        no_tmux_client_term,
        no_tmux_env_lookup,
        |name| {
            if name == "WARP_SESSION_ID" {
                Some("123".to_string())
            } else {
                None
            }
        },
    );
    assert_eq!(id, TerminalIdentity::Warp);
}

#[test]
fn detect_terminal_identity_inside_tmux_falls_back_to_session_env_konsole() {
    let id = detect_terminal_identity_with(
        tmux_set_only("/tmp/tmux-1000/default,123,4"),
        no_tmux_client_term,
        no_tmux_env_lookup,
        |name| {
            if name == "KONSOLE_DBUS_SESSION" {
                Some("/Sessions/3".to_string())
            } else {
                None
            }
        },
    );
    assert_eq!(id, TerminalIdentity::Konsole);
}

#[test]
fn detect_terminal_identity_inside_tmux_falls_back_to_session_env_foot_term() {
    let id = detect_terminal_identity_with(
        tmux_set_only("/tmp/tmux-1000/default,123,4"),
        no_tmux_client_term,
        no_tmux_env_lookup,
        |name| {
            if name == "TERM" {
                Some("foot".to_string())
            } else {
                None
            }
        },
    );
    assert_eq!(id, TerminalIdentity::Foot);
}

#[test]
fn detect_terminal_identity_inside_tmux_falls_back_to_live_client_wt_session() {
    let id = detect_terminal_identity_with(
        tmux_set_only("/tmp/tmux-1000/default,123,4"),
        no_tmux_client_term,
        |name| {
            if name == "WT_SESSION" {
                Some("live-wt".to_string())
            } else {
                None
            }
        },
        no_tmux_env_lookup,
    );
    assert_eq!(id, TerminalIdentity::WindowsTerminal);
}

#[test]
fn detect_terminal_identity_inside_tmux_falls_back_to_session_env_wt_session() {
    let id = detect_terminal_identity_with(
        tmux_set_only("/tmp/tmux-1000/default,123,4"),
        no_tmux_client_term,
        no_tmux_env_lookup,
        |name| {
            if name == "WT_SESSION" {
                Some("session-wt".to_string())
            } else {
                None
            }
        },
    );
    assert_eq!(id, TerminalIdentity::WindowsTerminal);
}

#[test]
fn detect_terminal_identity_inside_tmux_uses_live_client_env_before_session_env() {
    let id = detect_terminal_identity_with(
        tmux_set_only("/tmp/tmux-1000/default,123,4"),
        no_tmux_client_term,
        |name| {
            if name == "WARP_SESSION_ID" {
                Some("live-client".to_string())
            } else {
                None
            }
        },
        |name| {
            if name == "KITTY_WINDOW_ID" {
                Some("stale-session".to_string())
            } else {
                None
            }
        },
    );
    assert_eq!(id, TerminalIdentity::Warp);
}

#[test]
fn detect_terminal_identity_inside_tmux_returns_other_for_generic_client_termname() {
    let id = detect_terminal_identity_with(
        tmux_set_only("/tmp/tmux-1000/default,123,4"),
        || Some("xterm-256color".to_string()),
        no_tmux_env_lookup,
        no_tmux_env_lookup,
    );
    assert_eq!(id, TerminalIdentity::Other);
}

#[test]
fn detect_terminal_identity_outside_tmux_skips_tmux_helpers() {
    use std::cell::Cell;

    let client_calls = Cell::new(0u32);
    let client_env_calls = Cell::new(0u32);
    let env_calls = Cell::new(0u32);
    let id = detect_terminal_identity_with(
        |_| None,
        || {
            client_calls.set(client_calls.get() + 1);
            Some("xterm-kitty".to_string())
        },
        |_| {
            client_env_calls.set(client_env_calls.get() + 1);
            Some(String::new())
        },
        |_| {
            env_calls.set(env_calls.get() + 1);
            Some(String::new())
        },
    );
    assert_eq!(id, TerminalIdentity::Other);
    assert_eq!(client_calls.get(), 0);
    assert_eq!(client_env_calls.get(), 0);
    assert_eq!(env_calls.get(), 0);
}

#[test]
fn detect_terminal_identity_non_kitty_direct_env_takes_precedence_over_tmux_lookups() {
    use std::cell::Cell;

    let client_calls = Cell::new(0u32);
    let client_env_calls = Cell::new(0u32);
    let env_calls = Cell::new(0u32);
    let id = detect_terminal_identity_with(
        |name| match name {
            "TMUX" => Some("/tmp/tmux-1000/default,123,4".to_string()),
            "TERM_PROGRAM" => Some("ghostty".to_string()),
            _ => None,
        },
        || {
            client_calls.set(client_calls.get() + 1);
            Some("xterm-kitty".to_string())
        },
        |_| {
            client_env_calls.set(client_env_calls.get() + 1);
            None
        },
        |_| {
            env_calls.set(env_calls.get() + 1);
            None
        },
    );
    assert_eq!(id, TerminalIdentity::Ghostty);
    assert_eq!(
        client_calls.get(),
        0,
        "tmux helpers should not run when non-Kitty direct env detection succeeds"
    );
    assert_eq!(client_env_calls.get(), 0);
    assert_eq!(env_calls.get(), 0);
}

#[test]
fn detect_terminal_identity_recovers_warp_from_tmux_when_stale_kitty_marker_leaks() {
    let id = detect_terminal_identity_with(
        |name| match name {
            "TMUX" => Some("/tmp/tmux-1000/default,123,4".to_string()),
            "TERM" => Some("tmux-256color".to_string()),
            "TERM_PROGRAM" => Some("tmux".to_string()),
            "KITTY_WINDOW_ID" => Some("1".to_string()),
            _ => None,
        },
        || Some("xterm-256color".to_string()),
        no_tmux_env_lookup,
        |name| {
            if name == "WARP_SESSION_ID" {
                Some("123".to_string())
            } else {
                None
            }
        },
    );
    assert_eq!(id, TerminalIdentity::Warp);
}

#[test]
fn detect_terminal_identity_recovers_warp_from_live_client_env_when_stale_kitty_marker_leaks() {
    let id = detect_terminal_identity_with(
        |name| match name {
            "TMUX" => Some("/tmp/tmux-1000/default,123,4".to_string()),
            "TERM" => Some("tmux-256color".to_string()),
            "TERM_PROGRAM" => Some("tmux".to_string()),
            "KITTY_WINDOW_ID" => Some("1".to_string()),
            _ => None,
        },
        || Some("xterm-256color".to_string()),
        |name| {
            if name == "WARP_SESSION_ID" {
                Some("live-client".to_string())
            } else {
                None
            }
        },
        no_tmux_env_lookup,
    );
    assert_eq!(id, TerminalIdentity::Warp);
}

#[test]
fn detect_terminal_identity_recovers_konsole_from_tmux_when_stale_kitty_marker_leaks() {
    let id = detect_terminal_identity_with(
        |name| match name {
            "TMUX" => Some("/tmp/tmux-1000/default,123,4".to_string()),
            "TERM" => Some("tmux-256color".to_string()),
            "TERM_PROGRAM" => Some("tmux".to_string()),
            "KITTY_WINDOW_ID" => Some("1".to_string()),
            _ => None,
        },
        || Some("xterm-256color".to_string()),
        no_tmux_env_lookup,
        |name| {
            if name == "KONSOLE_DBUS_SESSION" {
                Some("/Sessions/3".to_string())
            } else {
                None
            }
        },
    );
    assert_eq!(id, TerminalIdentity::Konsole);
}

#[test]
fn detect_terminal_identity_recovers_konsole_from_live_client_env_when_stale_kitty_marker_leaks() {
    let id = detect_terminal_identity_with(
        |name| match name {
            "TMUX" => Some("/tmp/tmux-1000/default,123,4".to_string()),
            "TERM" => Some("tmux-256color".to_string()),
            "TERM_PROGRAM" => Some("tmux".to_string()),
            "KITTY_WINDOW_ID" => Some("1".to_string()),
            _ => None,
        },
        || Some("xterm-256color".to_string()),
        |name| {
            if name == "KONSOLE_DBUS_SESSION" {
                Some("/Sessions/live".to_string())
            } else {
                None
            }
        },
        no_tmux_env_lookup,
    );
    assert_eq!(id, TerminalIdentity::Konsole);
}

#[test]
fn detect_terminal_identity_recovers_foot_from_tmux_when_stale_kitty_marker_leaks() {
    let id = detect_terminal_identity_with(
        |name| match name {
            "TMUX" => Some("/tmp/tmux-1000/default,123,4".to_string()),
            "TERM" => Some("tmux-256color".to_string()),
            "TERM_PROGRAM" => Some("tmux".to_string()),
            "KITTY_WINDOW_ID" => Some("1".to_string()),
            _ => None,
        },
        || Some("foot".to_string()),
        no_tmux_env_lookup,
        no_tmux_env_lookup,
    );
    assert_eq!(id, TerminalIdentity::Foot);
}

#[test]
fn detect_terminal_identity_recovers_wt_from_live_client_env_when_stale_kitty_marker_leaks() {
    let id = detect_terminal_identity_with(
        |name| match name {
            "TMUX" => Some("/tmp/tmux-1000/default,123,4".to_string()),
            "TERM" => Some("tmux-256color".to_string()),
            "TERM_PROGRAM" => Some("tmux".to_string()),
            "KITTY_WINDOW_ID" => Some("1".to_string()),
            _ => None,
        },
        || Some("xterm-256color".to_string()),
        |name| {
            if name == "WT_SESSION" {
                Some("live-wt".to_string())
            } else {
                None
            }
        },
        no_tmux_env_lookup,
    );
    assert_eq!(id, TerminalIdentity::WindowsTerminal);
}

#[test]
fn detect_terminal_identity_keeps_kitty_when_tmux_client_reports_kitty() {
    let id = detect_terminal_identity_with(
        |name| match name {
            "TMUX" => Some("/tmp/tmux-1000/default,123,4".to_string()),
            "TERM" => Some("tmux-256color".to_string()),
            "TERM_PROGRAM" => Some("tmux".to_string()),
            "KITTY_WINDOW_ID" => Some("1".to_string()),
            _ => None,
        },
        || Some("xterm-kitty".to_string()),
        no_tmux_env_lookup,
        |name| {
            if name == "WARP_SESSION_ID" {
                Some("stale".to_string())
            } else {
                None
            }
        },
    );
    assert_eq!(id, TerminalIdentity::Kitty);
}

#[test]
fn detect_terminal_identity_keeps_live_client_kitty_over_stale_session_warp() {
    let id = detect_terminal_identity_with(
        |name| match name {
            "TMUX" => Some("/tmp/tmux-1000/default,123,4".to_string()),
            "TERM" => Some("tmux-256color".to_string()),
            "TERM_PROGRAM" => Some("tmux".to_string()),
            "KITTY_WINDOW_ID" => Some("1".to_string()),
            _ => None,
        },
        || Some("xterm-256color".to_string()),
        |name| {
            if name == "KITTY_WINDOW_ID" {
                Some("live-client".to_string())
            } else {
                None
            }
        },
        |name| {
            if name == "WARP_SESSION_ID" {
                Some("stale-session".to_string())
            } else {
                None
            }
        },
    );
    assert_eq!(id, TerminalIdentity::Kitty);
}

#[test]
fn detect_terminal_identity_recovers_ghostty_when_stale_alacritty_marker_leaks() {
    // tmux server first launched from Alacritty leaks ALACRITTY_SOCKET into every
    // pane; a later Ghostty client must still be detected as Ghostty so image
    // previews work. Regression for images silently disabled in tmux+Ghostty.
    let id = detect_terminal_identity_with(
        |name| match name {
            "TMUX" => Some("/tmp/tmux-1000/default,123,4".to_string()),
            "TERM" => Some("tmux-256color".to_string()),
            "TERM_PROGRAM" => Some("tmux".to_string()),
            "ALACRITTY_SOCKET" => Some("/run/user/1000/Alacritty-wayland-1.sock".to_string()),
            _ => None,
        },
        || Some("xterm-ghostty".to_string()),
        no_tmux_env_lookup,
        no_tmux_env_lookup,
    );
    assert_eq!(id, TerminalIdentity::Ghostty);
}

#[test]
fn detect_terminal_identity_recovers_kitty_when_stale_alacritty_marker_leaks() {
    // The recovery must also be correct when the live client really is Kitty
    // (not just the no-op "keep Alacritty" case).
    let id = detect_terminal_identity_with(
        |name| match name {
            "TMUX" => Some("/tmp/tmux-1000/default,123,4".to_string()),
            "TERM" => Some("tmux-256color".to_string()),
            "TERM_PROGRAM" => Some("tmux".to_string()),
            "ALACRITTY_SOCKET" => Some("/run/user/1000/Alacritty-wayland-1.sock".to_string()),
            _ => None,
        },
        || Some("xterm-kitty".to_string()),
        no_tmux_env_lookup,
        no_tmux_env_lookup,
    );
    assert_eq!(id, TerminalIdentity::Kitty);
}

#[test]
fn detect_terminal_identity_keeps_alacritty_in_tmux_when_client_is_really_alacritty() {
    // Real Alacritty inside tmux exposes no graphics-capable client signal, so
    // the leaked-marker recovery must fall back to the (correct) Alacritty guess
    // rather than mislabel it.
    let id = detect_terminal_identity_with(
        |name| match name {
            "TMUX" => Some("/tmp/tmux-1000/default,123,4".to_string()),
            "TERM" => Some("tmux-256color".to_string()),
            "TERM_PROGRAM" => Some("tmux".to_string()),
            "ALACRITTY_SOCKET" => Some("/run/user/1000/Alacritty-wayland-1.sock".to_string()),
            _ => None,
        },
        || Some("alacritty".to_string()),
        no_tmux_env_lookup,
        no_tmux_env_lookup,
    );
    assert_eq!(id, TerminalIdentity::Alacritty);
}

#[test]
fn detect_terminal_identity_alacritty_marker_kept_outside_tmux() {
    // Outside tmux the marker is authoritative and tmux helpers must not run.
    use std::cell::Cell;

    let client_calls = Cell::new(0u32);
    let id = detect_terminal_identity_with(
        |name| match name {
            "ALACRITTY_SOCKET" => Some("/run/user/1000/Alacritty-wayland-1.sock".to_string()),
            _ => None,
        },
        || {
            client_calls.set(client_calls.get() + 1);
            Some("xterm-ghostty".to_string())
        },
        no_tmux_env_lookup,
        no_tmux_env_lookup,
    );
    assert_eq!(id, TerminalIdentity::Alacritty);
    assert_eq!(client_calls.get(), 0);
}

#[test]
fn parse_show_environment_line_handles_set_unset_and_empty() {
    assert_eq!(
        parse_show_environment_line("KITTY_WINDOW_ID=42\n", "KITTY_WINDOW_ID"),
        Some("42".to_string())
    );
    assert_eq!(
        parse_show_environment_line("KITTY_WINDOW_ID=\n", "KITTY_WINDOW_ID"),
        Some(String::new())
    );
    assert_eq!(
        parse_show_environment_line("-KITTY_WINDOW_ID\n", "KITTY_WINDOW_ID"),
        None
    );
    assert_eq!(parse_show_environment_line("", "KITTY_WINDOW_ID"), None);
    assert_eq!(
        parse_show_environment_line("OTHER=value\nKITTY_WINDOW_ID=7\n", "KITTY_WINDOW_ID"),
        Some("7".to_string())
    );
}

#[test]
fn parse_proc_environ_handles_present_empty_and_missing_values() {
    let environ = b"TERM=xterm-256color\0WARP_SESSION_ID=123\0KITTY_WINDOW_ID=\0";
    assert_eq!(
        parse_proc_environ(environ, "WARP_SESSION_ID"),
        Some("123".to_string())
    );
    assert_eq!(
        parse_proc_environ(environ, "KITTY_WINDOW_ID"),
        Some(String::new())
    );
    assert_eq!(parse_proc_environ(environ, "KONSOLE_DBUS_SESSION"), None);
}
