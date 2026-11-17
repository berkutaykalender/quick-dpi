use serde::{Deserialize, Serialize};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Strategy {
    pub split: bool,
    pub split_at_host: bool,
    pub split_pos: u16,
    pub reverse: bool,
    pub fake: bool,
    pub fake_ttl: bool,
    pub fake_badsum: bool,
    pub fake_badseq: bool,
    pub fake_count: u8,
    pub auto_ttl: bool,
    pub ttl: u8,
    pub http_mixcase: bool,
}

impl Default for Strategy {
    fn default() -> Self {
        presets()[0].strategy.clone()
    }
}

pub struct Preset {
    pub id: &'static str,
    pub name_tr: &'static str,
    pub name_en: &'static str,
    pub strategy: Strategy,
}

impl Preset {
    pub fn name(&self) -> &'static str {
        crate::i18n::tr(self.name_tr, self.name_en)
    }
}

const BASE: Strategy = Strategy {
    split: true,
    split_at_host: false,
    split_pos: 2,
    reverse: false,
    fake: false,
    fake_ttl: false,
    fake_badsum: false,
    fake_badseq: false,
    fake_count: 1,
    auto_ttl: false,
    ttl: 4,
    http_mixcase: true,
};

pub fn presets() -> Vec<Preset> {
    vec![
        Preset { id: "split", name_tr: "Bölme", name_en: "Split", strategy: BASE },
        Preset { id: "split-sni", name_tr: "SNI'dan ters bölme", name_en: "Reverse split at SNI", strategy: Strategy { split_at_host: true, reverse: true, ..BASE } },
        Preset {
            id: "fake-badsum",
            name_tr: "Ön paket (bozuk sağlama) + ters bölme",
            name_en: "Decoy packet (bad checksum) + reverse split",
            strategy: Strategy { reverse: true, fake: true, fake_badsum: true, ..BASE },
        },
        Preset {
            id: "fake-ttl",
            name_tr: "Ön paket (otomatik TTL) + ters bölme",
            name_en: "Decoy packet (auto TTL) + reverse split",
            strategy: Strategy { reverse: true, fake: true, fake_ttl: true, auto_ttl: true, ..BASE },
        },
        Preset {
            id: "fake-seq",
            name_tr: "Ön paket (yanlış sıra + bozuk sağlama) + ters bölme",
            name_en: "Decoy packet (wrong seq + bad checksum) + reverse split",
            strategy: Strategy { reverse: true, fake: true, fake_badsum: true, fake_badseq: true, ..BASE },
        },
        Preset {
            id: "fake-all",
            name_tr: "Hepsi: ön paket ×2 (TTL + sağlama + sıra) + ters bölme",
            name_en: "All: decoy ×2 (TTL + checksum + seq) + reverse split",
            strategy: Strategy { reverse: true, fake: true, fake_ttl: true, fake_badsum: true, fake_badseq: true, fake_count: 2, auto_ttl: true, ..BASE },
        },
    ]
}

