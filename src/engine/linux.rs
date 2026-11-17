use super::{Action, EngineStatus, Processor};
use crate::config::{DnsTarget, EngineConfig, Rules, Strategy};
use std::io::{BufRead, BufReader, Write};
use std::net::IpAddr;
use std::os::fd::AsRawFd;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{RecvTimeoutError, channel};
use std::sync::{Arc, Mutex};
use std::time::Duration;

const QNUM: u16 = 4451;
const MARK: u32 = 0x4451;
const TABLE: &str = "quickdpi";
const ACCT: &str = "/proc/sys/net/netfilter/nf_conntrack_acct";

static STOP: AtomicBool = AtomicBool::new(false);

extern "C" fn on_signal(_: libc::c_int) {
    STOP.store(true, Ordering::SeqCst);
}

pub fn ruleset(cfg: &EngineConfig) -> String {
    let mut s = format!("table inet {TABLE}\ndelete table inet {TABLE}\n");
    if !cfg.enabled {
        return s;
    }
    s += &format!("table inet {TABLE} {{\n");
    s += "  chain out {\n    type filter hook output priority mangle; policy accept;\n";
    s += &format!("    meta mark {MARK:#x} return\n");
    s += "    ip daddr { 0.0.0.0/8, 10.0.0.0/8, 100.64.0.0/10, 127.0.0.0/8, 169.254.0.0/16, 172.16.0.0/12, 192.168.0.0/16, 224.0.0.0/3 } return\n";
    s += "    ip6 daddr { ::1, fc00::/7, fe80::/10, ff00::/8 } return\n";
    if cfg.block_quic {
        s += "    udp dport 443 reject\n";
    }
    if cfg.dns.as_ref().is_some_and(|d| d.v6.is_none()) {
        s += "    meta l4proto { udp, tcp } th dport 53 meta nfproto ipv6 reject\n";
    }
    s += &format!("    tcp dport {{ 80, 443 }} ct original packets 1-6 queue num {QNUM} bypass\n  }}\n");
    s += "  chain in {\n    type filter hook input priority mangle; policy accept;\n";
    s += &format!("    tcp sport {{ 80, 443 }} tcp flags & (syn | ack) == syn | ack queue num {QNUM} bypass\n  }}\n");
    if let Some(d) = &cfg.dns {
        s += "  chain dns {\n    type nat hook output priority -100; policy accept;\n";
        s += &format!("    meta mark {MARK:#x} return\n    ip daddr 127.0.0.0/8 return\n    ip6 daddr ::1 return\n");
        if let Some((a, p)) = d.v4 {
            s += &format!("    ip daddr != {a} udp dport 53 dnat ip to {a}:{p}\n    ip daddr != {a} tcp dport 53 dnat ip to {a}:{p}\n");
        }
        if let Some((a, p)) = d.v6 {
            s += &format!("    ip6 daddr != {a} udp dport 53 dnat ip6 to [{a}]:{p}\n    ip6 daddr != {a} tcp dport 53 dnat ip6 to [{a}]:{p}\n");
        }
        s += "  }\n";
    }
    s += "}\n";
    s
}

fn nft(script: &str) -> Result<(), String> {
    let mut c = Command::new("nft").arg("-f").arg("-").stdin(Stdio::piped()).stdout(Stdio::null()).stderr(Stdio::piped()).spawn().map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            crate::i18n::tr("nftables kurulu değil. Kurmak için: sudo apt install nftables (Fedora: sudo dnf install nftables, Arch: sudo pacman -S nftables)", "nftables is not installed. Install it: sudo apt install nftables (Fedora: sudo dnf install nftables, Arch: sudo pacman -S nftables)").to_string()
        } else {
            format!("nft: {e}")
        }
    })?;
    c.stdin.take().unwrap().write_all(script.as_bytes()).map_err(|e| e.to_string())?;
    let out = c.wait_with_output().map_err(|e| e.to_string())?;
    if out.status.success() { Ok(()) } else { Err(format!("nft: {}", String::from_utf8_lossy(&out.stderr).trim())) }
}

