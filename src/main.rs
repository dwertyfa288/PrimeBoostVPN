mod api;
mod app;
mod auth;
mod billing;
mod billing_api;
mod browser;
mod commands;
mod core;
mod core_assets;
mod device;
mod endpoints;
mod locations;
mod paths;
mod privileges;
mod processes;
mod runner;
mod session;
mod settings;
mod singbox_config;
mod split;
mod state;
mod system_proxy;
mod trial;
mod vless;
mod vpn;
mod xray;

use std::sync::Arc;

use astra_plugin_sdk::prelude::*;

use app::{App, SharedApp};
use settings::VpnSettings;

const POWER_ON_ICON: &str = r#"<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"><path d="M12 3v8"/><path d="M6.3 6.7a8 8 0 1 0 11.4 0"/></svg>"#;
const POWER_OFF_ICON: &str = r#"<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M12 2.5 4.5 5.3v5.6c0 4.9 3.2 8.4 7.5 9.8 4.3-1.4 7.5-4.9 7.5-9.8V5.3z"/><path d="m9 9 6 6"/><path d="m15 9-6 6"/></svg>"#;

const PAGE_ICON: &str = r#"<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.9" stroke-linejoin="round"><path d="M12 2.5 4.5 5.3v5.6c0 4.9 3.2 8.4 7.5 9.8 4.3-1.4 7.5-4.9 7.5-9.8V5.3z"/><g fill="currentColor" stroke="none" transform="translate(9.2 7.6) skewX(-18)"><rect x="0" y="1.8" width="1.8" height="5.6" rx=".6"/><rect x="2.9" y="0" width="1.8" height="8" rx=".6"/><rect x="5.8" y="1.8" width="1.8" height="5.6" rx=".6"/></g></svg>"#;

#[derive(Default)]
struct PrimeVpn {
    app: SharedApp,
}

#[derive(serde::Deserialize)]
struct PlanArgs {
    plan_id: i64,
    #[serde(default)]
    method: String,
}

#[derive(serde::Deserialize)]
struct LogArgs {
    message: String,
}

#[derive(serde::Deserialize)]
struct OpenArgs {
    target: String,
}

fn parse<T: serde::de::DeserializeOwned>(json: &str) -> Result<T, ToolError> {
    serde_json::from_str(json).map_err(|error| ToolError::BadArguments(error.to_string()))
}

