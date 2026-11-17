use crate::controller::Event;
use crate::i18n::tr;
use crate::icon::Tone;
use std::sync::mpsc::Sender;

#[derive(Clone, PartialEq, Debug)]
pub struct View {
    pub enabled: bool,
    pub tone: Tone,
    pub tip: String,
}

#[cfg(target_os = "linux")]
mod imp {
    use super::*;
    use ksni::blocking::TrayMethods;

    struct KTray {
        tx: Sender<Event>,
        view: View,
    }

    impl ksni::Tray for KTray {
        fn id(&self) -> String {
            "quickdpi".into()
        }

        fn title(&self) -> String {
            "QuickDPI".into()
        }

        fn icon_pixmap(&self) -> Vec<ksni::Icon> {
            [22u32, 32, 48, 64]
                .iter()
                .map(|&s| {
                    let data = crate::icon::rgba(s, self.view.tone).chunks_exact(4).flat_map(|p| [p[3], p[0], p[1], p[2]]).collect();
                    ksni::Icon { width: s as i32, height: s as i32, data }
                })
                .collect()
        }

        fn tool_tip(&self) -> ksni::ToolTip {
            ksni::ToolTip { title: "QuickDPI".into(), description: self.view.tip.clone(), ..Default::default() }
        }

        fn activate(&mut self, _x: i32, _y: i32) {
            let _ = self.tx.send(Event::Show);
        }

        fn menu(&self) -> Vec<ksni::MenuItem<Self>> {
            use ksni::menu::{CheckmarkItem, StandardItem};
            vec![
                StandardItem { label: tr("QuickDPI'ı göster", "Show QuickDPI").into(), activate: Box::new(|t: &mut Self| drop(t.tx.send(Event::Show))), ..Default::default() }.into(),
                ksni::MenuItem::Separator,
                StandardItem { label: self.view.tip.clone(), enabled: false, ..Default::default() }.into(),
                CheckmarkItem { label: tr("Etkin", "Enabled").into(), checked: self.view.enabled, activate: Box::new(|t: &mut Self| drop(t.tx.send(Event::Toggle))), ..Default::default() }.into(),
                StandardItem { label: tr("Otomatik ayarla", "Auto-tune").into(), activate: Box::new(|t: &mut Self| drop(t.tx.send(Event::AutoTune))), ..Default::default() }.into(),
                ksni::MenuItem::Separator,
                StandardItem { label: tr("Çıkış", "Quit").into(), icon_name: "application-exit".into(), activate: Box::new(|t: &mut Self| drop(t.tx.send(Event::Quit))), ..Default::default() }.into(),
            ]
        }
    }

    pub struct Tray {
        handle: ksni::blocking::Handle<KTray>,
        view: View,
    }

    impl Tray {
        pub fn start(tx: Sender<Event>, view: View) -> Option<Tray> {
            let handle = KTray { tx, view: view.clone() }.spawn().ok()?;
            Some(Tray { handle, view })
        }

        pub fn update(&mut self, view: View) {
            if view != self.view {
                self.view = view.clone();
                self.handle.update(move |t| t.view = view);
            }
        }

        pub fn pump(&mut self) {}
    }
}

#[cfg(windows)]
mod imp {
    use super::*;
    use tray_icon::menu::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem};
    use tray_icon::{Icon, MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};

    pub struct Tray {
        tray: TrayIcon,
        tx: Sender<Event>,
        view: View,
        show: MenuItem,
        status: MenuItem,
        toggle: CheckMenuItem,
        tune: MenuItem,
        quit: MenuItem,
    }

    fn icon(t: Tone) -> Option<Icon> {
        Icon::from_rgba(crate::icon::rgba(32, t), 32, 32).ok()
    }

    impl Tray {
        pub fn start(tx: Sender<Event>, view: View) -> Option<Tray> {
            let show = MenuItem::new(tr("QuickDPI'ı göster", "Show QuickDPI"), true, None);
            let status = MenuItem::new(&view.tip, false, None);
            let toggle = CheckMenuItem::new(tr("Etkin", "Enabled"), true, view.enabled, None);
            let tune = MenuItem::new(tr("Otomatik ayarla", "Auto-tune"), true, None);
            let quit = MenuItem::new(tr("Çıkış", "Quit"), true, None);
            let menu = Menu::new();
            menu.append_items(&[&show, &PredefinedMenuItem::separator(), &status, &toggle, &tune, &PredefinedMenuItem::separator(), &quit]).ok()?;
            let tray = TrayIconBuilder::new().with_menu(Box::new(menu)).with_menu_on_left_click(false).with_tooltip(format!("QuickDPI — {}", view.tip)).with_icon(icon(view.tone)?).build().ok()?;
            Some(Tray { tray, tx, view, show, status, toggle, tune, quit })
        }

        pub fn update(&mut self, view: View) {
            if view == self.view {
                return;
            }
            if view.tone != self.view.tone {
                let _ = self.tray.set_icon(icon(view.tone));
            }
            if view.tip != self.view.tip {
                let _ = self.tray.set_tooltip(Some(format!("QuickDPI — {}", view.tip)));
                self.status.set_text(&view.tip);
            }
            self.toggle.set_checked(view.enabled);
            self.view = view;
        }

        pub fn pump(&mut self) {
            use windows_sys::Win32::UI::WindowsAndMessaging::{DispatchMessageW, MSG, PM_REMOVE, PeekMessageW, TranslateMessage};
            unsafe {
                let mut msg: MSG = std::mem::zeroed();
                while PeekMessageW(&mut msg, std::ptr::null_mut(), 0, 0, PM_REMOVE) != 0 {
                    TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
            }
            while let Ok(e) = TrayIconEvent::receiver().try_recv() {
                match e {
                    TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } | TrayIconEvent::DoubleClick { button: MouseButton::Left, .. } => {
                        let _ = self.tx.send(Event::Show);
                    }
                    _ => {}
                }
            }
            while let Ok(e) = MenuEvent::receiver().try_recv() {
                let ev = if e.id == *self.show.id() {
                    Event::Show
                } else if e.id == *self.toggle.id() {
                    self.toggle.set_checked(self.view.enabled);
                    Event::Toggle
                } else if e.id == *self.tune.id() {
                    Event::AutoTune
                } else if e.id == *self.quit.id() {
                    Event::Quit
                } else {
                    continue;
                };
                let _ = self.tx.send(ev);
            }
        }
    }
}

pub use imp::Tray;
