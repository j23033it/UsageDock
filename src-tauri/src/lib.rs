mod codex;
mod model;
mod providers;
mod storage;
mod updates;

use model::{
    AppSettings, CodexConnection, CodexLoginMode, CodexLoginPrompt, ConnectionOverview,
    ConnectionStatus, DashboardSnapshot, OpenCodeConnection, ProviderId, ProviderStatus,
    ProviderUsage, UsageSource, now_iso,
};
use providers::{FetchResult, fetch_codex, fetch_opencode};
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Duration,
};
use tauri::menu::{MenuBuilder, MenuItemBuilder};
use tauri::tray::TrayIconBuilder;
use tauri::{Emitter, Manager, PhysicalPosition};
use tauri_plugin_autostart::ManagerExt as AutoStartManagerExt;
use tauri_plugin_notification::NotificationExt;
use tauri_plugin_opener::OpenerExt;
use tokio::sync::oneshot;

struct PendingCodexLogin {
    prompt: CodexLoginPrompt,
    cancel: Option<oneshot::Sender<()>>,
}

struct BackendData {
    settings: AppSettings,
    snapshot: DashboardSnapshot,
    connected: HashSet<ProviderId>,
    refreshing: HashSet<ProviderId>,
    notified: HashSet<(ProviderId, model::WindowKind, u8)>,
    widget_expanded: bool,
    codex_auth_starting: bool,
    pending_codex_login: Option<PendingCodexLogin>,
    last_codex_auth_error: Option<String>,
}

#[derive(Clone)]
struct BackendState {
    data: Arc<Mutex<BackendData>>,
    settings_path: PathBuf,
    snapshot_path: PathBuf,
    codex_home: PathBuf,
}

impl BackendState {
    fn save_snapshot(&self, snapshot: &DashboardSnapshot) -> Result<(), String> {
        storage::save_snapshot(&self.snapshot_path, snapshot)
    }
    fn save_settings(&self, settings: &AppSettings) -> Result<(), String> {
        storage::save_json(&self.settings_path, settings)
    }
}

fn initial_snapshot(path: &Path) -> DashboardSnapshot {
    storage::load_snapshot(path).unwrap_or_else(|| DashboardSnapshot {
        providers: vec![
            empty_provider(ProviderId::Codex),
            empty_provider(ProviderId::OpenCodeGo),
        ],
        refreshed_at: now_iso(),
    })
}

fn empty_provider(id: ProviderId) -> ProviderUsage {
    ProviderUsage {
        display_name: id.display_name().into(),
        id,
        plan_name: None,
        has_five_hour_limit: None,
        status: ProviderStatus::Unavailable,
        source: UsageSource::None,
        updated_at: None,
        last_error: None,
        windows: Vec::new(),
    }
}

fn reconcile_providers(data: &mut BackendData) {
    data.snapshot
        .providers
        .retain(|provider| data.connected.contains(&provider.id));
    for id in data.settings.provider_order.clone() {
        if data.connected.contains(&id)
            && !data
                .snapshot
                .providers
                .iter()
                .any(|provider| provider.id == id)
        {
            data.snapshot.providers.push(empty_provider(id));
        }
    }
    data.snapshot.providers.sort_by_key(|provider| {
        data.settings
            .provider_order
            .iter()
            .position(|id| id == &provider.id)
            .unwrap_or(usize::MAX)
    });
}

fn classify(status: &ProviderUsage) -> ProviderStatus {
    let Some(updated_at) = status.updated_at.as_deref() else {
        return ProviderStatus::Unavailable;
    };
    let Ok(updated) = chrono::DateTime::parse_from_rfc3339(updated_at) else {
        return ProviderStatus::Outdated;
    };
    let age = chrono::Utc::now().signed_duration_since(updated.with_timezone(&chrono::Utc));
    if age < chrono::Duration::minutes(2) {
        ProviderStatus::Fresh
    } else if age < chrono::Duration::minutes(10) {
        ProviderStatus::Stale
    } else {
        ProviderStatus::Outdated
    }
}

