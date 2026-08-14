#[cfg(unix)]
use std::{
    ffi::OsString,
    io::{self, Read, Write},
    os::unix::ffi::OsStringExt,
    path::PathBuf,
};

#[cfg(unix)]
const MAX_ITEMS: usize = 16_384;
#[cfg(unix)]
const MAX_PATH_BYTES: usize = 1024 * 1024;
#[cfg(unix)]
const MAX_ERROR_BYTES: usize = 16 * 1024;

#[cfg(unix)]
#[derive(Debug)]
pub(crate) enum Request {
    Trash(Vec<PathBuf>),
    Restore(PathBuf),
}

#[cfg(unix)]
pub(crate) struct Response {
    pub(crate) completed: usize,
    pub(crate) error: Option<String>,
}

#[cfg(unix)]
pub(crate) fn validate_request(request: &Request) -> io::Result<()> {
    match request {
        Request::Trash(paths) => {
            if paths.len() > MAX_ITEMS {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "too many paths",
                ));
            }
            for path in paths {
                validate_path(path)?;
            }
        }
        Request::Restore(path) => validate_path(path)?,
    }
    Ok(())
}

#[cfg(unix)]
pub(crate) fn write_request(mut writer: impl Write, request: &Request) -> io::Result<()> {
    validate_request(request)?;
    writer.write_all(b"EU\x01")?;
    match request {
        Request::Trash(paths) => {
            writer.write_all(&[1])?;
            write_u32(&mut writer, paths.len())?;
            for path in paths {
                write_path(&mut writer, path)?;
            }
        }
        Request::Restore(path) => {
            writer.write_all(&[2])?;
            write_path(&mut writer, path)?;
        }
    }
    Ok(())
}

#[cfg(unix)]
pub(crate) fn read_request(mut reader: impl Read) -> io::Result<Request> {
    let mut header = [0; 3];
    reader.read_exact(&mut header)?;
    if header != *b"EU\x01" {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "unsupported helper protocol",
        ));
    }
    let mut opcode = [0];
    reader.read_exact(&mut opcode)?;
    match opcode[0] {
        1 => {
            let count = read_u32(&mut reader)?;
            if count > MAX_ITEMS {
                return Err(io::Error::new(io::ErrorKind::InvalidData, "too many paths"));
            }
            let mut paths = Vec::with_capacity(count);
            for _ in 0..count {
                paths.push(read_path(&mut reader)?);
            }
            Ok(Request::Trash(paths))
        }
        2 => Ok(Request::Restore(read_path(&mut reader)?)),
        _ => Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "unknown request",
        )),
    }
}

#[cfg(unix)]
pub(crate) fn write_response(mut writer: impl Write, response: &Response) -> io::Result<()> {
    writer.write_all(&(response.completed as u64).to_le_bytes())?;
    match &response.error {
        Some(error) => {
            writer.write_all(&[1])?;
            let bytes = error.as_bytes();
            write_bytes(&mut writer, &bytes[..bytes.len().min(MAX_ERROR_BYTES)])?;
        }
        None => writer.write_all(&[0])?,
    }
    Ok(())
}

#[cfg(unix)]
pub(crate) fn read_response(mut reader: impl Read) -> io::Result<Response> {
    let mut completed = [0; 8];
    reader.read_exact(&mut completed)?;
    let mut failed = [0];
    reader.read_exact(&mut failed)?;
    let error = match failed[0] {
        0 => None,
        1 => Some(String::from_utf8_lossy(&read_bytes(&mut reader)?).into_owned()),
        _ => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "invalid response",
            ));
        }
    };
    Ok(Response {
        completed: u64::from_le_bytes(completed) as usize,
        error,
    })
}

#[cfg(unix)]
fn write_path(writer: impl Write, path: &std::path::Path) -> io::Result<()> {
    use std::os::unix::ffi::OsStrExt;
    write_bytes(writer, path.as_os_str().as_bytes())
}

#[cfg(unix)]
fn validate_path(path: &std::path::Path) -> io::Result<()> {
    use std::os::unix::ffi::OsStrExt;
    if path.as_os_str().as_bytes().len() > MAX_PATH_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "path too large",
        ));
    }
    Ok(())
}

#[cfg(unix)]
fn read_path(reader: impl Read) -> io::Result<PathBuf> {
    Ok(PathBuf::from(OsString::from_vec(read_bytes(reader)?)))
}

#[cfg(unix)]
fn write_u32(mut writer: impl Write, value: usize) -> io::Result<()> {
    let value = u32::try_from(value)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "value too large"))?;
    writer.write_all(&value.to_le_bytes())
}

#[cfg(unix)]
fn read_u32(mut reader: impl Read) -> io::Result<usize> {
    let mut bytes = [0; 4];
    reader.read_exact(&mut bytes)?;
    Ok(u32::from_le_bytes(bytes) as usize)
}

#[cfg(unix)]
fn write_bytes(mut writer: impl Write, bytes: &[u8]) -> io::Result<()> {
    if bytes.len() > MAX_PATH_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "frame too large",
        ));
    }
    write_u32(&mut writer, bytes.len())?;
    writer.write_all(bytes)
}

#[cfg(unix)]
fn read_bytes(mut reader: impl Read) -> io::Result<Vec<u8>> {
    let len = read_u32(&mut reader)?;
    if len > MAX_PATH_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "frame too large",
        ));
    }
    let mut bytes = vec![0; len];
    reader.read_exact(&mut bytes)?;
    Ok(bytes)
}

