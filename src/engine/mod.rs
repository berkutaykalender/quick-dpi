use crate::config::EngineConfig;
use crate::packet::{self as pk, Ip};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::net::IpAddr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, RwLock};

#[cfg(target_os = "linux")]
pub mod linux;
#[cfg(windows)]
pub mod windows;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct EngineStatus {
    pub running: bool,
    pub error: Option<String>,
    pub packets: u64,
    pub desynced: u64,
    pub excluded: u64,
    pub dns: u64,
    pub recent: Vec<(String, bool)>,
}

#[derive(Default)]
pub struct Stats {
    pub packets: AtomicU64,
    pub desynced: AtomicU64,
    pub excluded: AtomicU64,
    pub dns: AtomicU64,
}

pub struct Processor {
    pub cfg: RwLock<EngineConfig>,
    pub stats: Stats,
    hops: Mutex<HashMap<IpAddr, u8>>,
    recent: Mutex<VecDeque<(String, bool)>>,
}

pub enum Action {
    Pass,
    Replace { pre: Vec<Vec<u8>>, last: Vec<u8> },
}

const FAKE_HOST: &str = "www.w3.org";

impl Processor {
    pub fn new(cfg: EngineConfig) -> Self {
        Processor { cfg: RwLock::new(cfg), stats: Stats::default(), hops: Mutex::new(HashMap::new()), recent: Mutex::new(VecDeque::new()) }
    }

    pub fn status(&self, running: bool, error: Option<String>) -> EngineStatus {
        EngineStatus {
            running,
            error,
            packets: self.stats.packets.load(Ordering::Relaxed),
            desynced: self.stats.desynced.load(Ordering::Relaxed),
            excluded: self.stats.excluded.load(Ordering::Relaxed),
            dns: self.stats.dns.load(Ordering::Relaxed),
            recent: self.recent.lock().unwrap().iter().cloned().collect(),
        }
    }

    fn remember(&self, host: &str, applied: bool) {
        let mut r = self.recent.lock().unwrap();
        r.retain(|(h, _)| h != host);
        r.push_front((host.to_string(), applied));
        r.truncate(40);
    }

    pub fn observe_inbound(&self, p: &[u8]) {
        let Some(ip) = pk::parse(p) else { return };
        if ip.proto != pk::PROTO_TCP || pk::tcp_flags(p, &ip) & (pk::TCP_SYN | pk::TCP_ACK) != pk::TCP_SYN | pk::TCP_ACK {
            return;
        }
        let ttl = pk::ttl(p, &ip);
        let initial: u16 = if ttl <= 64 {
            64
        } else if ttl <= 128 {
            128
        } else {
            255
        };
        let hops = (initial - ttl as u16).min(255) as u8;
        let mut m = self.hops.lock().unwrap();
        if m.len() > 8192 {
            m.clear();
        }
        m.insert(pk::src_ip(p, &ip), hops);
    }

    fn fake_ttl(&self, dst: IpAddr, auto: bool, fixed: u8) -> u8 {
        if auto {
            if let Some(&h) = self.hops.lock().unwrap().get(&dst) {
                return h.saturating_sub(1).clamp(1, 10);
            }
        }
        fixed.max(1)
    }

    pub fn process_out(&self, p: &[u8]) -> Action {
        let Some(ip) = pk::parse(p) else { return Action::Pass };
        if ip.proto != pk::PROTO_TCP || pk::tcp_flags(p, &ip) & (pk::TCP_SYN | pk::TCP_RST | pk::TCP_FIN) != 0 {
            return Action::Pass;
        }
        let data = &p[ip.payload()..ip.total];
        if data.is_empty() {
            return Action::Pass;
        }
        self.stats.packets.fetch_add(1, Ordering::Relaxed);
        let dst = pk::dst_ip(p, &ip);
        if is_local(dst) {
            return Action::Pass;
        }
        let (tls, host) = if pk::is_client_hello(data) {
            (true, pk::tls_sni(data))
        } else if pk::is_http_request(data) {
            (false, pk::http_host(data))
        } else {
            return Action::Pass;
        };
        let cfg = self.cfg.read().unwrap();
        if !cfg.rules.applies(host.as_ref().map(|h| h.host.as_str()), dst) {
            self.stats.excluded.fetch_add(1, Ordering::Relaxed);
            if let Some(h) = &host {
                self.remember(&h.host, false);
            }
            return Action::Pass;
        }
        let s = cfg.strategy.clone();
        drop(cfg);
        let action = build(p, &ip, data, tls, host.as_ref(), &s, |auto, fixed| self.fake_ttl(dst, auto, fixed));
        if matches!(action, Action::Replace { .. }) {
            self.stats.desynced.fetch_add(1, Ordering::Relaxed);
            if let Some(h) = &host {
                self.remember(&h.host, true);
            }
        }
        action
    }
}

