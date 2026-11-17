use crate::config::{Config, EngineConfig, presets};
use crate::i18n::tr;
use crate::icon::Tone;
use crate::proto::{AppStatus, Cmd, TuneResult, TuneState};
use crate::tray::{Tray, View};
use crate::update::{self, UpdateState};
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{RecvTimeoutError, Sender, channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[cfg(target_os = "linux")]
use crate::engine::linux::Handle;
#[cfg(windows)]
use crate::engine::windows::Handle;

pub enum Event {
    Show,
    Toggle,
    AutoTune,
    Quit,
    Restart,
    Cmd(Cmd),
    WindowClosed(u64),
}

pub struct App {
    engine: Handle,
    cfg: Mutex<Config>,
    tune: Mutex<TuneState>,
    rev: AtomicU64,
    has_tray: std::sync::atomic::AtomicBool,
    update: Mutex<UpdateState>,
    latest: Mutex<Option<crate::update::Latest>>,
}

impl App {
    fn status(&self) -> AppStatus {
        let cfg = self.cfg.lock().unwrap();
        AppStatus { enabled: cfg.enabled, engine: self.engine.status(), strategy_name: cfg.strategy_name(), tune: self.tune.lock().unwrap().clone(), config_rev: self.rev.load(Ordering::SeqCst), tray: self.has_tray.load(Ordering::SeqCst), update: self.update.lock().unwrap().clone() }
    }

    fn apply(&self, user: bool) {
        if self.tune.lock().unwrap().running {
            return;
        }
        let ec = self.cfg.lock().unwrap().engine();
        self.engine.apply(&ec, user);
    }

    fn set_enabled(&self, on: bool) {
        {
            let mut c = self.cfg.lock().unwrap();
            c.enabled = on;
            c.save();
        }
        self.rev.fetch_add(1, Ordering::SeqCst);
        self.apply(true);
    }

    fn reload(&self) {
        let new = Config::load();
        {
            let mut c = self.cfg.lock().unwrap();
            if new.autostart != c.autostart {
                crate::autostart::set(new.autostart);
            }
            if new.lang != c.lang {
                crate::i18n::init(&new.lang);
            }
            *c = new;
        }
        self.apply(false);
    }
}

#[cfg(windows)]
pub fn tls() -> ureq::tls::TlsConfig {
    ureq::tls::TlsConfig::builder().provider(ureq::tls::TlsProvider::NativeTls).build()
}

#[cfg(not(windows))]
pub fn tls() -> ureq::tls::TlsConfig {
    ureq::tls::TlsConfig::default()
}

fn probe(host: &str) -> bool {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(8)))
        .http_status_as_error(false)
        .max_redirects(0)
        .user_agent("Mozilla/5.0 (QuickDPI)")
        .tls_config(tls())
        .build()
        .into();
    agent.get(&format!("https://{host}/")).call().is_ok()
}

fn probe_all(sites: &[String]) -> usize {
    let hs: Vec<_> = sites.iter().cloned().map(|s| std::thread::spawn(move || probe(&s))).collect();
    hs.into_iter().map(|h| h.join().unwrap_or(false)).filter(|ok| *ok).count()
}

