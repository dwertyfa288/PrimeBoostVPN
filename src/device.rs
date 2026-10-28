use std::sync::OnceLock;

use sha2::{Digest, Sha256};

static DEVICE_ID: OnceLock<String> = OnceLock::new();

pub fn id() -> &'static str {
    DEVICE_ID.get_or_init(|| {
        let mut hasher = Sha256::new();
        for part in hardware_parts() {
            hasher.update(part.trim().as_bytes());
            hasher.update(b"|");
        }
        hasher
            .finalize()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect()
    })
}

#[cfg(windows)]
fn hardware_parts() -> Vec<String> {
    use winreg::RegKey;
    use winreg::enums::{HKEY_LOCAL_MACHINE, KEY_READ, KEY_WOW64_64KEY};

    let machine = RegKey::predef(HKEY_LOCAL_MACHINE);
    let read = |path: &str, value: &str| {
        machine
            .open_subkey_with_flags(path, KEY_READ | KEY_WOW64_64KEY)
            .and_then(|key| key.get_value::<String, _>(value))
            .unwrap_or_default()
    };
    let bios = r"HARDWARE\DESCRIPTION\System\BIOS";
    vec![
        read(r"SOFTWARE\Microsoft\Cryptography", "MachineGuid"),
        read(bios, "SystemManufacturer"),
        read(bios, "SystemProductName"),
        read(bios, "BaseBoardManufacturer"),
        read(bios, "BaseBoardProduct"),
        read(r"HARDWARE\DESCRIPTION\System\CentralProcessor\0", "ProcessorNameString"),
    ]
}

#[cfg(not(windows))]
fn hardware_parts() -> Vec<String> {
    let read = |path: &str| std::fs::read_to_string(path).unwrap_or_default();
    vec![
        read("/etc/machine-id"),
        read("/var/lib/dbus/machine-id"),
        read("/sys/class/dmi/id/board_vendor"),
        read("/sys/class/dmi/id/board_name"),
        read("/sys/class/dmi/id/product_name"),
    ]
}
