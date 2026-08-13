#[cfg(unix)]
use std::{
    env,
    ffi::{CStr, CString, OsStr},
    mem,
    os::unix::ffi::{OsStrExt, OsStringExt},
    path::PathBuf,
    ptr,
};

#[cfg(unix)]
pub(super) fn home_dir() -> Option<PathBuf> {
    elevated_home_dir(
        unsafe { libc::geteuid() },
        env::var_os("SUDO_UID").as_deref(),
        env::var_os("DOAS_USER").as_deref(),
    )
    .or_else(dirs::home_dir)
}

#[cfg(not(unix))]
pub(super) fn home_dir() -> Option<std::path::PathBuf> {
    dirs::home_dir()
}

#[cfg(unix)]
fn elevated_home_dir(
    effective_uid: libc::uid_t,
    sudo_uid: Option<&OsStr>,
    doas_user: Option<&OsStr>,
) -> Option<PathBuf> {
    if effective_uid != 0 {
        return None;
    }
    parse_non_root_uid(sudo_uid)
        .and_then(passwd_home_by_uid)
        .or_else(|| doas_user.and_then(passwd_home_by_name))
}

#[cfg(unix)]
fn parse_non_root_uid(value: Option<&OsStr>) -> Option<libc::uid_t> {
    let uid = value?.to_str()?.parse::<libc::uid_t>().ok()?;
    (uid != 0).then_some(uid)
}

#[cfg(unix)]
fn passwd_home_by_uid(uid: libc::uid_t) -> Option<PathBuf> {
    passwd_home(|record, buffer, len, result| unsafe {
        libc::getpwuid_r(uid, record, buffer, len, result)
    })
}

#[cfg(unix)]
fn passwd_home_by_name(name: &OsStr) -> Option<PathBuf> {
    let bytes = name.as_bytes();
    if bytes.is_empty() || bytes == b"root" {
        return None;
    }
    let name = CString::new(bytes).ok()?;
    passwd_home(|record, buffer, len, result| unsafe {
        libc::getpwnam_r(name.as_ptr(), record, buffer, len, result)
    })
}

#[cfg(unix)]
fn passwd_home(
    mut lookup: impl FnMut(
        *mut libc::passwd,
        *mut libc::c_char,
        usize,
        *mut *mut libc::passwd,
    ) -> libc::c_int,
) -> Option<PathBuf> {
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
        if status != 0 || result.is_null() || record.pw_dir.is_null() {
            return None;
        }
        let home = unsafe { CStr::from_ptr(record.pw_dir) }.to_bytes();
        return (!home.is_empty())
            .then(|| PathBuf::from(std::ffi::OsString::from_vec(home.to_vec())));
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
        assert_eq!(
            elevated_home_dir(1000, Some(OsStr::new("1001")), Some(OsStr::new("paco"))),
            None
        );
    }

    #[test]
    fn elevated_process_resolves_sudo_uid() {
        let uid = unsafe { libc::getuid() };
        if uid == 0 {
            return;
        }
        let expected = passwd_home_by_uid(uid).expect("current user should have a home directory");
        assert_eq!(
            elevated_home_dir(0, Some(OsStr::new(&uid.to_string())), None),
            Some(expected)
        );
    }
}