fn wait_running(app: &App, max: Duration) -> Result<(), String> {
    let t = Instant::now();
    while t.elapsed() < max {
        let s = app.engine.status();
        if s.running {
            return Ok(());
        }
        if let Some(e) = s.error {
            return Err(e);
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    Err(tr("Motor başlatılamadı (zaman aşımı)", "Engine did not start (timeout)").into())
}

pub fn autotune(app: &Arc<App>) {
    {
        let mut t = app.tune.lock().unwrap();
        if t.running {
            return;
        }
        *t = TuneState { running: true, step: tr("Hazırlanıyor", "Preparing").into(), ..Default::default() };
    }
    let app = app.clone();
    std::thread::spawn(move || {
        let cfg = app.cfg.lock().unwrap().clone();
        let sites: Vec<String> = cfg.test_sites.iter().map(|s| crate::config::normalize_rule(s)).filter(|s| !s.is_empty()).collect();
        let total = sites.len();
        let base: EngineConfig = EngineConfig { enabled: true, ..cfg.engine() };
        let set_step = |s: String| app.tune.lock().unwrap().step = s;
        let finish = |msg: String, tuned: Option<String>| {
            if let Some(id) = tuned {
                let mut c = app.cfg.lock().unwrap();
                c.tuned = Some(id);
                c.tuned_at = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).ok().map(|d| d.as_secs());
                c.save();
                app.rev.fetch_add(1, Ordering::SeqCst);
            }
            {
                let mut t = app.tune.lock().unwrap();
                t.running = false;
                t.step.clear();
                t.message = Some(msg);
            }
            app.apply(false);
        };
        if total == 0 {
            finish(tr("Test sitesi yok: Gelişmiş sekmesinden ekleyin.", "No test sites: add some in the Advanced tab.").into(), None);
            return;
        }

        set_step(tr("Motor olmadan deneniyor", "Testing without the engine").into());
        app.engine.apply(&EngineConfig { enabled: false, ..base.clone() }, false);
        std::thread::sleep(Duration::from_millis(1500));
        let baseline = probe_all(&sites);
        app.tune.lock().unwrap().baseline = Some(baseline);

        let mut best: Option<(usize, &'static str)> = None;
        for p in presets() {
            set_step(p.name().to_string());
            app.engine.apply(&EngineConfig { strategy: p.strategy.clone(), ..base.clone() }, true);
            if let Err(e) = wait_running(&app, Duration::from_secs(120)) {
                finish(format!("{}: {e}", tr("Otomatik ayar durdu", "Auto-tune stopped")), None);
                return;
            }
            std::thread::sleep(Duration::from_millis(800));
            let ok = probe_all(&sites);
            app.tune.lock().unwrap().results.push(TuneResult { id: p.id.into(), name: p.name().into(), ok, total });
            if best.is_none_or(|b| ok > b.0) {
                best = Some((ok, p.id));
            }
            if ok == total {
                break;
            }
        }
        let (ok, id) = best.unwrap();
        let name = crate::config::preset(id).map(|p| p.name()).unwrap_or(id);
        let msg = if ok == 0 {
            tr("Hiçbir ayar test sitelerini açamadı. İnternet bağlantınızı ve test sitelerini kontrol edin.", "No setting could open the test sites. Check your connection and the test sites.").to_string()
        } else if baseline == total {
            format!("{} {name}", tr("Siteler koruma olmadan da açılıyordu. Yine de seçildi:", "Sites opened without protection too. Selected anyway:"))
        } else {
            format!("{} {name} ({ok}/{total})", tr("Seçildi:", "Selected:"))
        };
        finish(msg, (ok > 0).then(|| id.to_string()));
    });
}

struct Window {
    child: Child,
    stdin: ChildStdin,
    generation: u64,
    last: String,
}

impl Window {
    fn send(&mut self, line: &str) -> bool {
        writeln!(self.stdin, "{line}").and_then(|_| self.stdin.flush()).is_ok()
    }
}

fn spawn_window(tx: &Sender<Event>, generation: u64) -> Option<Window> {
    let exe = std::env::current_exe().ok()?;
    let mut child = Command::new(exe).arg("--window").stdin(Stdio::piped()).stdout(Stdio::piped()).spawn().ok()?;
    let stdin = child.stdin.take()?;
    let stdout = child.stdout.take()?;
    let tx = tx.clone();
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            if let Ok(c) = serde_json::from_str::<Cmd>(&line) {
                let _ = tx.send(Event::Cmd(c));
            }
        }
        let _ = tx.send(Event::WindowClosed(generation));
    });
    Some(Window { child, stdin, generation, last: String::new() })
}

fn view(s: &AppStatus) -> View {
    let (tone, tip) = if !s.enabled {
        (Tone::Off, tr("Kapalı", "Off").to_string())
    } else if s.tune.running {
        (Tone::Off, format!("{}: {}", tr("Otomatik ayar", "Auto-tune"), s.tune.step))
    } else if let Some(e) = &s.engine.error {
        (Tone::Error, e.clone())
    } else if s.engine.running {
        (Tone::On, format!("{} — {}", tr("Çalışıyor", "Active"), s.strategy_name))
    } else {
        (Tone::Off, tr("Başlatılıyor…", "Starting…").to_string())
    };
    View { enabled: s.enabled, tone, tip }
}

fn check_update(app: &Arc<App>, tx: &Sender<Event>, auto: bool) {
    if app.update.lock().unwrap().busy {
        return;
    }
    app.update.lock().unwrap().busy = true;
    let r = update::check();
    let mut u = app.update.lock().unwrap();
    u.busy = false;
    match r {
        Ok(Some(l)) => {
            u.available = Some(l.version.clone());
            u.message = None;
            *app.latest.lock().unwrap() = Some(l);
            let go = auto && update::silent() && app.cfg.lock().unwrap().auto_update;
            drop(u);
            if go {
                install_update(app, tx);
            }
        }
        Ok(None) => {
            u.available = None;
            if !auto {
                u.message = Some(tr("QuickDPI güncel.", "QuickDPI is up to date.").into());
            }
        }
        Err(e) => {
            if !auto {
                u.message = Some(format!("{}: {e}", tr("Güncelleme denetlenemedi", "Could not check for updates")));
            }
        }
    }
}

fn install_update(app: &Arc<App>, tx: &Sender<Event>) {
    let Some(l) = app.latest.lock().unwrap().clone() else { return };
    {
        let mut u = app.update.lock().unwrap();
        if u.busy {
            return;
        }
        u.busy = true;
        u.message = Some(format!("{} {}…", tr("Sürüm indiriliyor:", "Downloading version"), l.version));
    }
    let (app, tx) = (app.clone(), tx.clone());
    std::thread::spawn(move || {
        let r = update::install(&l);
        let mut u = app.update.lock().unwrap();
        u.busy = false;
        match r {
            Ok(()) => {
                u.message = Some(tr("Güncellendi, yeniden başlatılıyor…", "Updated, restarting…").into());
                let _ = tx.send(Event::Restart);
            }
            Err(e) => u.message = Some(format!("{}: {e}", tr("Güncelleme kurulamadı", "Update failed"))),
        }
    });
}