fn flush_dns() {
    let _ = Command::new("resolvectl").arg("flush-caches").stdout(Stdio::null()).stderr(Stdio::null()).status();
}

struct RawSockets {
    v4: i32,
    v6: i32,
}

impl RawSockets {
    fn open() -> Result<Self, String> {
        let open = |fam| unsafe {
            let fd = libc::socket(fam, libc::SOCK_RAW, libc::IPPROTO_RAW);
            if fd >= 0 {
                let m = MARK;
                libc::setsockopt(fd, libc::SOL_SOCKET, libc::SO_MARK, &m as *const u32 as *const libc::c_void, 4);
            }
            fd
        };
        let v4 = open(libc::AF_INET);
        if v4 < 0 {
            return Err(format!("ham soket açılamadı: {}", std::io::Error::last_os_error()));
        }
        Ok(RawSockets { v4, v6: open(libc::AF_INET6) })
    }

    fn send(&self, p: &[u8]) {
        let Some(ip) = crate::packet::parse(p) else { return };
        unsafe {
            match crate::packet::dst_ip(p, &ip) {
                IpAddr::V4(a) => {
                    let mut sa: libc::sockaddr_in = std::mem::zeroed();
                    sa.sin_family = libc::AF_INET as u16;
                    sa.sin_addr.s_addr = u32::from_ne_bytes(a.octets());
                    libc::sendto(self.v4, p.as_ptr() as *const _, p.len(), 0, &sa as *const _ as *const libc::sockaddr, std::mem::size_of::<libc::sockaddr_in>() as u32);
                }
                IpAddr::V6(a) if self.v6 >= 0 => {
                    let mut sa: libc::sockaddr_in6 = std::mem::zeroed();
                    sa.sin6_family = libc::AF_INET6 as u16;
                    sa.sin6_addr.s6_addr = a.octets();
                    libc::sendto(self.v6, p.as_ptr() as *const _, p.len(), 0, &sa as *const _ as *const libc::sockaddr, std::mem::size_of::<libc::sockaddr_in6>() as u32);
                }
                _ => {}
            }
        }
    }
}

impl Drop for RawSockets {
    fn drop(&mut self) {
        unsafe {
            libc::close(self.v4);
            if self.v6 >= 0 {
                libc::close(self.v6);
            }
        }
    }
}

fn open_queue() -> Result<nfq::Queue, String> {
    let mut q = nfq::Queue::open().map_err(|e| format!("NFQUEUE açılamadı: {e}"))?;
    q.bind(QNUM).map_err(|e| format!("NFQUEUE {QNUM} bağlanamadı: {e}"))?;
    let _ = q.set_fail_open(QNUM, true);
    let _ = q.set_queue_max_len(QNUM, 4096);
    let _ = q.set_copy_range(QNUM, 0xffff);
    let _ = q.set_recv_enobufs(false);
    q.set_nonblocking(true);
    Ok(q)
}

fn queue_loop(mut q: nfq::Queue, raw: RawSockets, proc: Arc<Processor>) {
    let fd = q.as_raw_fd();
    while !STOP.load(Ordering::Relaxed) {
        let mut pfd = libc::pollfd { fd, events: libc::POLLIN, revents: 0 };
        if unsafe { libc::poll(&mut pfd, 1, 300) } <= 0 {
            continue;
        }
        loop {
            let mut msg = match q.recv() {
                Ok(m) => m,
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(_) => break,
            };
            if msg.get_hook() == 1 {
                proc.observe_inbound(msg.get_payload());
            } else if let Action::Replace { pre, last } = proc.process_out(msg.get_payload()) {
                for p in &pre {
                    raw.send(p);
                }
                msg.set_payload(last);
            }
            msg.set_verdict(nfq::Verdict::Accept);
            let _ = q.verdict(msg);
        }
    }
}

