use serde::{Deserialize, Serialize};

use crate::paths;

const SETTINGS_FILE: &str = "settings.json";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Routing {
    #[default]
    Smart,
    Global,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SplitMode {
    #[default]
    Bypass,
    Only,
    Routing,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct SplitSettings {
    pub enabled: bool,
    pub mode: SplitMode,
    pub apps: Vec<String>,
    pub sites: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct VpnSettings {
    pub routing: Routing,
    pub ipv6: bool,
    pub tun_mode: bool,
    pub split: SplitSettings,
}

impl SplitSettings {
    pub fn has_apps(&self) -> bool {
        self.enabled && self.apps.iter().any(|app| !app.trim().is_empty())
    }

    pub fn has_sites(&self) -> bool {
        self.enabled && self.sites.iter().any(|site| !site.trim().is_empty())
    }
}

impl VpnSettings {
    pub fn sanitized(mut self) -> Self {
        self.split.apps = dedup(self.split.apps);
        self.split.sites = dedup(self.split.sites);
        self
    }
}

fn dedup(items: Vec<String>) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    items
        .into_iter()
        .map(|item| item.trim().to_owned())
        .filter(|item| !item.is_empty() && seen.insert(item.to_lowercase()))
        .collect()
}

pub fn load() -> VpnSettings {
    std::fs::read(paths::data_dir().join(SETTINGS_FILE))
        .ok()
        .and_then(|raw| serde_json::from_slice::<VpnSettings>(&raw).ok())
        .unwrap_or_default()
        .sanitized()
}

pub fn save(settings: &VpnSettings) {
    if let Ok(raw) = serde_json::to_vec_pretty(settings) {
        let _ = std::fs::write(paths::data_dir().join(SETTINGS_FILE), raw);
    }
}