fn start(app: &Arc<App>) {
    let (enabled, untuned) = {
        let c = app.cfg.lock().unwrap();
        (c.enabled, c.strategy == "auto" && c.tuned.is_none())
    };
    if enabled {
        if untuned {
            autotune(app);
        } else {
            app.apply(true);
        }
    }
}

pub fn run(hidden: bool) {
    let first_run = !Config::exists();
    let cfg = Config::load();
    crate::i18n::init(&cfg.lang);
    if first_run {
        cfg.save();
        crate::autostart::set(cfg.autostart);
    }
    crate::autostart::ensure_menu_entry();
    let app = Arc::new(App { engine: Handle::new(), cfg: Mutex::new(cfg.clone()), tune: Mutex::new(TuneState::default()), rev: AtomicU64::new(0), has_tray: Default::default(), update: Mutex::new(UpdateState { needs_auth: !update::silent(), ..Default::default() }), latest: Mutex::new(None) });
    update::cleanup();
    let (tx, rx) = channel();
    let txs = tx.clone();
    crate::instance::listen(move || drop(txs.send(Event::Show)));
    {
        let (app, tx) = (app.clone(), tx.clone());
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_secs(20));
            loop {
                check_update(&app, &tx, true);
                std::thread::sleep(Duration::from_secs(6 * 3600));
            }
        });
    }

    let accepted = cfg.accepted();
    if accepted {
        start(&app);
    }

    let mut tray = Tray::start(tx.clone(), view(&app.status()));
    app.has_tray.store(tray.is_some(), Ordering::SeqCst);
    let mut generation = 0u64;
    let mut restart = false;
    let mut win = if !hidden || tray.is_none() || !accepted { spawn_window(&tx, generation) } else { None };
    let tick = if cfg!(windows) { Duration::from_millis(40) } else { Duration::from_millis(300) };
    loop {
        if let Some(t) = tray.as_mut() {
            t.pump();
        }
        match rx.recv_timeout(tick) {
            Ok(Event::Show) => {
                if !win.as_mut().is_some_and(|w| w.send("show")) {
                    generation += 1;
                    win = spawn_window(&tx, generation);
                }
            }
            Ok(Event::Toggle) if !app.cfg.lock().unwrap().accepted() => {
                if !win.as_mut().is_some_and(|w| w.send("show")) {
                    generation += 1;
                    win = spawn_window(&tx, generation);
                }
            }
            Ok(Event::Toggle) => {
                let on = !app.cfg.lock().unwrap().enabled;
                app.set_enabled(on);
            }
            Ok(Event::Cmd(Cmd::Accept)) => {
                *app.cfg.lock().unwrap() = Config::load();
                start(&app);
            }
            Ok(Event::AutoTune) | Ok(Event::Cmd(Cmd::AutoTune)) => {
                if app.cfg.lock().unwrap().accepted() {
                    autotune(&app);
                }
            }
            Ok(Event::Quit) | Ok(Event::Cmd(Cmd::Quit)) => break,
            Ok(Event::Restart) => {
                restart = true;
                break;
            }
            Ok(Event::Cmd(Cmd::CheckUpdate)) => {
                let (app, tx) = (app.clone(), tx.clone());
                std::thread::spawn(move || check_update(&app, &tx, false));
            }
            Ok(Event::Cmd(Cmd::InstallUpdate)) => install_update(&app, &tx),
            Ok(Event::Cmd(Cmd::SetEnabled(b))) => app.set_enabled(b),
            Ok(Event::Cmd(Cmd::Reload)) => app.reload(),
            Ok(Event::WindowClosed(g)) => {
                if win.as_ref().is_some_and(|w| w.generation == g) {
                    if let Some(mut w) = win.take() {
                        let _ = w.child.wait();
                    }
                    if tray.is_none() || !app.cfg.lock().unwrap().close_to_tray {
                        break;
                    }
                }
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => break,
        }
        let st = app.status();
        if let Some(t) = tray.as_mut() {
            t.update(view(&st));
        }
        if let Some(w) = win.as_mut() {
            let line = serde_json::to_string(&st).unwrap_or_default();
            if line != w.last {
                w.send(&format!("S {line}"));
                w.last = line;
            }
        }
    }
    let had_window = win.is_some();
    if let Some(mut w) = win.take() {
        w.send("quit");
        let t = Instant::now();
        while t.elapsed() < Duration::from_secs(2) && matches!(w.child.try_wait(), Ok(None)) {
            std::thread::sleep(Duration::from_millis(50));
        }
        let _ = w.child.kill();
    }
    app.engine.shutdown();
    if restart {
        if let Ok(exe) = std::env::current_exe() {
            let exe = update::running_path(exe);
            let mut c = Command::new(exe);
            c.arg("--updated");
            if !had_window {
                c.arg("--hidden");
            }
            let _ = c.spawn();
        }
    }
}