fn refresh_is_due(
    last_refresh: Option<chrono::DateTime<chrono::Utc>>,
    interval_seconds: u64,
    now: chrono::DateTime<chrono::Utc>,
) -> bool {
    last_refresh.is_none_or(|last| {
        let elapsed = now.signed_duration_since(last);
        elapsed < chrono::Duration::zero()
            || elapsed >= chrono::Duration::seconds(interval_seconds as i64)
    })
}

fn reached_notification_threshold(
    remaining: f64,
    thresholds: &model::NotificationThresholds,
) -> Option<u8> {
    if !thresholds.enabled {
        return None;
    }
    [
        thresholds.exhausted_percent,
        thresholds.critical_percent,
        thresholds.warning_percent,
    ]
    .into_iter()
    .find(|threshold| remaining <= f64::from(*threshold))
}

fn collect_notifications(
    data: &mut BackendData,
    id: &ProviderId,
    windows: &[model::UsageWindow],
) -> Vec<(String, f64, u8)> {
    let thresholds = data.settings.notification_thresholds.clone();
    if !thresholds.enabled {
        data.notified.retain(|(provider, _, _)| provider != id);
        return Vec::new();
    }
    let configured = [
        thresholds.exhausted_percent,
        thresholds.critical_percent,
        thresholds.warning_percent,
    ];
    let mut notices = Vec::new();
    for window in windows {
        let Some(remaining) = window.remaining_percent else {
            continue;
        };
        data.notified.retain(|(provider, kind, threshold)| {
            provider != id || kind != &window.kind || remaining <= f64::from(*threshold)
        });
        if let Some(threshold) = reached_notification_threshold(remaining, &thresholds) {
            let key = (id.clone(), window.kind.clone(), threshold);
            if configured.contains(&threshold) && data.notified.insert(key) {
                notices.push((window.label.clone(), remaining, threshold));
            }
        }
    }
    notices
}

async fn refresh_one(app: &tauri::AppHandle, state: &BackendState, id: ProviderId) {
    let settings = {
        let mut data = state.data.lock().expect("状態ロック");
        if !data.connected.contains(&id) || !data.refreshing.insert(id.clone()) {
            return;
        }
        if let Some(provider) = data
            .snapshot
            .providers
            .iter_mut()
            .find(|provider| provider.id == id)
        {
            provider.status = ProviderStatus::Refreshing;
        }
        data.settings.clone()
    };
    let result = match id.clone() {
        ProviderId::Codex => fetch_codex(&settings, &state.codex_home).await,
        ProviderId::OpenCodeGo => match storage::read_opencode_key() {
            Some(key) => fetch_opencode(key).await,
            None => Err("OpenCode GoのAPIキーが設定されていません".into()),
        },
    };
    let (snapshot, notices) = {
        let mut data = state.data.lock().expect("状態ロック");
        let notices = apply_refresh_result(&mut data, &id, result);
        if let Err(error) = state.save_snapshot(&data.snapshot)
            && let Some(provider) = data
                .snapshot
                .providers
                .iter_mut()
                .find(|provider| provider.id == id)
        {
            provider.last_error = Some(error);
        }
        (data.snapshot.clone(), notices)
    };
    for (window, remaining, threshold) in notices {
        let body = if threshold == 0 {
            format!("{window}を使い切りました")
        } else {
            format!("{window}の残量が{remaining:.0}%です")
        };
        let _ = app
            .notification()
            .builder()
            .title(format!("{}の残量通知", id.display_name()))
            .body(body)
            .show();
    }
    let _ = app.emit("usage-updated", &snapshot);
}

fn apply_refresh_result(
    data: &mut BackendData,
    id: &ProviderId,
    result: Result<FetchResult, String>,
) -> Vec<(String, f64, u8)> {
    data.refreshing.remove(id);
    if !data.connected.contains(id) {
        reconcile_providers(data);
        return Vec::new();
    }
    reconcile_providers(data);
    let mut notices = Vec::new();
    match result {
        Ok(value) => {
            notices = collect_notifications(data, id, &value.windows);
            if let Some(provider) = data
                .snapshot
                .providers
                .iter_mut()
                .find(|provider| provider.id == *id)
            {
                provider.source = value.source;
                provider.plan_name = value.plan_name;
                provider.has_five_hour_limit = value.has_five_hour_limit;
                provider.windows = value.windows;
                provider.updated_at = Some(now_iso());
                provider.last_error = None;
                provider.status = ProviderStatus::Fresh;
            }
        }
        Err(error) => {
            if let Some(provider) = data
                .snapshot
                .providers
                .iter_mut()
                .find(|provider| provider.id == *id)
            {
                provider.last_error = Some(error);
                provider.status = classify(provider);
            }
        }
    }
    data.snapshot.refreshed_at = now_iso();
    notices
}

