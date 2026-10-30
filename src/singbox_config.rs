use serde_json::{Value, json};

use crate::settings::{Routing, SplitMode, VpnSettings};
use crate::split::{self, CaptureMode, SiteTargets};
use crate::vless::VlessConfig;

pub struct CoreOptions {
    pub proxy_port: u16,
    pub api_port: u16,
    pub api_secret: String,
    pub settings: VpnSettings,
    pub capture: CaptureMode,
    pub bridge: Option<Bridge>,
}

pub struct Bridge {
    pub port: u16,
    pub server_addresses: Vec<String>,
}

const PROXY_TAG: &str = "proxy";
const DIRECT_TAG: &str = "direct";
const MIXED_IN: &str = "mixed-in";
const TUN_IN: &str = "tun-in";
const REMOTE_DNS: &str = "remote";
const DIRECT_DNS: &str = "direct-dns";
const DIRECT_DNS_SERVER: &str = "77.88.8.8";
const CORE_PROCESS_NAMES: [&str; 3] = ["prime-vpn-core", "prime-vpn-core-linux", "prime-vpn-xray"];
const LAN_CIDRS: [&str; 7] = [
    "127.0.0.0/8",
    "169.254.0.0/16",
    "224.0.0.0/4",
    "240.0.0.0/4",
    "::1/128",
    "fc00::/7",
    "fe80::/10",
];
const DOMESTIC_SUFFIXES: [&str; 4] = ["ru", "su", "xn--p1ai", "cn"];
const DOMESTIC_KEYWORDS: [&str; 8] = [
    "yandex",
    "vk",
    "mail.ru",
    "rambler",
    "sber",
    "gosuslugi",
    "ozon",
    "wildberries",
];
const RULE_SETS: [(&str, &str); 4] = [
    ("geosite-ru", "rules/geosite-category-ru.srs"),
    ("geosite-cn", "rules/geosite-geolocation-cn.srs"),
    ("geoip-ru", "rules/geoip-ru.srs"),
    ("geoip-cn", "rules/geoip-cn.srs"),
];

pub fn build(server: &VlessConfig, options: &CoreOptions) -> Value {
    let settings = &options.settings;
    let capture = options.capture;
    let tun_inbound = capture.needs_tun();
    let routing = if settings.tun_mode && capture.full_tun {
        Routing::Global
    } else {
        settings.routing
    };

    let mut route = json!({
        "default_domain_resolver": "bootstrap",
        "rules": route_rules(server, options, routing),
        "final": final_outbound(settings),
    });
    if tun_inbound {
        route["auto_detect_interface"] = json!(true);
    }
    if routing == Routing::Smart {
        route["rule_set"] = Value::Array(
            RULE_SETS
                .iter()
                .map(|(tag, path)| json!({ "type": "local", "tag": tag, "format": "binary", "path": path }))
                .collect(),
        );
    }

    json!({
        "log": { "level": "warn" },
        "dns": dns(settings, capture),
        "inbounds": inbounds(options),
        "outbounds": [
            proxy_outbound(server, options.bridge.as_ref()),
            { "type": "direct", "tag": DIRECT_TAG },
            { "type": "block", "tag": "block" }
        ],
        "route": route,
        "experimental": {
            "clash_api": {
                "external_controller": format!("127.0.0.1:{}", options.api_port),
                "secret": options.api_secret
            }
        }
    })
}

fn inbounds(options: &CoreOptions) -> Vec<Value> {
    let mut inbounds = vec![json!({
        "type": "mixed",
        "tag": MIXED_IN,
        "listen": "127.0.0.1",
        "listen_port": options.proxy_port
    })];
    if options.capture.needs_tun() {
        inbounds.push(json!({
            "type": "tun",
            "tag": TUN_IN,
            "interface_name": "PrimeVPN",
            "address": ["172.19.0.1/30", "fdfe:dcba:9876::1/126"],
            "mtu": if options.capture.split_capture { 9000 } else { 1280 },
            "auto_route": true,
            "strict_route": options.capture.strict_route,
            "stack": "gvisor",
            "endpoint_independent_nat": true
        }));
    }
    inbounds
}

