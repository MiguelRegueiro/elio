#[cfg(unix)]
use crate::{
    config::InvokingUser,
    user_fs_helper::{Request, Response, read_response, validate_request, write_request},
};
#[cfg(unix)]
use std::{
    fs,
    io::Write,
    os::unix::process::CommandExt,
    path::Path,
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
        )
        .current_dir(&user.home)
        .env_remove("SUDO_UID")
        .env_remove("SUDO_GID")
        .env_remove("SUDO_USER")
        .env_remove("DOAS_USER")
        .env_remove("XDG_DATA_HOME")
        .env_remove("XDG_CONFIG_HOME")
        .env_remove("XDG_CACHE_HOME");
    if let Some(path) = &user.xdg_data_home {
        command.env("XDG_DATA_HOME", path);
    }
    if !valid_runtime_dir(std::env::var_os("XDG_RUNTIME_DIR").as_deref(), user.uid) {
        command.env_remove("XDG_RUNTIME_DIR");
    }

    let uid = user.uid;
    let gid = user.gid;
    let groups = user.groups.clone();
    unsafe {
        command.pre_exec(move || {
            if set_supplementary_groups(&groups) != 0 {
                return Err(std::io::Error::last_os_error());
            }
            if libc::setgid(gid) != 0 {
                return Err(std::io::Error::last_os_error());
            }
            if libc::setuid(uid) != 0 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }

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

#[cfg(target_os = "linux")]
unsafe fn set_supplementary_groups(groups: &[libc::gid_t]) -> libc::c_int {
    unsafe { libc::setgroups(groups.len(), groups.as_ptr()) }
}

#[cfg(not(target_os = "linux"))]
unsafe fn set_supplementary_groups(groups: &[libc::gid_t]) -> libc::c_int {
    let Ok(count) = libc::c_int::try_from(groups.len()) else {
        return -1;
    };
    unsafe { libc::setgroups(count, groups.as_ptr()) }
}

#[cfg(unix)]
fn valid_runtime_dir(path: Option<&std::ffi::OsStr>, uid: libc::uid_t) -> bool {
    use std::os::unix::fs::MetadataExt;
    let Some(path) = path.map(Path::new).filter(|path| path.is_absolute()) else {
        return false;
    };
    fs::metadata(path).is_ok_and(|metadata| metadata.is_dir() && metadata.uid() == uid)
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn runtime_dir_must_be_absolute_and_user_owned() {
        assert!(!valid_runtime_dir(
            Some(std::ffi::OsStr::new("relative")),
            1000
        ));
        assert!(!valid_runtime_dir(
            Some(std::ffi::OsStr::new("/missing")),
            1000
        ));
        let temp =
            std::env::temp_dir().join(format!("elio-runtime-dir-test-{}", std::process::id()));
        fs::create_dir_all(&temp).unwrap();
        let uid = unsafe { libc::geteuid() };
        assert!(valid_runtime_dir(Some(temp.as_os_str()), uid));
        fs::remove_dir(&temp).unwrap();
    }
}
