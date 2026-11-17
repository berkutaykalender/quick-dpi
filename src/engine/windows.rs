use super::{Action, EngineStatus, Processor};
use crate::config::{EngineConfig, Rules, Strategy};
use crate::packet as pk;
use std::collections::HashMap;
use std::net::IpAddr;
use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use windows_sys::Win32::Foundation::HANDLE;
use windows_sys::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};

const DLL: &[u8] = include_bytes!("../../vendor/windivert/WinDivert.dll");
const SYS: &[u8] = include_bytes!("../../vendor/windivert/WinDivert64.sys");
const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const INVALID: isize = -1;

type Addr = [u8; 80];
type OpenFn = unsafe extern "system" fn(*const u8, i32, i16, u64) -> HANDLE;
type RecvFn = unsafe extern "system" fn(HANDLE, *mut u8, u32, *mut u32, *mut Addr) -> i32;
type SendFn = unsafe extern "system" fn(HANDLE, *const u8, u32, *mut u32, *const Addr) -> i32;
type ShutdownFn = unsafe extern "system" fn(HANDLE, i32) -> i32;
type CloseFn = unsafe extern "system" fn(HANDLE) -> i32;

#[derive(Clone, Copy)]
struct Api {
    open: OpenFn,
    recv: RecvFn,
    send: SendFn,
    shutdown: ShutdownFn,
    close: CloseFn,
}

fn dir() -> PathBuf {
    std::env::var_os("ProgramData").map(PathBuf::from).unwrap_or_else(std::env::temp_dir).join("QuickDPI").join("WinDivert")
}

fn extract() -> Result<PathBuf, String> {
    let d = dir();
    std::fs::create_dir_all(&d).map_err(|e| format!("{}: {e}", d.display()))?;
    for (name, data) in [("WinDivert.dll", DLL), ("WinDivert64.sys", SYS)] {
        let p = d.join(name);
        if std::fs::read(&p).map(|b| b == data).unwrap_or(false) {
            continue;
        }
        std::fs::write(&p, data).map_err(|e| format!("{}: {e}", p.display()))?;
    }
    Ok(d.join("WinDivert.dll"))
}

fn load() -> Result<Api, String> {
    let path = extract()?;
    let wide: Vec<u16> = path.as_os_str().to_string_lossy().encode_utf16().chain(Some(0)).collect();
    unsafe {
        let m = LoadLibraryW(wide.as_ptr());
        if m.is_null() {
            return Err(format!("WinDivert.dll yüklenemedi ({})", std::io::Error::last_os_error()));
        }
        let f = |n: &[u8]| GetProcAddress(m, n.as_ptr()).ok_or_else(|| format!("WinDivert: {} yok", String::from_utf8_lossy(&n[..n.len() - 1])));
        Ok(Api {
            open: std::mem::transmute::<_, OpenFn>(f(b"WinDivertOpen\0")?),
            recv: std::mem::transmute::<_, RecvFn>(f(b"WinDivertRecv\0")?),
            send: std::mem::transmute::<_, SendFn>(f(b"WinDivertSend\0")?),
            shutdown: std::mem::transmute::<_, ShutdownFn>(f(b"WinDivertShutdown\0")?),
            close: std::mem::transmute::<_, CloseFn>(f(b"WinDivertClose\0")?),
        })
    }
}

pub fn filter(cfg: &EngineConfig) -> String {
    let mut out = vec!["(tcp.DstPort == 443 or tcp.DstPort == 80) and tcp.PayloadLength > 0".to_string()];
    if cfg.block_quic {
        out.push("udp.DstPort == 443".into());
    }
    let mut inb = vec!["tcp and (tcp.SrcPort == 443 or tcp.SrcPort == 80) and tcp.Syn and tcp.Ack".to_string()];
    if let Some(d) = &cfg.dns {
        out.push("udp.DstPort == 53".into());
        if let Some((a, p)) = d.v4 {
            inb.push(format!("udp.SrcPort == {p} and ip.SrcAddr == {a}"));
        }
        if let Some((a, p)) = d.v6 {
            inb.push(format!("udp.SrcPort == {p} and ipv6.SrcAddr == {a}"));
        }
    }
    let o = out.iter().map(|s| format!("({s})")).collect::<Vec<_>>().join(" or ");
    let i = inb.iter().map(|s| format!("({s})")).collect::<Vec<_>>().join(" or ");
    format!("!loopback and ((outbound and ({o})) or (inbound and ({i})))")
}

