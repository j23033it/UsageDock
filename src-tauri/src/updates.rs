use crate::model::{AppUpdate, UpdateStatus};
use chrono::Utc;
use std::{
    fs::{self, OpenOptions},
    io::Write,
    time::Duration,
};
use tauri::{AppHandle, Manager};
use tauri_plugin_updater::{Error as UpdaterError, Update, Updater, UpdaterExt};

pub const PUBLIC_KEY: &str = include_str!("../updater.pub");
const UPDATE_ENDPOINT: Option<&str> = option_env!("USAGEDOCK_UPDATE_ENDPOINT");
const CHECK_TIMEOUT: Duration = Duration::from_secs(30);
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(300);
const NETWORK_ATTEMPTS: usize = 3;
const UPDATE_LOG_LIMIT: u64 = 512 * 1024;

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

fn update_log_path(app: &AppHandle) -> Option<std::path::PathBuf> {
    app.path()
        .app_log_dir()
        .ok()
        .map(|directory| directory.join("updater.log"))
}

fn write_update_log(app: &AppHandle, phase: &str, detail: &str) {
    let Some(path) = update_log_path(app) else {
        return;
    };
    let Some(directory) = path.parent() else {
        return;
    };
    if fs::create_dir_all(directory).is_err() {
        return;
    }
    if fs::metadata(&path).is_ok_and(|metadata| metadata.len() >= UPDATE_LOG_LIMIT) {
        let previous = directory.join("updater.previous.log");
        let _ = fs::remove_file(&previous);
        let _ = fs::rename(&path, previous);
    }
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
        let detail = detail.replace(['\r', '\n'], " ");
        let _ = writeln!(file, "{} [{}] {}", Utc::now().to_rfc3339(), phase, detail);
    }
}

fn update_failure(
    app: &AppHandle,
    phase: &str,
    summary: &str,
    error: impl std::fmt::Display,
) -> String {
    let detail = error.to_string();
    write_update_log(app, phase, &detail);
    match update_log_path(app) {
        Some(path) => format!("{summary} 詳細: {detail}（ログ: {}）", path.display()),
        None => format!("{summary} 詳細: {detail}"),
    }
}

fn can_retry(error: &UpdaterError) -> bool {
    matches!(error, UpdaterError::Reqwest(_) | UpdaterError::Network(_))
}

fn build_updater(app: &AppHandle) -> Result<Updater, String> {
    let endpoint = endpoint()?;
    app.updater_builder()
        .endpoints(vec![endpoint])
        .map_err(|error| update_failure(app, "設定", "更新配信先を設定できませんでした。", error))?
        .timeout(CHECK_TIMEOUT)
        .build()
        .map_err(|error| update_failure(app, "初期化", "更新機能を初期化できませんでした。", error))
}

async fn find_update(app: &AppHandle) -> Result<Option<Update>, String> {
    let updater = build_updater(app)?;
    for attempt in 1..=NETWORK_ATTEMPTS {
        write_update_log(
            app,
            "確認",
            &format!("更新情報を確認します（{attempt}/{NETWORK_ATTEMPTS}）"),
        );
        match updater.check().await {
            Ok(update) => return Ok(update),
            Err(error) if can_retry(&error) && attempt < NETWORK_ATTEMPTS => {
                write_update_log(app, "確認再試行", &error.to_string());
                tokio::time::sleep(Duration::from_millis(500 * attempt as u64)).await;
            }
            Err(error) => {
                return Err(update_failure(
                    app,
                    "確認失敗",
                    "更新情報を確認できませんでした。通信状態を確認して再試行してください。",
                    error,
                ));
            }
        }
    }
    unreachable!("更新確認は成功またはエラーで終了します")
}

async fn download_update(app: &AppHandle, update: &mut Update) -> Result<Vec<u8>, String> {
    update.timeout = Some(DOWNLOAD_TIMEOUT);
    for attempt in 1..=NETWORK_ATTEMPTS {
        write_update_log(
            app,
            "ダウンロード",
            &format!(
                "バージョン{}を取得します（{attempt}/{NETWORK_ATTEMPTS}）",
                update.version
            ),
        );
        match update.download(|_, _| {}, || {}).await {
            Ok(bytes) => {
                write_update_log(
                    app,
                    "検証完了",
                    &format!("{}バイトの署名を確認しました", bytes.len()),
                );
                return Ok(bytes);
            }
            Err(error) if can_retry(&error) && attempt < NETWORK_ATTEMPTS => {
                write_update_log(app, "ダウンロード再試行", &error.to_string());
                tokio::time::sleep(Duration::from_millis(500 * attempt as u64)).await;
            }
            Err(error) => {
                let summary = match error {
                    UpdaterError::Minisign(_)
                    | UpdaterError::Base64(_)
                    | UpdaterError::SignatureUtf8(_) => {
                        "更新ファイルの署名を検証できませんでした。配信ファイルが安全でないため中止しました。"
                    }
                    _ => {
                        "更新ファイルをダウンロードできませんでした。通信状態を確認して再試行してください。"
                    }
                };
                return Err(update_failure(app, "ダウンロード失敗", summary, error));
            }
        }
    }
    unreachable!("更新ダウンロードは成功またはエラーで終了します")
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
    let _ = endpoint;
    let update = find_update(app).await?;
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
    let mut update = find_update(app)
        .await?
        .ok_or_else(|| "利用できる更新はありません".to_string())?;
    let bytes = download_update(app, &mut update).await?;
    write_update_log(
        app,
        "インストール",
        &format!("バージョン{}のインストーラーを起動します", update.version),
    );
    update.install(bytes).map_err(|error| {
        update_failure(
            app,
            "インストール失敗",
            "更新インストーラーを起動できませんでした。",
            error,
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::{Engine as _, engine::general_purpose::STANDARD};
    use minisign_verify::{PublicKey, Signature};

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

    #[test]
    fn リリース時に更新成果物の署名を検証する() {
        let Some(artifact_path) = std::env::var_os("USAGEDOCK_VERIFY_ARTIFACT") else {
            return;
        };
        let signature_path = std::env::var_os("USAGEDOCK_VERIFY_SIGNATURE")
            .expect("署名検証を行う場合は署名ファイルも指定してください");
        let artifact = fs::read(artifact_path).expect("更新成果物を読み込めません");
        let signature_envelope =
            fs::read_to_string(signature_path).expect("更新署名を読み込めません");
        let public_key_text = String::from_utf8(
            STANDARD
                .decode(PUBLIC_KEY.trim())
                .expect("公開鍵のBase64が不正です"),
        )
        .expect("公開鍵がUTF-8ではありません");
        let signature_text = String::from_utf8(
            STANDARD
                .decode(signature_envelope.trim())
                .expect("更新署名のBase64が不正です"),
        )
        .expect("更新署名がUTF-8ではありません");
        let public_key = PublicKey::decode(&public_key_text).expect("公開鍵を解釈できません");
        let signature = Signature::decode(&signature_text).expect("更新署名を解釈できません");
        public_key
            .verify(&artifact, &signature, true)
            .expect("更新成果物の署名検証に失敗しました");
    }
}
