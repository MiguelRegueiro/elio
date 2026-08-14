use std::{
    ffi::OsString,
    io::{self, ErrorKind, Write},
    path::Path,
    process::Command,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ShellInvocation {
    pub(crate) program: OsString,
    pub(crate) args: Vec<OsString>,
}

impl ShellInvocation {
    fn label(&self) -> String {
        if self.args.is_empty() {
            self.program.to_string_lossy().into_owned()
        } else {
            let args = self
                .args
                .iter()
                .map(|arg| arg.to_string_lossy())
                .collect::<Vec<_>>()
                .join(" ");
            format!("{} {args}", self.program.to_string_lossy())
        }
    }
}

pub(crate) fn run_in_current_terminal(cwd: &Path) -> Result<(), String> {
    ensure_cwd_exists(cwd)?;
    let cwd_label = crate::path_display::user_facing(cwd);

    #[cfg(unix)]
    let (invocations, invoking_user) = unix_shell_launch(
        crate::config::invoking_user_context(),
        std::env::var_os("SHELL"),
    )?;
    #[cfg(not(unix))]
    let invocations = shell_invocations();
    let tried: Vec<String> = invocations.iter().map(ShellInvocation::label).collect();

    #[cfg(unix)]
    print_shell_banner(
        &cwd_label,
        invoking_user.as_ref().map(|user| user.name.as_os_str()),
    )?;
    #[cfg(not(unix))]
    print_shell_banner(&cwd_label, None)?;

    for invocation in invocations {
        let mut command = Command::new(&invocation.program);
        command.args(&invocation.args).env("ELIO_SHELL", "1").env(
            "ELIO_LEVEL",
            next_shell_level(std::env::var_os("ELIO_LEVEL")),
        );

        #[cfg(unix)]
        if let Some(user) = &invoking_user {
            crate::invoking_user_command::prepare(&mut command, user, Some(cwd)).map_err(
                |error| format!("Could not prepare shell as invoking user in {cwd_label}: {error}"),
            )?;
        } else {
            command.current_dir(cwd);
        }
        #[cfg(not(unix))]
        command.current_dir(cwd);

        match command.status() {
            Ok(_) => return Ok(()),
            Err(error) if error.kind() == ErrorKind::NotFound => {
                ensure_cwd_exists(cwd)?;
            }
            Err(error) => {
                #[cfg(unix)]
                if let Some(user) = &invoking_user {
                    return Err(format!(
                        "Could not open shell as {} in {cwd_label}: {error}",
                        user.name.to_string_lossy()
                    ));
                }
                return Err(format!("Could not open shell in {cwd_label}: {error}"));
            }
        }
    }

    Err(format!(
        "Could not find a shell to open in {cwd_label} (tried {})",
        tried.join(", ")
    ))
}

fn print_shell_banner(cwd_label: &str, user: Option<&std::ffi::OsStr>) -> Result<(), String> {
    let mut stdout = io::stdout();
    let (label_style, value_style, dim_style, reset_style) = if shell_banner_color_enabled() {
        ("\x1b[1;36m", "\x1b[1m", "\x1b[2m", "\x1b[0m")
    } else {
        ("", "", "", "")
    };

    let opened = match user {
        Some(user) => format!(
            "{label_style}elio:{reset_style} opened shell as {value_style}{}{reset_style} in {value_style}{cwd_label}{reset_style}",
            user.to_string_lossy()
        ),
        None => format!(
            "{label_style}elio:{reset_style} opened shell in {value_style}{cwd_label}{reset_style}"
        ),
    };
    writeln!(stdout, "{opened}")
        .and_then(|()| {
            writeln!(
                stdout,
                "{dim_style}return:{reset_style} {}",
                shell_return_hint()
            )
        })
        .and_then(|()| writeln!(stdout))
        .and_then(|()| stdout.flush())
        .map_err(|error| format!("Could not prepare shell in {cwd_label}: {error}"))
}

fn shell_banner_color_enabled() -> bool {
    std::env::var_os("NO_COLOR").is_none()
        && std::env::var_os("TERM").is_none_or(|term| term != "dumb")
}

fn shell_return_hint() -> &'static str {
    #[cfg(windows)]
    {
        "exit"
    }

    #[cfg(not(windows))]
    {
        "exit or Ctrl+D"
    }
}

