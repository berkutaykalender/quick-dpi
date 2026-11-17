use crate::about;
use crate::config::{self, Config, RuleMode, Strategy, presets};
use crate::i18n::tr;
use crate::proto::{AppStatus, Cmd};
use eframe::egui::{self, Color32, CornerRadius, Key, KeyboardShortcut, Modifiers, RichText, Sense, Stroke, Vec2};
use std::io::{BufRead, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

static CTX: OnceLock<egui::Context> = OnceLock::new();
static QUITTING: AtomicBool = AtomicBool::new(false);
static SHOT_SENT: AtomicBool = AtomicBool::new(false);

const GREEN: Color32 = Color32::from_rgb(0x16, 0xa3, 0x4a);
const RED: Color32 = Color32::from_rgb(0xdc, 0x26, 0x26);
const AMBER: Color32 = Color32::from_rgb(0xd9, 0x77, 0x06);

const SC_TOGGLE: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::E);
const SC_TUNE: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::T);
const SC_QUIT: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::Q);
const SC_HIDE: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::W);
const SC_ADD: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::L);

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    General,
    Rules,
    Sites,
    Advanced,
    About,
}

const TABS: [Tab; 5] = [Tab::General, Tab::Rules, Tab::Sites, Tab::Advanced, Tab::About];
const TAB_KEYS: [Key; 5] = [Key::Num1, Key::Num2, Key::Num3, Key::Num4, Key::Num5];

fn tx(x: about::T) -> &'static str {
    tr(x.0, x.1)
}

fn open_url(url: &str) {
    #[cfg(windows)]
    let r = std::process::Command::new("explorer").arg(url).spawn();
    #[cfg(not(windows))]
    let r = std::process::Command::new("xdg-open").arg(url).spawn();
    drop(r);
}

struct Shared {
    status: Option<AppStatus>,
}

struct UiApp {
    cfg: Config,
    rev: u64,
    shared: Arc<Mutex<Shared>>,
    tab: Tab,
    new_rule: String,
    focus_rule: bool,
    sites_text: String,
    dns_text: String,
    dns_filter: String,
    dns_filter_focused: bool,
    pending: Option<(bool, Instant)>,
    terms_check: bool,
    show_terms: bool,
    logo: Option<egui::TextureHandle>,
}

fn send(c: &Cmd) {
    let mut out = std::io::stdout().lock();
    let _ = writeln!(out, "{}", serde_json::to_string(c).unwrap());
    let _ = out.flush();
}

fn shortcut_text(s: &KeyboardShortcut) -> String {
    CTX.get().map(|c| c.format_shortcut(s)).unwrap_or_default()
}

impl UiApp {
    fn new() -> Self {
        let cfg = Config::load();
        let shared = Arc::new(Mutex::new(Shared { status: None }));
        let sh = shared.clone();
        std::thread::spawn(move || {
            for line in std::io::stdin().lock().lines().map_while(Result::ok) {
                let Some(ctx) = CTX.get() else { continue };
                if let Some(js) = line.strip_prefix("S ") {
                    if let Ok(s) = serde_json::from_str::<AppStatus>(js) {
                        sh.lock().unwrap().status = Some(s);
                    }
                } else if line == "show" {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
                    ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                } else if line == "quit" {
                    QUITTING.store(true, Ordering::SeqCst);
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
                ctx.request_repaint();
            }
            QUITTING.store(true, Ordering::SeqCst);
            if let Some(ctx) = CTX.get() {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                ctx.request_repaint();
            }
        });
        UiApp {
            sites_text: cfg.test_sites.join("\n"),
            dns_text: cfg.dns_custom.clone(),
            dns_filter: String::new(),
            dns_filter_focused: false,
            cfg,
            rev: 0,
            shared,
            tab: Tab::General,
            new_rule: String::new(),
            focus_rule: false,
            pending: None,
            terms_check: false,
            show_terms: false,
            logo: None,
        }
    }

    fn save(&mut self) {
        self.cfg.save();
        send(&Cmd::Reload);
    }

    fn toggle(&mut self, st: Option<&AppStatus>) {
        if self.pending.is_some() {
            return;
        }
        let on = !st.map(|s| s.enabled).unwrap_or(self.cfg.enabled);
        self.cfg.enabled = on;
        self.pending = Some((on, Instant::now()));
        send(&Cmd::SetEnabled(on));
    }

    fn add_rule(&mut self, raw: &str) -> bool {
        let r = config::normalize_rule(raw);
        if r.is_empty() || self.cfg.rules.list.contains(&r) {
            return false;
        }
        self.cfg.rules.list.push(r);
        self.save();
        true
    }

    fn remove_rule(&mut self, r: &str) {
        self.cfg.rules.list.retain(|x| x != r);
        self.save();
    }

    fn shortcuts(&mut self, ctx: &egui::Context, st: Option<&AppStatus>) {
        if !self.cfg.accepted() {
            return;
        }
        let typing = ctx.memory(|m| m.focused().is_some());
        let (toggle, tune, quit, hide, add) = ctx.input_mut(|i| (i.consume_shortcut(&SC_TOGGLE), i.consume_shortcut(&SC_TUNE), i.consume_shortcut(&SC_QUIT), i.consume_shortcut(&SC_HIDE), i.consume_shortcut(&SC_ADD)));
        let esc = !typing && ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape));
        let tabs = TAB_KEYS;
        for (k, t) in tabs.iter().zip(TABS) {
            if ctx.input_mut(|i| i.consume_key(Modifiers::COMMAND, *k)) {
                self.tab = t;
            }
        }
        if toggle {
            self.toggle(st);
        }
        if tune && !st.is_some_and(|s| s.tune.running) {
            send(&Cmd::AutoTune);
        }
        if quit {
            QUITTING.store(true, Ordering::SeqCst);
            send(&Cmd::Quit);
        }
        if hide || esc {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        if add {
            self.tab = Tab::Rules;
            self.focus_rule = true;
        }
    }
}

