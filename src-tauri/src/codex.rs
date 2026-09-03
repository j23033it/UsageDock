use crate::{
    model::{CodexAuthType, CodexConnection, CodexLoginMode, CodexLoginPrompt, ConnectionStatus},
    providers::display_plan_name,
};
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Stdio,
    time::{Duration, SystemTime},
};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader, Lines},
    process::{Child, ChildStdout, Command},
    sync::oneshot,
    time::{sleep, timeout},
};

#[cfg(target_os = "windows")]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

const CODEX_CONFIG: &str =
    "cli_auth_credentials_store = \"keyring\"\ncheck_for_update_on_startup = false\n";
const CANCELLED: &str = "__usage_dock_login_cancelled__";

pub struct CodexClient {
    child: Child,
    stdin: tokio::process::ChildStdin,
    lines: Lines<BufReader<ChildStdout>>,
    pub executable_path: String,
}

pub struct LoginSession {
    client: CodexClient,
    pub prompt: CodexLoginPrompt,
}

fn prepare_home(home: &Path) -> Result<(), String> {
    fs::create_dir_all(home)
        .map_err(|_| "UsageDock用のCodex設定フォルダーを作成できませんでした".to_string())?;
    let config_path = home.join("config.toml");
    if !config_path.exists() {
        fs::write(config_path, CODEX_CONFIG)
            .map_err(|_| "UsageDock用のCodex設定を作成できませんでした".to_string())?;
    }
    Ok(())
}

fn desktop_codex_candidates() -> Vec<(PathBuf, SystemTime)> {
    let Some(local_app_data) = std::env::var_os("LOCALAPPDATA") else {
        return Vec::new();
    };
    let root = PathBuf::from(local_app_data)
        .join("OpenAI")
        .join("Codex")
        .join("bin");
    let Ok(entries) = fs::read_dir(root) else {
        return Vec::new();
    };
    let mut candidates = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        let executable = if path.is_dir() {
            path.join("codex.exe")
        } else {
            path
        };
        if executable
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.eq_ignore_ascii_case("codex.exe"))
            && executable.is_file()
        {
            let modified = executable
                .metadata()
                .and_then(|metadata| metadata.modified())
                .unwrap_or(SystemTime::UNIX_EPOCH);
            candidates.push((executable, modified));
        }
    }
    candidates
}

pub fn resolve_executable(configured: Option<&str>) -> Result<PathBuf, String> {
    if let Some(configured) = configured.map(str::trim).filter(|value| !value.is_empty()) {
        let path = PathBuf::from(configured);
        if path.is_file() {
            return Ok(path);
        }
        return Err("指定されたCodex実行ファイルが見つかりません".into());
    }
    if let Some((path, _)) = desktop_codex_candidates()
        .into_iter()
        .max_by_key(|(_, modified)| *modified)
    {
        return Ok(path);
    }
    Ok(PathBuf::from("codex"))
}

impl CodexClient {
    pub async fn connect(configured: Option<&str>, home: &Path) -> Result<Self, String> {
        prepare_home(home)?;
        let executable = resolve_executable(configured)?;
        let executable_path = executable.to_string_lossy().into_owned();
        let mut command = Command::new(&executable);
        #[cfg(target_os = "windows")]
        command.creation_flags(CREATE_NO_WINDOW);
        let mut child = command
            .args(["app-server", "--listen", "stdio://"])
            .env("CODEX_HOME", home)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .map_err(|_| "Codex App Serverを起動できませんでした".to_string())?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| "Codex App Serverの入力を開けませんでした".to_string())?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| "Codex App Serverの出力を開けませんでした".to_string())?;
        let mut client = Self {
            child,
            stdin,
            lines: BufReader::new(stdout).lines(),
            executable_path,
        };
        client
            .request(
                1,
                "initialize",
                json!({
                    "clientInfo": { "name": "usage_dock", "version": env!("CARGO_PKG_VERSION") },
                    "capabilities": {}
                }),
            )
            .await?;
        client
            .send(&json!({ "jsonrpc": "2.0", "method": "initialized", "params": {} }))
            .await?;
        Ok(client)
    }

    async fn send(&mut self, request: &Value) -> Result<(), String> {
        let payload = serde_json::to_vec(request)
            .map_err(|_| "Codex App Serverへの要求を作成できませんでした".to_string())?;
        self.stdin
            .write_all(&payload)
            .await
            .map_err(|_| "Codex App Serverへ送信できませんでした".to_string())?;
        self.stdin
            .write_all(b"\n")
            .await
            .map_err(|_| "Codex App Serverへ送信できませんでした".to_string())?;
        self.stdin
            .flush()
            .await
            .map_err(|_| "Codex App Serverへ送信できませんでした".to_string())
    }

    pub async fn request(&mut self, id: i64, method: &str, params: Value) -> Result<Value, String> {
        self.send(&json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params
        }))
        .await?;
        while let Some(line) = self
            .lines
            .next_line()
            .await
            .map_err(|_| "Codex App Serverの応答を読めませんでした".to_string())?
        {
            let Ok(value) = serde_json::from_str::<Value>(&line) else {
                continue;
            };
            if value.get("id").and_then(Value::as_i64) != Some(id) {
                continue;
            }
            if let Some(error) = value.get("error") {
                let message = error
                    .get("message")
                    .and_then(Value::as_str)
                    .unwrap_or("Codex App Serverが要求を拒否しました");
                return Err(message.to_string());
            }
            return Ok(value);
        }
        Err("Codex App Serverが応答を終了しました".into())
    }

    pub async fn kill(&mut self) {
        let _ = self.child.kill().await;
    }
}

