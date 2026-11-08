//! Who is at the other end of a connection: only processes of the same user may talk to each
//! other. Without this, another user of the computer could take the socket's place (create the
//! pipe first on Windows, or the fallback folder in `/tmp` on Unix) and read what is typed in the
//! bar, or connect to the resident process.

use std::io;

use interprocess::local_socket::prelude::*;

/// Fails with `PermissionDenied` unless the process at the other end of `stream` runs as the
/// same user as this one.
///
/// # Errors
/// When it belongs to another user, or when the system cannot tell.
pub fn check_same_user(stream: &LocalSocketStream) -> io::Result<()> {
    let credentials = stream.peer_creds()?;
    #[cfg(unix)]
    {
        let theirs = credentials.euid().ok_or_else(unknown_peer)?;
        if theirs != this_user() {
            return Err(other_user(&format!("user id {theirs}")));
        }
    }
    #[cfg(windows)]
    {
        let process = credentials.pid().ok_or_else(unknown_peer)?;
        if windows::account_of(Some(process))? != windows::account_of(None)? {
            return Err(other_user(&format!("process {process}")));
        }
    }
    #[cfg(not(any(unix, windows)))]
    let _ = credentials;
    Ok(())
}

fn unknown_peer() -> io::Error {
    io::Error::new(
        io::ErrorKind::PermissionDenied,
        "the system does not say which process is at the other end",
    )
}

fn other_user(who: &str) -> io::Error {
    io::Error::new(
        io::ErrorKind::PermissionDenied,
        format!("the other end ({who}) belongs to another user: refused"),
    )
}

/// The effective user id of this process.
#[cfg(unix)]
#[must_use]
pub fn this_user() -> libc::uid_t {
    // SAFETY: geteuid has no preconditions and cannot fail.
    #[allow(unsafe_code)]
    unsafe {
        libc::geteuid()
    }
}

#[cfg(windows)]
mod windows {
    use std::io;

    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
    use windows_sys::Win32::Security::{
        GetLengthSid, GetTokenInformation, TOKEN_QUERY, TOKEN_USER, TokenUser,
    };
    use windows_sys::Win32::System::Threading::{
        GetCurrentProcess, OpenProcess, OpenProcessToken, PROCESS_QUERY_LIMITED_INFORMATION,
    };

    /// The security identifier (SID) of the account a process runs as (`None`: this process),
    /// as bytes to compare.
    pub fn account_of(process: Option<u32>) -> io::Result<Vec<u8>> {
        // SAFETY: every handle is checked before use and closed once; the token information is
        // read into a buffer of the size Windows asked for, aligned for TOKEN_USER (u64 words),
        // and the SID is copied out before the buffer goes away.
        #[allow(unsafe_code)]
        unsafe {
            let handle: HANDLE = match process {
                Some(id) => OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, id),
                None => GetCurrentProcess(),
            };
            if handle.is_null() {
                return Err(io::Error::last_os_error());
            }
            let mut token: HANDLE = std::ptr::null_mut();
            let opened = OpenProcessToken(handle, TOKEN_QUERY, &raw mut token);
            if process.is_some() {
                CloseHandle(handle);
            }
            if opened == 0 {
                return Err(io::Error::last_os_error());
            }
            let mut size = 0u32;
            GetTokenInformation(token, TokenUser, std::ptr::null_mut(), 0, &raw mut size);
            let words = usize::try_from(size).unwrap_or(0).div_ceil(8).max(1);
            let mut buffer = vec![0u64; words];
            let read = GetTokenInformation(token, TokenUser, buffer.as_mut_ptr().cast(), size, &raw mut size);
            CloseHandle(token);
            if read == 0 {
                return Err(io::Error::last_os_error());
            }
            let user = &*buffer.as_ptr().cast::<TOKEN_USER>();
            let sid = user.User.Sid;
            let length = usize::try_from(GetLengthSid(sid)).unwrap_or(0);
            Ok(std::slice::from_raw_parts(sid.cast::<u8>(), length).to_vec())
        }
    }
}

#[cfg(all(test, unix))]
mod tests {
    #[test]
    fn this_user_owns_the_files_it_creates() {
        use std::os::unix::fs::MetadataExt;
        let file = std::env::temp_dir().join(format!("sf-owner-{}", std::process::id()));
        std::fs::write(&file, "x").unwrap();
        let owner = std::fs::metadata(&file).unwrap().uid();
        std::fs::remove_file(&file).unwrap();
        assert_eq!(super::this_user(), owner);
    }
}
