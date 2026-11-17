#[cfg(target_os = "linux")]
mod imp {
    use std::path::PathBuf;

    fn exe() -> String {
        std::env::var("APPIMAGE").ok().or_else(|| std::env::current_exe().ok().map(|p| p.display().to_string())).unwrap_or_else(|| "quickdpi".into())
    }

    fn entry(extra: &str) -> String {
        format!(
            "[Desktop Entry]\nType=Application\nName=QuickDPI\nGenericName=Network Security\nComment={}\nExec=\"{}\"{extra}\nIcon=quickdpi\nTerminal=false\nCategories=Network;Utility;\nStartupWMClass=quickdpi\nX-GNOME-Autostart-enabled=true\n",
            crate::i18n::tr("Ağ güvenliği ve bağlantı koruması", "Network security and connection protection"),
            exe()
        )
    }

    fn autostart_file() -> Option<PathBuf> {
        Some(dirs::config_dir()?.join("autostart").join("quickdpi.desktop"))
    }

    pub fn set(on: bool) {
        let Some(f) = autostart_file() else { return };
        if on {
            let _ = std::fs::create_dir_all(f.parent().unwrap());
            let _ = std::fs::write(&f, entry(" --hidden"));
        } else {
            let _ = std::fs::remove_file(f);
        }
    }

    pub fn ensure_menu_entry() {
        if std::path::Path::new("/usr/share/applications/quickdpi.desktop").exists() {
            return;
        }
        let Some(data) = dirs::data_dir() else { return };
        let app = data.join("applications").join("quickdpi.desktop");
        let want = entry("");
        if std::fs::read_to_string(&app).ok().as_deref() != Some(want.as_str()) {
            let _ = std::fs::create_dir_all(app.parent().unwrap());
            let _ = std::fs::write(&app, want);
        }
        let icon = data.join("icons/hicolor/256x256/apps/quickdpi.png");
        if !icon.exists() {
            let _ = std::fs::create_dir_all(icon.parent().unwrap());
            let _ = std::fs::write(&icon, crate::icon::png(256, &crate::icon::rgba(256, crate::icon::Tone::Brand)));
        }
    }
}

#[cfg(windows)]
mod imp {
    use std::os::windows::process::CommandExt;
    use std::process::{Command, Stdio};

    const TASK: &str = "QuickDPI";
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    pub fn set(on: bool) {
        if !on {
            let _ = Command::new("schtasks").args(["/Delete", "/F", "/TN", TASK]).creation_flags(CREATE_NO_WINDOW).stdout(Stdio::null()).stderr(Stdio::null()).status();
            return;
        }
        let Ok(exe) = std::env::current_exe() else { return };
        let user = std::env::var("USERDOMAIN").map(|d| format!("{d}\\")).unwrap_or_default() + &std::env::var("USERNAME").unwrap_or_default();
        let esc = |s: &str| s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;");
        let xml = format!(
            r#"<?xml version="1.0" encoding="UTF-16"?>
<Task version="1.2" xmlns="http://schemas.microsoft.com/windows/2004/02/mit/task">
  <Triggers><LogonTrigger><Enabled>true</Enabled><UserId>{user}</UserId><Delay>PT5S</Delay></LogonTrigger></Triggers>
  <Principals><Principal id="Author"><UserId>{user}</UserId><LogonType>InteractiveToken</LogonType><RunLevel>HighestAvailable</RunLevel></Principal></Principals>
  <Settings>
    <MultipleInstancesPolicy>IgnoreNew</MultipleInstancesPolicy>
    <DisallowStartIfOnBatteries>false</DisallowStartIfOnBatteries>
    <StopIfGoingOnBatteries>false</StopIfGoingOnBatteries>
    <ExecutionTimeLimit>PT0S</ExecutionTimeLimit>
    <Priority>5</Priority>
  </Settings>
  <Actions Context="Author"><Exec><Command>"{exe}"</Command><Arguments>--hidden</Arguments></Exec></Actions>
</Task>"#,
            user = esc(&user),
            exe = esc(&exe.display().to_string())
        );
        let path = std::env::temp_dir().join("quickdpi-task.xml");
        let mut bytes = vec![0xff, 0xfe];
        for u in xml.encode_utf16() {
            bytes.extend_from_slice(&u.to_le_bytes());
        }
        if std::fs::write(&path, bytes).is_ok() {
            let _ = Command::new("schtasks").args(["/Create", "/F", "/TN", TASK, "/XML"]).arg(&path).creation_flags(CREATE_NO_WINDOW).stdout(Stdio::null()).stderr(Stdio::null()).status();
            let _ = std::fs::remove_file(path);
        }
    }

    pub fn ensure_menu_entry() {}
}

pub use imp::{ensure_menu_entry, set};
