use serde_json::Value;

#[derive(Debug, Clone, Default)]
pub struct VlessConfig {
    pub uuid: String,
    pub address: String,
    pub port: u16,
    pub encryption: String,
    pub flow: String,
    pub network: String,
    pub security: String,
    pub sni: String,
    pub fingerprint: String,
    pub alpn: Vec<String>,
    pub public_key: String,
    pub short_id: String,
    pub path: String,
    pub host: String,
    pub service_name: String,
    pub spider_x: String,
    pub grpc_mode: String,
    pub xhttp_mode: String,
    pub xhttp_extra: Option<Value>,
    pub ws_early_data: u32,
    pub allow_insecure: bool,
    pub pinned_peer_cert_sha256: String,
    pub verify_peer_cert_by_name: String,
}

impl VlessConfig {
    pub fn requires_xray(&self) -> bool {
        self.network == "xhttp"
            || (self.network == "grpc" && self.grpc_mode == "multi")
            || (self.security == "reality" && matches!(self.network.as_str(), "tcp" | "grpc"))
            || (self.security == "tls"
                && (!self.pinned_peer_cert_sha256.is_empty() || !self.verify_peer_cert_by_name.is_empty()))
    }
}

pub fn parse(url: &str) -> Result<VlessConfig, String> {
    let rest = url
        .trim()
        .strip_prefix("vless://")
        .ok_or("Ключ должен начинаться с vless://")?;
    let rest = rest.split('#').next().unwrap_or_default();
    let (address_part, query) = rest.split_once('?').unwrap_or((rest, ""));
    let (uuid, host_port) = address_part
        .split_once('@')
        .ok_or("В ключе нет UUID")?;

    let (address, port) = split_host_port(host_port)?;
    if !is_uuid(uuid) {
        return Err("Некорректный UUID в ключе".into());
    }

    let params: Vec<(String, String)> = query
        .split('&')
        .filter(|pair| !pair.is_empty())
        .map(|pair| {
            let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
            (key.to_owned(), percent_decode(value))
        })
        .collect();
    let get = |names: &[&str]| -> String {
        names
            .iter()
            .find_map(|name| params.iter().find(|(key, _)| key == name).map(|(_, v)| v.clone()))
            .unwrap_or_default()
    };

    let security = non_empty(get(&["security"]), "none");
    let mut sni = get(&["sni", "serverName"]);
    if sni.is_empty() && security != "none" {
        sni = address.clone();
    }

    let network = normalize_transport(non_empty(get(&["type", "network"]), "tcp"));
    let xhttp_extra = match get(&["extra"]).as_str() {
        extra if network == "xhttp" && !extra.is_empty() => Some(parse_extra(extra)?),
        _ => None,
    };
    let (path, ws_early_data) = split_early_data(non_empty(get(&["path"]), "/"), &get(&["ed"]), &network)?;
    let mode = get(&["mode"]);

    let config = VlessConfig {
        uuid: uuid.to_owned(),
        address,
        port,
        encryption: non_empty(get(&["encryption"]), "none"),
        flow: get(&["flow"]),
        grpc_mode: non_empty(if network == "grpc" { mode.clone() } else { String::new() }, "gun"),
        xhttp_mode: non_empty(if network == "xhttp" { mode } else { String::new() }, "auto"),
        xhttp_extra,
        ws_early_data,
        allow_insecure: matches!(get(&["allowInsecure", "insecure"]).as_str(), "1" | "true"),
        pinned_peer_cert_sha256: get(&["pcs", "pinnedPeerCertSha256"]),
        verify_peer_cert_by_name: get(&["vcn", "verifyPeerCertByName"]),
        spider_x: get(&["spx", "spiderX"]),
        network,
        security,
        sni,
        fingerprint: non_empty(get(&["fp", "fingerprint"]), "chrome"),
        alpn: get(&["alpn"])
            .split(',')
            .filter(|item| !item.is_empty())
            .map(str::to_owned)
            .collect(),
        public_key: get(&["pbk", "publicKey"]),
        short_id: get(&["sid", "shortId"]),
        path,
        host: get(&["host"]),
        service_name: get(&["serviceName"]),
    };
    validate(&config)?;
    Ok(config)
}

