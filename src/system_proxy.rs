use crate::paths;

const MARKER_FILE: &str = "proxy.active";

fn marker_path() -> std::path::PathBuf {
    paths::data_dir().join(MARKER_FILE)
}

pub fn enable(port: u16) -> Result<(), String> {
    let _ = std::fs::write(marker_path(), port.to_string());
    platform::enable(port)
}

pub fn disable() {
    if let Ok(port) = std::fs::read_to_string(marker_path()) {
        if let Ok(port) = port.trim().parse::<u16>() {
            platform::disable_if_ours(port);
        }
    }
    let _ = std::fs::remove_file(marker_path());
}

#[cfg(windows)]
mod platform {
    use windows_sys::Win32::Networking::WinInet::{
        INTERNET_OPTION_REFRESH, INTERNET_OPTION_SETTINGS_CHANGED, InternetSetOptionW,
    };
    use winreg::RegKey;
    use winreg::enums::{HKEY_CURRENT_USER, KEY_READ, KEY_SET_VALUE};

    const SETTINGS_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Internet Settings";
    const BYPASS: &str = "localhost;127.*;10.*;172.16.*;172.17.*;172.18.*;172.19.*;172.20.*;172.21.*;172.22.*;172.23.*;172.24.*;172.25.*;172.26.*;172.27.*;172.28.*;172.29.*;172.30.*;172.31.*;192.168.*;<local>";

    fn proxy_server(port: u16) -> String {
        format!("http=127.0.0.1:{port};https=127.0.0.1:{port};socks=127.0.0.1:{port}")
    }

    pub fn enable(port: u16) -> Result<(), String> {
        let (key, _) = RegKey::predef(HKEY_CURRENT_USER)
            .create_subkey(SETTINGS_KEY)
            .map_err(|error| format!("Не удалось открыть настройки прокси: {error}"))?;
        key.set_value("ProxyEnable", &1u32)
            .and_then(|_| key.set_value("ProxyServer", &proxy_server(port)))
            .and_then(|_| key.set_value("ProxyOverride", &BYPASS))
            .map_err(|error| format!("Не удалось включить системный прокси: {error}"))?;
        refresh();
        Ok(())
    }

    pub fn disable_if_ours(port: u16) {
        let Ok(key) = RegKey::predef(HKEY_CURRENT_USER)
            .open_subkey_with_flags(SETTINGS_KEY, KEY_READ | KEY_SET_VALUE)
        else {
            return;
        };
        let current: String = key.get_value("ProxyServer").unwrap_or_default();
        if current != proxy_server(port) {
            return;
        }
        let _ = key.set_value("ProxyEnable", &0u32);
        let _ = key.delete_value("ProxyServer");
        let _ = key.delete_value("ProxyOverride");
        refresh();
    }

    fn refresh() {
        unsafe {
            InternetSetOptionW(std::ptr::null(), INTERNET_OPTION_SETTINGS_CHANGED, std::ptr::null(), 0);
            InternetSetOptionW(std::ptr::null(), INTERNET_OPTION_REFRESH, std::ptr::null(), 0);
        }
    }
}

#[cfg(not(windows))]
mod platform {
    use std::process::Command;

    fn gsettings(args: &[&str]) -> bool {
        Command::new("gsettings")
            .args(args)
            .status()
            .map(|status| status.success())
            .unwrap_or(false)
    }

    pub fn enable(port: u16) -> Result<(), String> {
        let port = port.to_string();
        let mut ok = true;
        for scheme in ["http", "https", "socks"] {
            let schema = format!("org.gnome.system.proxy.{scheme}");
            ok &= gsettings(&["set", &schema, "host", "127.0.0.1"]);
            ok &= gsettings(&["set", &schema, "port", &port]);
        }
        ok &= gsettings(&[
            "set",
            "org.gnome.system.proxy",
            "ignore-hosts",
            "['localhost', '127.0.0.0/8', '10.0.0.0/8', '172.16.0.0/12', '192.168.0.0/16', '::1']",
        ]);
        ok &= gsettings(&["set", "org.gnome.system.proxy", "mode", "manual"]);
        if ok {
            Ok(())
        } else {
            Err("Не удалось включить системный прокси через gsettings".into())
        }
    }

    pub fn disable_if_ours(port: u16) {
        let current = Command::new("gsettings")
            .args(["get", "org.gnome.system.proxy.http", "port"])
            .output()
            .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_owned())
            .unwrap_or_default();
        if current == port.to_string() {
            gsettings(&["set", "org.gnome.system.proxy", "mode", "none"]);
        }
    }
}
