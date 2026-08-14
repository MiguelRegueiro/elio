#[cfg(unix)]
use crate::{
    config::InvokingUser,
    user_fs_helper::{Request, Response, read_response, validate_request, write_request},
};
#[cfg(unix)]
use std::{
    io::Write,
    process::{Command, Stdio},
};

#[cfg(unix)]
pub(super) fn run(user: &InvokingUser, request: &Request) -> Result<Response, String> {
    validate_request(request).map_err(|e| format!("invalid invoking-user request: {e}"))?;
    let mut command = Command::new(std::env::current_exe().map_err(|e| e.to_string())?);
    command
        .arg("--internal-user-fs-helper")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env("HOME", &user.home)
        .env("USER", &user.name)
        .env("LOGNAME", &user.name)
        .env("ELIO_HELPER_UID", user.uid.to_string())
        .env("ELIO_HELPER_GID", user.gid.to_string())
        .env(
            "ELIO_HELPER_GROUPS",
            user.groups
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(","),
        );
    crate::invoking_user_command::prepare(&mut command, user, Some(&user.home))
        .map_err(|e| format!("could not prepare invoking-user helper: {e}"))?;

    let mut child = command
        .spawn()
        .map_err(|e| format!("could not start invoking-user helper: {e}"))?;
    let write_result = child
        .stdin
        .take()
        .ok_or_else(|| "invoking-user helper has no stdin".to_string())
        .and_then(|mut stdin| {
            write_request(&mut stdin, request).map_err(|e| e.to_string())?;
            stdin.flush().map_err(|e| e.to_string())
        });
    if let Err(error) = write_result {
        let _ = child.kill();
        let _ = child.wait();
        return Err(format!("could not send invoking-user request: {error}"));
    }

    let output = child
        .wait_with_output()
        .map_err(|e| format!("could not wait for invoking-user helper: {e}"))?;
    if !output.status.success() {
        let detail = String::from_utf8_lossy(&output.stderr);
        let detail = detail.trim();
        return Err(if detail.is_empty() {
            "invoking-user helper failed".to_string()
        } else {
            format!("invoking-user helper failed: {detail}")
        });
    }
    read_response(output.stdout.as_slice())
        .map_err(|e| format!("invalid invoking-user helper response: {e}"))
}