pub fn engine_main() -> i32 {
    if unsafe { libc::geteuid() } != 0 {
        eprintln!("quickdpi --engine root yetkisi ister");
        return 1;
    }
    unsafe {
        for s in [libc::SIGTERM, libc::SIGINT, libc::SIGHUP] {
            libc::signal(s, on_signal as *const () as libc::sighandler_t);
        }
        libc::signal(libc::SIGPIPE, libc::SIG_IGN);
    }
    let off = EngineConfig { enabled: false, strategy: Strategy::default(), rules: Rules::default(), dns: None, block_quic: false };
    let proc = Arc::new(Processor::new(off.clone()));
    let (tx, rx) = channel::<EngineConfig>();
    std::thread::spawn(move || {
        for line in std::io::stdin().lock().lines().map_while(Result::ok) {
            if let Ok(c) = serde_json::from_str::<EngineConfig>(&line) {
                if tx.send(c).is_err() {
                    break;
                }
            }
        }
    });

    let mut applied: Option<(bool, Option<DnsTarget>, bool)> = None;
    let mut queue: Option<std::thread::JoinHandle<()>> = None;
    let mut error: Option<String> = None;
    let mut acct_old: Option<String> = None;
    let mut out = std::io::stdout();
    let mut last_line = String::new();
    loop {
        match rx.recv_timeout(Duration::from_millis(700)) {
            Ok(cfg) => {
                *proc.cfg.write().unwrap() = cfg.clone();
                error = None;
                if cfg.enabled && queue.is_none() {
                    match open_queue().and_then(|q| RawSockets::open().map(|r| (q, r))) {
                        Ok((q, r)) => {
                            let p = proc.clone();
                            queue = Some(std::thread::spawn(move || queue_loop(q, r, p)));
                        }
                        Err(e) => error = Some(e),
                    }
                }
                let key = (cfg.enabled && error.is_none(), cfg.dns.clone(), cfg.block_quic);
                if applied.as_ref() != Some(&key) && error.is_none() {
                    let mut c = cfg.clone();
                    c.enabled = key.0;
                    if c.enabled && acct_old.is_none() {
                        acct_old = std::fs::read_to_string(ACCT).ok();
                    }
                    match nft(&ruleset(&c)) {
                        Ok(()) => {
                            if c.enabled {
                                let _ = std::fs::write(ACCT, "1");
                            }
                            if applied.as_ref().map(|a| (a.0, &a.1)) != Some((key.0, &key.1)) {
                                flush_dns();
                            }
                            applied = Some(key);
                        }
                        Err(e) => error = Some(e),
                    }
                }
            }
            Err(RecvTimeoutError::Disconnected) => break,
            Err(RecvTimeoutError::Timeout) => {}
        }
        if STOP.load(Ordering::SeqCst) {
            break;
        }
        let running = applied.as_ref().is_some_and(|a| a.0) && queue.is_some();
        let line = serde_json::to_string(&proc.status(running, error.clone())).unwrap_or_default();
        if line != last_line {
            if writeln!(out, "{line}").and_then(|_| out.flush()).is_err() {
                break;
            }
            last_line = line;
        }
    }
    STOP.store(true, Ordering::SeqCst);
    let _ = nft(&format!("table inet {TABLE}\ndelete table inet {TABLE}\n"));
    if let Some(v) = acct_old {
        let _ = std::fs::write(ACCT, v.trim());
    }
    if applied.is_some_and(|a| a.1.is_some()) {
        flush_dns();
    }
    if let Some(q) = queue {
        let _ = q.join();
    }
    0
}

pub struct Handle {
    child: Mutex<Option<(Child, ChildStdin)>>,
    status: Arc<Mutex<EngineStatus>>,
    denied: Arc<AtomicBool>,
}

impl Handle {
    pub fn new() -> Self {
        Handle { child: Mutex::new(None), status: Arc::new(Mutex::new(EngineStatus::default())), denied: Arc::new(AtomicBool::new(false)) }
    }

