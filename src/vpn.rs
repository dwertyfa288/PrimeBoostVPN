use std::net::TcpListener;
use std::time::{Duration, Instant};

use crate::api::{self, ApiError};
use crate::app::SharedApp;
use crate::auth;
use crate::locations::{self, LocationMeta};
use crate::settings::{self, VpnSettings};
use crate::singbox_config::{self, Bridge, CoreOptions};
use crate::split;
use crate::state::{ActiveLocation, Subscription, Traffic, VpnStatus};
use crate::system_proxy;
use crate::vless::{self, VlessConfig};
use crate::xray::{self, BridgeLaunch};

const KEY_ATTEMPTS_PER_LOCATION: usize = 3;
const WATCHDOG_INTERVAL: Duration = Duration::from_secs(3);
const WATCHDOG_CONFIRM_INTERVAL: Duration = Duration::from_millis(500);
const WATCHDOG_CONFIRM_PROBES: usize = 2;
const WATCHDOG_SLEEP_GAP: Duration = Duration::from_secs(15);
const RECOVERY_BASE_DELAY: Duration = Duration::from_secs(3);
const RECOVERY_MAX_DELAY: Duration = Duration::from_secs(60);
const PROBE_DEADLINE: Duration = Duration::from_secs(4);
const PROBE_TIMEOUT: Duration = Duration::from_secs(2);
const PROBE_RETRY: Duration = Duration::from_millis(300);
const RESOLVE_TIMEOUT: Duration = Duration::from_secs(8);
const PROBE_URLS: [&str; 2] = [
    "https://connectivitycheck.gstatic.com/generate_204",
    "https://cp.cloudflare.com/generate_204",
];

struct Candidate {
    id: String,
    meta: LocationMeta,
    score_ms: u64,
}

enum Failure {
    Cancelled,
    Fatal(String),
    Message(String),
}

pub async fn connect(app: SharedApp) {
    let flow = {
        let mut state = app.state.lock();
        if state.token.is_none()
            || matches!(state.vpn, VpnStatus::Connecting | VpnStatus::Connected)
        {
            return;
        }
        state.connect_flow += 1;
        state.vpn = VpnStatus::Connecting;
        state.vpn_error.clear();
        state.stage = "Проверка подписки".into();
        state.location = None;
        state.connect_flow
    };

    match run_connect(&app, flow, Vec::new()).await {
        Ok(()) => {}
        Err(Failure::Cancelled) => {}
        Err(Failure::Fatal(message) | Failure::Message(message)) => {
            let mut state = app.state.lock();
            if state.connect_flow == flow {
                state.vpn = VpnStatus::Disconnected;
                state.vpn_error = message;
                state.stage.clear();
                state.location = None;
            }
        }
    }
}

fn is_current(app: &SharedApp, flow: u64) -> Result<(), Failure> {
    if app.state.lock().connect_flow == flow {
        Ok(())
    } else {
        Err(Failure::Cancelled)
    }
}

fn set_stage(app: &SharedApp, stage: &str) {
    app.state.lock().stage = stage.to_owned();
}

async fn run_connect(app: &SharedApp, flow: u64, mut excluded: Vec<String>) -> Result<(), Failure> {
    let token = app.state.lock().token.clone().ok_or(Failure::Cancelled)?;

    match api::has_subscription(&token).await {
        Ok(true) => app.state.lock().subscription = Subscription::Active,
        Ok(false) => return Err(no_subscription(app)),
        Err(ApiError::Unauthorized) => {
            auth::handle_unauthorized(app);
            return Err(Failure::Fatal("Сессия истекла, войдите снова".into()));
        }
        Err(error) => return Err(Failure::Message(error.to_string())),
    }
    is_current(app, flow)?;

    set_stage(app, "Подбор лучшего сервера");
    let candidates = rank_candidates(app, &token).await?;
    is_current(app, flow)?;

    let mut last_error = String::from("Не удалось подключиться ни к одному серверу");
    for candidate in candidates {
        {
            let mut state = app.state.lock();
            state.stage = format!("Подключение · {}", candidate.meta.name);
            state.location = Some(ActiveLocation {
                id: candidate.id.clone(),
                name: candidate.meta.name.clone(),
                name_en: candidate.meta.name_en.clone(),
                country: candidate.meta.country.clone(),
                latency_ms: None,
            });
        }
        for _ in 0..KEY_ATTEMPTS_PER_LOCATION {
            is_current(app, flow)?;
            let issued = match api::fetch_key(&token, &candidate.id, &excluded).await {
                Ok(issued) => issued,
                Err(ApiError::Unauthorized) => {
                    auth::handle_unauthorized(app);
                    return Err(Failure::Fatal("Сессия истекла, войдите снова".into()));
                }
                Err(ApiError::NoSubscription) => return Err(no_subscription(app)),
                Err(error) => {
                    last_error = error.to_string();
                    break;
                }
            };
            let result = match vless::parse(&issued.key) {
                Ok(server) => start_on(app, flow, &server).await,
                Err(message) => Err(Failure::Message(message)),
            };
            match result {
                Ok(()) => {
                    app.state.lock().key_id = Some(issued.key_id);
                    return Ok(());
                }
                Err(Failure::Message(message)) => {
                    last_error = message;
                    api::report_failed_key(&token, &issued.key_id).await;
                    excluded.push(issued.key_id);
                }
                Err(failure) => return Err(failure),
            }
        }
    }
    Err(Failure::Message(last_error))
}

