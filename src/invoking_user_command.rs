#[cfg(unix)]
use crate::config::InvokingUser;
#[cfg(unix)]
use std::{
    ffi::{CString, OsStr},
    io,
    os::unix::{ffi::OsStrExt, process::CommandExt},
    path::{Path, PathBuf},
    process::Command,
};

/// Applies the identity policy for user-facing external applications.
///
/// Normal launches are left untouched. Elevated launches run as the invoking
/// user, use `cwd` when the action has an explicit directory, and otherwise
/// start from that user's home. Unresolved elevated identity fails closed.
#[cfg(unix)]
pub(crate) fn prepare_external(command: &mut Command, cwd: Option<&Path>) -> io::Result<()> {
    prepare_external_for_context(command, crate::config::invoking_user_context(), cwd)
}

#[cfg(unix)]
fn prepare_external_for_context(
    command: &mut Command,
    context: &crate::config::InvocationContext,
    cwd: Option<&Path>,
) -> io::Result<()> {
    match context {
        crate::config::InvocationContext::Normal
        | crate::config::InvocationContext::RootSession => Ok(()),
        crate::config::InvocationContext::Elevated(user) => {
            let cwd = cwd.unwrap_or(&user.home);
            prepare(command, user, Some(cwd))
        }
        crate::config::InvocationContext::ElevatedUnresolved => Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "could not resolve invoking user",
        )),
    }
}

#[cfg(unix)]
pub(crate) fn prepare(
    command: &mut Command,
    user: &InvokingUser,
    cwd: Option<&Path>,
) -> io::Result<()> {
    let cwd = cwd
        .map(|path| CString::new(path.as_os_str().as_bytes()))
        .transpose()
        .map_err(|_| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "working directory contains NUL",
            )
        })?;

    apply_user_environment(command, user);
    if let Some(path) = cwd.as_ref() {
        command.env("PWD", OsStr::from_bytes(path.as_bytes()));
    }

    let uid = user.uid;
    let gid = user.gid;
    let groups = user.groups.clone();
    unsafe {
        command.pre_exec(move || {
            drop_privileges(uid, gid, &groups)?;
            match &cwd {
                Some(cwd) if libc::chdir(cwd.as_ptr()) != 0 => {
                    return Err(io::Error::last_os_error());
                }
                _ => {}
            }
            Ok(())
        });
    }
    Ok(())
}

#[cfg(unix)]
fn apply_user_environment(command: &mut Command, user: &InvokingUser) {
    command
        .env("HOME", &user.home)
        .env("USER", &user.name)
        .env("USERNAME", &user.name)
        .env("LOGNAME", &user.name)
        .env("SHELL", &user.shell)
        .env_remove("MAIL")
        .env_remove("OLDPWD")
        .env_remove("SUDO_COMMAND")
        .env_remove("SUDO_GID")
        .env_remove("SUDO_UID")
        .env_remove("SUDO_USER")
        .env_remove("DOAS_USER");

    for name in crate::config::SESSION_ENVIRONMENT_KEYS {
        command.env_remove(name);
    }
    for (name, value) in &user.session_environment {
        if !matches!(
            name.to_str(),
            Some(
                "XAUTHORITY"
                    | "XDG_CACHE_HOME"
                    | "XDG_CONFIG_HOME"
                    | "XDG_DATA_HOME"
                    | "XDG_RUNTIME_DIR"
            )
        ) {
            command.env(name, value);
        }
    }

    if let Some(path) = &user.xdg_config_home {
        command.env("XDG_CONFIG_HOME", path);
    }
    if let Some(path) = &user.xdg_data_home {
        command.env("XDG_DATA_HOME", path);
    }
    set_owned_absolute_env(command, user, "XDG_CACHE_HOME");
    set_owned_path_env(command, user, "XAUTHORITY");
    if let Some(path) = crate::config::user_environment_value(user, "XDG_RUNTIME_DIR")
        && valid_runtime_dir(Some(path), user.uid)
    {
        command.env("XDG_RUNTIME_DIR", path);
    }
}