async fn refresh_all(app: &tauri::AppHandle, state: &BackendState) -> DashboardSnapshot {
    emit_connections(app, state).await;
    let ids = {
        let data = state.data.lock().expect("状態ロック");
        data.settings
            .provider_order
            .iter()
            .filter(|id| data.connected.contains(id))
            .cloned()
            .collect::<Vec<_>>()
    };
    for id in ids {
        refresh_one(app, state, id).await;
    }
    get_dashboard_inner(state)
}

fn get_dashboard_inner(state: &BackendState) -> DashboardSnapshot {
    let data = state.data.lock().expect("状態ロック");
    // 起動直後の接続確認を待つ間も、保存済みの正常値は破棄しない。
    let mut snapshot = data.snapshot.clone();
    snapshot
        .providers
        .retain(|provider| data.connected.contains(&provider.id));
    for provider in &mut snapshot.providers {
        if !matches!(provider.status, ProviderStatus::Refreshing) {
            provider.status = classify(provider);
        }
    }
    snapshot
}

fn reposition_widget(window: &tauri::WebviewWindow) {
    let Ok(Some(monitor)) = window.primary_monitor() else {
        return;
    };
    let Ok(size) = window.inner_size() else {
        return;
    };
    let (x, y) = widget_position(
        monitor.position().x,
        monitor.position().y,
        monitor.size().width,
        monitor.size().height,
        size.width,
        size.height,
    );
    let _ = window.set_position(PhysicalPosition::new(x, y));
}

fn widget_position(
    monitor_x: i32,
    monitor_y: i32,
    monitor_width: u32,
    monitor_height: u32,
    window_width: u32,
    window_height: u32,
) -> (i32, i32) {
    let horizontal_space = monitor_width.saturating_sub(window_width) as i32;
    let vertical_space = monitor_height.saturating_sub(window_height) as i32;
    let x = monitor_x + horizontal_space;
    let centered_y = monitor_y + vertical_space / 2;
    let upward_offset = (monitor_height as i32 * 35 / 100).min(vertical_space / 2);
    (x, centered_y - upward_offset)
}