fn state_of(st: Option<&AppStatus>) -> (Color32, &'static str, String) {
    let Some(s) = st else { return (Color32::GRAY, tr("Bağlanıyor…", "Connecting…"), String::new()) };
    if !s.enabled {
        return (Color32::GRAY, tr("Kapalı", "Off"), tr("Ağ koruması kapalı. Açmak için düğmeye basın.", "Network protection is off. Press the button to turn it on.").into());
    }
    if s.tune.running {
        return (AMBER, tr("Otomatik ayarlanıyor", "Auto-tuning"), s.tune.step.clone());
    }
    if let Some(e) = &s.engine.error {
        return (RED, tr("Hata", "Error"), e.clone());
    }
    if s.engine.running {
        return (GREEN, tr("Koruma açık", "Protection on"), s.strategy_name.clone());
    }
    (AMBER, tr("Başlatılıyor…", "Starting…"), tr("Yönetici izni isteniyor olabilir.", "Administrator permission may be requested.").into())
}

fn power_button(ui: &mut egui::Ui, color: Color32, busy: bool) -> egui::Response {
    let size = Vec2::splat(92.0);
    let (rect, resp) = ui.allocate_exact_size(size, if busy { Sense::hover() } else { Sense::click() });
    let resp = resp.on_hover_text(format!("{} ({})", tr("Aç / kapat", "Turn on / off"), shortcut_text(&SC_TOGGLE)));
    let p = ui.painter();
    let c = rect.center();
    let hover = resp.hovered() && !busy;
    let ring = if hover { color.gamma_multiply(1.15) } else { color };
    p.circle_filled(c, 46.0, ring.gamma_multiply(0.18));
    p.circle_filled(c, 38.0, ring);
    let w = Stroke::new(5.0, Color32::WHITE);
    let r = 16.0;
    let arc: Vec<egui::Pos2> = (0..=40)
        .map(|i| {
            let a = (-90.0f32 + 30.0 + 300.0 * i as f32 / 40.0).to_radians();
            c + Vec2::new(a.cos(), a.sin()) * r
        })
        .collect();
    p.add(egui::Shape::line(arc, w));
    p.line_segment([c + Vec2::new(0.0, -22.0), c + Vec2::new(0.0, -4.0)], w);
    if resp.has_focus() {
        p.circle_stroke(c, 45.0, ui.visuals().selection.stroke);
    }
    if hover {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp
}

fn card<R>(ui: &mut egui::Ui, title: &str, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    egui::Frame::group(ui.style())
        .fill(ui.visuals().faint_bg_color)
        .corner_radius(CornerRadius::same(8))
        .inner_margin(egui::Margin::same(12))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            if !title.is_empty() {
                ui.label(RichText::new(title).strong());
                ui.add_space(4.0);
            }
            add(ui)
        })
        .inner
}

fn stat(ui: &mut egui::Ui, value: u64, label: &str, tip: &str) {
    ui.vertical(|ui| {
        ui.add(egui::Label::new(RichText::new(value.to_string()).size(18.0).strong()).selectable(false));
        ui.add(egui::Label::new(RichText::new(label).small().weak()).selectable(false));
    })
    .response
    .on_hover_text(tip);
}

