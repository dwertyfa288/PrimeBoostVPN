use std::time::Duration;

use astra_plugin_sdk::prelude::ToolError;

use crate::app::SharedApp;
use crate::state::{AuthStatus, Subscription, VpnStatus};
use crate::vpn;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(90);
const DISCONNECT_TIMEOUT: Duration = Duration::from_secs(20);

fn location_name(app: &SharedApp) -> String {
    app.state
        .lock()
        .location
        .as_ref()
        .map(|location| location.name.clone())
        .unwrap_or_default()
}

pub async fn connect(app: SharedApp) -> Result<String, ToolError> {
    {
        let state = app.state.lock();
        if state.auth != AuthStatus::SignedIn {
            return Err(ToolError::Unauthorized(
                "Войдите в PrimeBoost на вкладке плагина, чтобы включить VPN".into(),
            ));
        }
        if state.subscription == Subscription::None {
            return Err(ToolError::Unavailable(
                "Нет активной подписки PrimeBoost — выберите тариф на вкладке плагина".into(),
            ));
        }
        if state.vpn == VpnStatus::Connected {
            drop(state);
            return Ok(format!("VPN уже включён. {}", status(&app)));
        }
    }
    if tokio::time::timeout(CONNECT_TIMEOUT, vpn::connect(app.clone())).await.is_err() {
        return Err(ToolError::Timeout("VPN не подключился вовремя".into()));
    }
    let state = app.state.lock();
    match state.vpn {
        VpnStatus::Connected => {
            drop(state);
            Ok(format!("VPN включён. {}", status(&app)))
        }
        _ if !state.vpn_error.is_empty() => Err(ToolError::Unavailable(state.vpn_error.clone())),
        _ => Ok("VPN подключается".into()),
    }
}

pub async fn disconnect(app: SharedApp) -> Result<String, ToolError> {
    if app.state.lock().vpn == VpnStatus::Disconnected {
        return Ok("VPN уже выключен".into());
    }
    if tokio::time::timeout(DISCONNECT_TIMEOUT, vpn::disconnect(app.clone())).await.is_err() {
        return Err(ToolError::Timeout("VPN не отключился вовремя".into()));
    }
    Ok("VPN выключен".into())
}

pub fn status(app: &SharedApp) -> String {
    let (signed_in, vpn, mode) = {
        let state = app.state.lock();
        let mode = if state.settings.tun_mode { "TUN" } else { "прокси" };
        (state.auth == AuthStatus::SignedIn, state.vpn, mode)
    };
    if !signed_in {
        return "Пользователь не вошёл в PrimeBoost".into();
    }
    match vpn {
        VpnStatus::Connected => {
            let location = location_name(app);
            if location.is_empty() {
                format!("Статус: подключено, режим {mode}")
            } else {
                format!("Статус: подключено, локация {location}, режим {mode}")
            }
        }
        VpnStatus::Connecting => "Статус: подключается".into(),
        VpnStatus::Disconnecting => "Статус: отключается".into(),
        VpnStatus::Disconnected => "Статус: выключено".into(),
    }
}