#[cfg(unix)]
fn set_owned_absolute_env(command: &mut Command, user: &InvokingUser, name: &str) {
    if let Some(path) = crate::config::user_environment_value(user, name)
        .map(PathBuf::from)
        .filter(|path| valid_owned_absolute_path(path, user.uid))
    {
        command.env(name, path);
    }
}

#[cfg(unix)]
fn set_owned_path_env(command: &mut Command, user: &InvokingUser, name: &str) {
    use std::os::unix::fs::MetadataExt;

    if let Some(path) = crate::config::user_environment_value(user, name)
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .filter(|path| std::fs::metadata(path).is_ok_and(|metadata| metadata.uid() == user.uid))
    {
        command.env(name, path);
    }
}

#[cfg(unix)]
fn valid_owned_absolute_path(path: &Path, uid: libc::uid_t) -> bool {
    use std::os::unix::fs::MetadataExt;

    path.is_absolute()
        && path
            .ancestors()
            .find_map(|ancestor| match std::fs::metadata(ancestor) {
                Ok(metadata) => Some(metadata.is_dir() && metadata.uid() == uid),
                Err(error) if error.kind() == io::ErrorKind::NotFound => None,
                Err(_) => Some(false),
            })
            == Some(true)
}

#[cfg(unix)]
fn valid_runtime_dir(path: Option<&OsStr>, uid: libc::uid_t) -> bool {
    use std::os::unix::fs::MetadataExt;

    let Some(path) = path.map(Path::new).filter(|path| path.is_absolute()) else {
        return false;
    };
    std::fs::metadata(path).is_ok_and(|metadata| metadata.is_dir() && metadata.uid() == uid)
}

#[cfg(unix)]
fn drop_privileges(uid: libc::uid_t, gid: libc::gid_t, groups: &[libc::gid_t]) -> io::Result<()> {
    if unsafe { set_supplementary_groups(groups) } != 0 {
        return Err(io::Error::last_os_error());
    }
    if unsafe { libc::setgid(gid) } != 0 {
        return Err(io::Error::last_os_error());
    }
    if unsafe { libc::setuid(uid) } != 0 {
        return Err(io::Error::last_os_error());
    }
    if !credentials_match(uid, gid) {
        return Err(io::Error::from_raw_os_error(libc::EPERM));
    }
    Ok(())
}

#[cfg(target_os = "linux")]
unsafe fn set_supplementary_groups(groups: &[libc::gid_t]) -> libc::c_int {
    unsafe { libc::setgroups(groups.len(), groups.as_ptr()) }
}

#[cfg(all(unix, not(target_os = "linux")))]
unsafe fn set_supplementary_groups(groups: &[libc::gid_t]) -> libc::c_int {
    let Ok(count) = libc::c_int::try_from(groups.len()) else {
        return -1;
    };
    unsafe { libc::setgroups(count, groups.as_ptr()) }
}

