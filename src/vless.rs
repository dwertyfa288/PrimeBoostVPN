#[derive(Debug, Clone, Default)]
pub struct VlessConfig {
    pub uuid: String,
    pub address: String,
    pub port: u16,
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

    let (address, port) = split_host_port(host_port);
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

    Ok(VlessConfig {
        uuid: uuid.to_owned(),
        address,
        port,
        flow: get(&["flow"]),
        network: non_empty(get(&["type"]), "tcp"),
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
        path: non_empty(get(&["path"]), "/"),
        host: get(&["host"]),
        service_name: get(&["serviceName"]),
    })
}

fn split_host_port(value: &str) -> (String, u16) {
    if let Some(stripped) = value.strip_prefix('[') {
        let (host, tail) = stripped.split_once(']').unwrap_or((stripped, ""));
        let port = tail.trim_start_matches(':').parse().unwrap_or(443);
        return (host.to_owned(), port);
    }
    match value.rsplit_once(':') {
        Some((host, port)) => (host.to_owned(), port.parse().unwrap_or(443)),
        None => (value.to_owned(), 443),
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
        assert_eq!(config.path, "/ws?ed=2048");
        assert_eq!(config.sni, "host.io");
    }
}
