use crate::model::{AppUpdate, UpdateStatus};
use std::time::Duration;
use tauri::AppHandle;
use tauri_plugin_updater::UpdaterExt;

pub const PUBLIC_KEY: &str = include_str!("../updater.pub");
const UPDATE_ENDPOINT: Option<&str> = option_env!("USAGEDOCK_UPDATE_ENDPOINT");

fn endpoint() -> Result<tauri::Url, String> {
    let value = UPDATE_ENDPOINT
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "このビルドには更新配信先が設定されていません".to_string())?;
    let url = tauri::Url::parse(value).map_err(|_| "更新配信先のURLが不正です".to_string())?;
    if url.scheme() != "https" {
        return Err("更新配信先はHTTPSである必要があります".into());
    }
    Ok(url)
}

pub async fn check(app: &AppHandle) -> Result<AppUpdate, String> {
    let current_version = app.package_info().version.to_string();
    let Ok(endpoint) = endpoint() else {
        return Ok(AppUpdate {
            status: UpdateStatus::Unconfigured,
            current_version,
            available_version: None,
            notes: None,
        });
    };
    let updater = app
        .updater_builder()
        .endpoints(vec![endpoint])
        .map_err(|_| "更新配信先を設定できませんでした".to_string())?
        .timeout(Duration::from_secs(20))
        .build()
        .map_err(|_| "更新機能を初期化できませんでした".to_string())?;
    let update = updater
        .check()
        .await
        .map_err(|_| "更新情報を確認できませんでした".to_string())?;
    Ok(match update {
        Some(update) => AppUpdate {
            status: UpdateStatus::Available,
            current_version,
            available_version: Some(update.version),
            notes: update.body,
        },
        None => AppUpdate {
            status: UpdateStatus::Current,
            current_version,
            available_version: None,
            notes: None,
        },
    })
}

pub async fn install(app: &AppHandle) -> Result<(), String> {
    let endpoint = endpoint()?;
    let updater = app
        .updater_builder()
        .endpoints(vec![endpoint])
        .map_err(|_| "更新配信先を設定できませんでした".to_string())?
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|_| "更新機能を初期化できませんでした".to_string())?;
    let update = updater
        .check()
        .await
        .map_err(|_| "更新情報を確認できませんでした".to_string())?
        .ok_or_else(|| "利用できる更新はありません".to_string())?;
    update
        .download_and_install(|_, _| {}, || {})
        .await
        .map_err(|_| "更新をダウンロードしてインストールできませんでした".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 公開鍵をビルドへ埋め込む() {
        assert!(!PUBLIC_KEY.trim().is_empty());
        let config: serde_json::Value =
            serde_json::from_str(include_str!("../tauri.conf.json")).expect("Tauri設定");
        assert_eq!(
            config["plugins"]["updater"]["pubkey"].as_str(),
            Some(PUBLIC_KEY.trim())
        );
    }
}