pub async fn read_connection(
    configured: Option<&str>,
    home: &Path,
) -> Result<CodexConnection, String> {
    let task = async {
        let mut client = CodexClient::connect(configured, home).await?;
        let executable_path = client.executable_path.clone();
        let response = client
            .request(2, "account/read", json!({ "refreshToken": false }))
            .await;
        client.kill().await;
        let response = response?;
        Ok(parse_account_response(&response, executable_path))
    };
    timeout(Duration::from_secs(10), task)
        .await
        .map_err(|_| "Codexの接続状態確認がタイムアウトしました".to_string())?
}

fn parse_account_response(response: &Value, executable_path: String) -> CodexConnection {
    let account = response
        .get("result")
        .and_then(|result| result.get("account"));
    let Some(account) = account.filter(|value| !value.is_null()) else {
        return CodexConnection {
            status: ConnectionStatus::Disconnected,
            auth_type: None,
            email: None,
            plan_name: None,
            executable_path: Some(executable_path),
            pending_login: None,
            error: None,
        };
    };
    let kind = account.get("type").and_then(Value::as_str);
    let auth_type = match kind {
        Some("chatgpt") => Some(CodexAuthType::Chatgpt),
        Some("apiKey") => Some(CodexAuthType::ApiKey),
        _ => None,
    };
    let connected = auth_type.is_some();
    CodexConnection {
        status: if connected {
            ConnectionStatus::Connected
        } else {
            ConnectionStatus::Unavailable
        },
        auth_type,
        email: account
            .get("email")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned),
        plan_name: account
            .get("planType")
            .and_then(Value::as_str)
            .and_then(display_plan_name),
        executable_path: Some(executable_path),
        pending_login: None,
        error: (!connected).then(|| "このCodex認証方式はUsageDockで利用できません".to_string()),
    }
}

pub async fn begin_login(
    configured: Option<&str>,
    home: &Path,
    mode: CodexLoginMode,
) -> Result<LoginSession, String> {
    let mut client = CodexClient::connect(configured, home).await?;
    let params = match mode {
        CodexLoginMode::Browser => json!({
            "type": "chatgpt",
            "useHostedLoginSuccessPage": true,
            "appBrand": "chatgpt"
        }),
        CodexLoginMode::DeviceCode => json!({ "type": "chatgptDeviceCode" }),
    };
    let response = client.request(10, "account/login/start", params).await?;
    let result = response
        .get("result")
        .ok_or_else(|| "Codexの認証開始情報を取得できませんでした".to_string())?;
    let login_id = result
        .get("loginId")
        .and_then(Value::as_str)
        .ok_or_else(|| "Codexの認証IDを取得できませんでした".to_string())?
        .to_string();
    let verification_url = result
        .get("authUrl")
        .or_else(|| result.get("verificationUrl"))
        .and_then(Value::as_str)
        .filter(|url| is_allowed_auth_url(url))
        .ok_or_else(|| "Codexから安全な認証URLを取得できませんでした".to_string())?
        .to_string();
    let user_code = result
        .get("userCode")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned);
    Ok(LoginSession {
        client,
        prompt: CodexLoginPrompt {
            login_id,
            mode,
            verification_url,
            user_code,
        },
    })
}

