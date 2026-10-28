use crate::settings::{SplitMode, SplitSettings, VpnSettings};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CaptureMode {
    pub full_tun: bool,
    pub split_capture: bool,
    pub strict_route: bool,
}

impl CaptureMode {
    pub fn needs_tun(self) -> bool {
        self.full_tun || self.split_capture
    }
}

pub fn always_capture() -> bool {
    cfg!(target_os = "linux")
}

pub fn resolve_capture(settings: &VpnSettings) -> CaptureMode {
    let split = &settings.split;
    let always = always_capture();
    let split_capture = split.mode != SplitMode::Bypass && (split.has_apps() || (always && split.has_sites()));
    let bypass_capture = split.has_apps() && split.mode == SplitMode::Bypass;
    let full_tun = !split_capture && (settings.tun_mode || bypass_capture || always);
    CaptureMode {
        full_tun,
        split_capture,
        strict_route: full_tun && settings.tun_mode && !bypass_capture,
    }
}

pub fn system_proxy_enabled(capture: CaptureMode, split: &SplitSettings) -> bool {
    !capture.full_tun && !(capture.split_capture && split.mode == SplitMode::Only)
}

pub fn process_regexes(targets: &[String]) -> Vec<String> {
    let mut regexes: Vec<String> = Vec::new();
    for target in targets {
        let target = target.trim();
        if target.is_empty() {
            continue;
        }
        let regex = if looks_like_path(target) {
            path_regex(target)
        } else {
            name_regex(target)
        };
        if let Some(regex) = regex {
            if !regexes.contains(&regex) {
                regexes.push(regex);
            }
        }
    }
    regexes
}

