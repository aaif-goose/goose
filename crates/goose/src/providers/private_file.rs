use std::io::{self, Write};
use std::path::{Path, PathBuf};

#[cfg(windows)]
fn to_windows_api_path(path: &Path) -> io::Result<Vec<u16>> {
    use std::os::windows::ffi::OsStrExt;

    const LEGACY_MAX_PATH: usize = 248;
    const SEP: u16 = b'\\' as u16;
    const ALT_SEP: u16 = b'/' as u16;
    const QUERY: u16 = b'?' as u16;
    const COLON: u16 = b':' as u16;
    const DOT: u16 = b'.' as u16;
    const VERBATIM_PREFIX: &[u16] = &[SEP, SEP, QUERY, SEP];
    const NT_PREFIX: &[u16] = &[SEP, QUERY, QUERY, SEP];
    const UNC_PREFIX: &[u16] = &[
        SEP,
        SEP,
        QUERY,
        SEP,
        b'U' as u16,
        b'N' as u16,
        b'C' as u16,
        SEP,
    ];

    let encode = |path: &Path| -> io::Result<Vec<u16>> {
        let mut encoded: Vec<u16> = path.as_os_str().encode_wide().collect();
        if encoded.contains(&0) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Windows paths cannot contain null characters",
            ));
        }
        encoded.push(0);
        Ok(encoded)
    };

    let encoded = encode(path)?;
    if encoded.starts_with(VERBATIM_PREFIX)
        || encoded.starts_with(NT_PREFIX)
        || encoded.as_slice() == [0]
    {
        return Ok(encoded);
    }
    if encoded.len() < LEGACY_MAX_PATH {
        match encoded.as_slice() {
            [drive, COLON, 0] | [drive, COLON, SEP | ALT_SEP, ..]
                if *drive != SEP && *drive != ALT_SEP =>
            {
                return Ok(encoded);
            }
            [SEP | ALT_SEP, SEP | ALT_SEP, ..] => return Ok(encoded),
            _ => {}
        }
    }

    let absolute = std::path::absolute(path)?;
    let encoded = encode(&absolute)?;
    let (prefix, suffix) = match encoded.as_slice() {
        [_, COLON, SEP, ..] => (VERBATIM_PREFIX, encoded.as_slice()),
        [SEP, SEP, DOT, SEP, rest @ ..] => (VERBATIM_PREFIX, rest),
        [SEP, SEP, QUERY, SEP, ..] | [SEP, QUERY, QUERY, SEP, ..] => (&[][..], encoded.as_slice()),
        [SEP, SEP, rest @ ..] => (UNC_PREFIX, rest),
        _ => (&[][..], encoded.as_slice()),
    };
    let mut normalized = Vec::with_capacity(prefix.len() + suffix.len());
    normalized.extend_from_slice(prefix);
    normalized.extend_from_slice(suffix);
    Ok(normalized)
}

#[cfg(windows)]
fn create_owner_only_file(path: &Path) -> io::Result<std::fs::File> {
    use std::os::windows::io::{FromRawHandle, RawHandle};
    use std::ptr;
    use winapi::shared::minwindef::HLOCAL;
    use winapi::shared::sddl::{
        ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
    };
    use winapi::um::fileapi::{CreateFileW, CREATE_NEW};
    use winapi::um::handleapi::INVALID_HANDLE_VALUE;
    use winapi::um::minwinbase::SECURITY_ATTRIBUTES;
    use winapi::um::winbase::LocalFree;
    use winapi::um::winnt::{
        FILE_ATTRIBUTE_TEMPORARY, FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE,
        GENERIC_READ, GENERIC_WRITE, PSECURITY_DESCRIPTOR,
    };

    let sddl: Vec<u16> = "D:P(A;;FA;;;OW)\0".encode_utf16().collect();
    let mut descriptor: PSECURITY_DESCRIPTOR = ptr::null_mut();

    if unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            sddl.as_ptr(),
            SDDL_REVISION_1 as u32,
            &mut descriptor,
            ptr::null_mut(),
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }

    let mut security_attributes = SECURITY_ATTRIBUTES {
        nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: descriptor,
        bInheritHandle: 0,
    };
    let path = to_windows_api_path(path)?;
    let handle = unsafe {
        CreateFileW(
            path.as_ptr(),
            GENERIC_READ | GENERIC_WRITE,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            &mut security_attributes,
            CREATE_NEW,
            FILE_ATTRIBUTE_TEMPORARY,
            ptr::null_mut(),
        )
    };
    let error = (handle == INVALID_HANDLE_VALUE).then(io::Error::last_os_error);

    unsafe {
        LocalFree(descriptor as HLOCAL);
    }
    if let Some(error) = error {
        Err(error)
    } else {
        Ok(unsafe { std::fs::File::from_raw_handle(handle as RawHandle) })
    }
}