    pub fn status(&self) -> EngineStatus {
        self.status.lock().unwrap().clone()
    }

    pub fn apply(&self, cfg: &EngineConfig, user: bool) {
        let mut guard = self.child.lock().unwrap();
        if let Some((c, _)) = guard.as_mut() {
            if !matches!(c.try_wait(), Ok(None)) {
                *guard = None;
            }
        }
        if guard.is_none() {
            if !cfg.enabled {
                *self.status.lock().unwrap() = EngineStatus::default();
                return;
            }
            if self.denied.load(Ordering::SeqCst) && !user {
                return;
            }
            self.denied.store(false, Ordering::SeqCst);
            match self.spawn() {
                Ok(c) => *guard = Some(c),
                Err(e) => {
                    self.status.lock().unwrap().error = Some(e);
                    return;
                }
            }
        }
        let (_, stdin) = guard.as_mut().unwrap();
        let line = serde_json::to_string(cfg).unwrap();
        if writeln!(stdin, "{line}").and_then(|_| stdin.flush()).is_err() {
            *guard = None;
        }
    }

    fn spawn(&self) -> Result<(Child, ChildStdin), String> {
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let mut cmd = if unsafe { libc::geteuid() } == 0 {
            Command::new(&exe)
        } else {
            let mut c = Command::new("pkexec");
            c.arg(&exe);
            c
        };
        cmd.arg("--engine").stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::inherit());
        let mut child = cmd.spawn().map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                crate::i18n::tr("pkexec kurulu değil. Kurmak için: sudo apt install pkexec (Fedora/Arch: polkit paketi)", "pkexec is not installed. Install it: sudo apt install pkexec (Fedora/Arch: the polkit package)").to_string()
            } else {
                format!("pkexec: {e}")
            }
        })?;
        let stdin = child.stdin.take().unwrap();
        let stdout = child.stdout.take().unwrap();
        let status = self.status.clone();
        let denied = self.denied.clone();
        *status.lock().unwrap() = EngineStatus { error: None, ..Default::default() };
        std::thread::spawn(move || {
            let mut got = false;
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                if let Ok(s) = serde_json::from_str::<EngineStatus>(&line) {
                    got = true;
                    *status.lock().unwrap() = s;
                }
            }
            let mut s = status.lock().unwrap();
            s.running = false;
            if !got {
                denied.store(true, Ordering::SeqCst);
                s.error = Some(crate::i18n::tr("Yönetici yetkisi verilmedi", "Administrator permission was not granted").into());
            } else if s.error.is_none() {
                s.error = Some(crate::i18n::tr("Motor beklenmedik şekilde durdu", "Engine stopped unexpectedly").into());
            }
        });
        Ok((child, stdin))
    }

    pub fn shutdown(&self) {
        if let Some((mut c, stdin)) = self.child.lock().unwrap().take() {
            drop(stdin);
            for _ in 0..30 {
                if matches!(c.try_wait(), Ok(Some(_))) {
                    return;
                }
                std::thread::sleep(Duration::from_millis(100));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ruleset_shape() {
        let mut c = crate::config::Config::default().engine();
        let s = ruleset(&c);
        assert!(s.contains("queue num 4451 bypass"));
        assert!(s.contains("udp dport 443 reject"));
        assert!(s.contains("dnat ip to 77.88.8.8:1253"));
        assert!(s.contains("dnat ip6 to [2a02:6b8::feed:ff]:1253"));
        assert!(!s.contains("nfproto ipv6 reject"));
        let mut a = crate::config::Config::default();
        a.dns_server = "adguard".into();
        let s2 = ruleset(&a.engine());
        assert!(s2.contains("dnat ip to 94.140.14.14:5353") && s2.contains("nfproto ipv6 reject"));
        c.enabled = false;
        assert!(!ruleset(&c).contains("chain"));
    }
}