fn normalize_transport(name: String) -> String {
    match name.as_str() {
        "raw" => "tcp".into(),
        "websocket" => "ws".into(),
        "h2" => "http".into(),
        "splithttp" => "xhttp".into(),
        _ => name,
    }
}

fn parse_extra(extra: &str) -> Result<Value, String> {
    match serde_json::from_str::<Value>(extra) {
        Ok(value) if value.is_object() => Ok(value),
        _ => Err("Некорректные параметры XHTTP в ключе".into()),
    }
}

fn split_early_data(path: String, explicit: &str, network: &str) -> Result<(String, u32), String> {
    if network != "ws" {
        return Ok((path, 0));
    }
    let (base, query) = path.split_once('?').unwrap_or((path.as_str(), ""));
    let mut early_data = explicit.to_owned();
    let mut remaining = Vec::new();
    for pair in query.split('&').filter(|pair| !pair.is_empty()) {
        match pair.strip_prefix("ed=") {
            Some(value) => {
                if early_data.is_empty() {
                    early_data = value.to_owned();
                }
            }
            None => remaining.push(pair),
        }
    }
    let early_data = if early_data.is_empty() {
        0
    } else {
        early_data.parse().map_err(|_| "Некорректный параметр early data в ключе".to_owned())?
    };
    let clean = if remaining.is_empty() {
        base.to_owned()
    } else {
        format!("{base}?{}", remaining.join("&"))
    };
    Ok((clean, early_data))
}

fn validate(config: &VlessConfig) -> Result<(), String> {
    if !matches!(config.network.as_str(), "tcp" | "ws" | "grpc" | "http" | "httpupgrade" | "xhttp") {
        return Err(format!("Транспорт {} не поддерживается", config.network));
    }
    if !matches!(config.security.as_str(), "none" | "tls" | "reality") {
        return Err(format!("Защита {} не поддерживается", config.security));
    }
    if config.encryption != "none" {
        return Err(format!("Шифрование {} не поддерживается", config.encryption));
    }
    if !config.flow.is_empty()
        && (config.flow != "xtls-rprx-vision" || config.network != "tcp" || config.security == "none")
    {
        return Err("XTLS Vision работает только с TCP и TLS или Reality".into());
    }
    if config.network == "xhttp" && !matches!(config.xhttp_mode.as_str(), "auto" | "packet-up" | "stream-up" | "stream-one") {
        return Err(format!("Режим XHTTP {} не поддерживается", config.xhttp_mode));
    }
    if config.network == "grpc" && !matches!(config.grpc_mode.as_str(), "gun" | "multi") {
        return Err(format!("Режим gRPC {} не поддерживается", config.grpc_mode));
    }
    if config.network == "http" && (!config.pinned_peer_cert_sha256.is_empty() || !config.verify_peer_cert_by_name.is_empty()) {
        return Err("Привязка сертификата не поддерживается с транспортом HTTP".into());
    }
    let xray_transport = config.network == "xhttp" || (config.network == "grpc" && config.grpc_mode == "multi");
    if xray_transport && config.security == "tls" && config.allow_insecure && config.pinned_peer_cert_sha256.is_empty() {
        return Err("Для этого ключа нужен доверенный сертификат или привязка pcs".into());
    }
    if config.security == "reality" && config.public_key.is_empty() {
        return Err("В ключе Reality нет публичного ключа".into());
    }
    Ok(())
}

fn split_host_port(value: &str) -> Result<(String, u16), String> {
    let invalid_port = || "Некорректный порт в ключе".to_owned();
    let parse_port = |port: &str| port.parse::<u16>().ok().filter(|port| *port > 0).ok_or_else(invalid_port);
    if let Some(stripped) = value.strip_prefix('[') {
        let (host, tail) = stripped.split_once(']').ok_or_else(invalid_port)?;
        let port = tail.strip_prefix(':').ok_or_else(invalid_port)?;
        return Ok((host.to_owned(), parse_port(port)?));
    }
    match value.rsplit_once(':') {
        Some((host, port)) => Ok((host.to_owned(), parse_port(port)?)),
        None => Ok((value.to_owned(), 443)),
    }
}