fn dns(settings: &VpnSettings, capture: CaptureMode) -> Value {
    let strategy = if settings.ipv6 { "prefer_ipv4" } else { "ipv4_only" };
    if !capture.split_capture {
        return json!({
            "servers": [
                { "tag": "bootstrap", "type": "udp", "server": "1.1.1.1" },
                { "tag": REMOTE_DNS, "type": "udp", "server": "8.8.8.8", "detour": PROXY_TAG },
                { "tag": "local", "type": "local" }
            ],
            "final": REMOTE_DNS,
            "strategy": strategy
        });
    }

    let apps = split::process_regexes(&settings.split.apps);
    let sites = split::site_targets(&settings.split.sites);
    let mut rules = Vec::new();
    if !apps.is_empty() {
        rules.push(json!({ "process_path_regex": apps, "action": "route", "server": REMOTE_DNS }));
    }
    if !sites.domains.is_empty() {
        rules.push(json!({
            "domain": sites.domains,
            "domain_suffix": sites.domains,
            "action": "route",
            "server": REMOTE_DNS
        }));
    }
    json!({
        "servers": [
            { "tag": "bootstrap", "type": "udp", "server": "1.1.1.1" },
            { "tag": REMOTE_DNS, "type": "udp", "server": "8.8.8.8", "detour": PROXY_TAG },
            { "tag": DIRECT_DNS, "type": "udp", "server": DIRECT_DNS_SERVER }
        ],
        "rules": rules,
        "final": DIRECT_DNS,
        "strategy": strategy
    })
}

fn final_outbound(settings: &VpnSettings) -> &'static str {
    let split = &settings.split;
    if (split.has_apps() || split.has_sites()) && split.mode == SplitMode::Only {
        DIRECT_TAG
    } else {
        PROXY_TAG
    }
}

fn route_rules(server: &VlessConfig, options: &CoreOptions, routing: Routing) -> Vec<Value> {
    let settings = &options.settings;
    let capture = options.capture;
    let tun_inbound = capture.needs_tun();
    let mut rules = Vec::new();

    if capture.split_capture {
        rules.push(json!({ "inbound": TUN_IN, "action": "sniff", "timeout": "1s" }));
        rules.push(json!({ "inbound": [MIXED_IN], "action": "sniff", "timeout": "1s" }));
    } else {
        let mut inbound = vec![MIXED_IN];
        if tun_inbound {
            inbound.push(TUN_IN);
        }
        rules.push(json!({ "inbound": inbound, "action": "sniff", "timeout": "1s" }));
    }

    if capture.full_tun {
        rules.push(json!({ "port": [53], "action": "hijack-dns" }));
    } else if capture.split_capture {
        rules.push(json!({ "inbound": TUN_IN, "port": [53], "action": "hijack-dns" }));
    }

    if !settings.ipv6 {
        rules.push(json!({ "ip_version": 6, "action": "reject" }));
    }

    if tun_inbound {
        rules.push(json!({ "network": "udp", "port": [135, 137, 138, 139, 5353], "action": "reject" }));
        rules.push(json!({ "ip_cidr": ["224.0.0.0/3", "ff00::/8"], "action": "reject" }));
        rules.push(json!({
            "process_path_regex": split::process_regexes(
                &CORE_PROCESS_NAMES.iter().map(|name| name.to_string()).collect::<Vec<_>>()
            ),
            "action": "route",
            "outbound": DIRECT_TAG
        }));
    }

    rules.push(server_direct_rule(&server.address));
    if let Some(bridge) = &options.bridge {
        rules.extend(bridge.server_addresses.iter().map(|address| server_direct_rule(address)));
    }
    rules.extend(split_rules(settings, tun_inbound));

    if capture.full_tun {
        rules.push(quic_reject(json!({})));
    }
    if capture.split_capture {
        rules.push(json!({ "inbound": TUN_IN, "action": "route", "outbound": DIRECT_TAG }));
    }

    if routing == Routing::Smart {
        rules.push(json!({ "ip_is_private": true, "action": "route", "outbound": DIRECT_TAG }));
        rules.push(json!({ "ip_cidr": LAN_CIDRS, "action": "route", "outbound": DIRECT_TAG }));
        rules.push(json!({
            "domain": ["localhost"],
            "domain_suffix": ["local", "lan"],
            "action": "route",
            "outbound": DIRECT_TAG
        }));
        rules.push(json!({
            "rule_set": RULE_SETS.iter().map(|(tag, _)| *tag).collect::<Vec<_>>(),
            "action": "route",
            "outbound": DIRECT_TAG
        }));
        rules.push(json!({
            "domain_suffix": DOMESTIC_SUFFIXES,
            "domain_keyword": DOMESTIC_KEYWORDS,
            "action": "route",
            "outbound": DIRECT_TAG
        }));
    }

    rules
}

