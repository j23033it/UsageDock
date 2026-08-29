use crate::model::{
    AppSettings, UsageSource, UsageWindow, WindowKind, clamp_percent, kind_for_duration,
    unix_seconds_to_iso,
};
use serde_json::{Value, json};
use std::{
    path::{Path, PathBuf},
    process::Stdio,
    time::SystemTime,
};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    process::Command,
    time::{Duration, timeout},
};

#[derive(Debug, Clone)]
pub struct FetchResult {
    pub source: UsageSource,
    pub plan_name: Option<String>,
    pub windows: Vec<UsageWindow>,
}

pub async fn fetch_codex(settings: &AppSettings) -> Result<FetchResult, String> {
    match fetch_codex_app_server(settings).await {
        Ok(result) => Ok(result),
        Err(_) => fetch_codex_log().await.map(|mut result| {
            result.source = UsageSource::LocalLog;
            result
        }),
    }
}

async fn fetch_codex_app_server(settings: &AppSettings) -> Result<FetchResult, String> {
    let executable = settings.codex_path.as_deref().unwrap_or("codex");
    let task = async move {
        let mut child = Command::new(executable)
            .args(["app-server", "--listen", "stdio://"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .map_err(|_| "Codex App Serverを起動できませんでした".to_string())?;
        let mut stdin = child
            .stdin
            .take()
            .ok_or_else(|| "Codex App Serverの入力を開けませんでした".to_string())?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| "Codex App Serverの出力を開けませんでした".to_string())?;
        let initialize = json!({
            "jsonrpc": "2.0", "id": 1, "method": "initialize",
            "params": { "clientInfo": { "name": "usage_dock", "version": "0.1.0" }, "capabilities": {} }
        });
        let initialized = json!({ "jsonrpc": "2.0", "method": "initialized", "params": {} });
        let read =
            json!({ "jsonrpc": "2.0", "id": 2, "method": "account/rateLimits/read", "params": {} });
        for request in [initialize, initialized, read] {
            stdin
                .write_all(serde_json::to_string(&request).unwrap().as_bytes())
                .await
                .map_err(|_| "Codex App Serverへ送信できませんでした".to_string())?;
            stdin
                .write_all(b"\n")
                .await
                .map_err(|_| "Codex App Serverへ送信できませんでした".to_string())?;
            stdin
                .flush()
                .await
                .map_err(|_| "Codex App Serverへ送信できませんでした".to_string())?;
        }
        let mut lines = BufReader::new(stdout).lines();
        let mut answer = None;
        while let Some(line) = lines
            .next_line()
            .await
            .map_err(|_| "Codex App Serverの応答を読めませんでした".to_string())?
        {
            let value: Value = match serde_json::from_str(&line) {
                Ok(value) => value,
                Err(_) => continue,
            };
            if value.get("id").and_then(Value::as_i64) == Some(2)
                || value
                    .get("result")
                    .and_then(|v| v.get("rateLimits"))
                    .is_some()
            {
                answer = Some(value);
                break;
            }
        }
        let _ = child.kill().await;
        let response = answer
            .ok_or_else(|| "Codex App Serverから利用状況を取得できませんでした".to_string())?;
        let windows = parse_codex_response(&response)
            .ok_or_else(|| "Codexの利用状況形式を認識できませんでした".to_string())?;
        Ok(FetchResult {
            source: UsageSource::AppServer,
            plan_name: None,
            windows,
        })
    };
    timeout(Duration::from_secs(10), task)
        .await
        .map_err(|_| "Codex App Serverがタイムアウトしました".to_string())?
}

pub fn parse_codex_response(response: &Value) -> Option<Vec<UsageWindow>> {
    let mut limits = Vec::new();
    collect_limit_objects(response, None, &mut limits);
    if limits.is_empty() {
        return None;
    }
    let mut windows = Vec::new();
    for (label, object) in limits {
        if let Some(window) = parse_limit(&label, &object)
            && !windows
                .iter()
                .any(|item: &UsageWindow| item.label == window.label)
        {
            windows.push(window);
        }
    }
    (!windows.is_empty()).then_some(windows)
}

fn collect_limit_objects(value: &Value, hint: Option<String>, output: &mut Vec<(String, Value)>) {
    let Some(object) = value.as_object() else {
        return;
    };
    if let Some(result) = object.get("result") {
        collect_limit_objects(result, hint.clone(), output);
    }
    if let Some(rate_limits) = object.get("rateLimits") {
        collect_limit_objects(rate_limits, hint.clone(), output);
    }
    if let Some(map) = object.get("rateLimitsByLimitId").and_then(Value::as_object) {
        for (id, item) in map {
            collect_limit_objects(item, Some(id.clone()), output);
        }
    }
    for key in ["primary", "secondary"] {
        if let Some(item) = object.get(key) {
            output.push((
                hint.clone().unwrap_or_else(|| key.to_string()),
                item.clone(),
            ));
        }
    }
    if (object.contains_key("usedPercent") || object.contains_key("used_percent"))
        && let Some(name) = hint
    {
        output.push((name, value.clone()));
    }
}

fn parse_limit(label: &str, value: &Value) -> Option<UsageWindow> {
    let object = value.as_object()?;
    let used = object
        .get("usedPercent")
        .or_else(|| object.get("used_percent"))?
        .as_f64()?;
    let duration = object
        .get("windowDurationMins")
        .or_else(|| object.get("windowDurationMinutes"))
        .or_else(|| object.get("window_minutes"))
        .and_then(Value::as_u64);
    let used = clamp_percent(used);
    let resets_at = object
        .get("resetsAt")
        .or_else(|| object.get("resets_at"))
        .and_then(unix_seconds_to_iso_or_string);
    Some(UsageWindow {
        kind: kind_for_duration(duration),
        label: friendly_label(label),
        used_percent: Some(used),
        remaining_percent: Some(100.0 - used),
        resets_at,
        window_duration_minutes: duration,
    })
}

fn unix_seconds_to_iso_or_string(value: &Value) -> Option<String> {
    unix_seconds_to_iso(value).or_else(|| value.as_str().map(ToOwned::to_owned))
}

fn friendly_label(label: &str) -> String {
    match label {
        "primary" => "Primary".into(),
        "secondary" => "Secondary".into(),
        other => other.to_string(),
    }
}

pub async fn fetch_codex_log() -> Result<FetchResult, String> {
    let home = std::env::var_os("USERPROFILE")
        .map(PathBuf::from)
        .ok_or_else(|| "Codexのログフォルダーが見つかりません".to_string())?;
    let root = home.join(".codex").join("sessions");
    let mut files = Vec::new();
    collect_rollouts(&root, &mut files);
    files.sort_by_key(|(_, modified)| std::cmp::Reverse(*modified));
    let mut latest = None;
    for (path, _) in files.into_iter().take(20) {
        if let Ok(contents) = std::fs::read_to_string(path) {
            for line in contents.lines() {
                let Ok(value) = serde_json::from_str::<Value>(line) else {
                    continue;
                };
                if value
                    .get("payload")
                    .and_then(|payload| payload.get("type"))
                    .and_then(Value::as_str)
                    != Some("token_count")
                {
                    continue;
                }
                if let Some(rate_limits) = value
                    .get("payload")
                    .and_then(|payload| payload.get("rate_limits"))
                    && let Some(parsed) = parse_codex_response(rate_limits)
                {
                    let timestamp = value
                        .get("timestamp")
                        .and_then(Value::as_str)
                        .or_else(|| {
                            value
                                .get("payload")
                                .and_then(|p| p.get("timestamp"))
                                .and_then(Value::as_str)
                        })
                        .unwrap_or("")
                        .to_string();
                    if latest
                        .as_ref()
                        .is_none_or(|(current, _): &(String, Vec<UsageWindow>)| {
                            timestamp >= *current
                        })
                    {
                        latest = Some((timestamp, parsed));
                    }
                }
            }
        }
    }
    let windows = latest
        .map(|(_, windows)| windows)
        .ok_or_else(|| "Codexのローカルログから利用状況を取得できませんでした".to_string())?;
    Ok(FetchResult {
        source: UsageSource::LocalLog,
        plan_name: None,
        windows,
    })
}

fn collect_rollouts(root: &Path, files: &mut Vec<(PathBuf, SystemTime)>) {
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_rollouts(&path, files);
            continue;
        }
        if path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with("rollout-") && name.ends_with(".jsonl"))
            && let Ok(modified) = entry.metadata().and_then(|metadata| metadata.modified())
        {
            files.push((path, modified));
        }
    }
}

