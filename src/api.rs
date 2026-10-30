use std::time::Duration;

use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Debug)]
pub enum ApiError {
    Unauthorized,
    NoSubscription,
    NoKeys,
    Server(String),
    Network(String),
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unauthorized => f.write_str("Сессия истекла, войдите снова"),
            Self::NoSubscription => f.write_str("Нет активной подписки"),
            Self::NoKeys => f.write_str("Нет рабочих серверов"),
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

#[derive(Debug, Clone, Deserialize)]
pub struct ServerLocation {
    #[serde(rename = "location")]
    pub id: String,
    pub available: bool,
    #[serde(default)]
    pub score_ms: u64,
}

#[derive(Deserialize)]
struct LocationsResponse {
    locations: Vec<ServerLocation>,
}

#[derive(Deserialize)]
pub struct IssuedKey {
    pub key_id: String,
    pub key: String,
}

pub async fn fetch_locations(token: &str) -> Result<Vec<ServerLocation>, ApiError> {
    let request = client()
        .get(format!("{}/vpn/locations", crate::endpoints::site_api()))
        .bearer_auth(token);
    let (status, body) = send(request).await?;
    match status {
        200 => serde_json::from_str::<LocationsResponse>(&body)
            .map(|response| response.locations.into_iter().filter(|location| location.available).collect())
            .map_err(|_| ApiError::Server("Некорректный ответ сервера".into())),
        401 | 403 => Err(ApiError::Unauthorized),
        _ => Err(ApiError::Server(server_message(&body, "Ошибка получения локаций"))),
    }
}

pub async fn fetch_key(token: &str, location: &str, excluded: &[String]) -> Result<IssuedKey, ApiError> {
    let request = client()
        .post(format!("{}/vpn/connect", crate::endpoints::site_api()))
        .bearer_auth(token)
        .json(&json!({ "location": location, "exclude": excluded }));
    let (status, body) = send(request).await?;
    match status {
        200 => match serde_json::from_str::<IssuedKey>(&body) {
            Ok(issued) if issued.key.starts_with("vless://") => Ok(issued),
            _ => Err(ApiError::Server("Сервер вернул некорректный ключ".into())),
        },
        401 | 403 => Err(ApiError::Unauthorized),
        402 => Err(ApiError::NoSubscription),
        503 => Err(ApiError::NoKeys),
        _ => Err(ApiError::Server(format!("Сервер ключей ответил {status}"))),
    }
}

pub async fn report_failed_key(token: &str, key_id: &str) {
    let request = client()
        .post(format!("{}/vpn/report", crate::endpoints::site_api()))
        .bearer_auth(token)
        .json(&json!({ "key_id": key_id }));
    let _ = send(request).await;
}