fn outbound(a: &Addr) -> bool {
    u32::from_le_bytes(a[8..12].try_into().unwrap()) & (1 << 17) != 0
}

fn mark_checksums(a: &Addr) -> Addr {
    let mut a = *a;
    let f = u32::from_le_bytes(a[8..12].try_into().unwrap()) | (1 << 21) | (1 << 22) | (1 << 23);
    a[8..12].copy_from_slice(&f.to_le_bytes());
    a
}

struct Running {
    handle: isize,
    thread: JoinHandle<()>,
    filter: String,
}

pub struct Handle {
    proc: Arc<Processor>,
    api: Mutex<Option<Api>>,
    run: Mutex<Option<Running>>,
    error: Mutex<Option<String>>,
    dns: Mutex<Option<crate::config::DnsTarget>>,
}

fn flush_dns() {
    let _ = std::process::Command::new("ipconfig").arg("/flushdns").creation_flags(CREATE_NO_WINDOW).stdout(std::process::Stdio::null()).status();
}

impl Handle {
    pub fn new() -> Self {
        let off = EngineConfig { enabled: false, strategy: Strategy::default(), rules: Rules::default(), dns: None, block_quic: false };
        Handle { proc: Arc::new(Processor::new(off)), api: Mutex::new(None), run: Mutex::new(None), error: Mutex::new(None), dns: Mutex::new(None) }
    }

    pub fn status(&self) -> EngineStatus {
        self.proc.status(self.run.lock().unwrap().is_some(), self.error.lock().unwrap().clone())
    }

    pub fn apply(&self, cfg: &EngineConfig, _user: bool) {
        *self.proc.cfg.write().unwrap() = cfg.clone();
        let want = cfg.enabled.then(|| filter(cfg));
        let mut run = self.run.lock().unwrap();
        if run.as_ref().map(|r| &r.filter) == want.as_ref() {
            return;
        }
        if let Some(r) = run.take() {
            self.stop(r);
        }
        *self.error.lock().unwrap() = None;
        if let Some(f) = want {
            match self.start(f) {
                Ok(r) => *run = Some(r),
                Err(e) => *self.error.lock().unwrap() = Some(e),
            }
        }
        let dns = if cfg.enabled { cfg.dns.clone() } else { None };
        let mut old = self.dns.lock().unwrap();
        if *old != dns {
            *old = dns;
            std::thread::spawn(flush_dns);
        }
    }

    fn api(&self) -> Result<Api, String> {
        let mut a = self.api.lock().unwrap();
        if a.is_none() {
            *a = Some(load()?);
        }
        Ok(a.unwrap())
    }

    fn start(&self, filter: String) -> Result<Running, String> {
        let api = self.api()?;
        let cf = std::ffi::CString::new(filter.clone()).unwrap();
        let h = unsafe { (api.open)(cf.as_ptr() as *const u8, 0, 0, 0) } as isize;
        if h == INVALID || h == 0 {
            let e = std::io::Error::last_os_error();
            let hint = match e.raw_os_error() {
                Some(5) => crate::i18n::tr(" (yönetici olarak çalıştırın)", " (run as administrator)"),
                Some(577) => crate::i18n::tr(" (sürücü imzası reddedildi; Secure Boot/HVCI)", " (driver signature rejected; Secure Boot/HVCI)"),
                _ => "",
            };
            return Err(format!("WinDivert: {e}{hint}"));
        }
        let proc = self.proc.clone();
        let thread = std::thread::spawn(move || packet_loop(api, h, proc));
        Ok(Running { handle: h, thread, filter })
    }