#[cfg(windows)]
pub(crate) fn shell_invocations() -> Vec<ShellInvocation> {
    windows_shell_invocations(std::env::var_os("COMSPEC"))
}

#[cfg(unix)]
fn unix_shell_launch(
    context: crate::config::InvocationContext,
    inherited_shell: Option<OsString>,
) -> Result<(Vec<ShellInvocation>, Option<crate::config::InvokingUser>), String> {
    match context {
        crate::config::InvocationContext::Normal
        | crate::config::InvocationContext::RootSession => {
            Ok((unix_shell_invocations(inherited_shell), None))
        }
        crate::config::InvocationContext::Elevated(user) => {
            Ok((unix_shell_invocations(Some(user.shell.clone())), Some(user)))
        }
        crate::config::InvocationContext::ElevatedUnresolved => {
            Err("Could not resolve invoking user; shell was not opened".to_string())
        }
    }
}

#[cfg(any(not(windows), test))]
fn unix_shell_invocations(shell: Option<OsString>) -> Vec<ShellInvocation> {
    let fallback = ShellInvocation {
        program: OsString::from("/bin/sh"),
        args: Vec::new(),
    };

    let Some(program) = non_empty_env_value(shell) else {
        return vec![fallback];
    };

    let configured = ShellInvocation {
        program,
        args: Vec::new(),
    };
    if configured.program == fallback.program {
        vec![configured]
    } else {
        vec![configured, fallback]
    }
}

#[cfg(any(windows, test))]
fn windows_shell_invocations(comspec: Option<OsString>) -> Vec<ShellInvocation> {
    let mut invocations = Vec::new();
    if let Some(program) = non_empty_env_value(comspec) {
        invocations.push(ShellInvocation {
            program,
            args: Vec::new(),
        });
    }

    invocations.extend([
        ShellInvocation {
            program: OsString::from("pwsh"),
            args: vec![OsString::from("-NoLogo")],
        },
        ShellInvocation {
            program: OsString::from("powershell"),
            args: vec![OsString::from("-NoLogo")],
        },
        ShellInvocation {
            program: OsString::from("cmd"),
            args: Vec::new(),
        },
    ]);
    invocations
}

fn non_empty_env_value(value: Option<OsString>) -> Option<OsString> {
    let value = value?;
    (!value.to_string_lossy().trim().is_empty()).then_some(value)
}

fn next_shell_level(current: Option<OsString>) -> OsString {
    let Some(current) = current else {
        return OsString::from("1");
    };
    let Ok(level) = current.to_string_lossy().trim().parse::<u32>() else {
        return OsString::from("1");
    };
    OsString::from(level.saturating_add(1).to_string())
}