pub fn preset(id: &str) -> Option<Preset> {
    presets().into_iter().find(|p| p.id == id)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum RuleMode {
    #[default]
    Except,
    Only,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Rules {
    pub mode: RuleMode,
    pub list: Vec<String>,
}

impl Rules {
    pub fn applies(&self, host: Option<&str>, ip: IpAddr) -> bool {
        let hit = self.list.iter().any(|r| rule_matches(r, host, ip));
        match self.mode {
            RuleMode::Except => !hit,
            RuleMode::Only => hit || (host.is_none() && !self.list.iter().any(|r| parse_cidr(r).is_some())),
        }
    }
}

pub fn normalize_rule(r: &str) -> String {
    let r = r.trim().to_ascii_lowercase();
    let r = r.trim_start_matches("https://").trim_start_matches("http://");
    let r = r.split('/').next().unwrap_or("").to_string();
    if parse_cidr(&r).is_some() {
        return r;
    }
    r.trim_start_matches("*.").trim_start_matches('.').trim_end_matches('.').to_string()
}

fn rule_matches(rule: &str, host: Option<&str>, ip: IpAddr) -> bool {
    if let Some((net, bits)) = parse_cidr(rule) {
        return cidr_contains(net, bits, ip);
    }
    let Some(h) = host else { return false };
    let r = rule.trim_start_matches("*.").trim_start_matches('.');
    !r.is_empty() && (h == r || h.ends_with(&format!(".{r}")))
}

pub fn parse_cidr(s: &str) -> Option<(IpAddr, u8)> {
    let (a, b) = match s.split_once('/') {
        Some((a, b)) => (a, Some(b)),
        None => (s, None),
    };
    let ip: IpAddr = a.parse().ok()?;
    let max = if ip.is_ipv4() { 32 } else { 128 };
    let bits = match b {
        Some(b) => b.parse::<u8>().ok().filter(|&n| n <= max)?,
        None => max,
    };
    Some((ip, bits))
}

fn cidr_contains(net: IpAddr, bits: u8, ip: IpAddr) -> bool {
    match (net, ip) {
        (IpAddr::V4(n), IpAddr::V4(i)) => {
            let m = if bits == 0 { 0 } else { u32::MAX << (32 - bits) };
            u32::from(n) & m == u32::from(i) & m
        }
        (IpAddr::V6(n), IpAddr::V6(i)) => {
            let m = if bits == 0 { 0 } else { u128::MAX << (128 - bits) };
            u128::from(n) & m == u128::from(i) & m
        }
        _ => false,
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DnsTarget {
    pub v4: Option<(Ipv4Addr, u16)>,
    pub v6: Option<(Ipv6Addr, u16)>,
}

pub struct DnsPreset {
    pub id: &'static str,
    pub name: &'static str,
    pub alt_port: bool,
    pub target: DnsTarget,
}

impl DnsPreset {
    pub fn label(&self) -> String {
        match self.target.v4 {
            Some((a, p)) => format!("{} — {a}:{p}", self.name),
            None => self.name.to_string(),
        }
    }
}

fn dp(id: &'static str, name: &'static str, v4: [u8; 4], port: u16, v6: Option<&str>) -> DnsPreset {
    DnsPreset { id, name, alt_port: port != 53, target: DnsTarget { v4: Some((Ipv4Addr::from(v4), port)), v6: v6.map(|a| (a.parse().unwrap(), port)) } }
}

pub fn dns_presets() -> Vec<DnsPreset> {
    vec![
        dp("yandex", "Yandex", [77, 88, 8, 8], 1253, Some("2a02:6b8::feed:0ff")),
        dp("yandex-2", "Yandex (yedek)", [77, 88, 8, 1], 1253, None),
        dp("adguard", "AdGuard", [94, 140, 14, 14], 5353, None),
        dp("adguard-2", "AdGuard (yedek)", [94, 140, 15, 15], 5353, None),
        dp("opendns", "OpenDNS (Cisco)", [208, 67, 222, 222], 5353, None),
        dp("opendns-2", "OpenDNS (Cisco, yedek)", [208, 67, 220, 220], 5353, None),
        dp("cleanbrowsing", "CleanBrowsing", [185, 228, 168, 9], 5353, None),
        dp("nextdns", "NextDNS", [45, 90, 28, 0], 5353, None),
        dp("safedns", "SafeDNS", [195, 46, 39, 39], 1253, None),
        dp("cloudflare", "Cloudflare", [1, 1, 1, 1], 53, Some("2606:4700:4700::1111")),
        dp("cloudflare-2", "Cloudflare (yedek)", [1, 0, 0, 1], 53, Some("2606:4700:4700::1001")),
        dp("google", "Google", [8, 8, 8, 8], 53, Some("2001:4860:4860::8888")),
        dp("google-2", "Google (yedek)", [8, 8, 4, 4], 53, Some("2001:4860:4860::8844")),
        dp("quad9", "Quad9", [9, 9, 9, 9], 53, Some("2620:fe::fe")),
        dp("quad9-2", "Quad9 (yedek)", [149, 112, 112, 112], 53, Some("2620:fe::9")),
        dp("opendns-53", "OpenDNS (Cisco)", [208, 67, 222, 222], 53, Some("2620:119:35::35")),
        dp("adguard-53", "AdGuard", [94, 140, 14, 14], 53, Some("2a10:50c0::ad1:ff")),
        dp("controld", "Control D", [76, 76, 2, 0], 53, None),
        dp("comodo", "Comodo Secure DNS", [8, 26, 56, 26], 53, None),
        dp("ultradns", "UltraDNS (Vercara)", [156, 154, 70, 1], 53, None),
        dp("dnswatch", "DNS.Watch", [84, 200, 69, 80], 53, None),
    ]
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EngineConfig {
    pub enabled: bool,
    pub strategy: Strategy,
    pub rules: Rules,
    pub dns: Option<DnsTarget>,
    pub block_quic: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub enabled: bool,
    pub strategy: String,
    pub custom: Strategy,
    pub tuned: Option<String>,
    pub tuned_at: Option<u64>,
    pub rules: Rules,
    pub dns_redirect: bool,
    pub dns_server: String,
    pub dns_custom: String,
    pub block_quic: bool,
    pub autostart: bool,
    pub close_to_tray: bool,
    pub test_sites: Vec<String>,
    pub lang: String,
    pub accepted_terms: u32,
    pub auto_update: bool,
}

pub const TERMS_VERSION: u32 = 1;
pub const WEBSITE: &str = "https://quick-dpi.earthquick.dev";

impl Default for Config {
    fn default() -> Self {
        Config {
            enabled: true,
            strategy: "auto".into(),
            custom: Strategy::default(),
            tuned: None,
            tuned_at: None,
            rules: Rules::default(),
            dns_redirect: true,
            dns_server: "yandex".into(),
            dns_custom: String::new(),
            block_quic: true,
            autostart: true,
            close_to_tray: true,
            test_sites: vec!["discord.com".into(), "www.roblox.com".into(), "www.wattpad.com".into()],
            lang: "auto".into(),
            accepted_terms: 0,
            auto_update: true,
        }
    }
}

pub fn dir() -> PathBuf {
    dirs::config_dir().unwrap_or_else(std::env::temp_dir).join("quickdpi")
}

fn path() -> PathBuf {
    dir().join("config.json")
}

impl Config {
    pub fn load() -> Config {
        std::fs::read_to_string(path()).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
    }

    pub fn accepted(&self) -> bool {
        self.accepted_terms >= TERMS_VERSION
    }

    pub fn exists() -> bool {
        path().exists()
    }

    pub fn save(&self) {
        let _ = std::fs::create_dir_all(dir());
        let tmp = path().with_extension("tmp");
        if let Ok(s) = serde_json::to_string_pretty(self) {
            if std::fs::write(&tmp, s).is_ok() {
                let _ = std::fs::rename(&tmp, path());
            }
        }
    }

    pub fn active_strategy_id(&self) -> String {
        if self.strategy == "auto" { self.tuned.clone().unwrap_or_else(|| presets()[0].id.to_string()) } else { self.strategy.clone() }
    }

    pub fn strategy_value(&self) -> Strategy {
        let id = self.active_strategy_id();
        if id == "custom" { self.custom.clone() } else { preset(&id).map(|p| p.strategy).unwrap_or_default() }
    }

    pub fn strategy_name(&self) -> String {
        let id = self.active_strategy_id();
        if id == "custom" { crate::i18n::tr("Özel", "Custom").into() } else { preset(&id).map(|p| p.name().to_string()).unwrap_or(id) }
    }

    pub fn dns_target(&self) -> Option<DnsTarget> {
        if !self.dns_redirect {
            return None;
        }
        if self.dns_server == "custom" {
            return parse_dns_custom(&self.dns_custom);
        }
        dns_presets().into_iter().find(|d| d.id == self.dns_server).map(|d| d.target)
    }

    pub fn engine(&self) -> EngineConfig {
        EngineConfig { enabled: self.enabled, strategy: self.strategy_value(), rules: self.rules.clone(), dns: self.dns_target(), block_quic: self.block_quic }
    }
}

pub fn parse_dns_custom(s: &str) -> Option<DnsTarget> {
    let mut t = DnsTarget { v4: None, v6: None };
    for part in s.split([',', ' ']).filter(|p| !p.trim().is_empty()) {
        let part = part.trim();
        if let Ok(sa) = part.parse::<std::net::SocketAddr>() {
            match sa {
                std::net::SocketAddr::V4(a) => t.v4 = Some((*a.ip(), a.port())),
                std::net::SocketAddr::V6(a) => t.v6 = Some((*a.ip(), a.port())),
            }
        } else if let Ok(ip) = part.parse::<IpAddr>() {
            match ip {
                IpAddr::V4(a) => t.v4 = Some((a, 53)),
                IpAddr::V6(a) => t.v6 = Some((a, 53)),
            }
        } else {
            return None;
        }
    }
    (t.v4.is_some() || t.v6.is_some()).then_some(t)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rules() {
        let ip: IpAddr = "1.2.3.4".parse().unwrap();
        let r = Rules { mode: RuleMode::Except, list: vec!["example.com".into(), "10.0.0.0/8".into()] };
        assert!(!r.applies(Some("example.com"), ip));
        assert!(!r.applies(Some("a.example.com"), ip));
        assert!(r.applies(Some("badexample.com"), ip));
        assert!(!r.applies(None, "10.1.2.3".parse().unwrap()));
        let o = Rules { mode: RuleMode::Only, list: vec!["discord.com".into()] };
        assert!(o.applies(Some("cdn.discord.com"), ip));
        assert!(!o.applies(Some("google.com"), ip));
        assert_eq!(normalize_rule(" https://*.Discord.com/app "), "discord.com");
    }

    #[test]
    fn dns_custom() {
        let t = parse_dns_custom("1.1.1.1:5353, [2606:4700::1111]:53").unwrap();
        assert_eq!(t.v4, Some(("1.1.1.1".parse().unwrap(), 5353)));
        assert!(t.v6.is_some());
        assert!(parse_dns_custom("nope").is_none());
    }
}