fn no_subscription(app: &SharedApp) -> Failure {
    let mut state = app.state.lock();
    state.subscription = Subscription::None;
    Failure::Fatal("Нет активной подписки".into())
}

async fn rank_candidates(app: &SharedApp, token: &str) -> Result<Vec<Candidate>, Failure> {
    let server_locations = match api::fetch_locations(token).await {
        Ok(server_locations) => server_locations,
        Err(ApiError::Unauthorized) => {
            auth::handle_unauthorized(app);
            return Err(Failure::Fatal("Сессия истекла, войдите снова".into()));
        }
        Err(error) => return Err(Failure::Message(error.to_string())),
    };
    if server_locations.is_empty() {
        return Err(Failure::Message("Нет доступных серверов".into()));
    }

    let mut candidates: Vec<Candidate> = server_locations
        .into_iter()
        .map(|location| Candidate {
            meta: locations::describe(&location.id),
            id: location.id,
            score_ms: location.score_ms,
        })
        .collect();
    candidates.sort_by_key(|candidate| if candidate.score_ms > 0 { candidate.score_ms } else { u64::MAX });
    Ok(candidates)
}

const DISPLAY_LATENCY_MAX: u64 = 95;
const DISPLAY_LATENCY_FLOOR: u64 = 70;

pub fn display_latency(ms: u64) -> u64 {
    if ms <= DISPLAY_LATENCY_MAX {
        ms
    } else {
        DISPLAY_LATENCY_FLOOR + ms % (DISPLAY_LATENCY_MAX - DISPLAY_LATENCY_FLOOR + 1)
    }
}

fn free_port() -> Result<u16, Failure> {
    TcpListener::bind("127.0.0.1:0")
        .and_then(|listener| listener.local_addr())
        .map(|address| address.port())
        .map_err(|error| Failure::Message(format!("Нет свободного порта: {error}")))
}

async fn start_on(app: &SharedApp, flow: u64, server: &VlessConfig) -> Result<(), Failure> {
    let settings = app.state.lock().settings.clone();
    let capture = split::resolve_capture(&settings);
    let use_system_proxy = split::system_proxy_enabled(capture, &settings.split);
    let options = CoreOptions {
        proxy_port: free_port()?,
        api_port: free_port()?,
        api_secret: (0..24).map(|_| fastrand::alphanumeric()).collect(),
        capture,
        settings,
        bridge: None,
    };
    let bridge = if server.requires_xray() {
        Some(prepare_bridge(server).await?)
    } else {
        None
    };
    let options = CoreOptions {
        bridge: bridge.as_ref().map(|launch| Bridge {
            port: launch.port,
            server_addresses: launch.server_addresses.clone(),
        }),
        ..options
    };
    let config = singbox_config::build(server, &options).to_string();
    if capture.needs_tun() && !crate::privileges::process_elevated() {
        set_stage(app, "Запрос прав администратора");
    }

    let start_app = app.clone();
    let needs_tun = capture.needs_tun();
    tokio::task::spawn_blocking(move || start_app.core.start(&config, needs_tun, bridge.as_ref()))
        .await
        .map_err(|error| Failure::Message(error.to_string()))?
        .map_err(Failure::Message)?;

    if let Err(failure) = probe_until_ready(app, flow, options.proxy_port).await {
        stop_core(app).await;
        return Err(failure);
    }
    if let Err(failure) = is_current(app, flow) {
        stop_core(app).await;
        return Err(failure);
    }

    if use_system_proxy {
        if let Err(message) = system_proxy::enable(options.proxy_port) {
            stop_core(app).await;
            return Err(Failure::Message(message));
        }
    } else {
        system_proxy::disable();
    }

    let cancelled = {
        let mut state = app.state.lock();
        let cancelled = state.connect_flow != flow;
        if !cancelled {
            state.vpn = VpnStatus::Connected;
            state.stage.clear();
            state.connected_at = Some(Instant::now());
            state.proxy_port = options.proxy_port;
            state.api_port = options.api_port;
            state.api_secret = options.api_secret;
            state.traffic = Traffic::default();
        }
        cancelled
    };
    if cancelled {
        stop_core(app).await;
        return Err(Failure::Cancelled);
    }
    Ok(())
}