fn ok() -> Result<String, ToolError> {
    Ok(r#"{"ok":true}"#.into())
}

impl PrimeVpn {
    fn spawn<F>(&self, task: impl FnOnce(SharedApp) -> F)
    where
        F: std::future::Future<Output = ()> + Send + 'static,
    {
        tokio::spawn(task(self.app.clone()));
    }

    fn snapshot(&self) -> Result<String, ToolError> {
        serde_json::to_string(&self.app.state.lock().snapshot())
            .map_err(|error| ToolError::Internal(error.to_string()))
    }

    fn open(&self, target: &str) -> Result<String, ToolError> {
        let url = match target {
            "site" | "subscribe" => format!("{}/", crate::endpoints::site_url()),
            "auth_link" => self.app.state.lock().auth_link.clone(),
            _ => return Err(ToolError::BadArguments(format!("unknown target `{target}`"))),
        };
        browser::open(&url).map_err(ToolError::Unavailable)?;
        ok()
    }
}

#[astra::plugin]
impl PrimeVpn {
    /// Turn PrimeBoost VPN on. Picks the fastest server automatically. Use when the user asks to enable, connect or turn on the VPN.
    #[tool]
    async fn vpn_connect(&self) -> Result<String, ToolError> {
        commands::connect(self.app.clone()).await
    }

    /// Turn PrimeBoost VPN off. Use when the user asks to disable, disconnect or turn off the VPN.
    #[tool]
    async fn vpn_disconnect(&self) -> Result<String, ToolError> {
        commands::disconnect(self.app.clone()).await
    }

    /// Report whether PrimeBoost VPN is on, the current location and the traffic mode (proxy or TUN).
    #[tool]
    async fn vpn_status(&self) -> Result<String, ToolError> {
        Ok(commands::status(&self.app))
    }

    /// Connect PrimeBoost VPN, the same as pressing «Connect».
    #[action(label = "Turn on PrimeBoost VPN", icon = POWER_ON_ICON)]
    async fn connect_action(&self) -> Result<String, ActionError> {
        commands::connect(self.app.clone()).await
    }

    /// Disconnect PrimeBoost VPN.
    #[action(label = "Turn off PrimeBoost VPN", icon = POWER_OFF_ICON)]
    async fn disconnect_action(&self) -> Result<String, ActionError> {
        commands::disconnect(self.app.clone()).await
    }

    #[hook]
    async fn on_start(&self, _ctx: &PluginContext) -> anyhow::Result<()> {
        vpn::cleanup_leftovers();
        self.app.state.lock().settings = settings::load();
        self.spawn(auth::restore);
        self.spawn(vpn::monitor);
        Ok(())
    }

    #[hook]
    async fn on_shutdown(&self, _ctx: &PluginContext) {
        vpn::shutdown(&self.app);
    }

    #[hook]
    async fn ui_contributions(&self) -> Vec<UiContribution> {
        vec![
            UiContribution::page("prime-vpn", "PrimeBoost VPN", "index.html")
                .with_icon_svg(PAGE_ICON)
                .transparent(),
        ]
    }

    #[hook]
    async fn handle_ui_call(
        &self,
        _ctx: &PluginContext,
        method: &str,
        params_json: &str,
    ) -> Result<String, ToolError> {
        let params = if params_json.trim().is_empty() { "{}" } else { params_json };
        match method {
            "state" => self.snapshot(),
            "login_google" => {
                self.spawn(auth::login_google);
                ok()
            }
            "login_telegram" => {
                self.spawn(auth::login_telegram);
                ok()
            }
            "claim_trial" => {
                self.spawn(trial::claim);
                ok()
            }
            "cancel_auth" => {
                auth::cancel(&self.app);
                ok()
            }
            "logout" => {
                self.spawn(auth::logout);
                ok()
            }
            "refresh" => {
                self.spawn(|app| async move { auth::refresh_subscription(&app).await });
                ok()
            }
            "connect" => {
                self.spawn(vpn::connect);
                ok()
            }
            "disconnect" => {
                self.spawn(vpn::disconnect);
                ok()
            }
            "set_settings" => {
                let settings: VpnSettings = parse(params)?;
                self.spawn(move |app| vpn::update_settings(app, settings));
                ok()
            }
            "list_processes" => {
                let list = tokio::task::spawn_blocking(processes::running)
                    .await
                    .map_err(|error| ToolError::Internal(error.to_string()))?;
                serde_json::to_string(&list).map_err(|error| ToolError::Internal(error.to_string()))
            }
            "billing_load" => {
                self.spawn(billing::load);
                ok()
            }
            "billing_quote" => {
                let args: PlanArgs = parse(params)?;
                let amount = billing::quote(&self.app, args.plan_id, &args.method)
                    .await
                    .map_err(ToolError::Unavailable)?;
                Ok(json!({ "amount": amount }).to_string())
            }
            "billing_buy" => {
                let args: PlanArgs = parse(params)?;
                if !matches!(args.method.as_str(), "balance" | "sbp" | "crypto") {
                    return Err(ToolError::BadArguments(format!("unknown method `{}`", args.method)));
                }
                self.spawn(move |app| billing::buy(app, args.plan_id, args.method));
                ok()
            }
            "billing_cancel" => {
                billing::cancel(&self.app);
                ok()
            }
            "billing_open" => {
                billing::open_payment(&self.app).map_err(ToolError::Unavailable)?;
                ok()
            }
            "ui_log" => {
                let args: LogArgs = parse(params)?;
                eprintln!("[ui] {}", args.message.chars().take(300).collect::<String>());
                ok()
            }
            "open" => {
                let args: OpenArgs = parse(params)?;
                self.open(&args.target)
            }
            _ => Err(ToolError::NotFound(format!("no UI method `{method}`"))),
        }
    }
}

astra::main!({
    runner::run_if_requested();
    PrimeVpn { app: Arc::new(App::default()) }
});

#[cfg(test)]
mod tests {
    use super::*;
    use astra_plugin_sdk::testing::Harness;

    #[tokio::test]
    async fn it_starts_and_answers() {
        let h = Harness::new(PrimeVpn::default())
            .with_config(json!({}))
            .start()
            .await
            .expect("the plugin started");

        assert!(h.health().await.0);
    }

    #[tokio::test]
    async fn ui_reports_state_and_rejects_unknown_targets() {
        let h = Harness::new(PrimeVpn::default())
            .with_config(json!({}))
            .start()
            .await
            .expect("the plugin started");

        let state: serde_json::Value =
            serde_json::from_str(&h.ui_call("state", json!({})).await.unwrap()).unwrap();
        assert_eq!(state["vpn"], "disconnected");
        assert!(state["settings"]["routing"] == "smart" || state["settings"]["routing"] == "global");
        assert!(state["settings"]["split"]["apps"].is_array());

        assert!(h.ui_call("open", json!({ "target": "elsewhere" })).await.is_err());
    }
}