    fn stop(&self, r: Running) {
        if let Ok(api) = self.api() {
            unsafe {
                (api.shutdown)(r.handle as HANDLE, 3);
            }
            let _ = r.thread.join();
            unsafe {
                (api.close)(r.handle as HANDLE);
            }
        }
    }

    pub fn shutdown(&self) {
        if let Some(r) = self.run.lock().unwrap().take() {
            self.stop(r);
        }
        if self.dns.lock().unwrap().take().is_some() {
            flush_dns();
        }
    }
}

fn packet_loop(api: Api, h: isize, proc: Arc<Processor>) {
    let h = h as HANDLE;
    let mut buf = vec![0u8; 65535];
    let mut addr: Addr = [0; 80];
    let mut nat: HashMap<(u16, bool), IpAddr> = HashMap::new();
    let send = |p: &[u8], a: &Addr| unsafe {
        (api.send)(h, p.as_ptr(), p.len() as u32, std::ptr::null_mut(), a);
    };
    loop {
        let mut len = 0u32;
        if unsafe { (api.recv)(h, buf.as_mut_ptr(), buf.len() as u32, &mut len, &mut addr) } == 0 {
            let e = std::io::Error::last_os_error().raw_os_error();
            if e == Some(232) || e == Some(6) || e == Some(995) {
                break;
            }
            continue;
        }
        let p = &mut buf[..len as usize];
        let Some(ip) = pk::parse(p) else {
            send(p, &addr);
            continue;
        };
        let out = outbound(&addr);
        if ip.proto == pk::PROTO_TCP {
            if !out {
                proc.observe_inbound(p);
                send(p, &addr);
                continue;
            }
            match proc.process_out(p) {
                Action::Pass => send(p, &addr),
                Action::Replace { pre, last } => {
                    let a = mark_checksums(&addr);
                    for x in &pre {
                        send(x, &a);
                    }
                    send(&last, &a);
                }
            }
            continue;
        }
        let cfg = proc.cfg.read().unwrap();
        let (block_quic, dns) = (cfg.block_quic, cfg.dns.clone());
        drop(cfg);
        if out && pk::dst_port(p, &ip) == 443 && block_quic {
            continue;
        }
        let Some(d) = dns else {
            send(p, &addr);
            continue;
        };
        let target: Option<(IpAddr, u16)> = if ip.v6 { d.v6.map(|(a, p)| (IpAddr::V6(a), p)) } else { d.v4.map(|(a, p)| (IpAddr::V4(a), p)) };
        let Some((tip, tport)) = target else {
            if !(out && pk::dst_port(p, &ip) == 53) {
                send(p, &addr);
            }
            continue;
        };
        if out && pk::dst_port(p, &ip) == 53 {
            let orig = pk::dst_ip(p, &ip);
            if orig != tip && !orig.is_loopback() {
                if nat.len() > 4096 {
                    nat.clear();
                }
                nat.insert((pk::src_port(p, &ip), ip.v6), orig);
                pk::set_dst_ip(p, &ip, tip);
                pk::set_dst_port(p, &ip, tport);
                pk::fix(p);
                proc.stats.dns.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                send(p, &mark_checksums(&addr));
                continue;
            }
        } else if !out && pk::src_ip(p, &ip) == tip && pk::src_port(p, &ip) == tport {
            if let Some(orig) = nat.get(&(pk::dst_port(p, &ip), ip.v6)).copied() {
                pk::set_src_ip(p, &ip, orig);
                pk::set_src_port(p, &ip, 53);
                pk::fix(p);
                send(p, &mark_checksums(&addr));
                continue;
            }
        }
        send(p, &addr);
    }
}