#[cfg(windows)]
pub(crate) fn create_private_named_temp_file(
    builder: &mut tempfile::Builder<'_, '_>,
    parent: &Path,
) -> io::Result<tempfile::NamedTempFile> {
    builder.make_in(parent, create_owner_only_file)
}

#[cfg(not(windows))]
pub(crate) fn create_private_named_temp_file(
    builder: &mut tempfile::Builder<'_, '_>,
    parent: &Path,
) -> io::Result<tempfile::NamedTempFile> {
    builder.tempfile_in(parent)
}

fn create_private_temporary_file(parent: &Path) -> io::Result<tempfile::NamedTempFile> {
    create_private_named_temp_file(&mut tempfile::Builder::new(), parent)
}

#[cfg(windows)]
fn persist_private_temporary_file(
    temporary: tempfile::NamedTempFile,
    path: &Path,
) -> io::Result<()> {
    use winapi::um::fileapi::SetFileAttributesW;
    use winapi::um::winbase::{MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH};
    use winapi::um::winnt::{FILE_ATTRIBUTE_NORMAL, FILE_ATTRIBUTE_TEMPORARY};

    let temporary_path = to_windows_api_path(temporary.path())?;
    let destination_path = to_windows_api_path(path)?;
    if unsafe { SetFileAttributesW(temporary_path.as_ptr(), FILE_ATTRIBUTE_NORMAL) } == 0 {
        return Err(io::Error::last_os_error());
    }
    if unsafe {
        MoveFileExW(
            temporary_path.as_ptr(),
            destination_path.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    } == 0
    {
        let error = io::Error::last_os_error();
        unsafe {
            SetFileAttributesW(temporary_path.as_ptr(), FILE_ATTRIBUTE_TEMPORARY);
        }
        return Err(error);
    }

    let (_file, mut temporary_path) = temporary.into_parts();
    temporary_path.disable_cleanup(true);
    Ok(())
}

#[cfg(not(windows))]
fn persist_private_temporary_file(
    temporary: tempfile::NamedTempFile,
    path: &Path,
) -> io::Result<()> {
    temporary
        .persist(path)
        .map(|_| ())
        .map_err(|error| error.error)
}

pub(crate) fn private_file_target_path(path: &Path) -> io::Result<PathBuf> {
    const MAX_SYMLINK_HOPS: usize = 1;

    let mut resolved = PathBuf::from(path);
    let mut hops = 0;
    loop {
        match std::fs::symlink_metadata(&resolved) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                if hops >= MAX_SYMLINK_HOPS {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        format!(
                            "Too many symlink levels (or a cycle) while resolving private file path: {path:?}"
                        ),
                    ));
                }
                hops += 1;

                let target = std::fs::read_link(&resolved)?;
                resolved = if target.is_absolute() {
                    target
                } else {
                    resolved
                        .parent()
                        .unwrap_or_else(|| Path::new("."))
                        .join(target)
                };
            }
            Ok(_) => return Ok(resolved),
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(resolved),
            Err(error) => return Err(error),
        }
    }
}

fn private_file_parent(path: &Path) -> &Path {
    path.parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."))
}

