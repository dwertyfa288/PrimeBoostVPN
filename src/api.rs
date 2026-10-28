use std::time::Duration;

use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Debug)]
pub enum ApiError {
    Unauthorized,
    NoSubscription,
    Server(String),
    Network(String),
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unauthorized => f.write_str("Сессия истекла, войдите снова"),
            Self::NoSubscription => f.write_str("Нет активной подписки"),
            Self::Server(message) => f.write_str(message),
            Self::Network(message) => write!(f, "Нет связи с сервером: {message}"),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct User {
    pub email: String,
}

#[derive(Debug, Clone)]
pub struct AuthResult {
    pub token: String,
    pub user: User,
}

pub enum PollStatus {
    Pending,
    Expired,
    Done(AuthResult),
}

pub(crate) fn client() -> reqwest::Client {
    reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(20))
        .no_proxy()
        .user_agent("PrimeVPN-Astra/0.1")
        .build()
        .expect("http client")
}

pub(crate) async fn send(request: reqwest::RequestBuilder) -> Result<(u16, String), ApiError> {
    let response = request
        .header(reqwest::header::CACHE_CONTROL, "no-cache")
        .send()
        .await
        .map_err(|error| ApiError::Network(error.without_url().to_string()))?;
    let status = response.status().as_u16();
    let body = response
        .text()
        .await
        .map_err(|error| ApiError::Network(error.to_string()))?;
    Ok((status, body))
}

pub(crate) fn server_message(body: &str, fallback: &str) -> String {
    serde_json::from_str::<Value>(body)
        .ok()
        .and_then(|value| value.get("error").and_then(Value::as_str).map(str::to_owned))
        .unwrap_or_else(|| fallback.to_owned())
}

fn parse_auth(body: &str) -> Option<AuthResult> {
    let value: Value = serde_json::from_str(body).ok()?;
    let token = value.get("token")?.as_str()?.to_owned();
    let user: User = serde_json::from_value(value.get("user")?.clone()).ok()?;
    Some(AuthResult { token, user })
}

pub async fn desktop_auth_status(hash: &str) -> Result<PollStatus, ApiError> {
    let (status, body) = send(client().get(format!("{}/auth/desktop-auth/{hash}", crate::endpoints::site_api()))).await?;
    let value: Value = serde_json::from_str(&body).unwrap_or(Value::Null);
    match value.get("status").and_then(Value::as_str) {
        Some("success") => parse_auth(&body)
            .map(PollStatus::Done)
            .ok_or_else(|| ApiError::Server("Некорректный ответ сервера".into())),
        _ if status >= 500 => Err(ApiError::Server(server_message(&body, "Ошибка сервера"))),
        _ => Ok(PollStatus::Pending),
    }
}

pub async fn telegram_start() -> Result<(String, String), ApiError> {
    let request = client()
        .post(format!("{}/auth/telegram/start", crate::endpoints::site_api()))
        .json(&json!({}));
    let (status, body) = send(request).await?;
    let value: Value = serde_json::from_str(&body).unwrap_or(Value::Null);
    let code = value.get("code").and_then(Value::as_str);
    let link = value.get("link").and_then(Value::as_str);
    match (status, code, link) {
        (200, Some(code), Some(link)) => Ok((code.to_owned(), link.to_owned())),
        _ => Err(ApiError::Server(server_message(&body, "Не удалось начать вход через Telegram"))),
    }
}

pub async fn telegram_status(code: &str) -> Result<PollStatus, ApiError> {
    let (status, body) = send(client().get(format!("{}/auth/telegram/status/{code}", crate::endpoints::site_api()))).await?;
    let value: Value = serde_json::from_str(&body).unwrap_or(Value::Null);
    match value.get("status").and_then(Value::as_str) {
        Some("confirmed") => parse_auth(&body)
            .map(PollStatus::Done)
            .ok_or_else(|| ApiError::Server("Некорректный ответ сервера".into())),
        Some("expired") | Some("not_found") => Ok(PollStatus::Expired),
        _ if status >= 500 => Err(ApiError::Server(server_message(&body, "Ошибка сервера"))),
        _ => Ok(PollStatus::Pending),
    }
}

pub async fn me(token: &str) -> Result<User, ApiError> {
    let (status, body) = send(client().get(format!("{}/auth/me", crate::endpoints::site_api())).bearer_auth(token)).await?;
    match status {
        200 => serde_json::from_str::<Value>(&body)
            .ok()
            .and_then(|value| serde_json::from_value(value.get("user")?.clone()).ok())
            .ok_or_else(|| ApiError::Server("Некорректный ответ сервера".into())),
        401 | 403 => Err(ApiError::Unauthorized),
        _ => Err(ApiError::Server(server_message(&body, "Ошибка сервера"))),
    }
}

pub async fn has_subscription(token: &str) -> Result<bool, ApiError> {
    let request = client()
        .get(format!("{}/subscriptions/check-access", crate::endpoints::site_api()))
        .bearer_auth(token);
    let (status, body) = send(request).await?;
    match status {
        200 => Ok(serde_json::from_str::<Value>(&body)
            .ok()
            .and_then(|value| value.get("has_access").and_then(Value::as_bool))
            .unwrap_or(false)),
        401 | 403 => Err(ApiError::Unauthorized),
        _ => Err(ApiError::Server(server_message(&body, "Ошибка проверки подписки"))),
    }
}

pub async fn fetch_key(token: &str, location: &str) -> Result<String, ApiError> {
    let request = client()
        .post(format!("{}/vpn/get-key", crate::endpoints::key_api()))
        .bearer_auth(token)
        .json(&json!({ "location": location }));
    let (status, body) = send(request).await?;
    match status {
        200 => {
            let key = body.trim();
            if key.starts_with("vless://") {
                Ok(key.to_owned())
            } else {
                Err(ApiError::Server("Сервер вернул некорректный ключ".into()))
            }
        }
        401 => Err(ApiError::Unauthorized),
        402 | 403 => Err(ApiError::NoSubscription),
        _ => Err(ApiError::Server(format!("Сервер ключей ответил {status}"))),
    }
}
