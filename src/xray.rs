use std::collections::VecDeque;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{IpAddr, SocketAddr, TcpListener, TcpStream, ToSocketAddrs};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::time::{Duration, Instant};

use parking_lot::Mutex;
use serde_json::{Value, json};

use crate::vless::VlessConfig;

const STARTUP_TIMEOUT: Duration = Duration::from_secs(5);
const SERVER_POINTERS: [(&str, &str); 2] = [
    ("/outbounds/0/settings/vnext/0", "/outbounds/0/streamSettings"),
    (
        "/outbounds/0/streamSettings/xhttpSettings/extra/downloadSettings",
        "/outbounds/0/streamSettings/xhttpSettings/extra/downloadSettings",
    ),
];

pub struct BridgeLaunch {
    pub config: String,
    pub port: u16,
    pub server_addresses: Vec<String>,
}

pub struct XrayBridge {
    child: Child,
}

impl XrayBridge {
    pub fn start(
        binary: &Path,
        launch: &BridgeLaunch,
        log: Arc<Mutex<VecDeque<String>>>,
        log_limit: usize,
    ) -> Result<Self, String> {
        let mut command = Command::new(binary);
        command
            .args(["run", "-config", "stdin:"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        crate::core::hide_window(&mut command);
        let mut child = command
            .spawn()
            .map_err(|error| format!("Не удалось запустить ядро XHTTP: {error}"))?;

        let written = child
            .stdin
            .take()
            .ok_or_else(|| "Нет доступа к stdin ядра XHTTP".to_owned())
            .and_then(|mut stdin| {
                stdin
                    .write_all(launch.config.as_bytes())
                    .map_err(|error| format!("Не удалось передать конфигурацию: {error}"))
            });
        let mut bridge = Self { child };
        written?;
        if let Some(stdout) = bridge.child.stdout.take() {
            forward_log(stdout, log.clone(), log_limit);
        }
        if let Some(stderr) = bridge.child.stderr.take() {
            forward_log(stderr, log, log_limit);
        }

        let address = SocketAddr::from(([127, 0, 0, 1], launch.port));
        let started = Instant::now();
        loop {
            if !bridge.is_running() {
                return Err("Ядро XHTTP завершилось при запуске".into());
            }
            if TcpStream::connect_timeout(&address, Duration::from_millis(100)).is_ok() {
                return Ok(bridge);
            }
            if started.elapsed() > STARTUP_TIMEOUT {
                return Err("Ядро XHTTP не открыло локальный порт".into());
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    pub fn pid(&self) -> u32 {
        self.child.id()
    }

    pub fn is_running(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
    }
}

impl Drop for XrayBridge {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn forward_log(reader: impl Read + Send + 'static, log: Arc<Mutex<VecDeque<String>>>, limit: usize) {
    std::thread::spawn(move || {
        for line in BufReader::new(reader).lines().map_while(Result::ok) {
            let mut log = log.lock();
            log.push_back(line);
            if log.len() > limit {
                log.pop_front();
            }
        }
    });
}

pub fn prepare(server: &VlessConfig) -> Result<BridgeLaunch, String> {
    let port = TcpListener::bind("127.0.0.1:0")
        .and_then(|listener| listener.local_addr())
        .map(|address| address.port())
        .map_err(|error| format!("Нет свободного порта: {error}"))?;
    let mut config = build_config(server, port);
    let server_addresses = resolve_server_addresses(&mut config)?;
    Ok(BridgeLaunch { config: config.to_string(), port, server_addresses })
}

pub fn build_config(server: &VlessConfig, port: u16) -> Value {
    let mut user = json!({ "id": server.uuid, "encryption": server.encryption });
    if !server.flow.is_empty() {
        user["flow"] = json!(server.flow);
    }
    json!({
        "log": { "loglevel": "warning" },
        "inbounds": [{
            "tag": "bridge-in",
            "listen": "127.0.0.1",
            "port": port,
            "protocol": "socks",
            "settings": { "auth": "noauth", "udp": true }
        }],
        "outbounds": [{
            "tag": "proxy",
            "protocol": "vless",
            "settings": { "vnext": [{ "address": server.address, "port": server.port, "users": [user] }] },
            "streamSettings": stream_settings(server)
        }]
    })
}

fn stream_settings(server: &VlessConfig) -> Value {
    let mut stream = json!({ "network": server.network, "security": server.security });
    match server.network.as_str() {
        "grpc" => {
            stream["grpcSettings"] = json!({
                "serviceName": server.service_name,
                "multiMode": server.grpc_mode == "multi"
            });
        }
        "ws" => {
            let mut ws = json!({ "path": server.path });
            if !server.host.is_empty() {
                ws["headers"] = json!({ "Host": server.host });
            }
            if server.ws_early_data > 0 {
                ws["maxEarlyData"] = json!(server.ws_early_data);
                ws["earlyDataHeaderName"] = json!("Sec-WebSocket-Protocol");
            }
            stream["wsSettings"] = ws;
        }
        "httpupgrade" => {
            stream["httpupgradeSettings"] = json!({ "path": server.path, "host": server.host });
        }
        "xhttp" => {
            let host = [&server.host, &server.sni, &server.address]
                .into_iter()
                .find(|value| !value.is_empty())
                .cloned()
                .unwrap_or_default();
            let mut xhttp = json!({ "host": host, "path": server.path, "mode": server.xhttp_mode });
            if let Some(extra) = &server.xhttp_extra {
                xhttp["extra"] = extra.clone();
            }
            stream["xhttpSettings"] = xhttp;
        }
        _ => {}
    }
    match server.security.as_str() {
        "tls" => {
            let mut tls = json!({
                "serverName": if server.sni.is_empty() { &server.address } else { &server.sni },
                "fingerprint": server.fingerprint
            });
            if !server.alpn.is_empty() {
                tls["alpn"] = json!(server.alpn);
            }
            if !server.pinned_peer_cert_sha256.is_empty() {
                tls["pinnedPeerCertSha256"] = json!(server.pinned_peer_cert_sha256);
            }
            if !server.verify_peer_cert_by_name.is_empty() {
                tls["verifyPeerCertByName"] = json!(server.verify_peer_cert_by_name);
            }
            stream["tlsSettings"] = tls;
        }
        "reality" => {
            stream["realitySettings"] = json!({
                "serverName": server.sni,
                "fingerprint": server.fingerprint,
                "publicKey": server.public_key,
                "shortId": server.short_id,
                "spiderX": server.spider_x
            });
        }
        _ => {}
    }
    stream
}

fn resolve_server_addresses(config: &mut Value) -> Result<Vec<String>, String> {
    let mut addresses = Vec::new();
    for (server_pointer, stream_pointer) in SERVER_POINTERS {
        let Some(address) = config
            .pointer(&format!("{server_pointer}/address"))
            .and_then(Value::as_str)
            .map(str::to_owned)
        else {
            continue;
        };
        if address.parse::<IpAddr>().is_ok() {
            addresses.push(address);
            continue;
        }
        if let Some(stream) = config.pointer_mut(stream_pointer) {
            preserve_server_names(stream, &address);
        }
        let resolved = resolve(&address)?;
        if let Some(server) = config.pointer_mut(server_pointer) {
            server["address"] = json!(resolved);
        }
        addresses.push(resolved);
    }
    Ok(addresses)
}

fn resolve(address: &str) -> Result<String, String> {
    let mut endpoints: Vec<SocketAddr> = (address, 443)
        .to_socket_addrs()
        .map_err(|error| format!("Не удалось найти адрес сервера: {error}"))?
        .collect();
    endpoints.sort_by_key(|endpoint| !endpoint.is_ipv4());
    endpoints
        .first()
        .map(|endpoint| endpoint.ip().to_string())
        .ok_or_else(|| "Не удалось найти адрес сервера".to_owned())
}

fn preserve_server_names(stream: &mut Value, address: &str) {
    if stream["security"] == "tls" {
        if !stream["tlsSettings"].is_object() {
            stream["tlsSettings"] = json!({});
        }
        if stream["tlsSettings"]["serverName"].as_str().unwrap_or_default().is_empty() {
            stream["tlsSettings"]["serverName"] = json!(address);
        }
    }
    if stream["network"] == "xhttp" {
        let hostname = stream["tlsSettings"]["serverName"]
            .as_str()
            .or_else(|| stream["realitySettings"]["serverName"].as_str())
            .filter(|name| !name.is_empty())
            .unwrap_or(address)
            .to_owned();
        if !stream["xhttpSettings"].is_object() {
            stream["xhttpSettings"] = json!({});
        }
        if stream["xhttpSettings"]["host"].as_str().unwrap_or_default().is_empty() {
            stream["xhttpSettings"]["host"] = json!(hostname);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::{Routing, VpnSettings};
    use crate::singbox_config::{self, Bridge, CoreOptions};
    use crate::split;
    use std::path::PathBuf;

    const UUID: &str = "11111111-2222-3333-4444-555555555555";

    fn core_binary(name: &str) -> PathBuf {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("core");
        if cfg!(windows) {
            root.join("windows").join(format!("{name}.exe"))
        } else {
            root.join("linux").join(name)
        }
    }

    fn free_port() -> u16 {
        TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port()
    }

    fn start_process(launch: &BridgeLaunch) -> XrayBridge {
        let log = Arc::new(Mutex::new(VecDeque::new()));
        XrayBridge::start(&core_binary("prime-vpn-xray"), launch, log, 50).unwrap()
    }

    fn start_origin() -> u16 {
        let listener = TcpListener::bind("0.0.0.0:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        std::thread::spawn(move || {
            for stream in listener.incoming().map_while(Result::ok) {
                std::thread::spawn(move || {
                    let mut stream = stream;
                    let mut buffer = [0u8; 2048];
                    let _ = stream.read(&mut buffer);
                    let _ = stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 12\r\nConnection: close\r\n\r\ntransport-ok");
                });
            }
        });
        port
    }

    fn reality_keys() -> (String, String) {
        let output = Command::new(core_binary("prime-vpn-xray")).arg("x25519").output().unwrap();
        let text = String::from_utf8_lossy(&output.stdout).into_owned();
        let value = |prefix: &str| {
            text.lines()
                .find(|line| line.starts_with(prefix))
                .and_then(|line| line.rsplit(' ').next())
                .unwrap()
                .to_owned()
        };
        (value("PrivateKey"), value("Password"))
    }

    fn start_reality_target(directory: &Path) -> Option<(Child, u16)> {
        let openssl = if cfg!(windows) { "C:/Program Files/Git/usr/bin/openssl.exe" } else { "openssl" };
        let key = directory.join("target.key");
        let cert = directory.join("target.crt");
        let generated = Command::new(openssl)
            .args(["req", "-x509", "-newkey", "rsa:2048", "-nodes", "-days", "1", "-subj", "/CN=localhost", "-keyout"])
            .arg(&key)
            .arg("-out")
            .arg(&cert)
            .stderr(Stdio::null())
            .status()
            .ok()?;
        if !generated.success() {
            return None;
        }
        let port = free_port();
        let script = format!(
            "const tls=require('tls'),fs=require('fs');tls.createServer({{cert:fs.readFileSync(process.argv[1]),key:fs.readFileSync(process.argv[2]),ALPNProtocols:['h2','http/1.1']}},s=>s.end()).listen({port},'127.0.0.1')"
        );
        let child = Command::new("node").args(["-e", &script]).arg(&cert).arg(&key).spawn().ok()?;
        let address = SocketAddr::from(([127, 0, 0, 1], port));
        for _ in 0..50 {
            if TcpStream::connect_timeout(&address, Duration::from_millis(100)).is_ok() {
                return Some((child, port));
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        None
    }

    fn server_launch(network: &str, security: Value, extra: Value, flow: &str) -> BridgeLaunch {
        let port = free_port();
        let mut client = json!({ "id": UUID });
        if !flow.is_empty() {
            client["flow"] = json!(flow);
        }
        let mut stream = json!({ "network": network });
        for (key, value) in security.as_object().unwrap().iter().chain(extra.as_object().unwrap()) {
            stream[key] = value.clone();
        }
        let config = json!({
            "log": { "loglevel": "warning" },
            "inbounds": [{
                "listen": "127.0.0.1",
                "port": port,
                "protocol": "vless",
                "settings": { "clients": [client], "decryption": "none" },
                "streamSettings": stream
            }],
            "outbounds": [{ "protocol": "freedom", "settings": { "finalRules": [{ "action": "allow", "ip": ["127.0.0.0/8"] }] } }]
        });
        BridgeLaunch { config: config.to_string(), port, server_addresses: Vec::new() }
    }

    async fn fetch_through(key: &str, origin: u16) -> String {
        let server = crate::vless::parse(key).unwrap();
        let bridge_launch = server.requires_xray().then(|| prepare(&server).unwrap());
        let _bridge = bridge_launch.as_ref().map(start_process);
        let proxy_port = free_port();
        let settings = VpnSettings { routing: Routing::Global, ..Default::default() };
        let options = CoreOptions {
            proxy_port,
            api_port: free_port(),
            api_secret: "secret".into(),
            capture: split::resolve_capture(&settings),
            settings,
            bridge: bridge_launch.as_ref().map(|launch| Bridge {
                port: launch.port,
                server_addresses: launch.server_addresses.clone(),
            }),
        };
        let core = core_binary("prime-vpn-core");
        let mut child = Command::new(&core)
            .args(["run", "-c", "stdin"])
            .current_dir(core.parent().unwrap().parent().unwrap())
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        child.stdin.take().unwrap().write_all(singbox_config::build(&server, &options).to_string().as_bytes()).unwrap();
        let address = SocketAddr::from(([127, 0, 0, 1], proxy_port));
        for _ in 0..50 {
            if TcpStream::connect_timeout(&address, Duration::from_millis(100)).is_ok() {
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        let client = reqwest::Client::builder()
            .proxy(reqwest::Proxy::all(format!("http://127.0.0.1:{proxy_port}")).unwrap())
            .timeout(Duration::from_secs(8))
            .build()
            .unwrap();
        let body = match client.get(format!("http://127.0.0.2:{origin}/")).send().await {
            Ok(response) => response.text().await.unwrap_or_default(),
            Err(error) => error.to_string(),
        };
        let _ = child.kill();
        let _ = child.wait();
        body
    }

    #[tokio::test]
    async fn every_transport_carries_traffic() {
        if !core_binary("prime-vpn-xray").exists() || !core_binary("prime-vpn-core").exists() {
            return;
        }
        let directory = std::env::temp_dir().join(format!("prime-vpn-e2e-{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        let Some((mut target, target_port)) = start_reality_target(&directory) else {
            return;
        };
        let (private_key, public_key) = reality_keys();
        let origin = start_origin();
        let reality = json!({
            "security": "reality",
            "realitySettings": {
                "target": format!("127.0.0.1:{target_port}"),
                "serverNames": ["localhost"],
                "privateKey": private_key,
                "shortIds": ["abcd"]
            }
        });
        let none = json!({ "security": "none" });
        let reality_query = format!("security=reality&sni=localhost&fp=chrome&pbk={public_key}&sid=abcd");
        let cases = [
            ("raw", reality.clone(), json!({}), "xtls-rprx-vision", format!("type=raw&flow=xtls-rprx-vision&{reality_query}")),
            ("raw", reality.clone(), json!({}), "xtls-rprx-vision", format!("type=tcp&flow=xtls-rprx-vision&{reality_query}")),
            ("grpc", reality.clone(), json!({ "grpcSettings": { "serviceName": "proxy" } }), "", format!("type=grpc&serviceName=proxy&{reality_query}")),
            ("grpc", reality.clone(), json!({ "grpcSettings": { "serviceName": "proxy", "multiMode": true } }), "", format!("type=grpc&serviceName=proxy&mode=multi&{reality_query}")),
            ("xhttp", reality.clone(), json!({ "xhttpSettings": { "path": "/edge", "mode": "auto" } }), "", format!("type=xhttp&path=%2Fedge&{reality_query}")),
            ("xhttp", reality.clone(), json!({ "xhttpSettings": { "path": "/edge", "mode": "stream-one" } }), "", format!("type=xhttp&path=%2Fedge&mode=stream-one&{reality_query}")),
            ("xhttp", none.clone(), json!({ "xhttpSettings": { "path": "/edge", "mode": "auto" } }), "", "type=xhttp&path=%2Fedge".to_owned()),
            ("ws", none.clone(), json!({ "wsSettings": { "path": "/edge" } }), "", "type=ws&path=%2Fedge".to_owned()),
            ("ws", none.clone(), json!({ "wsSettings": { "path": "/edge" } }), "", "type=ws&path=%2Fedge%3Fed%3D2048".to_owned()),
            ("httpupgrade", none.clone(), json!({ "httpupgradeSettings": { "path": "/edge" } }), "", "type=httpupgrade&path=%2Fedge".to_owned()),
            ("grpc", none.clone(), json!({ "grpcSettings": { "serviceName": "proxy" } }), "", "type=grpc&serviceName=proxy".to_owned()),
            ("raw", none, json!({}), "", "type=tcp".to_owned()),
        ];
        let mut failures = Vec::new();
        for (network, security, extra, flow, query) in cases {
            let launch = server_launch(network, security, extra, flow);
            let _server = start_process(&launch);
            let key = format!("vless://{UUID}@127.0.0.1:{}?{query}", launch.port);
            let body = fetch_through(&key, origin).await;
            if body != "transport-ok" {
                failures.push(format!("{query}: {body}"));
            }
        }
        let _ = target.kill();
        let _ = target.wait();
        let _ = std::fs::remove_dir_all(&directory);
        assert!(failures.is_empty(), "{failures:#?}");
    }
}