fn apply_widget_dimensions(
    window: &tauri::WebviewWindow,
    settings: &AppSettings,
    expanded: bool,
) -> Result<(), String> {
    let (collapsed_width, expanded_width, base_height) = match settings.widget_size.as_str() {
        "s" => (42_u32, 260_u32, 200_u32),
        "l" => (64_u32, 328_u32, 260_u32),
        _ => (52_u32, 292_u32, 224_u32),
    };
    let base_width = if expanded {
        expanded_width
    } else {
        collapsed_width
    };
    let count = window
        .app_handle()
        .try_state::<BackendState>()
        .map(|state| state.data.lock().expect("状態ロック").connected.len())
        .unwrap_or(0) as u32;
    let row_height = match settings.widget_size.as_str() {
        "s" => 60,
        "l" => 78,
        _ => 66,
    };
    let base_height = base_height - row_height * (2 - count.min(2));
    let scale = u32::from(settings.scale_percent);
    let width = (base_width * scale / 100).max(1);
    let height = (base_height * scale / 100).max(1);
    // 右端を固定したまま位置とサイズを一度に変更し、画面外へ飛ぶ中間状態をなくす。
    let scale_factor = window.scale_factor().map_err(|error| error.to_string())?;
    let physical_width = (f64::from(width) * scale_factor).round() as u32;
    let physical_height = (f64::from(height) * scale_factor).round() as u32;
    let monitor = window
        .primary_monitor()
        .map_err(|error| error.to_string())?
        .ok_or("モニターを取得できませんでした")?;
    let (x, y) = widget_position(
        monitor.position().x,
        monitor.position().y,
        monitor.size().width,
        monitor.size().height,
        physical_width,
        physical_height,
    );
    if window.inner_size().ok() == Some(tauri::PhysicalSize::new(physical_width, physical_height))
        && window.outer_position().ok() == Some(PhysicalPosition::new(x, y))
    {
        return Ok(());
    }
    #[cfg(target_os = "windows")]
    {
        #[link(name = "user32")]
        unsafe extern "system" {
            fn SetWindowPos(
                hwnd: *mut std::ffi::c_void,
                after: *mut std::ffi::c_void,
                x: i32,
                y: i32,
                width: i32,
                height: i32,
                flags: u32,
            ) -> i32;
        }
        let hwnd = window.hwnd().map_err(|error| error.to_string())?;
        // HWNDは生存中のウィンドウから取得。Z順とフォーカスは変更しない。
        let result = unsafe {
            SetWindowPos(
                hwnd.0 as _,
                std::ptr::null_mut(),
                x,
                y,
                physical_width as i32,
                physical_height as i32,
                0x0004 | 0x0010,
            )
        };
        if result == 0 {
            return Err("ウィジェットサイズを変更できませんでした".into());
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        window
            .set_position(PhysicalPosition::new(x, y))
            .map_err(|error| error.to_string())?;
        window
            .set_size(tauri::PhysicalSize::new(physical_width, physical_height))
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

#[tauri::command]
async fn get_dashboard(state: tauri::State<'_, BackendState>) -> Result<DashboardSnapshot, String> {
    Ok(get_dashboard_inner(&state))
}

#[tauri::command]
async fn refresh_usage(
    app: tauri::AppHandle,
    state: tauri::State<'_, BackendState>,
) -> Result<DashboardSnapshot, String> {
    Ok(refresh_all(&app, &state).await)
}

#[tauri::command]
async fn get_settings(state: tauri::State<'_, BackendState>) -> Result<AppSettings, String> {
    Ok(state.data.lock().expect("状態ロック").settings.clone())
}

#[tauri::command]
async fn check_for_update(app: tauri::AppHandle) -> Result<model::AppUpdate, String> {
    updates::check(&app).await
}

#[tauri::command]
async fn install_update(app: tauri::AppHandle) -> Result<(), String> {
    updates::install(&app).await
}

async fn connection_overview(state: &BackendState) -> ConnectionOverview {
    let (settings, pending, starting, last_error) = {
        let data = state.data.lock().expect("状態ロック");
        (
            data.settings.clone(),
            data.pending_codex_login
                .as_ref()
                .map(|pending| pending.prompt.clone()),
            data.codex_auth_starting,
            data.last_codex_auth_error.clone(),
        )
    };
    let codex = if starting || pending.is_some() {
        CodexConnection {
            status: ConnectionStatus::Connecting,
            auth_type: None,
            email: None,
            plan_name: None,
            executable_path: codex::resolve_executable(settings.codex_path.as_deref())
                .ok()
                .map(|path| path.to_string_lossy().into_owned()),
            pending_login: pending,
            error: None,
        }
    } else {
        match codex::read_connection(settings.codex_path.as_deref(), &state.codex_home).await {
            Ok(mut connection) => {
                if connection.error.is_none() {
                    connection.error = last_error;
                }
                connection
            }
            Err(error) => CodexConnection {
                status: ConnectionStatus::Error,
                auth_type: None,
                email: None,
                plan_name: None,
                executable_path: codex::resolve_executable(settings.codex_path.as_deref())
                    .ok()
                    .map(|path| path.to_string_lossy().into_owned()),
                pending_login: None,
                error: Some(error),
            },
        }
    };
    ConnectionOverview {
        codex,
        open_code_go: OpenCodeConnection {
            status: if storage::read_opencode_key().is_some() {
                ConnectionStatus::Connected
            } else {
                ConnectionStatus::Disconnected
            },
        },
    }
}

async fn emit_connections(app: &tauri::AppHandle, state: &BackendState) {
    let overview = connection_overview(state).await;
    let snapshot = {
        let mut data = state.data.lock().expect("状態ロック");
        for (id, status) in [
            (ProviderId::Codex, &overview.codex.status),
            (ProviderId::OpenCodeGo, &overview.open_code_go.status),
        ] {
            match status {
                ConnectionStatus::Connected => {
                    data.connected.insert(id);
                }
                ConnectionStatus::Disconnected => {
                    data.connected.remove(&id);
                }
                _ => {}
            }
        }
        reconcile_providers(&mut data);
        data.snapshot.clone()
    };
    let (settings, expanded) = {
        let data = state.data.lock().expect("状態ロック");
        (data.settings.clone(), data.widget_expanded)
    };
    if let Some(widget) = app.get_webview_window("widget") {
        let _ = apply_widget_dimensions(&widget, &settings, expanded);
    }
    let _ = app.emit("usage-updated", snapshot);
    let _ = app.emit("connections-updated", overview);
}

#[tauri::command]
async fn get_connections(
    state: tauri::State<'_, BackendState>,
) -> Result<ConnectionOverview, String> {
    Ok(connection_overview(&state).await)
}

#[tauri::command]
async fn start_codex_login(
    app: tauri::AppHandle,
    state: tauri::State<'_, BackendState>,
    mode: CodexLoginMode,
) -> Result<CodexLoginPrompt, String> {
    let settings = {
        let mut data = state.data.lock().expect("状態ロック");
        if let Some(pending) = &data.pending_codex_login {
            return Ok(pending.prompt.clone());
        }
        if data.codex_auth_starting {
            return Err("Codex認証を開始しています".into());
        }
        data.codex_auth_starting = true;
        data.last_codex_auth_error = None;
        data.settings.clone()
    };
    let session = codex::begin_login(settings.codex_path.as_deref(), &state.codex_home, mode).await;
    {
        state.data.lock().expect("状態ロック").codex_auth_starting = false;
    }
    let session = session?;
    let prompt = session.prompt.clone();
    app.opener()
        .open_url(&prompt.verification_url, None::<&str>)
        .map_err(|_| "認証ページをブラウザーで開けませんでした".to_string())?;
    let (cancel, cancel_receiver) = oneshot::channel();
    {
        state.data.lock().expect("状態ロック").pending_codex_login = Some(PendingCodexLogin {
            prompt: prompt.clone(),
            cancel: Some(cancel),
        });
    }
    let background_state = (*state).clone();
    let background_app = app.clone();
    let login_id = prompt.login_id.clone();
    tauri::async_runtime::spawn(async move {
        let result = session.wait(cancel_receiver).await;
        {
            let mut data = background_state.data.lock().expect("状態ロック");
            if data
                .pending_codex_login
                .as_ref()
                .is_some_and(|pending| pending.prompt.login_id == login_id)
            {
                data.pending_codex_login = None;
            }
            data.last_codex_auth_error = result
                .as_ref()
                .err()
                .filter(|error| !codex::was_cancelled(error))
                .cloned();
        }
        emit_connections(&background_app, &background_state).await;
        if result.is_ok() {
            refresh_one(&background_app, &background_state, ProviderId::Codex).await;
        }
        emit_connections(&background_app, &background_state).await;
    });
    emit_connections(&app, &state).await;
    Ok(prompt)
}

#[tauri::command]
async fn cancel_codex_login(
    app: tauri::AppHandle,
    state: tauri::State<'_, BackendState>,
) -> Result<(), String> {
    let cancel = {
        let mut data = state.data.lock().expect("状態ロック");
        let cancel = data
            .pending_codex_login
            .as_mut()
            .and_then(|pending| pending.cancel.take());
        data.pending_codex_login = None;
        cancel
    };
    if let Some(cancel) = cancel {
        let _ = cancel.send(());
    }
    emit_connections(&app, &state).await;
    Ok(())
}

#[tauri::command]
async fn set_codex_api_key(
    app: tauri::AppHandle,
    state: tauri::State<'_, BackendState>,
    api_key: String,
) -> Result<(), String> {
    let settings = {
        let data = state.data.lock().expect("状態ロック");
        if data.pending_codex_login.is_some() || data.codex_auth_starting {
            return Err("進行中のCodex認証を先にキャンセルしてください".into());
        }
        data.settings.clone()
    };
    codex::login_api_key(settings.codex_path.as_deref(), &state.codex_home, api_key).await?;
    state.data.lock().expect("状態ロック").last_codex_auth_error = None;
    emit_connections(&app, &state).await;
    refresh_one(&app, &state, ProviderId::Codex).await;
    emit_connections(&app, &state).await;
    Ok(())
}

#[tauri::command]
async fn disconnect_codex(
    app: tauri::AppHandle,
    state: tauri::State<'_, BackendState>,
) -> Result<(), String> {
    let settings = {
        let data = state.data.lock().expect("状態ロック");
        if data.pending_codex_login.is_some() || data.codex_auth_starting {
            return Err("進行中のCodex認証を先にキャンセルしてください".into());
        }
        data.settings.clone()
    };
    codex::logout(settings.codex_path.as_deref(), &state.codex_home).await?;
    let snapshot = {
        let mut data = state.data.lock().expect("状態ロック");
        data.last_codex_auth_error = None;
        data.connected.remove(&ProviderId::Codex);
        reconcile_providers(&mut data);
        let snapshot = data.snapshot.clone();
        state.save_snapshot(&snapshot)?;
        snapshot
    };
    let _ = app.emit("usage-updated", snapshot);
    emit_connections(&app, &state).await;
    Ok(())
}

#[tauri::command]
async fn save_settings(
    app: tauri::AppHandle,
    state: tauri::State<'_, BackendState>,
    settings: AppSettings,
) -> Result<(), String> {
    let settings = settings.clamped();
    state.save_settings(&settings)?;
    let auto_start = settings.auto_start;
    let settings_for_event = settings.clone();
    let (snapshot, widget_expanded) = {
        let mut data = state.data.lock().expect("状態ロック");
        data.settings = settings;
        reconcile_providers(&mut data);
        (data.snapshot.clone(), data.widget_expanded)
    };
    if auto_start {
        let _ = app.autolaunch().enable();
    } else {
        let _ = app.autolaunch().disable();
    }
    if let Some(widget) = app.get_webview_window("widget") {
        apply_widget_dimensions(&widget, &settings_for_event, widget_expanded)?;
    }
    let _ = app.emit("settings-updated", settings_for_event);
    let _ = app.emit("usage-updated", snapshot);
    Ok(())
}

#[tauri::command]
async fn set_opencode_api_key(
    app: tauri::AppHandle,
    state: tauri::State<'_, BackendState>,
    api_key: String,
) -> Result<(), String> {
    if api_key.trim().is_empty() {
        return Err("APIキーを入力してください".into());
    }
    let api_key = api_key.trim().to_string();
    fetch_opencode(api_key.clone()).await?;
    storage::save_opencode_key(&api_key)?;
    emit_connections(&app, &state).await;
    refresh_one(&app, &state, ProviderId::OpenCodeGo).await;
    emit_connections(&app, &state).await;
    Ok(())
}

#[tauri::command]
async fn disconnect_opencode(
    app: tauri::AppHandle,
    state: tauri::State<'_, BackendState>,
) -> Result<(), String> {
    storage::delete_opencode_key()?;
    let snapshot = {
        let mut data = state.data.lock().expect("状態ロック");
        data.connected.remove(&ProviderId::OpenCodeGo);
        reconcile_providers(&mut data);
        let snapshot = data.snapshot.clone();
        state.save_snapshot(&snapshot)?;
        snapshot
    };
    let _ = app.emit("usage-updated", snapshot);
    emit_connections(&app, &state).await;
    Ok(())
}

#[tauri::command]
async fn open_settings(
    app: tauri::AppHandle,
    provider_id: Option<ProviderId>,
) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("settings") {
        let _ = window.show();
        let _ = window.set_focus();
        if let Some(id) = provider_id {
            let _ = app.emit_to("settings", "settings-focus-provider", id);
        }
        return Ok(());
    }
    let url = match provider_id.as_ref() {
        Some(id) => format!("index.html?view=settings&provider={}", id.as_str()),
        None => "index.html".to_string(),
    };
    tauri::WebviewWindowBuilder::new(&app, "settings", tauri::WebviewUrl::App(url.into()))
        .title("UsageDock 設定")
        .inner_size(760.0, 680.0)
        .resizable(true)
        .build()
        .map(|_| ())
        .map_err(|_| "設定画面を開けませんでした".into())
}

#[tauri::command]
async fn set_widget_expanded(
    app: tauri::AppHandle,
    state: tauri::State<'_, BackendState>,
    expanded: bool,
) -> Result<(), String> {
    let Some(window) = app.get_webview_window("widget") else {
        return Err("ウィジェットが見つかりません".into());
    };
    let settings = {
        let mut data = state.data.lock().expect("状態ロック");
        data.widget_expanded = expanded;
        data.settings.clone()
    };
    apply_widget_dimensions(&window, &settings, expanded)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            if let Some(widget) = app.get_webview_window("widget") {
                let _ = widget.show();
                reposition_widget(&widget);
            }
        }))
        .plugin(tauri_plugin_autostart::Builder::new().build())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(
            tauri_plugin_updater::Builder::new()
                .pubkey(updates::PUBLIC_KEY)
                .build(),
        )
        .setup(|app| {
            let config_dir = app
                .path()
                .app_config_dir()
                .map_err(|error| error.to_string())?;
            let state = BackendState {
                data: Arc::new(Mutex::new(BackendData {
                    settings: storage::load_settings(&config_dir.join("settings.json")),
                    snapshot: initial_snapshot(&config_dir.join("snapshot.json")),
                    connected: HashSet::new(),
                    refreshing: HashSet::new(),
                    notified: HashSet::new(),
                    widget_expanded: false,
                    codex_auth_starting: false,
                    pending_codex_login: None,
                    last_codex_auth_error: None,
                })),
                settings_path: config_dir.join("settings.json"),
                snapshot_path: config_dir.join("snapshot.json"),
                codex_home: config_dir.join("codex"),
            };
            app.manage(state);
            let settings = app
                .state::<BackendState>()
                .data
                .lock()
                .expect("状態ロック")
                .settings
                .clone();
            if settings.auto_start {
                let _ = app.autolaunch().enable();
            } else {
                let _ = app.autolaunch().disable();
            }
            let settings_item = MenuItemBuilder::with_id("settings", "設定").build(app)?;
            let refresh_item = MenuItemBuilder::with_id("refresh", "今すぐ更新").build(app)?;
            let quit_item = MenuItemBuilder::with_id("quit", "終了").build(app)?;
            let menu = MenuBuilder::new(app)
                .items(&[&settings_item, &refresh_item, &quit_item])
                .build()?;
            let mut tray =
                TrayIconBuilder::new()
                    .menu(&menu)
                    .on_menu_event(|app, event| match event.id().as_ref() {
                        "settings" => {
                            let app = app.clone();
                            tauri::async_runtime::spawn(async move {
                                let _ = open_settings(app, None).await;
                            });
                        }
                        "refresh" => {
                            if let Some(state) = app.try_state::<BackendState>() {
                                let state = (*state).clone();
                                let app = app.clone();
                                tauri::async_runtime::spawn(async move {
                                    let _ = refresh_all(&app, &state).await;
                                });
                            }
                        }
                        "quit" => app.exit(0),
                        _ => {}
                    });
            if let Some(icon) = app.default_window_icon() {
                tray = tray.icon(icon.clone());
            }
            tray.build(app)?;
            if let Some(widget) = app.get_webview_window("widget") {
                apply_widget_dimensions(&widget, &settings, false)?;
            }
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                let mut last_refresh = None;
                loop {
                    let Some(state) = handle.try_state::<BackendState>() else {
                        break;
                    };
                    let interval = state
                        .data
                        .lock()
                        .expect("状態ロック")
                        .settings
                        .refresh_interval_seconds;
                    let now = chrono::Utc::now();
                    if refresh_is_due(last_refresh, interval, now) {
                        last_refresh = Some(now);
                        let _ = refresh_all(&handle, &state).await;
                    }
                    tokio::time::sleep(Duration::from_secs(5)).await;
                }
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_dashboard,
            refresh_usage,
            get_settings,
            save_settings,
            check_for_update,
            install_update,
            get_connections,
            start_codex_login,
            cancel_codex_login,
            set_codex_api_key,
            disconnect_codex,
            set_opencode_api_key,
            disconnect_opencode,
            open_settings,
            set_widget_expanded
        ]);
    builder
        .run(tauri::generate_context!())
        .expect("UsageDockの起動に失敗しました");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 最終成功時刻から状態を分類する() {
        let recent = ProviderUsage {
            updated_at: Some((chrono::Utc::now() - chrono::Duration::minutes(1)).to_rfc3339()),
            ..empty_provider(ProviderId::Codex)
        };
        let stale = ProviderUsage {
            updated_at: Some((chrono::Utc::now() - chrono::Duration::minutes(3)).to_rfc3339()),
            ..empty_provider(ProviderId::Codex)
        };
        let outdated = ProviderUsage {
            updated_at: Some((chrono::Utc::now() - chrono::Duration::minutes(11)).to_rfc3339()),
            ..empty_provider(ProviderId::Codex)
        };
        assert_eq!(classify(&recent), ProviderStatus::Fresh);
        assert_eq!(classify(&stale), ProviderStatus::Stale);
        assert_eq!(classify(&outdated), ProviderStatus::Outdated);
        assert_eq!(
            classify(&empty_provider(ProviderId::Codex)),
            ProviderStatus::Unavailable
        );
    }

    #[test]
    fn 壁時計で更新期限を判定する() {
        let now = chrono::Utc::now();
        assert!(refresh_is_due(None, 60, now));
        assert!(!refresh_is_due(
            Some(now - chrono::Duration::seconds(59)),
            60,
            now
        ));
        assert!(refresh_is_due(
            Some(now - chrono::Duration::hours(8)),
            60,
            now
        ));
        assert!(refresh_is_due(
            Some(now + chrono::Duration::minutes(1)),
            60,
            now
        ));
    }

    #[test]
    fn 残量に応じて最も強い通知閾値を選ぶ() {
        let thresholds = model::NotificationThresholds::default();
        assert_eq!(reached_notification_threshold(21.0, &thresholds), None);
        assert_eq!(reached_notification_threshold(20.0, &thresholds), Some(20));
        assert_eq!(reached_notification_threshold(9.0, &thresholds), Some(10));
        assert_eq!(reached_notification_threshold(0.0, &thresholds), Some(0));
    }

    #[test]
    fn 接続解除後の取得結果を破棄し再接続で表示を戻す() {
        let settings = AppSettings::default();
        let mut data = BackendData {
            settings,
            snapshot: DashboardSnapshot {
                providers: vec![empty_provider(ProviderId::Codex)],
                refreshed_at: now_iso(),
            },
            connected: HashSet::from([ProviderId::OpenCodeGo]),
            refreshing: HashSet::from([ProviderId::Codex]),
            notified: HashSet::new(),
            widget_expanded: false,
            codex_auth_starting: false,
            pending_codex_login: None,
            last_codex_auth_error: None,
        };
        let result = FetchResult {
            source: UsageSource::AppServer,
            plan_name: Some("Pro".into()),
            has_five_hour_limit: Some(true),
            windows: Vec::new(),
        };

        let notices = apply_refresh_result(&mut data, &ProviderId::Codex, Ok(result));

        assert!(notices.is_empty());
        assert!(data.refreshing.is_empty());
        assert!(
            data.snapshot
                .providers
                .iter()
                .all(|provider| provider.id != ProviderId::Codex)
        );
        assert!(
            data.snapshot
                .providers
                .iter()
                .any(|provider| provider.id == ProviderId::OpenCodeGo)
        );
        data.connected.insert(ProviderId::Codex);
        reconcile_providers(&mut data);
        assert_eq!(data.snapshot.providers.len(), 2);
        assert_eq!(data.snapshot.providers[0].id, ProviderId::Codex);
        data.connected.clear();
        reconcile_providers(&mut data);
        assert!(data.snapshot.providers.is_empty());
    }

    #[test]
    fn ウィジェットを右端かつ中央より上へ配置する() {
        let position = widget_position(0, 0, 1920, 1080, 52, 224);
        assert_eq!(position.0, 1868);
        assert_eq!(position.1, 50);
        assert!(position.1 < (1080 - 224) / 2);

        let compact_position = widget_position(0, 0, 1280, 300, 52, 224);
        assert_eq!(compact_position.1, 0);
    }
}