fn non_empty(value: String, fallback: &str) -> String {
    if value.is_empty() { fallback.to_owned() } else { value }
}

fn is_uuid(value: &str) -> bool {
    let groups: Vec<&str> = value.split('-').collect();
    groups.len() == 5
        && groups
            .iter()
            .zip([8, 4, 4, 4, 12])
            .all(|(group, len)| group.len() == len && group.chars().all(|c| c.is_ascii_hexdigit()))
}

fn percent_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'%' if index + 2 < bytes.len() => {
                let hex = std::str::from_utf8(&bytes[index + 1..index + 3]).unwrap_or("");
                match u8::from_str_radix(hex, 16) {
                    Ok(byte) => {
                        out.push(byte);
                        index += 3;
                    }
                    Err(_) => {
                        out.push(b'%');
                        index += 1;
                    }
                }
            }
            b'+' => {
                out.push(b' ');
                index += 1;
            }
            byte => {
                out.push(byte);
                index += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_reality_key() {
        let key = "vless://11111111-2222-3333-4444-555555555555@1.2.3.4:443?type=tcp&security=reality&pbk=PUB&fp=chrome&sni=www.example.com&sid=ab&flow=xtls-rprx-vision#NL";
        let config = parse(key).unwrap();
        assert_eq!(config.address, "1.2.3.4");
        assert_eq!(config.port, 443);
        assert_eq!(config.security, "reality");
        assert_eq!(config.public_key, "PUB");
        assert_eq!(config.short_id, "ab");
        assert_eq!(config.flow, "xtls-rprx-vision");
        assert_eq!(config.sni, "www.example.com");
    }

    #[test]
    fn decodes_path() {
        let key = "vless://11111111-2222-3333-4444-555555555555@host.io:8443?type=ws&security=tls&path=%2Fws%3Fed%3D2048";
        let config = parse(key).unwrap();
        assert_eq!(config.path, "/ws");
        assert_eq!(config.ws_early_data, 2048);
        assert_eq!(config.sni, "host.io");
    }

    const BASE: &str = "vless://11111111-2222-3333-4444-555555555555@host.io:443";

    #[test]
    fn transports_choose_the_right_core() {
        for (query, xray) in [
            ("type=tcp&security=reality&pbk=P&flow=xtls-rprx-vision", true),
            ("type=raw&security=reality&pbk=P", true),
            ("type=grpc&security=reality&pbk=P&serviceName=s", true),
            ("type=xhttp&security=reality&pbk=P&mode=stream-one", true),
            ("type=splithttp&security=tls", true),
            ("type=grpc&security=tls&mode=multi", true),
            ("type=tcp&security=tls&pcs=abc", true),
            ("type=ws&security=reality&pbk=P", false),
            ("type=ws&security=tls", false),
            ("type=grpc&security=tls", false),
            ("type=httpupgrade&security=tls", false),
            ("type=h2&security=tls", false),
            ("type=tcp&security=tls", false),
        ] {
            let config = parse(&format!("{BASE}?{query}")).unwrap();
            assert_eq!(config.requires_xray(), xray, "{query}");
        }
    }

    #[test]
    fn invalid_keys_are_rejected() {
        for query in [
            "type=kcp",
            "security=xtls",
            "encryption=aes",
            "type=ws&security=tls&flow=xtls-rprx-vision",
            "type=xhttp&mode=wrong",
            "type=grpc&mode=wrong",
            "type=xhttp&extra=%5B%5D",
            "type=xhttp&security=tls&allowInsecure=1",
            "security=reality",
        ] {
            assert!(parse(&format!("{BASE}?{query}")).is_err(), "{query}");
        }
        assert!(parse("vless://11111111-2222-3333-4444-555555555555@host.io:70000").is_err());
    }
}