pub(crate) fn write_private_file(path: &Path, contents: &str) -> io::Result<()> {
    let write_path = private_file_target_path(path)?;
    let parent = private_file_parent(&write_path);
    std::fs::create_dir_all(parent)?;

    let mut temporary = create_private_temporary_file(parent)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        temporary
            .as_file()
            .set_permissions(std::fs::Permissions::from_mode(0o600))?;
    }
    temporary.write_all(contents.as_bytes())?;
    temporary.as_file().sync_all()?;
    persist_private_temporary_file(temporary, &write_path)?;
    Ok(())
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::fs::File;
    use std::os::unix::fs::{MetadataExt, PermissionsExt};

    #[test]
    fn replaces_loose_existing_file_with_private_inode() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("token.json");
        std::fs::write(&path, "old").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
        let old_file = File::open(&path).unwrap();
        let old_inode = old_file.metadata().unwrap().ino();

        write_private_file(&path, "new-secret").unwrap();

        let metadata = std::fs::metadata(&path).unwrap();
        assert_ne!(metadata.ino(), old_inode);
        assert_eq!(metadata.permissions().mode() & 0o777, 0o600);
        assert_eq!(std::fs::read_to_string(path).unwrap(), "new-secret");
    }

    #[test]
    fn preserves_symlink_and_replaces_target() {
        use std::os::unix::fs::symlink;

        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("token.json");
        let target = directory.path().join("managed-token.json");
        std::fs::write(&target, "old").unwrap();
        std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o644)).unwrap();
        let old_inode = std::fs::metadata(&target).unwrap().ino();
        symlink("managed-token.json", &path).unwrap();

        write_private_file(&path, "new-secret").unwrap();

        assert!(std::fs::symlink_metadata(&path)
            .unwrap()
            .file_type()
            .is_symlink());
        let metadata = std::fs::metadata(&target).unwrap();
        assert_ne!(metadata.ino(), old_inode);
        assert_eq!(metadata.permissions().mode() & 0o777, 0o600);
        assert_eq!(std::fs::read_to_string(target).unwrap(), "new-secret");
    }

    #[test]
    fn parentless_relative_path_uses_current_directory() {
        let path = Path::new("token.json");
        let resolved = private_file_target_path(path).unwrap();

        assert_eq!(private_file_parent(&resolved), Path::new("."));
    }

    #[test]
    fn rejects_chained_symlinks() {
        use std::os::unix::fs::symlink;

        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("token.json");
        let intermediate = directory.path().join("managed-token.json");
        let target = directory.path().join("actual-token.json");
        std::fs::write(&target, "old").unwrap();
        symlink("actual-token.json", &intermediate).unwrap();
        symlink("managed-token.json", &path).unwrap();

        let error = write_private_file(&path, "new-secret").unwrap_err();

        assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
        assert_eq!(std::fs::read_to_string(target).unwrap(), "old");
        assert!(std::fs::symlink_metadata(path)
            .unwrap()
            .file_type()
            .is_symlink());
        assert!(std::fs::symlink_metadata(intermediate)
            .unwrap()
            .file_type()
            .is_symlink());
    }
}

#[cfg(all(test, windows))]
mod windows_tests {
    use super::*;
    use std::ffi::c_void;
    use std::fs::File;
    use std::os::windows::io::AsRawHandle;
    use std::ptr;
    use winapi::shared::minwindef::HLOCAL;
    use winapi::shared::winerror::ERROR_SUCCESS;
    use winapi::um::accctrl::SE_FILE_OBJECT;
    use winapi::um::aclapi::GetSecurityInfo;
    use winapi::um::securitybaseapi::{
        CreateWellKnownSid, EqualSid, GetAce, GetSecurityDescriptorControl,
    };
    use winapi::um::winbase::LocalFree;
    use winapi::um::winnt::{
        WinCreatorOwnerRightsSid, ACCESS_ALLOWED_ACE, ACCESS_ALLOWED_ACE_TYPE,
        DACL_SECURITY_INFORMATION, FILE_ALL_ACCESS, OWNER_SECURITY_INFORMATION, PACL,
        PSECURITY_DESCRIPTOR, PSID, SECURITY_MAX_SID_SIZE, SE_DACL_PROTECTED,
    };