fn split_rules(settings: &VpnSettings, tun_inbound: bool) -> Vec<Value> {
    let split = &settings.split;
    if !split.enabled {
        return Vec::new();
    }
    let outbound = if split.mode == SplitMode::Bypass { DIRECT_TAG } else { PROXY_TAG };
    let reject_quic = tun_inbound && outbound == PROXY_TAG;
    let apps = split::process_regexes(&split.apps);
    let SiteTargets { domains, ip_cidrs } = split::site_targets(&split.sites);
    let mut rules = Vec::new();

    if !apps.is_empty() {
        if reject_quic {
            rules.push(quic_reject(json!({ "process_path_regex": apps })));
        }
        rules.push(json!({ "process_path_regex": apps, "action": "route", "outbound": outbound }));
    }
    if reject_quic && !domains.is_empty() {
        rules.push(quic_reject(json!({ "domain": domains, "domain_suffix": domains })));
    }
    if reject_quic && !ip_cidrs.is_empty() {
        rules.push(quic_reject(json!({ "ip_cidr": ip_cidrs })));
    }
    if !domains.is_empty() {
        rules.push(json!({ "domain": domains, "domain_suffix": domains, "action": "route", "outbound": outbound }));
    }
    if !ip_cidrs.is_empty() {
        rules.push(json!({ "ip_cidr": ip_cidrs, "action": "route", "outbound": outbound }));
    }
    rules
}

fn quic_reject(mut matcher: Value) -> Value {
    matcher["inbound"] = json!(TUN_IN);
    matcher["network"] = json!("udp");
    matcher["port"] = json!([443]);
    matcher["action"] = json!("reject");
    matcher
}

fn server_direct_rule(address: &str) -> Value {
    let address = address.trim().trim_start_matches('[').trim_end_matches(']');
    if address.parse::<std::net::Ipv4Addr>().is_ok() {
        json!({ "ip_cidr": [format!("{address}/32")], "action": "route", "outbound": DIRECT_TAG })
    } else if address.parse::<std::net::Ipv6Addr>().is_ok() {
        json!({ "ip_cidr": [format!("{address}/128")], "action": "route", "outbound": DIRECT_TAG })
    } else {
        json!({ "domain": [address.to_lowercase()], "action": "route", "outbound": DIRECT_TAG })
    }
}

fn proxy_outbound(server: &VlessConfig, bridge: Option<&Bridge>) -> Value {
    match bridge {
        Some(bridge) => json!({
            "type": "socks",
            "tag": PROXY_TAG,
            "server": "127.0.0.1",
            "server_port": bridge.port,
            "version": "5",
        }),
        None => vless_outbound(server),
    }
}

fn vless_outbound(server: &VlessConfig) -> Value {
    let mut outbound = json!({
        "type": "vless",
        "tag": PROXY_TAG,
        "server": server.address,
        "server_port": server.port,
        "uuid": server.uuid,
    });
    if !server.flow.is_empty() {
        outbound["flow"] = json!(server.flow);
    }
    if let Some(tls) = tls(server) {
        outbound["tls"] = tls;
    }
    if let Some(transport) = transport(server) {
        outbound["transport"] = transport;
    }
    outbound
}

fn tls(server: &VlessConfig) -> Option<Value> {
    if server.security == "none" {
        return None;
    }
    let mut tls = json!({
        "enabled": true,
        "server_name": if server.sni.is_empty() { &server.address } else { &server.sni },
        "utls": { "enabled": true, "fingerprint": server.fingerprint },
    });
    if server.allow_insecure {
        tls["insecure"] = json!(true);
    }
    if !server.alpn.is_empty() {
        tls["alpn"] = json!(server.alpn);
    }
    if server.security == "reality" {
        let mut reality = json!({ "enabled": true, "public_key": server.public_key });
        if !server.short_id.is_empty() {
            reality["short_id"] = json!(server.short_id);
        }
        tls["reality"] = reality;
    }
    Some(tls)
}