impl UiApp {
    fn header(&mut self, ui: &mut egui::Ui, st: Option<&AppStatus>) {
        let (color, title, sub) = state_of(st);
        let tuning = st.is_some_and(|s| s.tune.running);
        if let Some((want, t)) = self.pending {
            if st.is_some_and(|s| s.enabled == want) || t.elapsed() > Duration::from_secs(3) {
                self.pending = None;
            }
        }
        let busy = self.pending.is_some() || tuning;
        ui.horizontal(|ui| {
            let resp = power_button(ui, color, busy);
            if resp.clicked() {
                self.toggle(st);
            }
            resp.context_menu(|ui| {
                let on = st.map(|s| s.enabled).unwrap_or(self.cfg.enabled);
                if ui.add(egui::Button::new(if on { tr("Kapat", "Turn off") } else { tr("Aç", "Turn on") }).shortcut_text(shortcut_text(&SC_TOGGLE))).clicked() {
                    self.toggle(st);
                    ui.close();
                }
                if ui.add_enabled(!tuning, egui::Button::new(tr("Otomatik ayarla", "Auto-tune")).shortcut_text(shortcut_text(&SC_TUNE))).clicked() {
                    send(&Cmd::AutoTune);
                    ui.close();
                }
            });
            ui.add_space(8.0);
            ui.vertical(|ui| {
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.add(egui::Label::new(RichText::new(title).size(22.0).strong().color(color)).selectable(false));
                    if tuning {
                        ui.spinner();
                    }
                });
                if !sub.is_empty() {
                    let sub_rt = RichText::new(&sub);
                    ui.add(egui::Label::new(if st.is_some_and(|s| s.engine.error.is_some() && s.enabled) { sub_rt.color(RED) } else { sub_rt.weak() }).wrap());
                }
                if let Some(s) = st.filter(|s| s.enabled && s.engine.error.is_some() && !s.tune.running) {
                    let _ = s;
                    if ui.button(tr("Yeniden dene", "Retry")).clicked() {
                        send(&Cmd::SetEnabled(true));
                    }
                }
                if let Some(s) = st.filter(|s| s.enabled && s.engine.running) {
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 22.0;
                        stat(ui, s.engine.desynced, tr("korunan", "protected"), tr("Koruma uygulanan bağlantılar", "Connections protected"));
                        stat(ui, s.engine.excluded, tr("istisna", "excluded"), tr("İstisna kuralına takılan bağlantılar", "Connections matched by an exclusion rule"));
                        stat(ui, s.engine.packets, tr("incelenen", "inspected"), tr("Motorun baktığı ilk veri paketleri", "First data packets inspected by the engine"));
                    });
                }
            });
        });
    }

    fn tabs(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            for (i, (t, name)) in [(Tab::General, tr("Genel", "General")), (Tab::Rules, tr("İstisnalar", "Exclusions")), (Tab::Sites, tr("Son siteler", "Recent sites")), (Tab::Advanced, tr("Gelişmiş", "Advanced")), (Tab::About, tr("Hakkında", "About"))].into_iter().enumerate() {
                let sc = KeyboardShortcut::new(Modifiers::COMMAND, TAB_KEYS[i]);
                if ui.selectable_label(self.tab == t, RichText::new(name).size(14.5)).on_hover_text(shortcut_text(&sc)).clicked() {
                    self.tab = t;
                }
            }
        });
    }

    fn general(&mut self, ui: &mut egui::Ui, st: Option<&AppStatus>) {
        let tuning = st.is_some_and(|s| s.tune.running);
        card(ui, tr("Strateji", "Strategy"), |ui| {
            let label = |id: &str| -> String {
                match id {
                    "auto" => tr("Otomatik (önerilen)", "Automatic (recommended)").into(),
                    "custom" => tr("Özel (Gelişmiş sekmesi)", "Custom (Advanced tab)").into(),
                    _ => config::preset(id).map(|p| p.name().to_string()).unwrap_or_else(|| id.to_string()),
                }
            };
            let mut sel = self.cfg.strategy.clone();
            egui::ComboBox::from_id_salt("strategy").width(ui.available_width()).selected_text(label(&sel)).show_ui(ui, |ui| {
                ui.selectable_value(&mut sel, "auto".to_string(), label("auto"));
                for p in presets() {
                    ui.selectable_value(&mut sel, p.id.to_string(), p.name());
                }
                ui.selectable_value(&mut sel, "custom".to_string(), label("custom"));
            });
            if sel != self.cfg.strategy {
                self.cfg.strategy = sel;
                self.save();
            }
            if self.cfg.strategy == "auto" {
                let found = self.cfg.tuned.as_deref().map(label).unwrap_or_else(|| tr("henüz denenmedi", "not tested yet").into());
                ui.add(egui::Label::new(RichText::new(format!("{} {found}", tr("Bulunan:", "Found:"))).weak()).wrap());
            }
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                let b = ui.add_enabled(!tuning, egui::Button::new(tr("Otomatik ayarla", "Auto-tune")).shortcut_text(shortcut_text(&SC_TUNE)));
                if b.on_hover_text(tr("Hazır ayarları sırayla dener, test sitelerini açan ilkini seçer.", "Tries presets in order and picks the first that opens the test sites.")).clicked() {
                    send(&Cmd::AutoTune);
                }
                if tuning {
                    ui.spinner();
                    if let Some(s) = st {
                        ui.add(egui::Label::new(RichText::new(&s.tune.step).weak()).truncate());
                    }
                }
            });
            if let Some(s) = st {
                let t = &s.tune;
                if let Some(b) = t.baseline {
                    let total = s.tune.results.first().map(|r| r.total).unwrap_or(self.cfg.test_sites.len());
                    ui.add_space(4.0);
                    egui::Grid::new("tune").num_columns(2).spacing([12.0, 4.0]).show(ui, |ui| {
                        ui.label(RichText::new(tr("Motor kapalıyken", "Without engine")).weak());
                        ui.label(format!("{b}/{total}"));
                        ui.end_row();
                        for r in &t.results {
                            ui.add(egui::Label::new(&r.name).truncate());
                            let c = if r.ok == r.total { GREEN } else if r.ok == 0 { RED } else { AMBER };
                            ui.label(RichText::new(format!("{}/{}", r.ok, r.total)).color(c));
                            ui.end_row();
                        }
                    });
                }
                if let Some(m) = &t.message {
                    ui.add_space(4.0);
                    ui.add(egui::Label::new(m).wrap());
                }
            }
        });
        ui.add_space(8.0);
        card(ui, tr("Ağ güvenliği", "Network security"), |ui| {
            let mut changed = false;
            changed |= ui
                .checkbox(&mut self.cfg.dns_redirect, tr("Güvenli DNS kullan", "Use secure DNS"))
                .on_hover_text(tr("Sistemin tüm DNS sorguları seçilen sunucuya gider.", "All system DNS queries go to the selected server."))
                .changed();
            if self.cfg.dns_redirect {
                ui.indent("dns", |ui| {
                    let mut srv = self.cfg.dns_server.clone();
                    let all = config::dns_presets();
                    let current = if srv == "custom" { tr("Özel", "Custom").to_string() } else { all.iter().find(|d| d.id == srv).map(|d| d.label()).unwrap_or(srv.clone()) };
                    let combo = egui::ComboBox::from_id_salt("dns").width(ui.available_width().min(340.0)).height(320.0).selected_text(current).show_ui(ui, |ui| {
                        let r = ui.add(egui::TextEdit::singleline(&mut self.dns_filter).hint_text(tr("Ara: ad ya da IP", "Search: name or IP")).desired_width(f32::INFINITY));
                        if !self.dns_filter_focused {
                            r.request_focus();
                            self.dns_filter_focused = true;
                        }
                        let q = self.dns_filter.to_lowercase();
                        let hits: Vec<&config::DnsPreset> = all.iter().filter(|d| q.is_empty() || d.label().to_lowercase().contains(&q)).collect();
                        if r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                            if let Some(d) = hits.first() {
                                srv = d.id.to_string();
                                ui.close();
                            }
                        }
                        for (alt, head) in [(true, tr("Özel port (önerilen)", "Dedicated port (recommended)")), (false, tr("Standart port 53", "Standard port 53"))] {
                            let group: Vec<_> = hits.iter().filter(|d| d.alt_port == alt).collect();
                            if group.is_empty() {
                                continue;
                            }
                            ui.add_space(4.0);
                            ui.label(RichText::new(head).small().weak());
                            for d in group {
                                let mut text = d.label();
                                if d.target.v6.is_some() {
                                    text += "  · IPv6";
                                }
                                ui.selectable_value(&mut srv, d.id.to_string(), text);
                            }
                        }
                        if hits.is_empty() {
                            ui.label(RichText::new(tr("Eşleşme yok", "No match")).weak());
                        }
                        ui.separator();
                        ui.selectable_value(&mut srv, "custom".to_string(), tr("Özel (IP:port yazın)", "Custom (enter IP:port)"));
                    });
                    if combo.inner.is_none() {
                        self.dns_filter.clear();
                        self.dns_filter_focused = false;
                    }
                    if srv != self.cfg.dns_server {
                        self.cfg.dns_server = srv;
                        changed = true;
                    }
                    if self.cfg.dns_server == "custom" {
                        let ok = config::parse_dns_custom(&self.dns_text).is_some();
                        let bad = (!ok && !self.dns_text.is_empty()).then_some(RED);
                        let r = ui.add(egui::TextEdit::singleline(&mut self.dns_text).hint_text("1.2.3.4:5353, [2001:db8::1]:5353").text_color_opt(bad));
                        if r.lost_focus() || (r.changed() && ok) {
                            if self.cfg.dns_custom != self.dns_text && ok {
                                self.cfg.dns_custom = self.dns_text.clone();
                                changed = true;
                            }
                        }
                    }
                });
            }
            changed |= ui
                .checkbox(&mut self.cfg.block_quic, tr("QUIC'i (HTTP/3) kapat", "Disable QUIC (HTTP/3)"))
                .on_hover_text(tr("Tarayıcılar TLS'e döner; koruma TLS bağlantılarında çalışır.", "Browsers fall back to TLS, where protection works."))
                .changed();
            if changed {
                self.save();
            }
        });
        ui.add_space(8.0);
        card(ui, tr("Uygulama", "Application"), |ui| {
            let mut changed = false;
            changed |= ui.checkbox(&mut self.cfg.autostart, tr("Oturum açılınca başlat (tepside)", "Start at login (in the tray)")).changed();
            changed |= ui.checkbox(&mut self.cfg.close_to_tray, tr("Pencere kapanınca tepside çalışmaya devam et", "Keep running in the tray when the window closes")).changed();
            ui.horizontal(|ui| {
                ui.label(tr("Dil", "Language"));
                let mut l = self.cfg.lang.clone();
                let name = |s: &str| match s {
                    "tr" => "Türkçe",
                    "en" => "English",
                    _ => tr("Sistem", "System"),
                };
                egui::ComboBox::from_id_salt("lang").selected_text(name(&l)).show_ui(ui, |ui| {
                    for v in ["auto", "tr", "en"] {
                        ui.selectable_value(&mut l, v.to_string(), name(v));
                    }
                });
                if l != self.cfg.lang {
                    self.cfg.lang = l;
                    crate::i18n::init(&self.cfg.lang);
                    changed = true;
                }
            });
            if changed {
                self.save();
            }
        });
    }

    fn rules(&mut self, ui: &mut egui::Ui) {
        card(ui, tr("Hangi sitelere uygulansın?", "Apply to which sites?"), |ui| {
            let mut m = self.cfg.rules.mode;
            ui.radio_value(&mut m, RuleMode::Except, tr("Tüm sitelere, listedekiler hariç", "All sites except those listed"));
            ui.radio_value(&mut m, RuleMode::Only, tr("Yalnızca listedeki sitelere", "Only the sites listed"));
            if m != self.cfg.rules.mode {
                self.cfg.rules.mode = m;
                self.save();
            }
            ui.add(egui::Label::new(RichText::new(tr("Alan adı alt alan adlarını da kapsar: ornek.com yazınca www.ornek.com da dahil olur. IP ya da CIDR de yazılabilir.", "A domain also covers its subdomains (example.com covers www.example.com). IPs and CIDRs work too.")).small().weak()).wrap());
        });
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            let w = ui.available_width() - 80.0;
            let r = ui.add(egui::TextEdit::singleline(&mut self.new_rule).desired_width(w).hint_text(tr("ornek.com ya da 1.2.3.0/24", "example.com or 1.2.3.0/24")));
            if self.focus_rule {
                r.request_focus();
                self.focus_rule = false;
            }
            let enter = r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
            let add = ui.add_enabled(!self.new_rule.trim().is_empty(), egui::Button::new(tr("Ekle", "Add"))).on_hover_text(shortcut_text(&SC_ADD)).clicked();
            if (enter || add) && !self.new_rule.trim().is_empty() {
                let raw = std::mem::take(&mut self.new_rule);
                self.add_rule(&raw);
                r.request_focus();
            }
        });
        ui.add_space(6.0);
        if self.cfg.rules.list.is_empty() {
            ui.add(egui::Label::new(RichText::new(match self.cfg.rules.mode {
                RuleMode::Except => tr("Liste boş: koruma tüm sitelere uygulanır.", "The list is empty: protection applies to every site."),
                RuleMode::Only => tr("Liste boş: hiçbir siteye uygulanmaz (adı okunamayanlar hariç).", "The list is empty: nothing is protected (except unnamed connections)."),
            }).weak()).wrap());
            return;
        }
        let mut remove = None;
        egui::ScrollArea::vertical().auto_shrink([false, true]).show(ui, |ui| {
            for r in &self.cfg.rules.list {
                let row = ui.horizontal(|ui| {
                    ui.add(egui::Label::new(r).selectable(false).truncate());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.small_button(tr("Kaldır", "Remove")).clicked() {
                            remove = Some(r.clone());
                        }
                    });
                });
                let resp = ui.interact(row.response.rect, ui.scope_id().with(("rule", r)), Sense::click());
                resp.context_menu(|ui| {
                    if ui.button(tr("Kaldır", "Remove")).clicked() {
                        remove = Some(r.clone());
                        ui.close();
                    }
                    if ui.button(tr("Kopyala", "Copy")).clicked() {
                        ui.ctx().copy_text(r.clone());
                        ui.close();
                    }
                });
                ui.separator();
            }
        });
        if let Some(r) = remove {
            self.remove_rule(&r);
        }
    }

    fn sites(&mut self, ui: &mut egui::Ui, st: Option<&AppStatus>) {
        let recent = st.map(|s| s.engine.recent.clone()).unwrap_or_default();
        ui.add(egui::Label::new(RichText::new(tr("Son bağlanılan siteler. Sağ tıkla listeye ekleyip çıkarabilirsiniz.", "Recently visited sites. Right-click to add or remove them from the list.")).weak()).wrap());
        ui.add_space(6.0);
        if recent.is_empty() {
            ui.label(RichText::new(tr("Henüz site yok.", "No sites yet.")).weak());
            return;
        }
        let mut add = None;
        let mut remove = None;
        egui::ScrollArea::vertical().auto_shrink([false, true]).show(ui, |ui| {
            for (host, applied) in &recent {
                let listed = self.cfg.rules.list.iter().any(|r| host == r || host.ends_with(&format!(".{r}")));
                let row = ui.horizontal(|ui| {
                    let (c, t) = if *applied { (GREEN, tr("korundu", "protected")) } else { (Color32::GRAY, tr("istisna", "excluded")) };
                    ui.add_sized([78.0, 18.0], egui::Label::new(RichText::new(t).small().color(c)).selectable(false));
                    ui.add(egui::Label::new(host).selectable(false).truncate());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if listed {
                            if ui.small_button(tr("Listeden çıkar", "Unlist")).clicked() {
                                remove = Some(host.clone());
                            }
                        } else if ui.small_button(tr("Listeye ekle", "Add to list")).clicked() {
                            add = Some(host.clone());
                        }
                    });
                });
                let resp = ui.interact(row.response.rect, ui.scope_id().with(("recent", host)), Sense::click());
                resp.context_menu(|ui| {
                    if listed {
                        if ui.button(tr("Listeden çıkar", "Remove from list")).clicked() {
                            remove = Some(host.clone());
                            ui.close();
                        }
                    } else {
                        if ui.button(tr("Listeye ekle", "Add to list")).clicked() {
                            add = Some(host.clone());
                            ui.close();
                        }
                        let parent = host.splitn(2, '.').nth(1).filter(|p| p.contains('.')).map(str::to_string);
                        if let Some(p) = parent {
                            if ui.button(format!("{} *.{p}", tr("Listeye ekle:", "Add to list:"))).clicked() {
                                add = Some(p);
                                ui.close();
                            }
                        }
                    }
                    if ui.button(tr("Kopyala", "Copy")).clicked() {
                        ui.ctx().copy_text(host.clone());
                        ui.close();
                    }
                });
                ui.separator();
            }
        });
        if let Some(h) = add {
            self.add_rule(&h);
        }
        if let Some(h) = remove {
            let rule = self.cfg.rules.list.iter().find(|r| h == **r || h.ends_with(&format!(".{r}"))).cloned();
            if let Some(r) = rule {
                self.remove_rule(&r);
            }
        }
    }

    fn advanced(&mut self, ui: &mut egui::Ui, st: Option<&AppStatus>) {
        card(ui, tr("Özel strateji", "Custom strategy"), |ui| {
            ui.add(egui::Label::new(RichText::new(tr("Burada değişiklik yapmak stratejiyi \"Özel\" yapar.", "Changing anything here switches the strategy to \"Custom\".")).small().weak()).wrap());
            let mut s: Strategy = if self.cfg.strategy == "custom" { self.cfg.custom.clone() } else { self.cfg.strategy_value() };
            let before = s.clone();
            ui.checkbox(&mut s.split, tr("İlk paketi böl", "Split the first packet"));
            ui.add_enabled_ui(s.split, |ui| {
                ui.indent("split", |ui| {
                    ui.checkbox(&mut s.split_at_host, tr("Site adının ortasından böl", "Split in the middle of the hostname"));
                    ui.add_enabled_ui(!s.split_at_host, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(tr("Bölme noktası (bayt)", "Split position (bytes)"));
                            ui.add(egui::DragValue::new(&mut s.split_pos).range(1..=1000));
                        });
                    });
                    ui.checkbox(&mut s.reverse, tr("Ters sırada gönder", "Send in reverse order"));
                });
            });
            ui.checkbox(&mut s.fake, tr("Önce ön paket gönder", "Send a decoy packet first"));
            ui.add_enabled_ui(s.fake, |ui| {
                ui.indent("fake", |ui| {
                    ui.horizontal(|ui| {
                        ui.checkbox(&mut s.fake_ttl, tr("Düşük TTL", "Low TTL"));
                        ui.add_enabled_ui(s.fake_ttl, |ui| {
                            ui.checkbox(&mut s.auto_ttl, tr("otomatik", "automatic"));
                            ui.add(egui::DragValue::new(&mut s.ttl).range(1..=64).prefix("TTL "));
                        });
                    });
                    ui.checkbox(&mut s.fake_badsum, tr("Bozuk sağlama toplamı", "Bad checksum"));
                    ui.checkbox(&mut s.fake_badseq, tr("Yanlış sıra numarası", "Wrong sequence number"));
                    ui.horizontal(|ui| {
                        ui.label(tr("Adet", "Count"));
                        ui.add(egui::DragValue::new(&mut s.fake_count).range(1..=4));
                    });
                });
            });
            ui.checkbox(&mut s.http_mixcase, tr("HTTP'de Host başlığını karıştır (hoSt:)", "Mix the HTTP Host header case (hoSt:)"));
            if s != before {
                self.cfg.custom = s;
                self.cfg.strategy = "custom".into();
                self.save();
            }
        });
        ui.add_space(8.0);
        card(ui, tr("Test siteleri", "Test sites"), |ui| {
            ui.add(egui::Label::new(RichText::new(tr("Otomatik ayar bağlantıyı bu sitelerle sınar. Her satıra bir site.", "Auto-tune checks the connection with these sites. One per line.")).small().weak()).wrap());
            let r = ui.add(egui::TextEdit::multiline(&mut self.sites_text).desired_rows(4).desired_width(f32::INFINITY));
            if r.lost_focus() {
                let list: Vec<String> = self.sites_text.lines().map(config::normalize_rule).filter(|s| !s.is_empty()).collect();
                if list != self.cfg.test_sites {
                    self.cfg.test_sites = list;
                    self.save();
                }
            }
        });
        if let Some(s) = st {
            ui.add_space(8.0);
            card(ui, tr("Motor", "Engine"), |ui| {
                egui::Grid::new("eng").num_columns(2).spacing([16.0, 4.0]).show(ui, |ui| {
                    ui.label(tr("Durum", "State"));
                    ui.label(if s.engine.running { tr("çalışıyor", "running") } else { tr("durdu", "stopped") });
                    ui.end_row();
                    ui.label(tr("Yönlendirilen DNS sorgusu", "Redirected DNS queries"));
                    ui.label(if cfg!(windows) { s.engine.dns.to_string() } else { tr("nftables DNAT", "nftables DNAT").to_string() });
                    ui.end_row();
                    ui.label(tr("Platform", "Platform"));
                    ui.label(if cfg!(windows) { "WinDivert 2.2.2" } else { "nftables + NFQUEUE" });
                    ui.end_row();
                });
            });
        }
    }
}

