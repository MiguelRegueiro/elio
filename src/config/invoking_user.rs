#[cfg(unix)]
use std::{
    env,
    ffi::{CStr, CString, OsStr, OsString},
    mem,
    os::unix::ffi::{OsStrExt, OsStringExt},
    path::PathBuf,
    ptr,
};

#[cfg(unix)]
#[derive(Clone, Debug)]
pub(crate) struct InvokingUser {
    pub(crate) uid: libc::uid_t,
    pub(crate) gid: libc::gid_t,
    pub(crate) name: OsString,
    pub(crate) home: PathBuf,
    pub(crate) shell: OsString,
    pub(crate) groups: Vec<libc::gid_t>,
    pub(crate) xdg_data_home: Option<PathBuf>,
}

#[cfg(unix)]
#[derive(Clone, Debug)]
pub(crate) enum InvocationContext {
    Normal,
    Elevated(InvokingUser),
    ElevatedUnresolved,
}

#[cfg(unix)]
pub(crate) fn context() -> InvocationContext {
    invocation_context(
        unsafe { libc::geteuid() },
        env::var_os("SUDO_UID").as_deref(),
        env::var_os("DOAS_USER").as_deref(),
    )
}

#[cfg(unix)]
pub(crate) fn home_dir() -> Option<PathBuf> {
    match context() {
        InvocationContext::Elevated(user) => Some(user.home),
        InvocationContext::Normal | InvocationContext::ElevatedUnresolved => dirs::home_dir(),
    }
}

#[cfg(not(unix))]
pub(crate) fn home_dir() -> Option<std::path::PathBuf> {
    dirs::home_dir()
}

pub(crate) fn trash_home_dir() -> Option<std::path::PathBuf> {
    #[cfg(all(unix, not(target_os = "macos")))]
    return trash_home_for_context(context());
    #[cfg(any(not(unix), target_os = "macos"))]
    return dirs::home_dir();
}