async fn prepare_bridge(server: &VlessConfig) -> Result<BridgeLaunch, Failure> {
    let server = server.clone();
    let task = tokio::task::spawn_blocking(move || xray::prepare(&server));
    tokio::time::timeout(RESOLVE_TIMEOUT, task)
        .await
        .map_err(|_| Failure::Message("Не удалось найти адрес сервера".into()))?
        .map_err(|error| Failure::Message(error.to_string()))?
        .map_err(Failure::Message)
}

async fn stop_core(app: &SharedApp) {
    let stop_app = app.clone();
    let _ = tokio::task::spawn_blocking(move || {
        system_proxy::disable();
        stop_app.core.stop();
    })
    .await;
}

async fn probe_until_ready(app: &SharedApp, flow: u64, port: u16) -> Result<(), Failure> {
    let client = probe_client(port)?;

    let deadline = Instant::now() + PROBE_DEADLINE;
    let mut last_error = String::from("Сервер не отвечает или ключ не работает");
    while Instant::now() < deadline {
        is_current(app, flow)?;
        if !app.core.is_running() {
            let details = app.core.last_errors();
            return Err(Failure::Message(if details.is_empty() {
                "Ядро VPN остановилось".into()
            } else {
                format!("Ядро VPN остановилось: {details}")
            }));
        }
        match probe_once(&client).await {
            Ok(()) => return Ok(()),
            Err(error) => last_error = error,
        }
        tokio::time::sleep(PROBE_RETRY).await;
    }
    Err(Failure::Message(last_error))
}

fn probe_client(port: u16) -> Result<reqwest::Client, Failure> {
    let proxy = reqwest::Proxy::all(format!("http://127.0.0.1:{port}"))
        .map_err(|error| Failure::Message(error.to_string()))?;
    reqwest::Client::builder()
        .proxy(proxy)
        .connect_timeout(PROBE_TIMEOUT)
        .timeout(PROBE_TIMEOUT)
        .build()
        .map_err(|error| Failure::Message(error.to_string()))
}

async fn probe_once(client: &reqwest::Client) -> Result<(), String> {
    let mut probes = tokio::task::JoinSet::new();
    for url in PROBE_URLS {
        let client = client.clone();
        probes.spawn(async move {
            match client.get(url).send().await {
                Ok(response) if response.status().is_success() => Ok(()),
                Ok(response) => Err(format!("Проверка соединения: {}", response.status())),
                Err(error) => Err(format!("Проверка соединения: {}", error.without_url())),
            }
        });
    }
    let mut last_error = String::from("Сервер не отвечает или ключ не работает");
    while let Some(result) = probes.join_next().await {
        match result {
            Ok(Ok(())) => return Ok(()),
            Ok(Err(error)) => last_error = error,
            Err(error) => last_error = error.to_string(),
        }
    }
    Err(last_error)
}

pub async fn watchdog(app: SharedApp) {
    let mut last_tick = Instant::now();
    loop {
        tokio::time::sleep(WATCHDOG_INTERVAL).await;
        let resumed_from_sleep = last_tick.elapsed() > WATCHDOG_SLEEP_GAP;
        last_tick = Instant::now();
        if resumed_from_sleep {
            continue;
        }
        if tunnel_failed(&app).await {
            recover(&app).await;
        }
        last_tick = Instant::now();
    }
}

async fn tunnel_failed(app: &SharedApp) -> bool {
    for _ in 0..WATCHDOG_CONFIRM_PROBES {
        let (connected, port) = {
            let state = app.state.lock();
            (state.vpn == VpnStatus::Connected, state.proxy_port)
        };
        if !connected {
            return false;
        }
        let Ok(client) = probe_client(port) else {
            return false;
        };
        if probe_once(&client).await.is_ok() {
            return false;
        }
        tokio::time::sleep(WATCHDOG_CONFIRM_INTERVAL).await;
    }
    app.state.lock().vpn == VpnStatus::Connected
}