#[cfg(unix)]
fn credentials_match(uid: libc::uid_t, gid: libc::gid_t) -> bool {
    if unsafe { libc::getuid() } != uid
        || unsafe { libc::geteuid() } != uid
        || unsafe { libc::getgid() } != gid
        || unsafe { libc::getegid() } != gid
    {
        return false;
    }

    #[cfg(any(target_os = "linux", target_os = "freebsd"))]
    {
        let (mut real_uid, mut effective_uid, mut saved_uid) = (0, 0, 0);
        let (mut real_gid, mut effective_gid, mut saved_gid) = (0, 0, 0);
        if unsafe { libc::getresuid(&mut real_uid, &mut effective_uid, &mut saved_uid) } != 0
            || unsafe { libc::getresgid(&mut real_gid, &mut effective_gid, &mut saved_gid) } != 0
        {
            return false;
        }
        if (real_uid, effective_uid, saved_uid) != (uid, uid, uid)
            || (real_gid, effective_gid, saved_gid) != (gid, gid, gid)
        {
            return false;
        }
    }

    true
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::ffi::OsString;

    fn test_user() -> InvokingUser {
        InvokingUser {
            uid: 1000,
            gid: 1000,
            name: OsString::from("paco"),
            home: PathBuf::from("/home/paco"),
            shell: OsString::from("/bin/fish"),
            groups: vec![1000],
            session_environment: vec![
                (
                    OsString::from("DBUS_SESSION_BUS_ADDRESS"),
                    OsString::from("unix:path=/run/user/1000/bus"),
                ),
                (OsString::from("DISPLAY"), OsString::from(":1")),
                (OsString::from("EDITOR"), OsString::from("nvim")),
                (
                    OsString::from("PATH"),
                    OsString::from("/home/paco/.local/bin:/usr/bin"),
                ),
                (
                    OsString::from("WAYLAND_DISPLAY"),
                    OsString::from("wayland-1"),
                ),
            ],
            xdg_config_home: Some(PathBuf::from("/home/paco/.config")),
            xdg_data_home: Some(PathBuf::from("/home/paco/.local/share")),
        }
    }

    fn command_env(command: &Command, name: &str) -> Option<Option<OsString>> {
        command
            .get_envs()
            .find(|(key, _)| *key == OsStr::new(name))
            .map(|(_, value)| value.map(OsStr::to_os_string))
    }

    #[test]
    fn user_environment_replaces_identity_and_removes_elevation_metadata() {
        let mut command = Command::new("true");
        apply_user_environment(&mut command, &test_user());

        assert_eq!(
            command_env(&command, "HOME"),
            Some(Some(OsString::from("/home/paco")))
        );
        assert_eq!(
            command_env(&command, "USER"),
            Some(Some(OsString::from("paco")))
        );
        assert_eq!(
            command_env(&command, "USERNAME"),
            Some(Some(OsString::from("paco")))
        );
        assert_eq!(
            command_env(&command, "LOGNAME"),
            Some(Some(OsString::from("paco")))
        );
        assert_eq!(
            command_env(&command, "SHELL"),
            Some(Some(OsString::from("/bin/fish")))
        );
        assert_eq!(command_env(&command, "SUDO_UID"), Some(None));
        assert_eq!(command_env(&command, "SUDO_USER"), Some(None));
        assert_eq!(command_env(&command, "DOAS_USER"), Some(None));
        assert_eq!(command_env(&command, "MAIL"), Some(None));
        assert_eq!(command_env(&command, "OLDPWD"), Some(None));
        assert_eq!(
            command_env(&command, "XDG_DATA_HOME"),
            Some(Some(OsString::from("/home/paco/.local/share")))
        );
        assert_eq!(
            command_env(&command, "XDG_CONFIG_HOME"),
            Some(Some(OsString::from("/home/paco/.config")))
        );
        assert_eq!(
            command_env(&command, "DISPLAY"),
            Some(Some(OsString::from(":1")))
        );
        assert_eq!(
            command_env(&command, "WAYLAND_DISPLAY"),
            Some(Some(OsString::from("wayland-1")))
        );
        assert_eq!(
            command_env(&command, "EDITOR"),
            Some(Some(OsString::from("nvim")))
        );
    }

    #[test]
    fn absent_invoking_user_session_values_remove_root_values() {
        let mut user = test_user();
        user.session_environment.clear();
        let mut command = Command::new("true");
        apply_user_environment(&mut command, &user);

        assert_eq!(command_env(&command, "DISPLAY"), Some(None));
        assert_eq!(command_env(&command, "WAYLAND_DISPLAY"), Some(None));
        assert_eq!(
            command_env(&command, "DBUS_SESSION_BUS_ADDRESS"),
            Some(None)
        );
        assert_eq!(command_env(&command, "EDITOR"), Some(None));
    }

    #[test]
    fn prepare_rejects_nul_in_working_directory_before_spawn() {
        use std::os::unix::ffi::OsStrExt;

        let mut command = Command::new("true");
        let path = Path::new(OsStr::from_bytes(b"/tmp/a\0b"));
        let error = prepare(&mut command, &test_user(), Some(path)).unwrap_err();

        assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
    }

    #[test]
    fn prepare_sets_pwd_to_requested_working_directory() {
        let mut command = Command::new("true");
        prepare(
            &mut command,
            &test_user(),
            Some(Path::new("/home/paco/Documents")),
        )
        .unwrap();

        assert_eq!(
            command_env(&command, "PWD"),
            Some(Some(OsString::from("/home/paco/Documents")))
        );
    }

    #[test]
    fn normal_external_command_is_left_unchanged() {
        let mut command = Command::new("true");
        prepare_external_for_context(
            &mut command,
            &crate::config::InvocationContext::Normal,
            Some(Path::new("/ignored")),
        )
        .unwrap();

        assert_eq!(command.get_current_dir(), None);
        assert_eq!(command_env(&command, "HOME"), None);
    }

    #[test]
    fn elevated_detached_command_uses_invoking_user_home() {
        let mut command = Command::new("true");
        prepare_external_for_context(
            &mut command,
            &crate::config::InvocationContext::Elevated(test_user()),
            None,
        )
        .unwrap();

        assert_eq!(
            command_env(&command, "PWD"),
            Some(Some(OsString::from("/home/paco")))
        );
        assert_eq!(
            command_env(&command, "HOME"),
            Some(Some(OsString::from("/home/paco")))
        );
    }

    #[test]
    fn elevated_external_command_uses_requested_working_directory() {
        let mut command = Command::new("true");
        prepare_external_for_context(
            &mut command,
            &crate::config::InvocationContext::Elevated(test_user()),
            Some(Path::new("/home/paco/Documents")),
        )
        .unwrap();

        assert_eq!(
            command_env(&command, "PWD"),
            Some(Some(OsString::from("/home/paco/Documents")))
        );
    }

    #[test]
    fn unresolved_elevated_external_command_fails_closed() {
        let mut command = Command::new("true");
        let error = prepare_external_for_context(
            &mut command,
            &crate::config::InvocationContext::ElevatedUnresolved,
            None,
        )
        .unwrap_err();

        assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
        assert_eq!(error.to_string(), "could not resolve invoking user");
    }

    #[test]
    fn runtime_dir_must_be_absolute_existing_and_user_owned() {
        assert!(!valid_runtime_dir(Some(OsStr::new("relative")), 1000));
        assert!(!valid_runtime_dir(Some(OsStr::new("/missing")), 1000));
        let temp = std::env::temp_dir().join(format!(
            "elio-runtime-dir-test-{}-{}",
            std::process::id(),
            std::thread::current().name().unwrap_or("unnamed")
        ));
        std::fs::create_dir_all(&temp).unwrap();
        let uid = unsafe { libc::geteuid() };
        assert!(valid_runtime_dir(Some(temp.as_os_str()), uid));
        std::fs::remove_dir(&temp).unwrap();
    }

    #[test]
    fn owned_absolute_path_accepts_missing_leaf_under_user_directory() {
        let temp = std::env::temp_dir().join(format!(
            "elio-user-env-test-{}-{}",
            std::process::id(),
            std::thread::current().name().unwrap_or("unnamed")
        ));
        std::fs::create_dir_all(&temp).unwrap();
        let uid = unsafe { libc::geteuid() };
        assert!(valid_owned_absolute_path(&temp.join("missing"), uid));
        assert!(!valid_owned_absolute_path(Path::new("relative"), uid));
        std::fs::remove_dir(&temp).unwrap();
    }
}