pub async fn fetch_opencode(api_key: String) -> Result<FetchResult, String> {
    let response = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .map_err(|_| "OpenCode Goへの接続を準備できませんでした".to_string())?
        .get("https://opencode.ai/zen/go/v1/usage")
        .bearer_auth(api_key)
        .send()
        .await
        .map_err(safe_opencode_error)?;
    let status = response.status();
    if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
        return Err("OpenCode GoのAPIキーが認証されませんでした".into());
    }
    if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
        return Err("OpenCode Goの利用上限に達しました".into());
    }
    if status.is_server_error() {
        return Err("OpenCode Goのサーバーで問題が発生しました".into());
    }
    if !status.is_success() {
        return Err("OpenCode Goから利用状況を取得できませんでした".into());
    }
    let body: Value = response
        .json()
        .await
        .map_err(|_| "OpenCode Goの応答を読み取れませんでした".to_string())?;
    let windows = parse_opencode_response(&body)
        .ok_or_else(|| "OpenCode Goの応答形式を認識できませんでした".to_string())?;
    Ok(FetchResult {
        source: UsageSource::Api,
        plan_name: None,
        windows,
    })
}

fn safe_opencode_error(error: reqwest::Error) -> String {
    if error.is_timeout() {
        "OpenCode Goがタイムアウトしました".into()
    } else {
        "OpenCode Goへ接続できませんでした".into()
    }
}

