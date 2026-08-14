#[cfg(unix)]
use std::{
    env,
    ffi::{CStr, CString, OsStr, OsString},
    mem,
    os::unix::ffi::{OsStrExt, OsStringExt},
    path::PathBuf,
    ptr,
    sync::OnceLock,
};

#[cfg(target_os = "linux")]
use std::io::Read;

#[cfg(unix)]
pub(crate) const SESSION_ENVIRONMENT_KEYS: &[&str] = &[
    "DBUS_SESSION_BUS_ADDRESS",
    "DESKTOP_SESSION",
    "DISPLAY",
    "EDITOR",
    "ELIO_ZOXIDE_OPTS",
    "FZF_DEFAULT_OPTS",
    "PATH",
    "VISUAL",
    "WAYLAND_DISPLAY",
    "XAUTHORITY",
    "XDG_CACHE_HOME",
    "XDG_CONFIG_DIRS",
    "XDG_CONFIG_HOME",
    "XDG_CURRENT_DESKTOP",
    "XDG_DATA_DIRS",
    "XDG_DATA_HOME",
    "XDG_RUNTIME_DIR",
    "XDG_SESSION_DESKTOP",
    "XDG_SESSION_TYPE",
    "_ZO_DATA_DIR",
    "_ZO_ECHO",
    "_ZO_EXCLUDE_DIRS",
    "_ZO_MAXAGE",
    "_ZO_RESOLVE_SYMLINKS",
];

#[cfg(unix)]
#[derive(Debug)]
pub(crate) struct InvokingUser {
    pub(crate) uid: libc::uid_t,
    pub(crate) gid: libc::gid_t,
    pub(crate) name: OsString,
    pub(crate) home: PathBuf,
    pub(crate) shell: OsString,
    pub(crate) groups: Vec<libc::gid_t>,
    pub(crate) session_environment: Vec<(OsString, OsString)>,
    pub(crate) xdg_config_home: Option<PathBuf>,
    pub(crate) xdg_data_home: Option<PathBuf>,
}

#[cfg(unix)]
#[derive(Debug)]
pub(crate) enum InvocationContext {
    Normal,
    RootSession,
    Elevated(InvokingUser),
    ElevatedUnresolved,
}

#[cfg(unix)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum IdentityClaim {
    Absent,
    Root,
    User(libc::uid_t),
    Invalid,
}

#[cfg(unix)]
static INVOCATION_CONTEXT: OnceLock<InvocationContext> = OnceLock::new();

#[cfg(unix)]
pub(crate) fn context() -> &'static InvocationContext {
    INVOCATION_CONTEXT.get_or_init(detect_context)
}

#[cfg(unix)]
fn detect_context() -> InvocationContext {
    let sudo_uid = env::var_os("SUDO_UID");
    let sudo_user = env::var_os("SUDO_USER");
    let doas_user = env::var_os("DOAS_USER");
    let has_other_sudo_metadata = ["SUDO_COMMAND", "SUDO_GID"]
        .into_iter()
        .any(|name| env::var_os(name).is_some());
    invocation_context(
        unsafe { libc::geteuid() },
        sudo_uid.as_deref(),
        sudo_user.as_deref(),
        has_other_sudo_metadata,
        doas_user.as_deref(),
    )
}

#[cfg(unix)]
pub(crate) fn env_var(name: &str) -> Option<OsString> {
    env_var_for_context(context(), name, env::var_os(name))
}

#[cfg(unix)]
fn env_var_for_context(
    context: &InvocationContext,
    name: &str,
    normal_value: Option<OsString>,
) -> Option<OsString> {
    match context {
        InvocationContext::Normal | InvocationContext::RootSession => normal_value,
        InvocationContext::Elevated(user) => {
            user_environment_value(user, name).map(OsStr::to_os_string)
        }
        InvocationContext::ElevatedUnresolved => None,
    }
}

#[cfg(not(unix))]
pub(crate) fn env_var(name: &str) -> Option<std::ffi::OsString> {
    std::env::var_os(name)
}