    fn assert_owner_only_protected_dacl(file: &File) {
        let mut owner: PSID = ptr::null_mut();
        let mut dacl: PACL = ptr::null_mut();
        let mut descriptor: PSECURITY_DESCRIPTOR = ptr::null_mut();
        let status = unsafe {
            GetSecurityInfo(
                file.as_raw_handle(),
                SE_FILE_OBJECT,
                OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION,
                &mut owner,
                ptr::null_mut(),
                &mut dacl,
                ptr::null_mut(),
                &mut descriptor,
            )
        };
        assert_eq!(status, ERROR_SUCCESS);

        let mut control = 0;
        let mut revision = 0;
        assert_ne!(
            unsafe { GetSecurityDescriptorControl(descriptor, &mut control, &mut revision) },
            0
        );
        assert_ne!(control & SE_DACL_PROTECTED, 0);
        assert!(!dacl.is_null());
        assert_eq!(unsafe { (*dacl).AceCount }, 1);

        let mut ace: *mut c_void = ptr::null_mut();
        assert_ne!(unsafe { GetAce(dacl, 0, &mut ace) }, 0);
        let allowed = ace.cast::<ACCESS_ALLOWED_ACE>();
        assert_eq!(
            unsafe { (*allowed).Header.AceType },
            ACCESS_ALLOWED_ACE_TYPE
        );
        assert_eq!(
            unsafe { (*allowed).Mask } & FILE_ALL_ACCESS,
            FILE_ALL_ACCESS
        );

        let mut expected_sid = [0u8; SECURITY_MAX_SID_SIZE];
        let mut expected_sid_size = expected_sid.len() as u32;
        assert_ne!(
            unsafe {
                CreateWellKnownSid(
                    WinCreatorOwnerRightsSid,
                    ptr::null_mut(),
                    expected_sid.as_mut_ptr().cast(),
                    &mut expected_sid_size,
                )
            },
            0
        );
        let actual_sid = unsafe { &mut (*allowed).SidStart as *mut u32 as PSID };
        assert_ne!(
            unsafe { EqualSid(actual_sid, expected_sid.as_mut_ptr().cast()) },
            0
        );
        unsafe {
            LocalFree(descriptor as HLOCAL);
        }
    }

    #[test]
    fn creates_temporary_file_with_owner_only_protected_dacl() {
        let directory = tempfile::tempdir().unwrap();
        let temporary = create_private_temporary_file(directory.path()).unwrap();

        assert_owner_only_protected_dacl(temporary.as_file());
    }

    #[test]
    fn normalizes_long_windows_paths_to_verbatim_form() {
        let path = std::path::PathBuf::from(format!(r"C:\{}", "a".repeat(250)));

        let encoded = to_windows_api_path(&path).unwrap();

        let prefix: Vec<u16> = r"\\?\C:\".encode_utf16().collect();
        assert!(encoded.starts_with(&prefix));
        assert_eq!(encoded.last(), Some(&0));
    }

    #[test]
    fn writes_owner_only_protected_dacl() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("token.json");

        write_private_file(&path, "new-secret").unwrap();

        let file = File::open(&path).unwrap();
        assert_owner_only_protected_dacl(&file);
        assert_eq!(std::fs::read_to_string(path).unwrap(), "new-secret");
    }

    #[test]
    fn writes_owner_only_file_beyond_legacy_path_limit() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory
            .path()
            .join("a".repeat(120))
            .join("b".repeat(120))
            .join("token.json");

        write_private_file(&path, "new-secret").unwrap();

        let file = File::open(&path).unwrap();
        assert_owner_only_protected_dacl(&file);
        assert_eq!(std::fs::read_to_string(path).unwrap(), "new-secret");
    }
}
