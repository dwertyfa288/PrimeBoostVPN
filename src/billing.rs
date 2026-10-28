use std::time::{Duration, Instant};

use serde::Serialize;

use crate::api::ApiError;
use crate::app::SharedApp;
use crate::auth;
use crate::billing_api::{self, Account, Plan, Requisites};
use crate::browser;

const POLL_INTERVAL: Duration = Duration::from_secs(3);
const REQUISITES_DEADLINE: Duration = Duration::from_secs(20);
const PAYMENT_DEADLINE: Duration = Duration::from_secs(30 * 60);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckoutStage {
    Creating,
    Preparing,
    Ready,
    Link,
    Confirming,
    Success,
    Failed,
}

#[derive(Debug, Clone, Serialize)]
pub struct CheckoutView {
    pub plan_id: i64,
    pub plan_name: String,
    pub method: String,
    pub amount: f64,
    pub stage: CheckoutStage,
    pub qr: String,
    pub qr_image: String,
    pub has_link: bool,
    pub error: String,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct BillingView {
    pub plans: Vec<Plan>,
    pub account: Option<Account>,
    pub loading: bool,
    pub error: String,
    pub checkout: Option<CheckoutView>,
}

#[derive(Default)]
pub struct BillingState {
    pub view: BillingView,
    pub flow: u64,
    pub link: Option<String>,
}

fn token(app: &SharedApp) -> Option<String> {
    app.state.lock().token.clone()
}

fn handle_error(app: &SharedApp, error: &ApiError) {
    if matches!(error, ApiError::Unauthorized) {
        auth::handle_unauthorized(app);
    }
}

pub async fn load(app: SharedApp) {
    app.state.lock().billing.view.loading = true;
    let plans = billing_api::plans().await;
    let account = match token(&app) {
        Some(token) => Some(billing_api::account(&token).await),
        None => None,
    };

    let mut state = app.state.lock();
    let billing = &mut state.billing.view;
    billing.loading = false;
    billing.error.clear();
    match plans {
        Ok(plans) => billing.plans = plans,
        Err(error) => billing.error = error.to_string(),
    }
    match account {
        Some(Ok(account)) => billing.account = Some(account),
        Some(Err(error)) => {
            billing.error = error.to_string();
            let unauthorized = matches!(error, ApiError::Unauthorized);
            drop(state);
            if unauthorized {
                auth::handle_unauthorized(&app);
            }
        }
        None => {}
    }
}

pub async fn quote(app: &SharedApp, plan_id: i64, method: &str) -> Result<f64, String> {
    let token = token(app).ok_or("Войдите в аккаунт")?;
    billing_api::quote(&token, plan_id, method).await.map_err(|error| {
        handle_error(app, &error);
        error.to_string()
    })
}

fn update<F: FnOnce(&mut CheckoutView)>(app: &SharedApp, flow: u64, change: F) -> bool {
    let mut state = app.state.lock();
    if state.billing.flow != flow {
        return false;
    }
    if let Some(checkout) = state.billing.view.checkout.as_mut() {
        change(checkout);
    }
    true
}

fn fail(app: &SharedApp, flow: u64, error: ApiError) {
    handle_error(app, &error);
    update(app, flow, |checkout| {
        checkout.stage = CheckoutStage::Failed;
        checkout.error = error.to_string();
    });
}

pub async fn buy(app: SharedApp, plan_id: i64, method: String) {
    let Some(token) = token(&app) else { return };
    let plan_name = app
        .state
        .lock()
        .billing
        .view
        .plans
        .iter()
        .find(|plan| plan.id == plan_id)
        .map(|plan| plan.name.clone())
        .unwrap_or_default();
    let flow = {
        let mut state = app.state.lock();
        state.billing.flow += 1;
        state.billing.link = None;
        state.billing.view.checkout = Some(CheckoutView {
            plan_id,
            plan_name,
            method: method.clone(),
            amount: 0.0,
            stage: CheckoutStage::Creating,
            qr: String::new(),
            qr_image: String::new(),
            has_link: false,
            error: String::new(),
        });
        state.billing.flow
    };

    if method == "balance" {
        match billing_api::pay_from_balance(&token, plan_id).await {
            Ok(()) => finish(&app, flow).await,
            Err(error) => fail(&app, flow, error),
        }
        return;
    }

    let checkout = match billing_api::create_payment(&token, plan_id, &method).await {
        Ok(checkout) => checkout,
        Err(error) => return fail(&app, flow, error),
    };
    {
        let mut state = app.state.lock();
        if state.billing.flow != flow {
            return;
        }
        state.billing.link = checkout.payment_url.clone();
        if let Some(view) = state.billing.view.checkout.as_mut() {
            view.amount = checkout.amount;
            view.stage = CheckoutStage::Preparing;
            view.has_link = checkout.payment_url.is_some();
        }
    }

    if !wait_requisites(&app, flow, &token, &checkout.order_id).await {
        return;
    }
    wait_payment(&app, flow, &token, &checkout.order_id).await;
}

async fn wait_requisites(app: &SharedApp, flow: u64, token: &str, order_id: &str) -> bool {
    let started = Instant::now();
    loop {
        match billing_api::requisites(token, order_id).await {
            Ok(Requisites::Ready { qr, qr_image, payment_link, amount }) => {
                let mut state = app.state.lock();
                if state.billing.flow != flow {
                    return false;
                }
                if payment_link.is_some() {
                    state.billing.link = payment_link;
                }
                let has_link = state.billing.link.is_some();
                if let Some(view) = state.billing.view.checkout.as_mut() {
                    view.stage = CheckoutStage::Ready;
                    view.qr = qr;
                    view.qr_image = qr_image;
                    view.has_link = has_link;
                    if amount > 0.0 {
                        view.amount = amount;
                    }
                }
                return true;
            }
            Ok(Requisites::Paid) => {
                return update(app, flow, |view| view.stage = CheckoutStage::Confirming);
            }
            Ok(Requisites::Preparing { payment_url }) => {
                if let Some(url) = payment_url {
                    let mut state = app.state.lock();
                    if state.billing.flow == flow {
                        state.billing.link = Some(url);
                        if let Some(view) = state.billing.view.checkout.as_mut() {
                            view.has_link = true;
                        }
                    }
                }
            }
            Err(ApiError::Network(_)) => {}
            Err(error) => {
                let has_link = app.state.lock().billing.link.is_some();
                if has_link && !matches!(error, ApiError::Unauthorized) {
                    return update(app, flow, |view| view.stage = CheckoutStage::Link);
                }
                fail(app, flow, error);
                return false;
            }
        }
        if started.elapsed() > REQUISITES_DEADLINE {
            let has_link = app.state.lock().billing.link.is_some();
            if has_link {
                return update(app, flow, |view| view.stage = CheckoutStage::Link);
            }
            fail(app, flow, ApiError::Server("Реквизиты оплаты не готовы, попробуйте ещё раз".into()));
            return false;
        }
        tokio::time::sleep(POLL_INTERVAL).await;
        if app.state.lock().billing.flow != flow {
            return false;
        }
    }
}

async fn wait_payment(app: &SharedApp, flow: u64, token: &str, order_id: &str) {
    let started = Instant::now();
    while started.elapsed() < PAYMENT_DEADLINE {
        tokio::time::sleep(POLL_INTERVAL).await;
        if app.state.lock().billing.flow != flow {
            return;
        }
        match billing_api::payment_status(token, order_id).await {
            Ok(status) if status == "success" => return finish(app, flow).await,
            Ok(_) | Err(ApiError::Network(_)) => {}
            Err(error) => return fail(app, flow, error),
        }
    }
    fail(app, flow, ApiError::Server("Время ожидания оплаты истекло".into()));
}

async fn finish(app: &SharedApp, flow: u64) {
    if !update(app, flow, |view| view.stage = CheckoutStage::Success) {
        return;
    }
    auth::refresh_subscription(app).await;
    if let Some(token) = token(app) {
        if let Ok(account) = billing_api::account(&token).await {
            app.state.lock().billing.view.account = Some(account);
        }
    }
}

pub fn cancel(app: &SharedApp) {
    let mut state = app.state.lock();
    state.billing.flow += 1;
    state.billing.link = None;
    state.billing.view.checkout = None;
}

pub fn open_payment(app: &SharedApp) -> Result<(), String> {
    let link = app.state.lock().billing.link.clone().ok_or("Ссылка оплаты ещё не готова")?;
    browser::open_payment(&link)
}