pub fn parse_opencode_response(value: &Value) -> Option<Vec<UsageWindow>> {
    let rolling = value.get("usage")?.as_object()?;
    let mut windows = Vec::new();
    for (label, item) in rolling {
        let object = item.as_object()?;
        let percent = object.get("percent")?.as_f64()?;
        let used = clamp_percent(percent);
        let kind = match label.as_str() {
            "rolling" => WindowKind::FiveHour,
            "weekly" => WindowKind::Weekly,
            "monthly" => WindowKind::Monthly,
            _ => WindowKind::Custom,
        };
        let resets_at = object
            .get("resetsAt")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned);
        windows.push(UsageWindow {
            kind,
            label: label.clone(),
            used_percent: Some(used),
            remaining_percent: Some(100.0 - used),
            resets_at,
            window_duration_minutes: None,
        });
    }
    (!windows.is_empty()).then_some(windows)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codex応答を複数枠へ変換する() {
        let value = serde_json::json!({"result":{"rateLimits":{"primary":{"usedPercent":120,"windowDurationMins":300,"resetsAt":1700000000},"secondary":{"usedPercent":10,"windowDurationMins":10080}},"rateLimitsByLimitId":{"extra":{"usedPercent":50,"windowDurationMins":43200}}}});
        let windows = parse_codex_response(&value).unwrap();
        assert_eq!(windows.len(), 3);
        assert_eq!(windows[0].remaining_percent, Some(0.0));
        assert!(windows[0].resets_at.is_some());
    }

    #[test]
    fn opencode応答を変換する() {
        let value = serde_json::json!({"usage":{"rolling":{"status":"ok","percent":25,"resetsAt":"2025-01-01T00:00:00Z"},"weekly":{"status":"rate-limited","percent":110,"resetsAt":"2025-01-02T00:00:00Z"}}});
        let windows = parse_opencode_response(&value).unwrap();
        assert_eq!(windows[0].remaining_percent, Some(75.0));
        assert_eq!(windows[1].remaining_percent, Some(0.0));
    }

    #[test]
    fn 不正なログ行を安全に無視する() {
        let value = serde_json::json!({"payload":{"type":"other"}});
        assert!(
            value
                .get("payload")
                .and_then(|p| p.get("type"))
                .and_then(Value::as_str)
                != Some("token_count")
        );
    }
}