impl LoginSession {
    pub async fn wait(mut self, mut cancel: oneshot::Receiver<()>) -> Result<(), String> {
        let login_id = self.prompt.login_id.clone();
        let deadline = sleep(Duration::from_secs(600));
        tokio::pin!(deadline);
        loop {
            tokio::select! {
                _ = &mut deadline => {
                    self.client.kill().await;
                    return Err("Codex認証が10分以内に完了しませんでした".into());
                }
                _ = &mut cancel => {
                    let _ = timeout(
                        Duration::from_secs(2),
                        self.client.request(
                            90,
                            "account/login/cancel",
                            json!({ "loginId": login_id }),
                        ),
                    ).await;
                    self.client.kill().await;
                    return Err(CANCELLED.into());
                }
                line = self.client.lines.next_line() => {
                    let line = line
                        .map_err(|_| "Codex認証の応答を読めませんでした".to_string())?
                        .ok_or_else(|| "Codex認証が途中で終了しました".to_string())?;
                    let Ok(value) = serde_json::from_str::<Value>(&line) else {
                        continue;
                    };
                    if value.get("method").and_then(Value::as_str) != Some("account/login/completed") {
                        continue;
                    }
                    let params = value.get("params").unwrap_or(&Value::Null);
                    if params.get("loginId").and_then(Value::as_str) != Some(login_id.as_str()) {
                        continue;
                    }
                    self.client.kill().await;
                    if params.get("success").and_then(Value::as_bool) == Some(true) {
                        return Ok(());
                    }
                    return Err(params
                        .get("error")
                        .and_then(Value::as_str)
                        .unwrap_or("Codex認証を完了できませんでした")
                        .to_string());
                }
            }
        }
    }
}

pub async fn login_api_key(
    configured: Option<&str>,
    home: &Path,
    api_key: String,
) -> Result<(), String> {
    let api_key = api_key.trim();
    if api_key.len() < 20 {
        return Err("有効なOpenAI APIキーを入力してください".into());
    }
    let task = async {
        let mut client = CodexClient::connect(configured, home).await?;
        let result = client
            .request(
                20,
                "account/login/start",
                json!({ "type": "apiKey", "apiKey": api_key }),
            )
            .await;
        client.kill().await;
        result.map(|_| ())
    };
    timeout(Duration::from_secs(15), task)
        .await
        .map_err(|_| "Codex APIキー認証がタイムアウトしました".to_string())?
}

pub async fn logout(configured: Option<&str>, home: &Path) -> Result<(), String> {
    let task = async {
        let mut client = CodexClient::connect(configured, home).await?;
        let result = client.request(30, "account/logout", json!({})).await;
        client.kill().await;
        result.map(|_| ())
    };
    timeout(Duration::from_secs(10), task)
        .await
        .map_err(|_| "Codexの接続解除がタイムアウトしました".to_string())?
}

pub fn is_allowed_auth_url(value: &str) -> bool {
    let Ok(url) = reqwest::Url::parse(value) else {
        return false;
    };
    if url.scheme() != "https" || url.username() != "" || url.password().is_some() {
        return false;
    }
    let Some(host) = url.host_str() else {
        return false;
    };
    host == "chatgpt.com"
        || host.ends_with(".chatgpt.com")
        || host == "openai.com"
        || host.ends_with(".openai.com")
}

pub fn was_cancelled(error: &str) -> bool {
    error == CANCELLED
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 許可した認証urlだけを受け入れる() {
        assert!(is_allowed_auth_url("https://chatgpt.com/auth/login?x=1"));
        assert!(is_allowed_auth_url("https://auth.openai.com/codex/device"));
        assert!(!is_allowed_auth_url("http://auth.openai.com/codex/device"));
        assert!(!is_allowed_auth_url(
            "https://openai.com.evil.example/login"
        ));
        assert!(!is_allowed_auth_url("file:///C:/Windows/System32/calc.exe"));
    }

    #[test]
    fn account応答を接続状態へ変換する() {
        let response = json!({
            "result": {
                "account": { "type": "chatgpt", "email": "user@example.com", "planType": "pro" },
                "requiresOpenaiAuth": true
            }
        });
        let connection = parse_account_response(&response, "codex.exe".into());
        assert_eq!(connection.status, ConnectionStatus::Connected);
        assert_eq!(connection.auth_type, Some(CodexAuthType::Chatgpt));
        assert_eq!(connection.plan_name.as_deref(), Some("ChatGPT Pro"));
    }
}