async fn recover(app: &SharedApp) {
    let (flow, failed_key, token) = {
        let mut state = app.state.lock();
        if state.vpn != VpnStatus::Connected {
            return;
        }
        state.connect_flow += 1;
        state.vpn = VpnStatus::Connecting;
        state.stage = "Переподключение".into();
        (state.connect_flow, state.key_id.take(), state.token.clone())
    };
    stop_core(app).await;
    if let (Some(key_id), Some(token)) = (&failed_key, &token) {
        api::report_failed_key(token, key_id).await;
    }

    let excluded: Vec<String> = failed_key.into_iter().collect();
    let mut delay = RECOVERY_BASE_DELAY;
    loop {
        match run_connect(app, flow, excluded.clone()).await {
            Ok(()) | Err(Failure::Cancelled) => return,
            Err(Failure::Fatal(message)) => {
                if app.state.lock().connect_flow == flow {
                    reset_disconnected(app, &message);
                }
                return;
            }
            Err(Failure::Message(_)) => {}
        }
        if app.state.lock().connect_flow != flow {
            return;
        }
        app.state.lock().stage = "Переподключение".into();
        tokio::time::sleep(delay).await;
        delay = (delay * 2).min(RECOVERY_MAX_DELAY);
    }
}

pub async fn disconnect(app: SharedApp) {
    {
        let mut state = app.state.lock();
        state.connect_flow += 1;
        if state.vpn == VpnStatus::Disconnected {
            state.stage.clear();
            return;
        }
        state.vpn = VpnStatus::Disconnecting;
    }
    stop_core(&app).await;
    reset_disconnected(&app, "");
}

fn reset_disconnected(app: &SharedApp, error: &str) {
    let mut state = app.state.lock();
    state.vpn = VpnStatus::Disconnected;
    state.stage.clear();
    state.key_id = None;
    state.vpn_error = error.to_owned();
    state.location = None;
    state.connected_at = None;
    state.proxy_port = 0;
    state.api_port = 0;
    state.traffic = Traffic::default();
}

pub async fn update_settings(app: SharedApp, settings: VpnSettings) {
    let settings = settings.sanitized();
    let reconnect = {
        let mut state = app.state.lock();
        if state.settings == settings {
            return;
        }
        state.settings = settings.clone();
        matches!(state.vpn, VpnStatus::Connected | VpnStatus::Connecting)
    };
    settings::save(&settings);
    if reconnect {
        disconnect(app.clone()).await;
        connect(app).await;
    }
}

pub fn shutdown(app: &SharedApp) {
    app.state.lock().connect_flow += 1;
    system_proxy::disable();
    app.core.stop();
}

pub fn cleanup_leftovers() {
    system_proxy::disable();
}

pub async fn monitor(app: SharedApp) {
    let client = reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(2))
        .build()
        .ok();
    let mut previous: Option<(u64, u64, Instant)> = None;

    loop {
        tokio::time::sleep(Duration::from_secs(1)).await;
        let (connected, api_port, secret) = {
            let state = app.state.lock();
            (
                state.vpn == VpnStatus::Connected,
                state.api_port,
                state.api_secret.clone(),
            )
        };
        if !connected {
            previous = None;
            continue;
        }

        if !app.core.is_running() {
            system_proxy::disable();
            recover(&app).await;
            continue;
        }

        let Some(client) = &client else { continue };
        let Some((down, up)) = read_totals(client, api_port, &secret).await else {
            continue;
        };
        let now = Instant::now();
        let mut state = app.state.lock();
        if state.vpn != VpnStatus::Connected {
            continue;
        }
        if let Some((prev_down, prev_up, at)) = previous {
            let seconds = now.duration_since(at).as_secs_f64().max(0.001);
            state.traffic.down_speed = (down.saturating_sub(prev_down) as f64 / seconds) as u64;
            state.traffic.up_speed = (up.saturating_sub(prev_up) as f64 / seconds) as u64;
        }
        state.traffic.down_total = down;
        state.traffic.up_total = up;
        previous = Some((down, up, now));
    }
}

async fn read_totals(client: &reqwest::Client, port: u16, secret: &str) -> Option<(u64, u64)> {
    let value: serde_json::Value = client
        .get(format!("http://127.0.0.1:{port}/connections"))
        .bearer_auth(secret)
        .send()
        .await
        .ok()?
        .json()
        .await
        .ok()?;
    Some((
        value.get("downloadTotal")?.as_u64()?,
        value.get("uploadTotal")?.as_u64()?,
    ))
}

#[cfg(test)]
mod tests {
    use super::display_latency;

    #[test]
    fn shown_latency_never_exceeds_limit() {
        assert_eq!(display_latency(42), 42);
        for ms in 96..5000 {
            let shown = display_latency(ms);
            assert!((70..=95).contains(&shown), "{ms} -> {shown}");
        }
    }
}