#[cfg(unix)]
pub(crate) fn home_dir() -> Option<PathBuf> {
    match context() {
        InvocationContext::Elevated(user) => Some(user.home.clone()),
        InvocationContext::Normal
        | InvocationContext::RootSession
        | InvocationContext::ElevatedUnresolved => dirs::home_dir(),
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
fn trash_home_for_context(context: &InvocationContext) -> Option<PathBuf> {
    match context {
        InvocationContext::Normal | InvocationContext::RootSession => dirs::home_dir(),
        InvocationContext::Elevated(user) => Some(user.home.clone()),
        InvocationContext::ElevatedUnresolved => None,
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
pub(crate) fn trash_data_dir() -> Option<PathBuf> {
    match context() {
        InvocationContext::Normal | InvocationContext::RootSession => dirs::data_dir(),
        InvocationContext::Elevated(user) => user
            .xdg_data_home
            .clone()
            .or_else(|| Some(user.home.join(".local/share"))),
        InvocationContext::ElevatedUnresolved => None,
    }
}

#[cfg(unix)]
fn invocation_context(
    effective_uid: libc::uid_t,
    sudo_uid: Option<&OsStr>,
    sudo_user: Option<&OsStr>,
    has_other_sudo_metadata: bool,
    doas_user: Option<&OsStr>,
) -> InvocationContext {
    if effective_uid != 0 {
        return InvocationContext::Normal;
    }

    let uid = match agreed_invoking_uid(
        sudo_identity_claim(sudo_uid, sudo_user, has_other_sudo_metadata),
        doas_identity_claim(doas_user),
    ) {
        Ok(None) => return InvocationContext::RootSession,
        Ok(Some(uid)) => uid,
        Err(()) => return InvocationContext::ElevatedUnresolved,
    };
    let user = passwd_by_uid(uid);
    let Some(mut user) = user else {
        return InvocationContext::ElevatedUnresolved;
    };
    let Some(groups) = supplementary_groups(&user.name, user.gid) else {
        return InvocationContext::ElevatedUnresolved;
    };
    user.groups = groups;
    user.session_environment = invoking_user_environment(user.uid);
    user.xdg_config_home =
        validated_xdg_home(user_environment_value(&user, "XDG_CONFIG_HOME"), &user);
    user.xdg_data_home = validated_xdg_home(user_environment_value(&user, "XDG_DATA_HOME"), &user);
    InvocationContext::Elevated(user)
}

#[cfg(unix)]
fn sudo_identity_claim(
    sudo_uid: Option<&OsStr>,
    sudo_user: Option<&OsStr>,
    has_other_sudo_metadata: bool,
) -> IdentityClaim {
    if sudo_uid.is_none() && sudo_user.is_none() && !has_other_sudo_metadata {
        return IdentityClaim::Absent;
    }
    let Some(uid) = parse_uid(sudo_uid) else {
        return IdentityClaim::Invalid;
    };
    if sudo_user.and_then(uid_by_name) != Some(uid) {
        return IdentityClaim::Invalid;
    }
    identity_claim(uid)
}

#[cfg(unix)]
fn doas_identity_claim(doas_user: Option<&OsStr>) -> IdentityClaim {
    match doas_user {
        None => IdentityClaim::Absent,
        Some(user) => uid_by_name(user)
            .map(identity_claim)
            .unwrap_or(IdentityClaim::Invalid),
    }
}

#[cfg(unix)]
fn identity_claim(uid: libc::uid_t) -> IdentityClaim {
    if uid == 0 {
        IdentityClaim::Root
    } else {
        IdentityClaim::User(uid)
    }
}

#[cfg(unix)]
fn agreed_invoking_uid(
    sudo: IdentityClaim,
    doas: IdentityClaim,
) -> Result<Option<libc::uid_t>, ()> {
    use IdentityClaim::{Absent, Root, User};

    match (sudo, doas) {
        (Absent, Absent | Root) | (Root, Absent | Root) => Ok(None),
        (User(uid), Absent) | (Absent, User(uid)) => Ok(Some(uid)),
        (User(sudo_uid), User(doas_uid)) if sudo_uid == doas_uid => Ok(Some(sudo_uid)),
        _ => Err(()),
    }
}

#[cfg(unix)]
fn parse_uid(value: Option<&OsStr>) -> Option<libc::uid_t> {
    value?.to_str()?.parse().ok()
}

#[cfg(unix)]
fn passwd_by_uid(uid: libc::uid_t) -> Option<InvokingUser> {
    passwd(|record, buffer, len, result| unsafe {
        libc::getpwuid_r(uid, record, buffer, len, result)
    })
}

#[cfg(unix)]
fn uid_by_name(name: &OsStr) -> Option<libc::uid_t> {
    let bytes = name.as_bytes();
    if bytes.is_empty() {
        return None;
    }
    let name = CString::new(bytes).ok()?;
    passwd(|record, buffer, len, result| unsafe {
        libc::getpwnam_r(name.as_ptr(), record, buffer, len, result)
    })
    .map(|user| user.uid)
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
        if status != 0 || result.is_null() || record.pw_dir.is_null() || record.pw_name.is_null() {
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
            session_environment: Vec::new(),
            xdg_config_home: None,
            xdg_data_home: None,
        });
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
fn validated_xdg_home(value: Option<&OsStr>, user: &InvokingUser) -> Option<PathBuf> {
    use std::os::unix::fs::MetadataExt;

    value
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
fn validated_xdg_home(_value: Option<&OsStr>, _user: &InvokingUser) -> Option<PathBuf> {
    None
}

#[cfg(unix)]
pub(crate) fn user_environment_value<'a>(user: &'a InvokingUser, name: &str) -> Option<&'a OsStr> {
    user.session_environment
        .iter()
        .find_map(|(key, value)| (key == name).then_some(value.as_os_str()))
}

#[cfg(unix)]
fn invoking_user_environment(uid: libc::uid_t) -> Vec<(OsString, OsString)> {
    #[cfg(target_os = "linux")]
    {
        // Do not fall back to root Elio's environment: if the trusted
        // same-user ancestor cannot be verified, omit session values.
        linux_ancestor_environment(uid).unwrap_or_default()
    }

    #[cfg(not(target_os = "linux"))]
    {
        let _ = uid;
        SESSION_ENVIRONMENT_KEYS
            .iter()
            .filter_map(|name| env::var_os(name).map(|value| (OsString::from(name), value)))
            .collect()
    }
}

#[cfg(target_os = "linux")]
fn linux_ancestor_environment(uid: libc::uid_t) -> Option<Vec<(OsString, OsString)>> {
    // sudo/doas may strip desktop-session variables before Elio starts. Walk
    // only through privileged ancestors to the first process wholly owned by
    // the already-resolved invoking UID, then recover only the fixed allowlist.
    let mut pid = unsafe { libc::getppid() };
    let mut elevation_environment = None;
    for _ in 0..32 {
        if pid <= 1 {
            return None;
        }
        let (parent, process_uids) = linux_process_identity(pid)?;
        if process_uids.iter().all(|process_uid| *process_uid == uid) {
            let mut environment = linux_process_environment(pid)?;
            let (_, verified_uids) = linux_process_identity(pid)?;
            if !verified_uids.iter().all(|process_uid| *process_uid == uid) {
                return None;
            }
            if let Some(elevation_environment) = elevation_environment {
                environment = merge_linux_environments(environment, elevation_environment);
            }
            return Some(environment);
        }
        let privileged_bridge = process_uids
            .iter()
            .all(|process_uid| *process_uid == 0 || *process_uid == uid)
            && process_uids.contains(&0);
        if !privileged_bridge || parent == pid {
            return None;
        }
        if elevation_environment.is_none() {
            elevation_environment = linux_elevation_environment(pid, process_uids);
        }
        pid = parent;
    }
    None
}

#[cfg(target_os = "linux")]
fn linux_elevation_environment(
    pid: libc::pid_t,
    expected_uids: [libc::uid_t; 4],
) -> Option<Vec<(OsString, OsString)>> {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};

    let executable = std::fs::read_link(format!("/proc/{pid}/exe")).ok()?;
    let name = executable.file_name()?.as_bytes();
    if name != b"sudo" && name != b"doas" {
        return None;
    }
    let metadata = std::fs::metadata(&executable).ok()?;
    if metadata.uid() != 0 || metadata.permissions().mode() & 0o4000 == 0 {
        return None;
    }
    let environment = linux_process_environment(pid)?;
    let (_, verified_uids) = linux_process_identity(pid)?;
    (verified_uids == expected_uids).then_some(environment)
}

#[cfg(target_os = "linux")]
fn merge_linux_environments(
    base: Vec<(OsString, OsString)>,
    elevation: Vec<(OsString, OsString)>,
) -> Vec<(OsString, OsString)> {
    let mut environment: std::collections::BTreeMap<_, _> = base.into_iter().collect();
    environment.extend(elevation);
    environment.into_iter().collect()
}

#[cfg(target_os = "linux")]
fn linux_process_identity(pid: libc::pid_t) -> Option<(libc::pid_t, [libc::uid_t; 4])> {
    let status = std::fs::read(format!("/proc/{pid}/status")).ok()?;
    parse_linux_process_identity(&status)
}

#[cfg(target_os = "linux")]
fn parse_linux_process_identity(status: &[u8]) -> Option<(libc::pid_t, [libc::uid_t; 4])> {
    let text = std::str::from_utf8(status).ok()?;
    let parent = text.lines().find_map(|line| {
        line.strip_prefix("PPid:")
            .and_then(|value| value.trim().parse().ok())
    })?;
    let uids = text.lines().find_map(|line| {
        let mut values = line.strip_prefix("Uid:")?.split_whitespace();
        Some([
            values.next()?.parse().ok()?,
            values.next()?.parse().ok()?,
            values.next()?.parse().ok()?,
            values.next()?.parse().ok()?,
        ])
    })?;
    Some((parent, uids))
}

#[cfg(target_os = "linux")]
fn linux_process_environment(pid: libc::pid_t) -> Option<Vec<(OsString, OsString)>> {
    let file = std::fs::File::open(format!("/proc/{pid}/environ")).ok()?;
    let mut bytes = Vec::new();
    file.take(1024 * 1024 + 1).read_to_end(&mut bytes).ok()?;
    if bytes.len() > 1024 * 1024 {
        return None;
    }
    Some(parse_allowlisted_environment(&bytes))
}

#[cfg(target_os = "linux")]
fn parse_allowlisted_environment(bytes: &[u8]) -> Vec<(OsString, OsString)> {
    let mut environment = std::collections::BTreeMap::new();
    for item in bytes.split(|byte| *byte == 0) {
        let Some(separator) = item.iter().position(|byte| *byte == b'=') else {
            continue;
        };
        let (name, value) = item.split_at(separator);
        if SESSION_ENVIRONMENT_KEYS
            .iter()
            .any(|candidate| candidate.as_bytes() == name)
        {
            environment.insert(
                OsString::from_vec(name.to_vec()),
                OsString::from_vec(value[1..].to_vec()),
            );
        }
    }
    environment.into_iter().collect()
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

    fn test_user() -> InvokingUser {
        InvokingUser {
            uid: 1000,
            gid: 1000,
            name: OsString::from("paco"),
            home: PathBuf::from("/home/paco"),
            shell: OsString::from("/bin/sh"),
            groups: vec![1000],
            session_environment: vec![(OsString::from("EDITOR"), OsString::from("nvim"))],
            xdg_config_home: None,
            xdg_data_home: None,
        }
    }

    fn assert_elevated_user(context: InvocationContext, expected: &InvokingUser) {
        let InvocationContext::Elevated(actual) = context else {
            panic!("user should resolve as invoking user");
        };
        assert_eq!(actual.uid, expected.uid);
        assert_eq!(actual.gid, expected.gid);
        assert_eq!(actual.home, expected.home);
        assert_eq!(actual.shell, expected.shell);
        assert!(actual.groups.contains(&actual.gid));
    }

    #[test]
    fn uid_parser_accepts_root_and_non_root_numeric_ids() {
        assert_eq!(parse_uid(Some(OsStr::new("1000"))), Some(1000));
        assert_eq!(parse_uid(Some(OsStr::new("0"))), Some(0));
        assert_eq!(parse_uid(Some(OsStr::new("paco"))), None);
        assert_eq!(parse_uid(None), None);
    }

    #[test]
    fn non_root_process_ignores_elevation_metadata() {
        assert!(matches!(
            invocation_context(
                1000,
                Some(OsStr::new("invalid")),
                Some(OsStr::new("unknown")),
                true,
                Some(OsStr::new("unknown")),
            ),
            InvocationContext::Normal
        ));
    }

    #[test]
    fn direct_and_coherent_root_claims_are_root_sessions() {
        let root = passwd_by_uid(0).expect("root should have a passwd record");
        for context in [
            invocation_context(0, None, None, false, None),
            invocation_context(0, Some(OsStr::new("0")), Some(&root.name), false, None),
            invocation_context(0, None, None, false, Some(&root.name)),
            invocation_context(
                0,
                Some(OsStr::new("0")),
                Some(&root.name),
                false,
                Some(&root.name),
            ),
        ] {
            assert!(matches!(context, InvocationContext::RootSession));
        }
    }

    #[test]
    fn coherent_non_root_claims_resolve_the_invoking_user() {
        let uid = unsafe { libc::getuid() };
        if uid == 0 {
            return;
        }
        let expected = passwd_by_uid(uid).expect("current user should have a passwd record");
        let uid = uid.to_string();
        for context in [
            invocation_context(0, Some(OsStr::new(&uid)), Some(&expected.name), false, None),
            invocation_context(0, None, None, false, Some(&expected.name)),
            invocation_context(
                0,
                Some(OsStr::new(&uid)),
                Some(&expected.name),
                false,
                Some(&expected.name),
            ),
        ] {
            assert_elevated_user(context, &expected);
        }
    }

    #[test]
    fn sudo_claim_requires_matching_uid_and_user() {
        let root = passwd_by_uid(0).expect("root should have a passwd record");
        for (uid, user, other_metadata, expected) in [
            (
                Some(OsStr::new("0")),
                Some(root.name.as_os_str()),
                false,
                IdentityClaim::Root,
            ),
            (
                Some(OsStr::new("1")),
                Some(root.name.as_os_str()),
                false,
                IdentityClaim::Invalid,
            ),
            (
                Some(OsStr::new("invalid")),
                Some(root.name.as_os_str()),
                false,
                IdentityClaim::Invalid,
            ),
            (Some(OsStr::new("0")), None, false, IdentityClaim::Invalid),
            (
                None,
                Some(root.name.as_os_str()),
                false,
                IdentityClaim::Invalid,
            ),
            (None, None, true, IdentityClaim::Invalid),
            (None, None, false, IdentityClaim::Absent),
        ] {
            assert_eq!(sudo_identity_claim(uid, user, other_metadata), expected);
        }
    }

    #[test]
    fn doas_claim_resolves_users_by_uid() {
        let root = passwd_by_uid(0).expect("root should have a passwd record");
        assert_eq!(doas_identity_claim(Some(&root.name)), IdentityClaim::Root);
        assert_eq!(doas_identity_claim(None), IdentityClaim::Absent);
        assert_eq!(
            doas_identity_claim(Some(OsStr::new("elio-user-that-does-not-exist"))),
            IdentityClaim::Invalid
        );
    }

    #[test]
    fn identity_claims_must_be_absent_or_coherent() {
        use IdentityClaim::{Absent, Invalid, Root, User};

        for (sudo, doas, expected) in [
            (Absent, Absent, Ok(None)),
            (Root, Absent, Ok(None)),
            (Absent, Root, Ok(None)),
            (Root, Root, Ok(None)),
            (User(1000), Absent, Ok(Some(1000))),
            (Absent, User(1000), Ok(Some(1000))),
            (User(1000), User(1000), Ok(Some(1000))),
            (Root, User(1000), Err(())),
            (User(1000), Root, Err(())),
            (User(1000), User(1001), Err(())),
            (Invalid, User(1000), Err(())),
            (User(1000), Invalid, Err(())),
            (Invalid, Absent, Err(())),
            (Absent, Invalid, Err(())),
        ] {
            assert_eq!(agreed_invoking_uid(sudo, doas), expected);
        }
    }

    #[test]
    fn elevated_environment_uses_invoking_user_value_not_root_value() {
        assert_eq!(
            env_var_for_context(
                &InvocationContext::Elevated(test_user()),
                "EDITOR",
                Some(OsString::from("root-editor")),
            ),
            Some(OsString::from("nvim"))
        );
        assert_eq!(
            env_var_for_context(
                &InvocationContext::Elevated(test_user()),
                "DISPLAY",
                Some(OsString::from(":root")),
            ),
            None
        );
    }

    #[test]
    fn normal_environment_remains_unchanged() {
        assert_eq!(
            env_var_for_context(
                &InvocationContext::Normal,
                "EDITOR",
                Some(OsString::from("nvim")),
            ),
            Some(OsString::from("nvim"))
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_environment_parser_keeps_only_allowlisted_values() {
        let environment = parse_allowlisted_environment(
            b"HOME=/root\0DISPLAY=:0\0EDITOR=nvim --clean\0ELIO_ZOXIDE_OPTS=--no-mouse\0FZF_DEFAULT_OPTS=--height=40%\0SUDO_UID=1000\0",
        );

        assert_eq!(
            environment,
            vec![
                (OsString::from("DISPLAY"), OsString::from(":0")),
                (OsString::from("EDITOR"), OsString::from("nvim --clean")),
                (
                    OsString::from("ELIO_ZOXIDE_OPTS"),
                    OsString::from("--no-mouse"),
                ),
                (
                    OsString::from("FZF_DEFAULT_OPTS"),
                    OsString::from("--height=40%"),
                ),
            ]
        );
    }

    #[test]
    fn zoxide_environment_values_are_allowlisted() {
        for name in [
            "_ZO_DATA_DIR",
            "_ZO_ECHO",
            "_ZO_EXCLUDE_DIRS",
            "_ZO_MAXAGE",
            "_ZO_RESOLVE_SYMLINKS",
        ] {
            assert!(
                SESSION_ENVIRONMENT_KEYS.contains(&name),
                "missing zoxide environment value: {name}"
            );
        }
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_ancestor_environment_reads_current_user_parent() {
        let uid = unsafe { libc::getuid() };
        if uid == 0 {
            return;
        }
        let environment = linux_ancestor_environment(uid)
            .expect("non-root test process should have a same-user ancestor");
        assert!(
            environment.iter().any(|(name, _)| name == "PATH"),
            "allowlisted parent environment should contain PATH"
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn elevation_environment_overrides_stale_shell_values() {
        let merged = merge_linux_environments(
            vec![
                (OsString::from("DISPLAY"), OsString::from(":0")),
                (OsString::from("EDITOR"), OsString::from("vi")),
            ],
            vec![(OsString::from("EDITOR"), OsString::from("nvim"))],
        );

        assert_eq!(
            merged,
            vec![
                (OsString::from("DISPLAY"), OsString::from(":0")),
                (OsString::from("EDITOR"), OsString::from("nvim")),
            ]
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_status_parser_reads_parent_and_all_uids() {
        assert_eq!(
            parse_linux_process_identity(b"Name:\ttest\nPPid:\t42\nUid:\t1000\t0\t0\t0\n"),
            Some((42, [1000, 0, 0, 0]))
        );
        assert_eq!(parse_linux_process_identity(b"Name:\ttest\n"), None);
    }

    #[cfg(not(target_os = "macos"))]
    #[test]
    fn unresolved_elevated_context_has_no_trash_home() {
        assert_eq!(
            trash_home_for_context(&InvocationContext::ElevatedUnresolved),
            None
        );
    }

    #[test]
    fn process_context_is_resolved_once() {
        assert!(std::ptr::eq(context(), context()));
    }
}
