#[cfg(windows)]
pub fn process_elevated() -> bool {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::Security::{GetTokenInformation, TOKEN_ELEVATION, TOKEN_QUERY, TokenElevation};
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

    unsafe {
        let mut token = std::ptr::null_mut();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) == 0 {
            return false;
        }
        let mut elevation = TOKEN_ELEVATION { TokenIsElevated: 0 };
        let mut length = 0u32;
        let ok = GetTokenInformation(
            token,
            TokenElevation,
            &mut elevation as *mut _ as *mut _,
            std::mem::size_of::<TOKEN_ELEVATION>() as u32,
            &mut length,
        );
        CloseHandle(token);
        ok != 0 && elevation.TokenIsElevated != 0
    }
}

#[cfg(unix)]
pub fn process_elevated() -> bool {
    unsafe { libc::geteuid() == 0 }
}

#[cfg(unix)]
mod linux {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;
    use std::path::{Path, PathBuf};
    use std::process::Command;

    const CAPABILITIES: &str = "cap_net_admin,cap_net_raw,cap_net_bind_service+ep";
    const CAP_NET_ADMIN_BIT: u32 = 12;
    const VFS_CAP_FLAGS_EFFECTIVE: u32 = 0x1;
    const SETCAP_LOCATIONS: [&str; 4] = ["/usr/sbin/setcap", "/usr/bin/setcap", "/sbin/setcap", "/bin/setcap"];

    pub fn has_tun_capability(path: &Path) -> bool {
        let (Ok(path), Ok(name)) = (
            CString::new(path.as_os_str().as_bytes()),
            CString::new("security.capability"),
        ) else {
            return false;
        };
        let mut buffer = [0u8; 24];
        let length = unsafe {
            libc::getxattr(
                path.as_ptr(),
                name.as_ptr(),
                buffer.as_mut_ptr() as *mut libc::c_void,
                buffer.len(),
            )
        };
        if length < 8 {
            return false;
        }
        let magic = u32::from_le_bytes([buffer[0], buffer[1], buffer[2], buffer[3]]);
        let permitted = u32::from_le_bytes([buffer[4], buffer[5], buffer[6], buffer[7]]);
        magic & VFS_CAP_FLAGS_EFFECTIVE != 0 && permitted & (1 << CAP_NET_ADMIN_BIT) != 0
    }

    fn find_in_path(program: &str) -> Option<PathBuf> {
        let paths = std::env::var_os("PATH").unwrap_or_else(|| "/usr/bin:/bin:/usr/sbin:/sbin".into());
        std::env::split_paths(&paths)
            .map(|dir| dir.join(program))
            .find(|candidate| candidate.is_file())
    }

    pub fn ensure_tun_capability(core: &Path) -> Result<(), String> {
        if has_tun_capability(core) {
            return Ok(());
        }
        let setcap = SETCAP_LOCATIONS
            .iter()
            .map(PathBuf::from)
            .find(|path| path.exists())
            .ok_or("Не найден setcap: установи libcap2-bin (Debian/Ubuntu) или libcap")?;
        let pkexec = find_in_path("pkexec").ok_or_else(|| {
            format!("Не найден pkexec. Выполни: sudo setcap {CAPABILITIES} {}", core.display())
        })?;
        let status = Command::new(pkexec)
            .arg(setcap)
            .arg(CAPABILITIES)
            .arg(core)
            .status()
            .map_err(|error| format!("Не удалось запустить pkexec: {error}"))?;
        if !status.success() {
            return Err("Права для TUN не выданы".into());
        }
        if !has_tun_capability(core) {
            return Err("Права не применились: файловая система смонтирована с nosuid?".into());
        }
        Ok(())
    }
}

#[cfg(unix)]
pub use linux::ensure_tun_capability;
