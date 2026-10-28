use serde::{Deserialize, Serialize};

use crate::paths;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StoredSession {
    pub token: String,
    pub email: String,
}

const SESSION_FILE: &str = "session.bin";

pub fn load() -> Option<StoredSession> {
    let raw = std::fs::read(paths::data_dir().join(SESSION_FILE)).ok()?;
    let plain = unprotect(&raw)?;
    serde_json::from_slice(&plain).ok()
}

pub fn save(session: &StoredSession) {
    let Ok(plain) = serde_json::to_vec(session) else {
        return;
    };
    if let Some(sealed) = protect(&plain) {
        let path = paths::data_dir().join(SESSION_FILE);
        if std::fs::write(&path, sealed).is_ok() {
            restrict_permissions(&path);
        }
    }
}

pub fn clear() {
    let _ = std::fs::remove_file(paths::data_dir().join(SESSION_FILE));
}

#[cfg(windows)]
fn protect(data: &[u8]) -> Option<Vec<u8>> {
    dpapi::run(data, true)
}

#[cfg(windows)]
fn unprotect(data: &[u8]) -> Option<Vec<u8>> {
    dpapi::run(data, false)
}

#[cfg(not(windows))]
fn protect(data: &[u8]) -> Option<Vec<u8>> {
    Some(data.to_vec())
}

#[cfg(not(windows))]
fn unprotect(data: &[u8]) -> Option<Vec<u8>> {
    Some(data.to_vec())
}

#[cfg(unix)]
fn restrict_permissions(path: &std::path::Path) {
    use std::os::unix::fs::PermissionsExt;
    let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
}

#[cfg(not(unix))]
fn restrict_permissions(_path: &std::path::Path) {}

#[cfg(windows)]
mod dpapi {
    use windows_sys::Win32::Foundation::LocalFree;
    use windows_sys::Win32::Security::Cryptography::{
        CRYPT_INTEGER_BLOB, CRYPTPROTECT_UI_FORBIDDEN, CryptProtectData, CryptUnprotectData,
    };

    pub fn run(data: &[u8], encrypt: bool) -> Option<Vec<u8>> {
        let input = CRYPT_INTEGER_BLOB {
            cbData: data.len() as u32,
            pbData: data.as_ptr() as *mut u8,
        };
        let mut output = CRYPT_INTEGER_BLOB {
            cbData: 0,
            pbData: std::ptr::null_mut(),
        };
        let ok = unsafe {
            if encrypt {
                CryptProtectData(
                    &input,
                    std::ptr::null(),
                    std::ptr::null(),
                    std::ptr::null(),
                    std::ptr::null(),
                    CRYPTPROTECT_UI_FORBIDDEN,
                    &mut output,
                )
            } else {
                CryptUnprotectData(
                    &input,
                    std::ptr::null_mut(),
                    std::ptr::null(),
                    std::ptr::null(),
                    std::ptr::null(),
                    CRYPTPROTECT_UI_FORBIDDEN,
                    &mut output,
                )
            }
        };
        if ok == 0 || output.pbData.is_null() {
            return None;
        }
        let bytes =
            unsafe { std::slice::from_raw_parts(output.pbData, output.cbData as usize) }.to_vec();
        unsafe {
            LocalFree(output.pbData as _);
        }
        Some(bytes)
    }
}
