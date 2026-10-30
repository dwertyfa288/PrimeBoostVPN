use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use sha2::{Digest, Sha256};

use crate::paths;

#[cfg(windows)]
const CORE_NAME: &str = "prime-vpn-core.exe";
#[cfg(windows)]
const CORE: &[u8] = include_bytes!("../core/windows/prime-vpn-core.exe");
#[cfg(windows)]
const XRAY_NAME: &str = "prime-vpn-xray.exe";
#[cfg(windows)]
const XRAY: &[u8] = include_bytes!("../core/windows/prime-vpn-xray.exe");

#[cfg(not(windows))]
const CORE_NAME: &str = "prime-vpn-core";
#[cfg(not(windows))]
const CORE: &[u8] = include_bytes!("../core/linux/prime-vpn-core");
#[cfg(not(windows))]
const XRAY_NAME: &str = "prime-vpn-xray";
#[cfg(not(windows))]
const XRAY: &[u8] = include_bytes!("../core/linux/prime-vpn-xray");

#[cfg(windows)]
const PLATFORM_FILES: &[(&str, &[u8])] = &[("wintun.dll", include_bytes!("../core/wintun.dll"))];
#[cfg(not(windows))]
const PLATFORM_FILES: &[(&str, &[u8])] = &[];

const RULES: &[(&str, &[u8])] = &[
    ("rules/geoip-cn.srs", include_bytes!("../core/rules/geoip-cn.srs")),
    ("rules/geoip-ru.srs", include_bytes!("../core/rules/geoip-ru.srs")),
    ("rules/geosite-category-ru.srs", include_bytes!("../core/rules/geosite-category-ru.srs")),
    ("rules/geosite-geolocation-cn.srs", include_bytes!("../core/rules/geosite-geolocation-cn.srs")),
];

static READY: OnceLock<PathBuf> = OnceLock::new();

fn digest(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}

fn place(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if let Ok(existing) = std::fs::read(path) {
        if existing.len() == bytes.len() && digest(&existing) == digest(bytes) {
            return Ok(());
        }
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| format!("Не удалось подготовить ядро VPN: {error}"))?;
    }
    let temporary = path.with_extension("tmp");
    std::fs::write(&temporary, bytes).map_err(|error| format!("Не удалось подготовить ядро VPN: {error}"))?;
    std::fs::rename(&temporary, path).map_err(|error| {
        let _ = std::fs::remove_file(&temporary);
        format!("Не удалось подготовить ядро VPN: {error}")
    })
}

pub fn xray_path() -> PathBuf {
    paths::core_dir().join(XRAY_NAME)
}

pub fn ensure() -> Result<PathBuf, String> {
    if let Some(path) = READY.get() {
        return Ok(path.clone());
    }
    let dir = paths::core_dir();
    let core = dir.join(CORE_NAME);
    place(&core, CORE)?;
    place(&dir.join(XRAY_NAME), XRAY)?;
    for (name, bytes) in PLATFORM_FILES.iter().chain(RULES) {
        place(&dir.join(name), bytes)?;
    }
    Ok(READY.get_or_init(|| core).clone())
}