fn is_windows_path(value: &str) -> bool {
    let bytes = value.as_bytes();
    (bytes.len() > 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' && matches!(bytes[2], b'\\' | b'/'))
        || value.contains('\\')
}

fn looks_like_path(value: &str) -> bool {
    is_windows_path(value) || value.contains('/')
}

fn escape_regex(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for c in value.chars() {
        if ".*+?^${}()|[]\\".contains(c) {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

const SEPARATOR: &str = r"[\\/]";
const ANY_SEGMENT: &str = r"[^\\/]+";

fn is_versioned_segment(segment: &str) -> bool {
    let core = segment
        .strip_prefix("app-")
        .or_else(|| segment.strip_prefix("App-"))
        .unwrap_or(segment);
    let parts: Vec<&str> = core.split('.').collect();
    parts.len() >= 2 && parts.iter().all(|part| !part.is_empty() && part.chars().all(|c| c.is_ascii_digit()))
}

fn name_regex(name: &str) -> Option<String> {
    let lower = name.to_lowercase();
    let base = if lower.ends_with(".exe") { &name[..name.len() - 4] } else { name };
    if base.is_empty() {
        return None;
    }
    Some(format!(r"(?i)(^|{SEPARATOR}){}(\.exe)?$", escape_regex(base)))
}

fn path_regex(path: &str) -> Option<String> {
    if is_windows_path(path) {
        let segments: Vec<String> = path
            .split(['\\', '/'])
            .filter(|segment| !segment.is_empty())
            .map(|segment| {
                if is_versioned_segment(segment) {
                    ANY_SEGMENT.to_owned()
                } else {
                    escape_regex(segment)
                }
            })
            .collect();
        return Some(format!("(?i)^{}$", segments.join(SEPARATOR)));
    }
    Some(format!("^{}$", escape_regex(path)))
}

pub struct SiteTargets {
    pub domains: Vec<String>,
    pub ip_cidrs: Vec<String>,
}

pub fn site_targets(sites: &[String]) -> SiteTargets {
    let mut domains: Vec<String> = Vec::new();
    let mut ip_cidrs: Vec<String> = Vec::new();
    for site in sites {
        let Some(value) = normalize_site(site) else { continue };
        let bucket = if is_ipv4_or_cidr(&value) { &mut ip_cidrs } else { &mut domains };
        if !bucket.contains(&value) {
            bucket.push(value);
        }
    }
    SiteTargets { domains, ip_cidrs }
}

pub fn normalize_site(site: &str) -> Option<String> {
    let mut value = site.trim().to_lowercase();
    if value.is_empty() {
        return None;
    }
    if is_ipv4_or_cidr(&value) {
        return Some(value);
    }
    if let Some(index) = value.find("://") {
        let scheme = &value[..index];
        if !scheme.is_empty() && scheme.chars().all(|c| c.is_ascii_alphanumeric() || "+.-".contains(c)) {
            value = value[index + 3..].to_owned();
        }
    }
    value = value
        .split(['/', '?', '#'])
        .next()
        .unwrap_or_default()
        .to_owned();
    value = value
        .trim_start_matches("*.")
        .trim_start_matches('.')
        .to_owned();
    if let Some((host, port)) = value.rsplit_once(':') {
        if !port.is_empty() && port.chars().all(|c| c.is_ascii_digit()) {
            value = host.to_owned();
        }
    }
    let value = value.trim_end_matches('.').to_owned();
    if value.is_empty() || value.chars().any(char::is_whitespace) {
        return None;
    }
    Some(value)
}

pub fn is_ipv4_or_cidr(value: &str) -> bool {
    let (address, prefix) = value.split_once('/').unwrap_or((value, ""));
    address.parse::<std::net::Ipv4Addr>().is_ok()
        && (prefix.is_empty() || prefix.parse::<u8>().is_ok_and(|bits| bits <= 32))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::{SplitMode, SplitSettings, VpnSettings};

    fn settings(mode: SplitMode, apps: &[&str], sites: &[&str], tun: bool) -> VpnSettings {
        VpnSettings {
            tun_mode: tun,
            split: SplitSettings {
                enabled: true,
                mode,
                apps: apps.iter().map(|s| s.to_string()).collect(),
                sites: sites.iter().map(|s| s.to_string()).collect(),
            },
            ..Default::default()
        }
    }

    #[test]
    fn name_and_path_regexes_match_desktop_client() {
        assert_eq!(
            process_regexes(&["Telegram.exe".into()]),
            vec![r"(?i)(^|[\\/])Telegram(\.exe)?$".to_owned()]
        );
        assert_eq!(
            process_regexes(&[r"C:\Users\me\AppData\Local\Discord\app-1.0.9\Discord.exe".into()]),
            vec![r"(?i)^C:[\\/]Users[\\/]me[\\/]AppData[\\/]Local[\\/]Discord[\\/][^\\/]+[\\/]Discord\.exe$".to_owned()]
        );
    }

    #[test]
    fn sites_are_normalized() {
        assert_eq!(normalize_site("https://www.YouTube.com/watch?v=1").as_deref(), Some("www.youtube.com"));
        assert_eq!(normalize_site("*.example.com:443").as_deref(), Some("example.com"));
        assert_eq!(normalize_site("10.0.0.0/8").as_deref(), Some("10.0.0.0/8"));
        assert!(normalize_site("  ").is_none());
    }

    #[test]
    fn capture_matches_desktop_client_rules() {
        if always_capture() {
            return;
        }
        let bypass = resolve_capture(&settings(SplitMode::Bypass, &["chrome.exe"], &[], false));
        assert!(bypass.full_tun && !bypass.split_capture && !bypass.strict_route);

        let only = resolve_capture(&settings(SplitMode::Only, &["chrome.exe"], &[], true));
        assert!(only.split_capture && !only.full_tun);
        assert!(!system_proxy_enabled(only, &settings(SplitMode::Only, &["a"], &[], true).split));

        let sites_only = resolve_capture(&settings(SplitMode::Only, &[], &["example.com"], false));
        assert!(!sites_only.needs_tun());

        let tun = resolve_capture(&VpnSettings { tun_mode: true, ..Default::default() });
        assert!(tun.full_tun && tun.strict_route);
    }
}
