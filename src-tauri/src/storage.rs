use crate::model::{AppSettings, DashboardSnapshot};
use std::{fs, path::Path};

pub fn load_json<T: serde::de::DeserializeOwned>(path: &Path) -> Option<T> {
    fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
}

pub fn save_json<T: serde::Serialize>(path: &Path, value: &T) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|_| "設定フォルダーを作成できませんでした".to_string())?;
    }
    let text = serde_json::to_string_pretty(value)
        .map_err(|_| "設定を保存できませんでした".to_string())?;
    fs::write(path, text).map_err(|_| "設定を保存できませんでした".to_string())
}

pub fn load_settings(path: &Path) -> AppSettings {
    load_json::<AppSettings>(path).unwrap_or_default().clamped()
}
pub fn load_snapshot(path: &Path) -> Option<DashboardSnapshot> {
    load_json(path)
}

#[cfg(windows)]
pub fn save_opencode_key(key: &str) -> Result<(), String> {
    keyring::Entry::new("UsageDock", "opencode-go")
        .map_err(|_| "APIキーを安全に保存できませんでした".to_string())?
        .set_password(key)
        .map_err(|_| "APIキーを安全に保存できませんでした".to_string())
}

#[cfg(not(windows))]
pub fn save_opencode_key(_key: &str) -> Result<(), String> {
    Err("Windows Credential Managerはこの環境で利用できません".into())
}

#[cfg(windows)]
pub fn read_opencode_key() -> Option<String> {
    keyring::Entry::new("UsageDock", "opencode-go")
        .ok()?
        .get_password()
        .ok()
}
#[cfg(not(windows))]
pub fn read_opencode_key() -> Option<String> {
    None
}

#[cfg(windows)]
pub fn delete_opencode_key() -> Result<(), String> {
    match keyring::Entry::new("UsageDock", "opencode-go")
        .map_err(|_| "APIキーを削除できませんでした".to_string())?
        .delete_credential()
    {
        Ok(()) => Ok(()),
        Err(keyring::Error::NoEntry) => Ok(()),
        Err(_) => Err("APIキーを削除できませんでした".into()),
    }
}
#[cfg(not(windows))]
pub fn delete_opencode_key() -> Result<(), String> {
    Err("Windows Credential Managerはこの環境で利用できません".into())
}