fn transport(server: &VlessConfig) -> Option<Value> {
    match server.network.as_str() {
        "ws" => {
            let mut ws = json!({ "type": "ws", "path": server.path });
            if !server.host.is_empty() {
                ws["headers"] = json!({ "Host": server.host });
            }
            if server.ws_early_data > 0 {
                ws["max_early_data"] = json!(server.ws_early_data);
                ws["early_data_header_name"] = json!("Sec-WebSocket-Protocol");
            }
            Some(ws)
        }
        "httpupgrade" => {
            let mut upgrade = json!({ "type": "httpupgrade", "path": server.path });
            if !server.host.is_empty() {
                upgrade["host"] = json!(server.host);
            }
            Some(upgrade)
        }
        "grpc" => Some(json!({ "type": "grpc", "service_name": server.service_name })),
        "http" => {
            let mut http = json!({ "type": "http", "path": server.path });
            if !server.host.is_empty() {
                http["host"] = json!([server.host]);
            }
            Some(http)
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::SplitSettings;

    fn server() -> VlessConfig {
        crate::vless::parse(
            "vless://11111111-2222-3333-4444-555555555555@1.2.3.4:443?type=tcp&security=reality&pbk=Q3VyZTI1NTE5UHVibGljS2V5RXhhbXBsZUtleTAwMDA&sid=ab&sni=www.example.com&flow=xtls-rprx-vision",
        )
        .unwrap()
    }

    fn options(settings: VpnSettings) -> CoreOptions {
        CoreOptions {
            proxy_port: 41000,
            api_port: 41001,
            api_secret: "secret".into(),
            capture: split::resolve_capture(&settings),
            settings,
            bridge: None,
        }
    }

    fn variants() -> Vec<(&'static str, VpnSettings)> {
        let split = |mode, apps: &[&str], sites: &[&str]| SplitSettings {
            enabled: true,
            mode,
            apps: apps.iter().map(|s| s.to_string()).collect(),
            sites: sites.iter().map(|s| s.to_string()).collect(),
        };
        vec![
            ("smart", VpnSettings::default()),
            ("global-ipv6", VpnSettings { routing: Routing::Global, ipv6: true, ..Default::default() }),
            ("tun", VpnSettings { tun_mode: true, ..Default::default() }),
            ("bypass-apps", VpnSettings { split: split(SplitMode::Bypass, &["chrome.exe"], &["vk.com"]), ..Default::default() }),
            ("only-apps", VpnSettings { split: split(SplitMode::Only, &[r"C:\Apps\Discord\app-1.0.9\Discord.exe"], &["youtube.com", "8.8.8.8"]), ..Default::default() }),
            ("routing-apps", VpnSettings { split: split(SplitMode::Routing, &["telegram"], &[]), ..Default::default() }),
            ("only-sites", VpnSettings { split: split(SplitMode::Only, &[], &["youtube.com"]), ..Default::default() }),
        ]
    }

    #[test]
    fn config_passes_core_check() {
        let binary = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(if cfg!(windows) {
            "core/windows/prime-vpn-core.exe"
        } else {
            "core/linux/prime-vpn-core"
        });
        if !binary.exists() {
            return;
        }
        for (name, settings) in variants() {
            let path = std::env::temp_dir().join(format!("prime-vpn-check-{name}.json"));
            std::fs::write(&path, build(&server(), &options(settings)).to_string()).unwrap();
            let output = std::process::Command::new(&binary)
                .args(["check", "-c"])
                .arg(&path)
                .current_dir(binary.parent().unwrap().parent().unwrap())
                .output()
                .unwrap();
            let _ = std::fs::remove_file(&path);
            assert!(output.status.success(), "{name}: {}", String::from_utf8_lossy(&output.stderr));
        }
    }

    #[test]
    fn ipv6_and_split_rules_are_applied() {
        let config = build(&server(), &options(VpnSettings::default()));
        assert_eq!(config["dns"]["strategy"], "ipv4_only");
        assert!(config["route"]["rules"].as_array().unwrap().iter().any(|r| r["ip_version"] == 6));

        let with_ipv6 = build(&server(), &options(VpnSettings { ipv6: true, ..Default::default() }));
        assert_eq!(with_ipv6["dns"]["strategy"], "prefer_ipv4");
        assert!(!with_ipv6["route"]["rules"].as_array().unwrap().iter().any(|r| r["ip_version"] == 6));

        let (_, only) = variants().into_iter().find(|(name, _)| *name == "only-apps").unwrap();
        let config = build(&server(), &options(only));
        assert_eq!(config["route"]["final"], DIRECT_TAG);
        assert_eq!(config["inbounds"][1]["type"], "tun");
    }
}