#[cfg(unix)]
pub fn run() -> anyhow::Result<()> {
    validate_credentials()?;
    let request = read_request(io::stdin().lock())?;
    let response = match request {
        Request::Trash(paths) => crate::app::run_user_trash_helper(&paths),
        Request::Restore(path) => match crate::fs::restore_trash_item(&path) {
            Ok(()) => Response {
                completed: 1,
                error: None,
            },
            Err(error) => Response {
                completed: 0,
                error: Some(error.to_string()),
            },
        },
    };
    write_response(io::stdout().lock(), &response)?;
    Ok(())
}

#[cfg(unix)]
fn validate_credentials() -> anyhow::Result<()> {
    let expected_uid = std::env::var("ELIO_HELPER_UID")?.parse::<libc::uid_t>()?;
    let expected_gid = std::env::var("ELIO_HELPER_GID")?.parse::<libc::gid_t>()?;
    let mut expected_groups = std::env::var("ELIO_HELPER_GROUPS")?
        .split(',')
        .map(str::parse::<libc::gid_t>)
        .collect::<Result<Vec<_>, _>>()?;
    let (uid, euid, gid, egid) = unsafe {
        (
            libc::getuid(),
            libc::geteuid(),
            libc::getgid(),
            libc::getegid(),
        )
    };
    anyhow::ensure!(
        expected_uid != 0 && uid == expected_uid && euid == expected_uid,
        "invalid helper uid"
    );
    anyhow::ensure!(
        gid == expected_gid && egid == expected_gid,
        "invalid helper gid"
    );
    let mut group_count = unsafe { libc::getgroups(0, std::ptr::null_mut()) };
    anyhow::ensure!(group_count >= 0, "cannot read helper groups");
    let mut groups = vec![0; group_count as usize];
    group_count = unsafe { libc::getgroups(group_count, groups.as_mut_ptr()) };
    anyhow::ensure!(group_count >= 0, "cannot read helper groups");
    groups.truncate(group_count as usize);
    groups.sort_unstable();
    groups.dedup();
    expected_groups.sort_unstable();
    expected_groups.dedup();
    anyhow::ensure!(groups == expected_groups, "invalid helper groups");
    #[cfg(any(target_os = "linux", target_os = "freebsd"))]
    {
        let (mut real_uid, mut effective_uid, mut saved_uid) = (0, 0, 0);
        let (mut real_gid, mut effective_gid, mut saved_gid) = (0, 0, 0);
        anyhow::ensure!(
            unsafe { libc::getresuid(&mut real_uid, &mut effective_uid, &mut saved_uid) } == 0,
            "cannot read helper uid state"
        );
        anyhow::ensure!(
            unsafe { libc::getresgid(&mut real_gid, &mut effective_gid, &mut saved_gid) } == 0,
            "cannot read helper gid state"
        );
        anyhow::ensure!(
            real_uid == expected_uid && effective_uid == expected_uid && saved_uid == expected_uid,
            "invalid saved helper uid"
        );
        anyhow::ensure!(
            real_gid == expected_gid && effective_gid == expected_gid && saved_gid == expected_gid,
            "invalid saved helper gid"
        );
    }
    Ok(())
}

#[cfg(not(unix))]
pub fn run() -> anyhow::Result<()> {
    anyhow::bail!("user filesystem helper is unsupported on this platform")
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::ffi::OsStrExt;

    #[test]
    fn protocol_round_trips_non_utf8_paths() {
        let path = PathBuf::from(OsString::from_vec(b"/tmp/a\xffb".to_vec()));
        let request = Request::Trash(vec![path.clone()]);
        let mut bytes = Vec::new();
        write_request(&mut bytes, &request).unwrap();
        let Request::Trash(paths) = read_request(bytes.as_slice()).unwrap() else {
            panic!("wrong request type");
        };
        assert_eq!(paths[0].as_os_str().as_bytes(), path.as_os_str().as_bytes());
    }

    #[test]
    fn protocol_rejects_oversized_frame() {
        let mut bytes = b"EU\x01".to_vec();
        bytes.push(2);
        bytes.extend_from_slice(&((MAX_PATH_BYTES as u32) + 1).to_le_bytes());
        assert_eq!(
            read_request(bytes.as_slice()).unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );
    }

    #[test]
    fn protocol_rejects_unknown_version() {
        let bytes = b"EU\x02\x02";
        assert_eq!(
            read_request(bytes.as_slice()).unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );
    }

    #[test]
    fn request_rejects_too_many_items_before_writing() {
        let request = Request::Trash(vec![PathBuf::from("/tmp/a"); MAX_ITEMS + 1]);
        let mut bytes = Vec::new();
        assert_eq!(
            write_request(&mut bytes, &request).unwrap_err().kind(),
            io::ErrorKind::InvalidInput
        );
        assert!(bytes.is_empty());
    }

    #[test]
    fn response_truncates_error_without_losing_completion() {
        let response = Response {
            completed: 7,
            error: Some("x".repeat(MAX_PATH_BYTES + 1)),
        };
        let mut bytes = Vec::new();
        write_response(&mut bytes, &response).unwrap();
        let decoded = read_response(bytes.as_slice()).unwrap();
        assert_eq!(decoded.completed, 7);
        assert_eq!(decoded.error.unwrap().len(), MAX_ERROR_BYTES);
    }
}
