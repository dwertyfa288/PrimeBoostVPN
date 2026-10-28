use std::time::Duration;

use crate::api::{self, ApiError, AuthResult, PollStatus};
use crate::app::SharedApp;
use crate::browser;
use crate::session;
use crate::state::{AuthStatus, Subscription};
use crate::vpn;

const POLL_INTERVAL: Duration = Duration::from_secs(2);
const TELEGRAM_POLLS: u32 = 300;
const GOOGLE_POLLS: u32 = 150;

pub async fn restore(app: SharedApp) {
    let Some(stored) = session::load() else {
        app.state.lock().auth = AuthStatus::SignedOut;
        return;
    };
    {
        let mut state = app.state.lock();
        state.token = Some(stored.token.clone());
        state.email = stored.email.clone();
        state.auth = AuthStatus::SignedIn;
    }
    match api::me(&stored.token).await {
        Ok(user) => {
            app.state.lock().email = user.email;
            app.persist_session();
            refresh_subscription(&app).await;
        }
        Err(ApiError::Unauthorized) => sign_out_local(&app, "Сессия истекла, войдите снова"),
        Err(_) => refresh_subscription(&app).await,
    }
}

pub async fn refresh_subscription(app: &SharedApp) {
    let Some(token) = app.state.lock().token.clone() else {
        return;
    };
    match api::has_subscription(&token).await {
        Ok(active) => {
            app.state.lock().subscription = if active {
                Subscription::Active
            } else {
                Subscription::None
            };
            if active {
                let mut state = app.state.lock();
                state.trial.available = false;
                state.trial.checked = true;
            } else {
                crate::trial::refresh(app).await;
            }
        }
        Err(ApiError::Unauthorized) => sign_out_local(app, "Сессия истекла, войдите снова"),
        Err(_) => {}
    }
}

fn begin_flow(app: &SharedApp, status: AuthStatus) -> u64 {
    let mut state = app.state.lock();
    state.auth_flow += 1;
    state.auth = status;
    state.auth_error.clear();
    state.auth_link.clear();
    state.auth_flow
}

fn flow_active(app: &SharedApp, flow: u64) -> bool {
    app.state.lock().auth_flow == flow
}

fn fail_flow(app: &SharedApp, flow: u64, message: String) {
    let mut state = app.state.lock();
    if state.auth_flow == flow {
        state.auth = AuthStatus::SignedOut;
        state.auth_error = message;
        state.auth_link.clear();
    }
}

async fn complete(app: &SharedApp, flow: u64, result: AuthResult) {
    {
        let mut state = app.state.lock();
        if state.auth_flow != flow {
            return;
        }
        state.token = Some(result.token);
        state.email = result.user.email;
        state.auth = AuthStatus::SignedIn;
        state.auth_error.clear();
        state.auth_link.clear();
        state.subscription = Subscription::Unknown;
    }
    app.persist_session();
    refresh_subscription(app).await;
}

pub async fn login_google(app: SharedApp) {
    let flow = begin_flow(&app, AuthStatus::WaitingGoogle);
    let hash = format!("desktop_{}", fastrand::u64(..));
    let link = format!("{}/auth/google?app_hash={hash}", crate::endpoints::site_url());
    app.state.lock().auth_link = link.clone();
    let _ = browser::open(&link);

    for _ in 0..GOOGLE_POLLS {
        tokio::time::sleep(POLL_INTERVAL).await;
        if !flow_active(&app, flow) {
            return;
        }
        match api::desktop_auth_status(&hash).await {
            Ok(PollStatus::Done(result)) => return complete(&app, flow, result).await,
            Ok(_) | Err(ApiError::Network(_)) => {}
            Err(error) => return fail_flow(&app, flow, error.to_string()),
        }
    }
    fail_flow(&app, flow, "Время ожидания истекло".into());
}

pub async fn login_telegram(app: SharedApp) {
    let flow = begin_flow(&app, AuthStatus::WaitingTelegram);
    let (code, link) = match api::telegram_start().await {
        Ok(started) => started,
        Err(error) => return fail_flow(&app, flow, error.to_string()),
    };
    app.state.lock().auth_link = link.clone();
    let _ = browser::open(&link);

    for _ in 0..TELEGRAM_POLLS {
        tokio::time::sleep(POLL_INTERVAL).await;
        if !flow_active(&app, flow) {
            return;
        }
        match api::telegram_status(&code).await {
            Ok(PollStatus::Done(result)) => return complete(&app, flow, result).await,
            Ok(PollStatus::Expired) => {
                return fail_flow(&app, flow, "Ссылка устарела, попробуйте снова".into());
            }
            Ok(PollStatus::Pending) | Err(ApiError::Network(_)) => {}
            Err(error) => return fail_flow(&app, flow, error.to_string()),
        }
    }
    fail_flow(&app, flow, "Время ожидания истекло".into());
}

pub fn cancel(app: &SharedApp) {
    let mut state = app.state.lock();
    state.auth_flow += 1;
    state.auth_link.clear();
    if state.auth != AuthStatus::SignedIn {
        state.auth = AuthStatus::SignedOut;
    }
}

pub async fn logout(app: SharedApp) {
    vpn::disconnect(app.clone()).await;
    sign_out_local(&app, "");
}

fn sign_out_local(app: &SharedApp, message: &str) {
    session::clear();
    let mut state = app.state.lock();
    state.auth_flow += 1;
    state.token = None;
    state.email.clear();
    state.auth = AuthStatus::SignedOut;
    state.auth_error = message.to_owned();
    state.auth_link.clear();
    state.subscription = Subscription::Unknown;
    state.billing = Default::default();
    state.trial = Default::default();
}

pub fn handle_unauthorized(app: &SharedApp) {
    sign_out_local(app, "Сессия истекла, войдите снова");
}
