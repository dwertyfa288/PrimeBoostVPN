use std::time::Instant;

use serde::Serialize;

use crate::billing::{BillingState, BillingView};
use crate::settings::VpnSettings;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthStatus {
    Checking,
    SignedOut,
    WaitingGoogle,
    WaitingTelegram,
    SignedIn,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Subscription {
    Unknown,
    Active,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum VpnStatus {
    Disconnected,
    Connecting,
    Connected,
    Disconnecting,
}

#[derive(Debug, Clone, Serialize)]
pub struct ActiveLocation {
    pub id: String,
    pub name: String,
    pub name_en: String,
    pub country: String,
    pub latency_ms: Option<u64>,
}

pub struct State {
    pub auth: AuthStatus,
    pub token: Option<String>,
    pub email: String,
    pub auth_error: String,
    pub auth_link: String,
    pub auth_flow: u64,
    pub subscription: Subscription,
    pub vpn: VpnStatus,
    pub stage: String,
    pub vpn_error: String,
    pub key_id: Option<String>,
    pub location: Option<ActiveLocation>,
    pub connected_at: Option<Instant>,
    pub connect_flow: u64,
    pub settings: VpnSettings,
    pub proxy_port: u16,
    pub api_port: u16,
    pub api_secret: String,
    pub traffic: Traffic,
    pub billing: BillingState,
    pub trial: crate::trial::TrialView,
}

#[derive(Default, Clone, Serialize)]
pub struct Traffic {
    pub down_speed: u64,
    pub up_speed: u64,
    pub down_total: u64,
    pub up_total: u64,
}

impl Default for State {
    fn default() -> Self {
        Self {
            auth: AuthStatus::Checking,
            token: None,
            email: String::new(),
            auth_error: String::new(),
            auth_link: String::new(),
            auth_flow: 0,
            subscription: Subscription::Unknown,
            vpn: VpnStatus::Disconnected,
            stage: String::new(),
            vpn_error: String::new(),
            key_id: None,
            location: None,
            connected_at: None,
            connect_flow: 0,
            settings: VpnSettings::default(),
            proxy_port: 0,
            api_port: 0,
            api_secret: String::new(),
            traffic: Traffic::default(),
            billing: BillingState::default(),
            trial: Default::default(),
        }
    }
}

#[derive(Serialize)]
pub struct Snapshot {
    pub auth: AuthStatus,
    pub email: String,
    pub auth_error: String,
    pub auth_link: String,
    pub subscription: Subscription,
    pub vpn: VpnStatus,
    pub stage: String,
    pub vpn_error: String,
    pub location: Option<ActiveLocation>,
    pub uptime_secs: u64,
    pub settings: VpnSettings,
    pub needs_admin: bool,
    pub proxy_port: u16,
    pub traffic: Traffic,
    pub platform: &'static str,
    pub billing: BillingView,
    pub trial: crate::trial::TrialView,
}

impl State {
    pub fn snapshot(&self) -> Snapshot {
        Snapshot {
            auth: self.auth,
            email: self.email.clone(),
            auth_error: self.auth_error.clone(),
            auth_link: self.auth_link.clone(),
            subscription: self.subscription,
            vpn: self.vpn,
            stage: self.stage.clone(),
            vpn_error: self.vpn_error.clone(),
            location: self.location.clone(),
            uptime_secs: self.connected_at.map(|at| at.elapsed().as_secs()).unwrap_or(0),
            settings: self.settings.clone(),
            needs_admin: crate::split::resolve_capture(&self.settings).needs_tun()
                && !crate::privileges::process_elevated(),
            proxy_port: self.proxy_port,
            traffic: self.traffic.clone(),
            platform: std::env::consts::OS,
            billing: self.billing.view.clone(),
            trial: self.trial.clone(),
        }
    }
}