fn ensure_cwd_exists(cwd: &Path) -> Result<(), String> {
    let cwd_label = crate::path_display::user_facing(cwd);
    match cwd.try_exists() {
        Ok(true) => Ok(()),
        Ok(false) => Err(format!(
            "Cannot open shell in {}: folder no longer exists",
            cwd_label
        )),
        Err(error) => Err(format!("Cannot open shell in {cwd_label}: {error}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unix_shell_uses_shell_env_then_sh_fallback() {
        assert_eq!(
            unix_shell_invocations(Some(OsString::from("/bin/bash"))),
            vec![
                ShellInvocation {
                    program: OsString::from("/bin/bash"),
                    args: Vec::new(),
                },
                ShellInvocation {
                    program: OsString::from("/bin/sh"),
                    args: Vec::new(),
                },
            ]
        );
    }

    #[test]
    fn unix_shell_uses_only_sh_when_shell_is_empty() {
        assert_eq!(
            unix_shell_invocations(Some(OsString::from(""))),
            vec![ShellInvocation {
                program: OsString::from("/bin/sh"),
                args: Vec::new(),
            }]
        );
    }

    #[test]
    fn unix_shell_does_not_duplicate_sh_fallback() {
        assert_eq!(
            unix_shell_invocations(Some(OsString::from("/bin/sh"))),
            vec![ShellInvocation {
                program: OsString::from("/bin/sh"),
                args: Vec::new(),
            }]
        );
    }

    #[cfg(unix)]
    fn test_invoking_user(shell: &str) -> crate::config::InvokingUser {
        crate::config::InvokingUser {
            uid: 1000,
            gid: 1000,
            name: OsString::from("paco"),
            home: "/home/paco".into(),
            shell: OsString::from(shell),
            groups: vec![1000],
            session_environment: Vec::new(),
            xdg_config_home: None,
            xdg_data_home: None,
        }
    }

    #[cfg(unix)]
    #[test]
    fn elevated_shell_uses_passwd_shell_not_inherited_root_shell() {
        let user = test_invoking_user("/bin/fish");
        let (invocations, actual_user) = unix_shell_launch(
            crate::config::InvocationContext::Elevated(user),
            Some(OsString::from("/bin/root-shell")),
        )
        .unwrap();

        assert_eq!(invocations[0].program, OsString::from("/bin/fish"));
        assert_eq!(invocations[1].program, OsString::from("/bin/sh"));
        assert_eq!(actual_user.unwrap().name, OsString::from("paco"));
    }

    #[cfg(unix)]
    #[test]
    fn unresolved_elevated_shell_fails_closed() {
        let error = unix_shell_launch(
            crate::config::InvocationContext::ElevatedUnresolved,
            Some(OsString::from("/bin/root-shell")),
        )
        .unwrap_err();

        assert_eq!(
            error,
            "Could not resolve invoking user; shell was not opened"
        );
    }

    #[test]
    fn windows_shell_uses_comspec_before_powershell_fallbacks() {
        assert_eq!(
            windows_shell_invocations(Some(OsString::from(r"C:\Windows\System32\cmd.exe"))),
            vec![
                ShellInvocation {
                    program: OsString::from(r"C:\Windows\System32\cmd.exe"),
                    args: Vec::new(),
                },
                ShellInvocation {
                    program: OsString::from("pwsh"),
                    args: vec![OsString::from("-NoLogo")],
                },
                ShellInvocation {
                    program: OsString::from("powershell"),
                    args: vec![OsString::from("-NoLogo")],
                },
                ShellInvocation {
                    program: OsString::from("cmd"),
                    args: Vec::new(),
                },
            ]
        );
    }

    #[test]
    fn windows_shell_falls_back_when_comspec_is_empty() {
        assert_eq!(
            windows_shell_invocations(Some(OsString::from(" "))),
            vec![
                ShellInvocation {
                    program: OsString::from("pwsh"),
                    args: vec![OsString::from("-NoLogo")],
                },
                ShellInvocation {
                    program: OsString::from("powershell"),
                    args: vec![OsString::from("-NoLogo")],
                },
                ShellInvocation {
                    program: OsString::from("cmd"),
                    args: Vec::new(),
                },
            ]
        );
    }

    #[test]
    fn shell_level_starts_at_one() {
        assert_eq!(next_shell_level(None), OsString::from("1"));
        assert_eq!(
            next_shell_level(Some(OsString::from(""))),
            OsString::from("1")
        );
        assert_eq!(
            next_shell_level(Some(OsString::from("not-a-number"))),
            OsString::from("1")
        );
    }

    #[test]
    fn shell_level_increments_existing_level() {
        assert_eq!(
            next_shell_level(Some(OsString::from("1"))),
            OsString::from("2")
        );
        assert_eq!(
            next_shell_level(Some(OsString::from(" 41 "))),
            OsString::from("42")
        );
    }

    #[test]
    fn cwd_check_reports_deleted_folder() {
        let missing = std::env::temp_dir().join(format!(
            "elio-missing-shell-cwd-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock should be after unix epoch")
                .as_nanos()
        ));

        let error = ensure_cwd_exists(&missing).expect_err("missing cwd should fail");

        assert!(
            error.contains("folder no longer exists"),
            "unexpected error: {error}"
        );
    }
}