#[cfg(all(unix, not(target_os = "macos")))]
fn trash_home_for_context(context: InvocationContext) -> Option<PathBuf> {
    match context {
        InvocationContext::Normal => dirs::home_dir(),
        InvocationContext::Elevated(user) => Some(user.home),
        InvocationContext::ElevatedUnresolved => None,
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
pub(crate) fn trash_data_dir() -> Option<PathBuf> {
    match context() {
        InvocationContext::Normal => dirs::data_dir(),
        InvocationContext::Elevated(user) => user
            .xdg_data_home
            .or_else(|| Some(user.home.join(".local/share"))),
        InvocationContext::ElevatedUnresolved => None,
    }
}

#[cfg(unix)]
fn invocation_context(
    effective_uid: libc::uid_t,
    sudo_uid: Option<&OsStr>,
    doas_user: Option<&OsStr>,
) -> InvocationContext {
    if effective_uid != 0 {
        return InvocationContext::Normal;
    }
    let user = parse_non_root_uid(sudo_uid)
        .and_then(passwd_by_uid)
        .or_else(|| doas_user.and_then(passwd_by_name));
    let Some(mut user) = user else {
        return InvocationContext::ElevatedUnresolved;
    };
    let Some(groups) = supplementary_groups(&user.name, user.gid) else {
        return InvocationContext::ElevatedUnresolved;
    };
    user.groups = groups;
    user.xdg_data_home = validated_xdg_data_home(&user);
    InvocationContext::Elevated(user)
}

#[cfg(unix)]
fn parse_non_root_uid(value: Option<&OsStr>) -> Option<libc::uid_t> {
    let uid = value?.to_str()?.parse::<libc::uid_t>().ok()?;
    (uid != 0).then_some(uid)
}

#[cfg(unix)]
fn passwd_by_uid(uid: libc::uid_t) -> Option<InvokingUser> {
    passwd(|record, buffer, len, result| unsafe {
        libc::getpwuid_r(uid, record, buffer, len, result)
    })
}

#[cfg(unix)]
fn passwd_by_name(name: &OsStr) -> Option<InvokingUser> {
    let bytes = name.as_bytes();
    if bytes.is_empty() || bytes == b"root" {
        return None;
    }
    let name = CString::new(bytes).ok()?;
    passwd(|record, buffer, len, result| unsafe {
        libc::getpwnam_r(name.as_ptr(), record, buffer, len, result)
    })
}

#[cfg(unix)]
fn passwd(
    mut lookup: impl FnMut(
        *mut libc::passwd,
        *mut libc::c_char,
        usize,
        *mut *mut libc::passwd,
    ) -> libc::c_int,
) -> Option<InvokingUser> {
    let mut buffer = vec![0_u8; passwd_buffer_size()];
    loop {
        let mut record = unsafe { mem::zeroed::<libc::passwd>() };
        let mut result = ptr::null_mut();
        let status = lookup(
            &mut record,
            buffer.as_mut_ptr().cast(),
            buffer.len(),
            &mut result,
        );
        if status == libc::ERANGE && buffer.len() < 1024 * 1024 {
            buffer.resize(buffer.len() * 2, 0);
            continue;
        }
        if status != 0
            || result.is_null()
            || record.pw_dir.is_null()
            || record.pw_name.is_null()
            || record.pw_uid == 0
        {
            return None;
        }
        let home = unsafe { CStr::from_ptr(record.pw_dir) }.to_bytes();
        let name = unsafe { CStr::from_ptr(record.pw_name) }.to_bytes();
        let shell = if record.pw_shell.is_null() {
            &[][..]
        } else {
            unsafe { CStr::from_ptr(record.pw_shell) }.to_bytes()
        };
        if home.is_empty() || name.is_empty() {
            return None;
        }
        return Some(InvokingUser {
            uid: record.pw_uid,
            gid: record.pw_gid,
            name: OsString::from_vec(name.to_vec()),
            home: PathBuf::from(OsString::from_vec(home.to_vec())),
            shell: if shell.is_empty() {
                OsString::from("/bin/sh")
            } else {
                OsString::from_vec(shell.to_vec())
            },
            groups: vec![record.pw_gid],
            xdg_data_home: None,
        });
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
fn validated_xdg_data_home(user: &InvokingUser) -> Option<PathBuf> {
    use std::os::unix::fs::MetadataExt;

    env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .filter(|path| {
            path.ancestors()
                .find_map(|ancestor| match std::fs::metadata(ancestor) {
                    Ok(metadata) => Some(metadata.is_dir() && metadata.uid() == user.uid),
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
                    Err(_) => Some(false),
                })
                == Some(true)
        })
}

#[cfg(target_os = "macos")]
fn validated_xdg_data_home(_user: &InvokingUser) -> Option<PathBuf> {
    None
}

#[cfg(all(unix, not(target_os = "macos")))]
fn supplementary_groups(name: &OsStr, primary_gid: libc::gid_t) -> Option<Vec<libc::gid_t>> {
    let name = CString::new(name.as_bytes()).ok()?;
    let mut count: libc::c_int = 0;
    unsafe {
        libc::getgrouplist(name.as_ptr(), primary_gid, ptr::null_mut(), &mut count);
    }
    if count <= 0 {
        return None;
    }
    let mut groups = vec![primary_gid; count as usize];
    let status =
        unsafe { libc::getgrouplist(name.as_ptr(), primary_gid, groups.as_mut_ptr(), &mut count) };
    if status < 0 || count <= 0 {
        return None;
    }
    groups.truncate(count as usize);
    groups.push(primary_gid);
    groups.sort_unstable();
    groups.dedup();
    Some(groups)
}

#[cfg(target_os = "macos")]
fn supplementary_groups(name: &OsStr, primary_gid: libc::gid_t) -> Option<Vec<libc::gid_t>> {
    const INITIAL_CAPACITY: usize = 16;
    const MAX_CAPACITY: usize = 16 * 1024;

    let name = CString::new(name.as_bytes()).ok()?;
    let primary_group = libc::c_int::try_from(primary_gid).ok()?;
    let mut capacity = INITIAL_CAPACITY;

    loop {
        let mut native_groups = vec![primary_group; capacity];
        let mut count = libc::c_int::try_from(native_groups.len()).ok()?;
        let status = unsafe {
            libc::getgrouplist(
                name.as_ptr(),
                primary_group,
                native_groups.as_mut_ptr(),
                &mut count,
            )
        };

        if status == 0 {
            let returned = usize::try_from(count).ok()?;
            if returned == 0 || returned > native_groups.len() {
                return None;
            }
            let mut groups = native_groups
                .into_iter()
                .take(returned)
                .map(libc::gid_t::try_from)
                .collect::<Result<Vec<_>, _>>()
                .ok()?;
            groups.push(primary_gid);
            groups.sort_unstable();
            groups.dedup();
            return Some(groups);
        }

        // Darwin does not support a null-buffer sizing probe, and on -1 the
        // returned count is only the number of entries that fit. Grow from
        // the previous capacity instead, with a fixed upper bound.
        if status != -1 || capacity >= MAX_CAPACITY {
            return None;
        }
        let next_capacity = capacity.checked_mul(2)?.min(MAX_CAPACITY);
        if next_capacity <= capacity {
            return None;
        }
        capacity = next_capacity;
    }
}

#[cfg(unix)]
fn passwd_buffer_size() -> usize {
    let size = unsafe { libc::sysconf(libc::_SC_GETPW_R_SIZE_MAX) };
    if size > 0 { size as usize } else { 16 * 1024 }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn sudo_uid_accepts_only_non_root_numeric_ids() {
        assert_eq!(parse_non_root_uid(Some(OsStr::new("1000"))), Some(1000));
        assert_eq!(parse_non_root_uid(Some(OsStr::new("0"))), None);
        assert_eq!(parse_non_root_uid(Some(OsStr::new("paco"))), None);
        assert_eq!(parse_non_root_uid(None), None);
    }

    #[test]
    fn non_root_process_ignores_elevation_metadata() {
        assert!(matches!(
            invocation_context(1000, Some(OsStr::new("1001")), Some(OsStr::new("paco"))),
            InvocationContext::Normal
        ));
    }

    #[test]
    fn elevated_process_resolves_sudo_uid() {
        let uid = unsafe { libc::getuid() };
        if uid == 0 {
            return;
        }
        let expected = passwd_by_uid(uid).expect("current user should have a passwd record");
        let InvocationContext::Elevated(actual) =
            invocation_context(0, Some(OsStr::new(&uid.to_string())), None)
        else {
            panic!("current user should resolve as invoking user");
        };
        assert_eq!(actual.uid, expected.uid);
        assert_eq!(actual.gid, expected.gid);
        assert_eq!(actual.home, expected.home);
        assert_eq!(actual.shell, expected.shell);
        assert!(actual.groups.contains(&actual.gid));
    }

    #[test]
    fn unresolved_elevated_context_does_not_become_normal() {
        assert!(matches!(
            invocation_context(0, Some(OsStr::new("invalid")), Some(OsStr::new("root"))),
            InvocationContext::ElevatedUnresolved
        ));
    }

    #[cfg(not(target_os = "macos"))]
    #[test]
    fn unresolved_elevated_context_has_no_trash_home() {
        assert_eq!(
            trash_home_for_context(InvocationContext::ElevatedUnresolved),
            None
        );
    }
}
