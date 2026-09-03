use crate::model::{AppSettings, DashboardSnapshot};
use serde::{Deserialize, Serialize};
use std::{
    ffi::OsString,
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

const SNAPSHOT_CACHE_VERSION: u32 = 1;

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredSnapshot {
    cache_version: u32,
    snapshot: DashboardSnapshot,
}

fn sibling_with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path
        .file_name()
        .map(OsString::from)
        .unwrap_or_else(|| OsString::from("data"));
    name.push(suffix);
    path.with_file_name(name)
}

pub fn load_json<T: serde::de::DeserializeOwned>(path: &Path) -> Option<T> {
    [
        path.to_path_buf(),
        sibling_with_suffix(path, ".tmp"),
        sibling_with_suffix(path, ".bak"),
    ]
    .into_iter()
    .find_map(|candidate| {
        fs::read_to_string(candidate)
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
    })
}

pub fn save_json<T: serde::Serialize>(path: &Path, value: &T) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|_| "設定フォルダーを作成できませんでした".to_string())?;
    }
    let text = serde_json::to_string_pretty(value)
        .map_err(|_| "設定を保存できませんでした".to_string())?;
    let temporary = sibling_with_suffix(path, ".tmp");
    let backup = sibling_with_suffix(path, ".bak");
    let mut file = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(&temporary)
        .map_err(|_| "設定を保存できませんでした".to_string())?;
    file.write_all(text.as_bytes())
        .and_then(|_| file.sync_all())
        .map_err(|_| "設定を保存できませんでした".to_string())?;
    drop(file);

    let had_existing = path.exists();
    if had_existing {
        let _ = fs::remove_file(&backup);
        fs::rename(path, &backup).map_err(|_| "設定を保存できませんでした".to_string())?;
    }
    if fs::rename(&temporary, path).is_err() {
        if had_existing {
            let _ = fs::rename(&backup, path);
        }
        return Err("設定を保存できませんでした".to_string());
    }
    let _ = fs::remove_file(backup);
    Ok(())
}

pub fn load_settings(path: &Path) -> AppSettings {
    load_json::<AppSettings>(path).unwrap_or_default().clamped()
}
pub fn load_snapshot(path: &Path) -> Option<DashboardSnapshot> {
    load_json::<StoredSnapshot>(path)
        .filter(|stored| stored.cache_version == SNAPSHOT_CACHE_VERSION)
        .map(|stored| stored.snapshot)
}

pub fn save_snapshot(path: &Path, snapshot: &DashboardSnapshot) -> Result<(), String> {
    save_json(
        path,
        &StoredSnapshot {
            cache_version: SNAPSHOT_CACHE_VERSION,
            snapshot: snapshot.clone(),
        },
    )
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

#[cfg(test)]
mod tests {
    use super::*;

    fn test_path(name: &str) -> PathBuf {
        let unique = format!(
            "usage-dock-{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("現在時刻")
                .as_nanos()
        );
        std::env::temp_dir().join(unique).join("data.json")
    }

    #[test]
    fn 途中ファイルまたはバックアップから復旧する() {
        let path = test_path("recovery");
        fs::create_dir_all(path.parent().expect("親フォルダー")).expect("テストフォルダー作成");
        fs::write(&path, "壊れたJSON").expect("壊れたファイル作成");
        fs::write(sibling_with_suffix(&path, ".tmp"), r#"{"value":42}"#).expect("一時ファイル作成");

        let value = load_json::<serde_json::Value>(&path).expect("復旧データ");

        assert_eq!(value["value"], 42);
        fs::remove_dir_all(path.parent().expect("親フォルダー")).expect("テストフォルダー削除");
    }

    #[test]
    fn 旧形式のスナップショットを再利用しない() {
        let path = test_path("legacy-snapshot");
        fs::create_dir_all(path.parent().expect("親フォルダー")).expect("テストフォルダー作成");
        fs::write(
            &path,
            r#"{"providers":[],"refreshedAt":"2026-01-01T00:00:00Z"}"#,
        )
        .expect("旧形式作成");

        assert!(load_snapshot(&path).is_none());
        fs::remove_dir_all(path.parent().expect("親フォルダー")).expect("テストフォルダー削除");
    }

    #[test]
    fn 現行形式のスナップショットを保存して読み戻す() {
        let path = test_path("snapshot-roundtrip");
        let snapshot = DashboardSnapshot {
            providers: Vec::new(),
            refreshed_at: "2026-01-01T00:00:00Z".into(),
        };

        save_snapshot(&path, &snapshot).expect("スナップショット保存");

        assert_eq!(load_snapshot(&path), Some(snapshot));
        fs::remove_dir_all(path.parent().expect("親フォルダー")).expect("テストフォルダー削除");
    }
}