fn build(p: &[u8], ip: &Ip, data: &[u8], tls: bool, host: Option<&pk::HostAt>, s: &crate::config::Strategy, ttl_for: impl Fn(bool, u8) -> u8) -> Action {
    let mut body = data.to_vec();
    if !tls && s.http_mixcase {
        pk::http_mix_host(&mut body);
    }
    let seq = pk::tcp_seq(p, ip);
    let mut pre = Vec::new();
    if s.fake {
        let fake = if tls { pk::fake_client_hello(FAKE_HOST) } else { pk::fake_http(FAKE_HOST) };
        for _ in 0..s.fake_count.clamp(1, 4) {
            let fseq = if s.fake_badseq { seq.wrapping_sub(10000) } else { seq };
            let mut f = pk::segment(p, ip, &fake, fseq);
            let fi = pk::parse(&f).expect("kendi ürettiğimiz paket");
            if s.fake_badseq {
                let a = ip.l4 + 8;
                let ack = u32::from_be_bytes(f[a..a + 4].try_into().unwrap()).wrapping_sub(66000);
                f[a..a + 4].copy_from_slice(&ack.to_be_bytes());
            }
            if s.fake_ttl {
                pk::set_ttl(&mut f, &fi, ttl_for(s.auto_ttl, s.ttl));
            }
            pk::fix(&mut f);
            if s.fake_badsum {
                let c = fi.l4 + 16;
                f[c] ^= 0x5a;
                f[c + 1] ^= 0xa5;
            }
            pre.push(f);
        }
    }
    let len = body.len();
    if s.split && len >= 2 {
        let pos = match host {
            Some(h) if s.split_at_host => h.offset + h.host.len() / 2,
            _ => s.split_pos as usize,
        }
        .clamp(1, len - 1);
        let a = pk::segment(p, ip, &body[..pos], seq);
        let b = pk::segment(p, ip, &body[pos..], seq.wrapping_add(pos as u32));
        if s.reverse {
            pre.push(b);
            return Action::Replace { pre, last: a };
        }
        pre.push(a);
        return Action::Replace { pre, last: b };
    }
    if pre.is_empty() && body == data {
        return Action::Pass;
    }
    Action::Replace { pre, last: pk::segment(p, ip, &body, seq) }
}

pub fn is_local(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(a) => a.is_private() || a.is_loopback() || a.is_link_local() || a.is_unspecified() || a.is_multicast() || (a.octets()[0] == 100 && a.octets()[1] & 0xc0 == 64),
        IpAddr::V6(a) => a.is_loopback() || a.is_unspecified() || a.is_multicast() || (a.segments()[0] & 0xfe00) == 0xfc00 || (a.segments()[0] & 0xffc0) == 0xfe80,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Rules, Strategy, preset};

    fn pkt(payload: &[u8]) -> Vec<u8> {
        let mut p = vec![0u8; 40];
        p[0] = 0x45;
        p[8] = 64;
        p[9] = pk::PROTO_TCP;
        p[12..16].copy_from_slice(&[192, 168, 1, 2]);
        p[16..20].copy_from_slice(&[8, 8, 4, 4]);
        p[22..24].copy_from_slice(&443u16.to_be_bytes());
        p[24..28].copy_from_slice(&5000u32.to_be_bytes());
        p[32] = 0x50;
        p[33] = pk::TCP_ACK | pk::TCP_PSH;
        p.extend_from_slice(payload);
        pk::fix(&mut p);
        p
    }

    fn cfg(s: Strategy) -> EngineConfig {
        EngineConfig { enabled: true, strategy: s, rules: Rules::default(), dns: None, block_quic: true }
    }

    fn reassemble(segs: &[&Vec<u8>]) -> Vec<u8> {
        let mut parts: Vec<(u32, Vec<u8>)> = segs
            .iter()
            .map(|s| {
                let i = pk::parse(s).unwrap();
                (pk::tcp_seq(s, &i), s[i.payload()..].to_vec())
            })
            .collect();
        parts.sort_by_key(|x| x.0);
        parts.into_iter().flat_map(|x| x.1).collect()
    }

    #[test]
    fn split_reverse_at_sni() {
        let hello = pk::fake_client_hello("discord.com");
        let p = pkt(&hello);
        let pr = Processor::new(cfg(preset("split-sni").unwrap().strategy));
        let Action::Replace { pre, last } = pr.process_out(&p) else { panic!() };
        assert_eq!(pre.len(), 1);
        let i0 = pk::parse(&pre[0]).unwrap();
        let il = pk::parse(&last).unwrap();
        assert!(pk::tcp_seq(&pre[0], &i0) > pk::tcp_seq(&last, &il));
        assert_eq!(reassemble(&[&pre[0], &last]), hello);
        assert_eq!(pr.status(true, None).recent[0], ("discord.com".to_string(), true));
    }

    #[test]
    fn fakes_and_exclusion() {
        let hello = pk::fake_client_hello("discord.com");
        let p = pkt(&hello);
        let pr = Processor::new(cfg(preset("fake-all").unwrap().strategy));
        let Action::Replace { pre, last } = pr.process_out(&p) else { panic!() };
        assert_eq!(pre.len(), 3);
        let fi = pk::parse(&pre[0]).unwrap();
        assert_eq!(pk::ttl(&pre[0], &fi), 4);
        assert_eq!(pk::tcp_seq(&pre[0], &fi), 5000u32.wrapping_sub(10000));
        assert_eq!(reassemble(&[&pre[2], &last]), hello);
        assert_eq!(last.len() - 40, 2);

        let mut c = cfg(Strategy::default());
        c.rules.list.push("discord.com".into());
        let pr = Processor::new(c);
        assert!(matches!(pr.process_out(&p), Action::Pass));
        assert_eq!(pr.status(true, None).excluded, 1);
    }

    #[test]
    fn non_hello_passes() {
        let p = pkt(b"\x17\x03\x03\x00\x10random-app-data");
        let pr = Processor::new(cfg(Strategy::default()));
        assert!(matches!(pr.process_out(&p), Action::Pass));
    }
}
