use std::{
    io,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

#[cfg(not(unix))]
use std::env;

#[cfg(unix)]
use std::ffi::OsString;
#[cfg(unix)]
use std::os::unix::ffi::OsStringExt;

pub(crate) enum QueryResult {
    Selected(PathBuf),
    Cancelled,
    NotFound,
    PickerNotFound,
    Empty,
    OnlyCurrentDirectory,
    LaunchFailed,
}

pub(crate) fn preflight(cwd: &Path) -> Option<QueryResult> {
    match has_match_excluding(cwd) {
        Ok(true) => None,
        Ok(false) => match has_any_match() {
            Ok(true) => Some(QueryResult::OnlyCurrentDirectory),
            Ok(false) => Some(QueryResult::Empty),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Some(QueryResult::NotFound),
            Err(_) => None,
        },
        Err(error) if error.kind() == io::ErrorKind::NotFound => Some(QueryResult::NotFound),
        Err(_) => None,
    }
}

pub(crate) fn run_query_in_terminal(cwd: &Path) -> QueryResult {
    let mut command = match zoxide_command() {
        Ok(command) => command,
        Err(_) => return QueryResult::LaunchFailed,
    };
    let output = match command
        .args(["query", "-i", "--exclude"])
        .arg(cwd)
        .env("SHELL", "sh")
        .env("CLICOLOR", "1")
        .env("CLICOLOR_FORCE", "1")
        .env("_ZO_FZF_OPTS", fzf_options())
        .stdin(Stdio::inherit())
        .output()
    {
        Ok(output) => output,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return QueryResult::NotFound,
        Err(_) => return QueryResult::LaunchFailed,
    };

    if !output.status.success() {
        if stderr_mentions_missing_fzf(&output.stderr) {
            return QueryResult::PickerNotFound;
        }
        return QueryResult::Cancelled;
    }

    match path_from_command_stdout(output.stdout) {
        Some(path) => QueryResult::Selected(path),
        None => QueryResult::Cancelled,
    }
}

fn has_match_excluding(cwd: &Path) -> io::Result<bool> {
    let output = zoxide_command()?
        .args(["query", "-l", "--exclude"])
        .arg(cwd)
        .output()?;
    Ok(!output.stdout.is_empty())
}

fn has_any_match() -> io::Result<bool> {
    let output = zoxide_command()?.args(["query", "-l"]).output()?;
    Ok(!output.stdout.is_empty())
}

fn zoxide_command() -> io::Result<Command> {
    let command = Command::new("zoxide");
    #[cfg(unix)]
    {
        let mut command = command;
        crate::invoking_user_command::prepare_external(&mut command, None)?;
        Ok(command)
    }
    #[cfg(not(unix))]
    {
        Ok(command)
    }
}

fn stderr_mentions_missing_fzf(stderr: &[u8]) -> bool {
    // zoxide reports a missing interactive picker through stderr; keep this
    // narrow so ordinary cancellations remain silent.
    let stderr = String::from_utf8_lossy(stderr).to_ascii_lowercase();
    stderr.contains("fzf")
        && (stderr.contains("not found")
            || stderr.contains("not installed")
            || stderr.contains("no such file")
            || stderr.contains("could not find"))
}

fn fzf_options() -> String {
    fzf_options_with(invoking_user_environment_string)
}

fn invoking_user_environment_string(name: &str) -> Option<String> {
    #[cfg(unix)]
    let value = crate::config::invoking_user_env_var(name);
    #[cfg(not(unix))]
    let value = env::var_os(name);

    value.and_then(|value| value.into_string().ok())
}

fn fzf_options_with(mut environment_value: impl FnMut(&str) -> Option<String>) -> String {
    let defaults = fzf_default_options().join(" ");

    match (
        environment_value("FZF_DEFAULT_OPTS"),
        environment_value("ELIO_ZOXIDE_OPTS"),
    ) {
        (Some(base), Some(extra)) => format!("{base} {defaults} {extra}"),
        (Some(base), None) => format!("{base} {defaults}"),
        (None, Some(extra)) => format!("{defaults} {extra}"),
        (None, None) => defaults,
    }
}

fn fzf_default_options() -> Vec<&'static str> {
    let mut defaults = vec![
        "--exact",
        "--no-sort",
        "--bind=ctrl-z:ignore,btab:up,tab:down",
        "--cycle",
        "--keep-right",
        "--layout=reverse",
        "--height=100%",
        "--border",
        "--info=inline",
        "--tabstop=1",
        "--exit-0",
    ];
    defaults.extend_from_slice(fzf_preview_options());
    defaults
}

#[cfg(target_os = "linux")]
fn fzf_preview_options() -> &'static [&'static str] {
    &[
        "--preview-window=down,30%,sharp",
        "--preview='\\command -p ls -Cp --color=always --group-directories-first {2..}'",
    ]
}

#[cfg(all(unix, not(target_os = "linux")))]
fn fzf_preview_options() -> &'static [&'static str] {
    &[
        "--preview-window=down,30%,sharp",
        "--preview='\\command -p ls -Cp {2..}'",
    ]
}

#[cfg(not(unix))]
fn fzf_preview_options() -> &'static [&'static str] {
    &[]
}

fn path_from_command_stdout(mut stdout: Vec<u8>) -> Option<PathBuf> {
    while matches!(stdout.last(), Some(b'\n' | b'\r')) {
        stdout.pop();
    }
    if stdout.is_empty() {
        return None;
    }

    #[cfg(unix)]
    {
        Some(PathBuf::from(OsString::from_vec(stdout)))
    }
    #[cfg(not(unix))]
    {
        String::from_utf8(stdout).ok().map(PathBuf::from)
    }
}

#[cfg(test)]
mod fzf_tests {
    use super::{fzf_default_options, fzf_options_with};

    #[test]
    fn fzf_options_use_invoking_user_values() {
        let options = fzf_options_with(|name| match name {
            "FZF_DEFAULT_OPTS" => Some("--height=40%".to_string()),
            "ELIO_ZOXIDE_OPTS" => Some("--no-mouse".to_string()),
            _ => None,
        });

        assert!(options.starts_with("--height=40% "));
        assert!(options.ends_with(" --no-mouse"));
        assert!(options.contains("--exact"));
    }

    #[test]
    fn fzf_options_include_base_picker_flags() {
        let options = fzf_default_options();
        assert!(options.contains(&"--exact"));
        assert!(options.contains(&"--exit-0"));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_fzf_options_include_colored_preview() {
        let options = fzf_default_options();
        assert!(options.contains(&"--preview-window=down,30%,sharp"));
        assert!(
            options
                .iter()
                .any(|option| option.contains("--color=always --group-directories-first"))
        );
    }

    #[cfg(all(unix, not(target_os = "linux")))]
    #[test]
    fn non_linux_unix_fzf_options_include_plain_preview() {
        let options = fzf_default_options();
        assert!(options.contains(&"--preview-window=down,30%,sharp"));
        assert!(options.contains(&"--preview='\\command -p ls -Cp {2..}'"));
    }
}
