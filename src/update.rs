use crate::config::WEBSITE;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::time::Duration;

const MAX_SIZE: u64 = 128 * 1024 * 1024;

#[derive(Clone, Debug, Deserialize)]
pub struct Latest {
    pub version: String,
    pub files: Vec<Asset>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Asset {
    pub name: String,
    pub sha256: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct UpdateState {
    pub available: Option<String>,
    pub busy: bool,
    pub needs_auth: bool,
    pub message: Option<String>,
}

pub const CURRENT: &str = env!("CARGO_PKG_VERSION");

fn parse(v: &str) -> Option<(u32, u32, u32)> {
    let mut it = v.trim().trim_start_matches('v').split('.').map(|p| p.parse::<u32>().ok());
    Some((it.next()??, it.next()??, it.next()??))
}

pub fn newer(latest: &str, current: &str) -> bool {
    matches!((parse(latest), parse(current)), (Some(a), Some(b)) if a > b)
}

fn agent(timeout: u64) -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(timeout)))
        .user_agent(concat!("QuickDPI/", env!("CARGO_PKG_VERSION")))
        .tls_config(crate::controller::tls())
        .build()
        .into()
}

pub fn check() -> Result<Option<Latest>, String> {
    let body = agent(20)
        .get(&format!("{WEBSITE}/downloads/latest.json"))
        .call()
        .map_err(|e| e.to_string())?
        .body_mut()
        .read_to_string()
        .map_err(|e| e.to_string())?;
    let l: Latest = serde_json::from_str(&body).map_err(|e| e.to_string())?;
    Ok(newer(&l.version, CURRENT).then_some(l))
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Kind {
    WindowsExe,
    LinuxTar,
    LinuxDeb,
}

pub fn kind() -> Kind {
    if cfg!(windows) {
        Kind::WindowsExe
    } else if std::env::current_exe().is_ok_and(|p| p.starts_with("/usr")) {
        Kind::LinuxDeb
    } else {
        Kind::LinuxTar
    }
}

pub fn silent() -> bool {
    kind() != Kind::LinuxDeb
}

fn asset_name(k: Kind) -> &'static str {
    match k {
        Kind::WindowsExe => "quickdpi-windows-x86_64.exe",
        Kind::LinuxTar => "quickdpi-linux-x86_64.tar.gz",
        Kind::LinuxDeb => "quickdpi_amd64.deb",
    }
}

fn download(l: &Latest, k: Kind) -> Result<Vec<u8>, String> {
    let name = asset_name(k);
    let a = l.files.iter().find(|f| f.name == name).ok_or_else(|| format!("{name} yok"))?;
    let data = agent(600)
        .get(&format!("{WEBSITE}/downloads/{name}"))
        .call()
        .map_err(|e| e.to_string())?
        .body_mut()
        .with_config()
        .limit(MAX_SIZE)
        .read_to_vec()
        .map_err(|e| e.to_string())?;
    let sum: String = Sha256::digest(&data).iter().map(|b| format!("{b:02x}")).collect();
    if !sum.eq_ignore_ascii_case(a.sha256.trim()) {
        return Err("sha256 tutmadı".into());
    }
    Ok(data)
}

fn old_path(exe: &Path) -> PathBuf {
    let mut s = exe.as_os_str().to_owned();
    s.push(".old");
    s.into()
}

pub fn running_path(exe: PathBuf) -> PathBuf {
    let Some(s) = exe.to_str() else { return exe };
    let s = s.strip_suffix(" (deleted)").unwrap_or(s);
    PathBuf::from(s.strip_suffix(".old").unwrap_or(s))
}

pub fn cleanup() {
    if let Ok(exe) = std::env::current_exe() {
        let _ = std::fs::remove_file(old_path(&exe));
    }
}

fn replace_exe(new: &[u8]) -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let mut tmp = exe.as_os_str().to_owned();
    tmp.push(".new");
    let tmp = PathBuf::from(tmp);
    std::fs::write(&tmp, new).map_err(|e| format!("{}: {e}", tmp.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o755));
    }
    let old = old_path(&exe);
    let _ = std::fs::remove_file(&old);
    std::fs::rename(&exe, &old).map_err(|e| e.to_string())?;
    if let Err(e) = std::fs::rename(&tmp, &exe) {
        let _ = std::fs::rename(&old, &exe);
        return Err(e.to_string());
    }
    #[cfg(unix)]
    let _ = std::fs::remove_file(&old);
    Ok(())
}

#[cfg(unix)]
fn untar(data: &[u8]) -> Result<Vec<u8>, String> {
    let dir = std::env::temp_dir().join(format!("quickdpi-update-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let tgz = dir.join("p.tar.gz");
    std::fs::write(&tgz, data).map_err(|e| e.to_string())?;
    let ok = std::process::Command::new("tar").arg("-xzf").arg(&tgz).arg("-C").arg(&dir).arg("quickdpi").status().is_ok_and(|s| s.success());
    let r = if ok { std::fs::read(dir.join("quickdpi")).map_err(|e| e.to_string()) } else { Err("tar açılamadı".into()) };
    let _ = std::fs::remove_dir_all(&dir);
    r
}

#[cfg(unix)]
fn install_deb(data: &[u8]) -> Result<(), String> {
    let path = std::env::temp_dir().join(format!("quickdpi-update-{}.deb", std::process::id()));
    std::fs::write(&path, data).map_err(|e| e.to_string())?;
    let st = std::process::Command::new("pkexec").arg("dpkg").arg("-i").arg(&path).status();
    let _ = std::fs::remove_file(&path);
    match st {
        Ok(s) if s.success() => Ok(()),
        Ok(_) => Err("kurulum iptal edildi ya da başarısız".into()),
        Err(e) => Err(e.to_string()),
    }
}

pub fn install(l: &Latest) -> Result<(), String> {
    let k = kind();
    let data = download(l, k)?;
    match k {
        Kind::WindowsExe => replace_exe(&data),
        #[cfg(unix)]
        Kind::LinuxTar => replace_exe(&untar(&data)?),
        #[cfg(unix)]
        Kind::LinuxDeb => install_deb(&data),
        #[cfg(not(unix))]
        _ => Err("desteklenmiyor".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_order() {
        assert!(newer("0.1.1", "0.1.0"));
        assert!(newer("1.0.0", "0.9.9"));
        assert!(newer("v0.2.0", "0.1.10"));
        assert!(!newer("0.1.0", "0.1.0"));
        assert!(!newer("0.0.9", "0.1.0"));
        assert!(!newer("bozuk", "0.1.0"));
        assert_eq!(running_path("/a/quickdpi.old (deleted)".into()), PathBuf::from("/a/quickdpi"));
        assert_eq!(running_path("C:\\q\\quickdpi.exe.old".into()), PathBuf::from("C:\\q\\quickdpi.exe"));
        assert_eq!(running_path("/usr/bin/quickdpi".into()), PathBuf::from("/usr/bin/quickdpi"));
    }
}
