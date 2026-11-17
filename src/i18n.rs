use std::sync::atomic::{AtomicBool, Ordering};

static EN: AtomicBool = AtomicBool::new(false);

pub fn init(lang: &str) {
    let en = match lang {
        "tr" => false,
        "en" => true,
        _ => !system_is_turkish(),
    };
    EN.store(en, Ordering::Relaxed);
}

pub fn is_en() -> bool {
    EN.load(Ordering::Relaxed)
}

pub fn tr(tr: &'static str, en: &'static str) -> &'static str {
    if is_en() { en } else { tr }
}

#[cfg(windows)]
fn system_is_turkish() -> bool {
    unsafe { windows_sys::Win32::Globalization::GetUserDefaultUILanguage() & 0x3ff == 0x1f }
}

#[cfg(not(windows))]
fn system_is_turkish() -> bool {
    ["LC_ALL", "LC_MESSAGES", "LANG", "LANGUAGE"].iter().filter_map(|k| std::env::var(k).ok()).find(|v| !v.is_empty()).is_some_and(|v| v.to_lowercase().starts_with("tr"))
}