impl UiApp {
    fn logo(&mut self, ctx: &egui::Context) -> egui::TextureHandle {
        self.logo
            .get_or_insert_with(|| {
                let img = egui::ColorImage::from_rgba_unmultiplied([96, 96], &crate::icon::rgba(96, crate::icon::Tone::Brand));
                ctx.load_texture("logo", img, egui::TextureOptions::LINEAR)
            })
            .clone()
    }

    fn update_card(&mut self, ui: &mut egui::Ui, st: Option<&AppStatus>) {
        let u = st.map(|s| s.update.clone()).unwrap_or_default();
        card(ui, tr("Güncelleme", "Updates"), |ui| {
            match &u.available {
                Some(v) => ui.label(RichText::new(format!("{} {v} ({} {})", tr("Yeni sürüm:", "New version:"), tr("kurulu", "installed"), crate::update::CURRENT)).strong()),
                None => ui.label(format!("{} {}", tr("Kurulu sürüm:", "Installed version:"), crate::update::CURRENT)),
            };
            if let Some(m) = &u.message {
                ui.add(egui::Label::new(RichText::new(m).weak()).wrap());
            }
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                if u.busy {
                    ui.spinner();
                } else if u.available.is_some() {
                    let label = if u.needs_auth { tr("Güncelle (parola sorar)", "Update (asks for password)") } else { tr("Şimdi güncelle", "Update now") };
                    if ui.button(RichText::new(label).strong()).clicked() {
                        send(&Cmd::InstallUpdate);
                    }
                } else if ui.button(tr("Güncellemeleri denetle", "Check for updates")).clicked() {
                    send(&Cmd::CheckUpdate);
                }
            });
            if !u.needs_auth {
                ui.add_space(4.0);
                if ui.checkbox(&mut self.cfg.auto_update, tr("Yeni sürümleri kendiliğinden kur", "Install new versions automatically")).changed() {
                    self.save();
                }
            }
        });
    }

    fn about(&mut self, ui: &mut egui::Ui, st: Option<&AppStatus>) {
        let logo = self.logo(ui.ctx());
        card(ui, "", |ui| {
            ui.horizontal(|ui| {
                ui.add(egui::Image::new(&logo).fit_to_exact_size(Vec2::splat(40.0)));
                ui.label(RichText::new("QuickDPI").size(20.0).strong());
                ui.label(RichText::new(concat!("v", env!("CARGO_PKG_VERSION"))).weak());
            });
            ui.label(tx(about::TAGLINE));
            ui.add_space(6.0);
            let site = config::WEBSITE.trim_start_matches("https://");
            if ui.link(RichText::new(site).strong()).on_hover_text(config::WEBSITE).clicked() {
                open_url(config::WEBSITE);
            }
        });
        ui.add_space(8.0);
        self.update_card(ui, st);
        ui.add_space(8.0);
        card(ui, tx(about::WHY_TITLE), |ui| {
            ui.add(egui::Label::new(tx(about::WHY)).wrap());
        });
        ui.add_space(8.0);
        card(ui, tx(about::DOES_TITLE), |ui| {
            for d in about::DOES {
                ui.horizontal_top(|ui| {
                    ui.label("•");
                    ui.add(egui::Label::new(tx(*d)).wrap());
                });
                ui.add_space(2.0);
            }
        });
        ui.add_space(8.0);
        card(ui, tx(about::NOT_TITLE), |ui| {
            ui.add(egui::Label::new(tx(about::NOT)).wrap());
        });
        ui.add_space(8.0);
        card(ui, "", |ui| {
            ui.add(egui::Label::new(RichText::new(tx(about::LICENSE)).small().weak()).wrap());
            ui.add_space(4.0);
            if ui.button(tr("Kullanım koşulları", "Terms of use")).clicked() {
                self.show_terms = true;
            }
        });
    }

    fn terms(&mut self, ctx: &egui::Context) {
        let must = !self.cfg.accepted();
        if !must && !self.show_terms {
            return;
        }
        let resp = egui::Modal::new(egui::Id::unique("terms")).show(ctx, |ui| {
            ui.set_width(420.0f32.min(ctx.content_rect().width() - 48.0));
            ui.label(RichText::new(tx(about::TERMS_TITLE)).size(18.0).strong());
            ui.add_space(6.0);
            ui.add(egui::Label::new(tx(about::TERMS_INTRO)).wrap());
            ui.add_space(6.0);
            egui::ScrollArea::vertical().max_height(260.0).show(ui, |ui| {
                for t in about::TERMS {
                    ui.horizontal_top(|ui| {
                        ui.label("•");
                        ui.add(egui::Label::new(tx(*t)).wrap());
                    });
                    ui.add_space(2.0);
                }
            });
            ui.add_space(8.0);
            if must {
                ui.checkbox(&mut self.terms_check, tx(about::TERMS_CHECK));
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.add_enabled(self.terms_check, egui::Button::new(RichText::new(tx(about::TERMS_ACCEPT)).strong())).clicked() {
                        self.cfg.accepted_terms = config::TERMS_VERSION;
                        self.cfg.save();
                        send(&Cmd::Accept);
                    }
                    if ui.button(tx(about::TERMS_DECLINE)).clicked() {
                        QUITTING.store(true, Ordering::SeqCst);
                        send(&Cmd::Quit);
                    }
                });
            } else if ui.button(tr("Kapat", "Close")).clicked() {
                self.show_terms = false;
            }
        });
        if !must && resp.should_close() {
            self.show_terms = false;
        }
    }

    fn shot(&mut self, ctx: &egui::Context, ready: bool) {
        let Ok(path) = std::env::var("QDPI_SHOT") else { return };
        let n = ctx.cumulative_frame_nr();
        if n == 1 {
            if let Some(t) = std::env::var("QDPI_TAB").ok().and_then(|t| t.parse::<usize>().ok()) {
                self.tab = TABS[t.clamp(1, 5) - 1];
            }
        }
        if n >= 12 && (ready || n > 600) && !SHOT_SENT.swap(true, Ordering::SeqCst) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::ScreenshotCallback::new(move |img| {
                let [w, h] = img.size;
                let rgba: Vec<u8> = img.pixels.iter().flat_map(|c| c.to_array()).collect();
                let s = w.max(h);
                let mut sq = vec![0u8; s * s * 4];
                for y in 0..h {
                    sq[y * s * 4..y * s * 4 + w * 4].copy_from_slice(&rgba[y * w * 4..(y + 1) * w * 4]);
                }
                let _ = std::fs::write(&path, crate::icon::png(s as u32, &sq));
                std::process::exit(0);
            })));
        }
        ctx.request_repaint();
    }
}

