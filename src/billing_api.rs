use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::api::{ApiError, client, send, server_message};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Plan {
    pub id: i64,
    pub name: String,
    pub price: f64,
    pub duration_days: i64,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub is_recommended: i64,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct Account {
    pub balance: f64,
    pub discount: f64,
    pub plan_name: Option<String>,
    pub remaining_days: Option<i64>,
    pub expires_at: Option<String>,
}

pub struct Checkout {
    pub order_id: String,
    pub payment_url: Option<String>,
    pub amount: f64,
}

pub enum Requisites {
    Preparing { payment_url: Option<String> },
    Ready { qr: String, qr_image: String, payment_link: Option<String>, amount: f64 },
    Paid,
}

fn check_auth(status: u16) -> Result<(), ApiError> {
    if status == 401 || status == 403 {
        Err(ApiError::Unauthorized)
    } else {
        Ok(())
    }
}

fn json_body(body: &str) -> Value {
    serde_json::from_str(body).unwrap_or(Value::Null)
}

pub async fn plans() -> Result<Vec<Plan>, ApiError> {
    let (status, body) = send(client().get(format!("{}/subscriptions/plans", crate::endpoints::site_api()))).await?;
    if status != 200 {
        return Err(ApiError::Server(server_message(&body, "Не удалось загрузить тарифы")));
    }
    let value = json_body(&body);
    serde_json::from_value(value.get("plans").cloned().unwrap_or(Value::Null))
        .map_err(|_| ApiError::Server("Некорректный список тарифов".into()))
}

pub async fn account(token: &str) -> Result<Account, ApiError> {
    let (status, body) = send(client().get(format!("{}/auth/me", crate::endpoints::site_api())).bearer_auth(token)).await?;
    check_auth(status)?;
    let user = json_body(&body).get("user").cloned().unwrap_or(Value::Null);
    let mut account = Account {
        balance: user.get("balance").and_then(Value::as_f64).unwrap_or(0.0),
        discount: user.get("active_discount").and_then(Value::as_f64).unwrap_or(0.0),
        ..Default::default()
    };

    let (status, body) = send(client().get(format!("{}/subscriptions/current", crate::endpoints::site_api())).bearer_auth(token)).await?;
    check_auth(status)?;
    if let Some(subscription) = json_body(&body).get("subscription").filter(|value| value.is_object()) {
        account.plan_name = subscription.get("plan_name").and_then(Value::as_str).map(str::to_owned);
        account.remaining_days = subscription.get("remaining_days").and_then(Value::as_i64);
        account.expires_at = subscription.get("expires_at").and_then(Value::as_str).map(str::to_owned);
    }
    Ok(account)
}

pub async fn quote(token: &str, plan_id: i64, method: &str) -> Result<f64, ApiError> {
    let method = if method == "crypto" { "crypto" } else { "sbp" };
    let request = client()
        .get(format!("{}/payment/quote/{plan_id}?method={method}", crate::endpoints::site_api()))
        .bearer_auth(token);
    let (status, body) = send(request).await?;
    check_auth(status)?;
    if status != 200 {
        return Err(ApiError::Server(server_message(&body, "Не удалось рассчитать стоимость")));
    }
    json_body(&body)
        .get("amount")
        .and_then(Value::as_f64)
        .ok_or_else(|| ApiError::Server("Сервер не вернул стоимость".into()))
}

pub async fn pay_from_balance(token: &str, plan_id: i64) -> Result<(), ApiError> {
    let request = client()
        .post(format!("{}/subscriptions/subscribe", crate::endpoints::site_api()))
        .bearer_auth(token)
        .json(&json!({ "plan_id": plan_id }));
    let (status, body) = send(request).await?;
    check_auth(status)?;
    if (200..300).contains(&status) {
        Ok(())
    } else {
        Err(ApiError::Server(server_message(&body, "Не удалось оплатить с баланса")))
    }
}

pub async fn create_payment(token: &str, plan_id: i64, method: &str) -> Result<Checkout, ApiError> {
    let request = client()
        .post(format!("{}/payment/create", crate::endpoints::site_api()))
        .bearer_auth(token)
        .json(&json!({ "purpose": "plan", "plan_id": plan_id, "method": method, "source": "astra" }));
    let (status, body) = send(request).await?;
    check_auth(status)?;
    let value = json_body(&body);
    if !(200..300).contains(&status) {
        return Err(ApiError::Server(server_message(&body, "Не удалось создать платёж")));
    }
    let order_id = value
        .get("order_id")
        .and_then(Value::as_str)
        .ok_or_else(|| ApiError::Server("Сервер не вернул номер заказа".into()))?;
    Ok(Checkout {
        order_id: order_id.to_owned(),
        payment_url: value.get("payment_url").and_then(Value::as_str).map(str::to_owned),
        amount: value.get("amount").and_then(Value::as_f64).unwrap_or(0.0),
    })
}

pub async fn requisites(token: &str, order_id: &str) -> Result<Requisites, ApiError> {
    let request = client()
        .get(format!("{}/payment/h2h/{order_id}", crate::endpoints::site_api()))
        .bearer_auth(token);
    let (status, body) = send(request).await?;
    check_auth(status)?;
    let value = json_body(&body);
    let text = |key: &str| value.get(key).and_then(Value::as_str).map(str::to_owned);
    match value.get("status").and_then(Value::as_str) {
        Some("paid") => Ok(Requisites::Paid),
        Some("ready") => Ok(Requisites::Ready {
            qr: text("qr").unwrap_or_default(),
            qr_image: text("qr_image").filter(|image| image.starts_with("data:image/")).unwrap_or_default(),
            payment_link: text("payment_link"),
            amount: value.get("amount").and_then(Value::as_f64).unwrap_or(0.0),
        }),
        Some("preparing") => Ok(Requisites::Preparing { payment_url: text("payment_url") }),
        _ => Err(ApiError::Server(server_message(&body, "Реквизиты оплаты недоступны"))),
    }
}

pub async fn payment_status(token: &str, order_id: &str) -> Result<String, ApiError> {
    let request = client()
        .get(format!("{}/payment/check/{order_id}", crate::endpoints::site_api()))
        .bearer_auth(token);
    let (status, body) = send(request).await?;
    check_auth(status)?;
    if status != 200 {
        return Err(ApiError::Server(server_message(&body, "Платёж не найден")));
    }
    Ok(json_body(&body)
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or("pending")
        .to_owned())
}
