use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::api::{ApiError, client, send, server_message};
use crate::app::SharedApp;
use crate::auth;
use crate::device;

#[derive(Debug, Clone, Default, Serialize)]
pub struct TrialView {
    pub available: bool,
    pub days: i64,
    pub claiming: bool,
    pub activated: bool,
    pub checked: bool,
    pub error: String,
}

#[derive(Deserialize)]
struct TrialStatus {
    #[serde(default)]
    available: bool,
    #[serde(default)]
    days: i64,
}

async fn fetch_status(token: &str) -> Result<TrialStatus, ApiError> {
    let request = client()
        .get(format!("{}/subscriptions/device-trial", crate::endpoints::site_api()))
        .query(&[("device_id", device::id())])
        .bearer_auth(token);
    let (status, body) = send(request).await?;
    match status {
        200 => serde_json::from_str(&body).map_err(|_| ApiError::Server("Некорректный ответ сервера".into())),
        401 | 403 => Err(ApiError::Unauthorized),
        _ => Err(ApiError::Server(server_message(&body, "Ошибка сервера"))),
    }
}

async fn claim_request(token: &str) -> Result<(), ApiError> {
    let request = client()
        .post(format!("{}/subscriptions/device-trial", crate::endpoints::site_api()))
        .bearer_auth(token)
        .json(&json!({ "device_id": device::id() }));
    let (status, body) = send(request).await?;
    match status {
        200 => Ok(()),
        401 => Err(ApiError::Unauthorized),
        _ => Err(ApiError::Server(server_message(&body, "Не удалось активировать пробный период"))),
    }
}

pub async fn refresh(app: &SharedApp) {
    let Some(token) = app.state.lock().token.clone() else {
        return;
    };
    match fetch_status(&token).await {
        Ok(status) => {
            let mut state = app.state.lock();
            state.trial.available = status.available;
            state.trial.days = status.days;
            state.trial.checked = true;
        }
        Err(ApiError::Unauthorized) => auth::handle_unauthorized(app),
        Err(_) => {
            let mut state = app.state.lock();
            state.trial.available = false;
            state.trial.checked = true;
        }
    }
}

pub async fn claim(app: SharedApp) {
    let Some(token) = app.state.lock().token.clone() else {
        return;
    };
    {
        let mut state = app.state.lock();
        state.trial.claiming = true;
        state.trial.error.clear();
    }
    let result = claim_request(&token).await;
    {
        let mut state = app.state.lock();
        state.trial.claiming = false;
        match &result {
            Ok(()) => {
                state.trial.available = false;
                state.trial.activated = true;
            }
            Err(error) => {
                state.trial.available = false;
                state.trial.error = error.to_string();
            }
        }
    }
    match result {
        Err(ApiError::Unauthorized) => auth::handle_unauthorized(&app),
        _ => {
            auth::refresh_subscription(&app).await;
            crate::billing::load(app).await;
        }
    }
}