impl eframe::App for UiApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let st = self.shared.lock().unwrap().status.clone();
        if let Some(s) = &st {
            if s.config_rev != self.rev {
                self.rev = s.config_rev;
                let fresh = Config::load();
                if fresh != self.cfg {
                    self.sites_text = fresh.test_sites.join("\n");
                    self.cfg = fresh;
                }
            }
        }
        self.shortcuts(&ctx, st.as_ref());
        self.shot(&ctx, st.is_some());
        if ctx.input(|i| i.viewport().close_requested()) && st.as_ref().is_some_and(|s| !s.tray && s.enabled) && !QUITTING.load(Ordering::SeqCst) {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
        }
        if self.pending.is_some() || st.as_ref().is_some_and(|s| s.tune.running) {
            ctx.request_repaint_after(Duration::from_millis(250));
        }

        egui::Panel::top("head").frame(egui::Frame::central_panel(ui.style()).inner_margin(egui::Margin { left: 16, right: 16, top: 14, bottom: 10 })).show(ui, |ui| {
            let logo = self.logo(&ctx);
            ui.horizontal(|ui| {
                ui.add(egui::Image::new(&logo).fit_to_exact_size(Vec2::splat(22.0)));
                ui.add(egui::Label::new(RichText::new("QuickDPI").size(15.0).strong()).selectable(false));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.menu_button(tr("Menü", "Menu"), |ui| {
                        if ui.add(egui::Button::new(tr("Tepsiye gizle", "Hide to tray")).shortcut_text(format!("Esc · {}", shortcut_text(&SC_HIDE)))).clicked() {
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                            ui.close();
                        }
                        if ui.add(egui::Button::new(tr("İstisna ekle", "Add exclusion")).shortcut_text(shortcut_text(&SC_ADD))).clicked() {
                            self.tab = Tab::Rules;
                            self.focus_rule = true;
                            ui.close();
                        }
                        ui.separator();
                        if ui.add(egui::Button::new(tr("Çıkış", "Quit")).shortcut_text(shortcut_text(&SC_QUIT))).clicked() {
                            QUITTING.store(true, Ordering::SeqCst);
                            send(&Cmd::Quit);
                            ui.close();
                        }
                    });
                });
            });
            ui.add_space(6.0);
            self.header(ui, st.as_ref());
            ui.add_space(10.0);
            self.tabs(ui);
        });
        egui::Panel::bottom("foot").frame(egui::Frame::central_panel(ui.style()).inner_margin(egui::Margin::symmetric(16, 6))).show(ui, |ui| {
            ui.horizontal(|ui| {
                let hint = format!("{} {} · {} {} · Esc {}", shortcut_text(&SC_TOGGLE), tr("aç/kapat", "on/off"), shortcut_text(&SC_TUNE), tr("otomatik ayar", "auto-tune"), tr("tepsiye", "to tray"));
                ui.add(egui::Label::new(RichText::new(hint).small().weak()).truncate());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.add(egui::Label::new(RichText::new(concat!("v", env!("CARGO_PKG_VERSION"))).small().weak()).selectable(false));
                });
            });
        });
        egui::CentralPanel::default_margins().frame(egui::Frame::central_panel(ui.style()).inner_margin(egui::Margin::symmetric(16, 10))).show(ui, |ui| {
            if let Some(v) = st.as_ref().and_then(|s| s.update.available.clone()).filter(|_| self.tab != Tab::About) {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(format!("{} {v}", tr("Yeni sürüm hazır:", "New version available:"))).strong());
                    if ui.small_button(tr("Ayrıntılar", "Details")).clicked() {
                        self.tab = Tab::About;
                    }
                });
                ui.add_space(6.0);
            }
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| match self.tab {
                Tab::General => self.general(ui, st.as_ref()),
                Tab::Rules => self.rules(ui),
                Tab::Sites => self.sites(ui, st.as_ref()),
                Tab::Advanced => self.advanced(ui, st.as_ref()),
                Tab::About => self.about(ui, st.as_ref()),
            });
        });
        self.terms(&ctx);
    }
}

pub fn run() -> eframe::Result<()> {
    let cfg = Config::load();
    crate::i18n::init(&cfg.lang);
    let icon = egui::IconData { rgba: crate::icon::rgba(128, crate::icon::Tone::Brand), width: 128, height: 128 };
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_title("QuickDPI").with_app_id("quickdpi").with_inner_size([500.0, 640.0]).with_min_inner_size([380.0, 460.0]).with_icon(Arc::new(icon)),
        ..Default::default()
    };
    eframe::run_native(
        "QuickDPI",
        options,
        Box::new(|cc| {
            let _ = CTX.set(cc.egui_ctx.clone());
            cc.egui_ctx.all_styles_mut(|s| {
                s.spacing.item_spacing = Vec2::new(8.0, 6.0);
                s.spacing.button_padding = Vec2::new(10.0, 4.0);
            });
            Ok(Box::new(UiApp::new()))
        }),
    )
}
