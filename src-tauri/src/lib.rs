mod model;
mod providers;
mod storage;

use model::{
    AppSettings, DashboardSnapshot, ProviderId, ProviderStatus, ProviderUsage, UsageSource, now_iso,
};
use providers::{fetch_codex, fetch_opencode};
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Duration,
};
use tauri::menu::{MenuBuilder, MenuItemBuilder};
use tauri::tray::TrayIconBuilder;
use tauri::{Emitter, Manager, PhysicalPosition, PhysicalSize, WindowEvent};
use tauri_plugin_autostart::ManagerExt as AutoStartManagerExt;
use tauri_plugin_notification::NotificationExt;

struct BackendData {
    settings: AppSettings,
    snapshot: DashboardSnapshot,
    refreshing: HashSet<ProviderId>,
    notified: HashSet<(ProviderId, model::WindowKind, u8)>,
}

#[derive(Clone)]
struct BackendState {
    data: Arc<Mutex<BackendData>>,
    settings_path: PathBuf,
    snapshot_path: PathBuf,
}

impl BackendState {
    fn save_snapshot(&self, snapshot: &DashboardSnapshot) {
        let _ = storage::save_json(&self.snapshot_path, snapshot);
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
        status: ProviderStatus::Unavailable,
        source: UsageSource::None,
        updated_at: None,
        last_error: None,
        windows: Vec::new(),
    }
}

fn provider_enabled(settings: &AppSettings, id: &ProviderId) -> bool {
    match id {
        ProviderId::Codex => settings.codex_enabled,
        ProviderId::OpenCodeGo => settings.open_code_go_enabled,
    }
}

fn reconcile_providers(data: &mut BackendData) {
    data.snapshot
        .providers
        .retain(|provider| provider_enabled(&data.settings, &provider.id));
    for id in data.settings.provider_order.clone() {
        if provider_enabled(&data.settings, &id)
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
        if !data.refreshing.insert(id.clone()) {
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
        ProviderId::Codex => fetch_codex(&settings).await,
        ProviderId::OpenCodeGo => match storage::read_opencode_key() {
            Some(key) => fetch_opencode(key).await,
            None => Err("OpenCode GoのAPIキーが設定されていません".into()),
        },
    };
    let (snapshot, notices) = {
        let mut data = state.data.lock().expect("状態ロック");
        data.refreshing.remove(&id);
        let mut notices = Vec::new();
        match result {
            Ok(value) => {
                notices = collect_notifications(&mut data, &id, &value.windows);
                let provider = data
                    .snapshot
                    .providers
                    .iter_mut()
                    .find(|provider| provider.id == id)
                    .expect("プロバイダー");
                provider.source = value.source;
                provider.plan_name = value.plan_name;
                provider.windows = value.windows;
                provider.updated_at = Some(now_iso());
                provider.last_error = None;
                provider.status = ProviderStatus::Fresh;
            }
            Err(error) => {
                let provider = data
                    .snapshot
                    .providers
                    .iter_mut()
                    .find(|provider| provider.id == id)
                    .expect("プロバイダー");
                provider.last_error = Some(error);
                provider.status = classify(provider);
            }
        }
        data.snapshot.refreshed_at = now_iso();
        reconcile_providers(&mut data);
        state.save_snapshot(&data.snapshot);
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

async fn refresh_all(app: &tauri::AppHandle, state: &BackendState) -> DashboardSnapshot {
    let ids = {
        let data = state.data.lock().expect("状態ロック");
        data.settings
            .provider_order
            .iter()
            .filter(|id| match id {
                ProviderId::Codex => data.settings.codex_enabled,
                ProviderId::OpenCodeGo => data.settings.open_code_go_enabled,
            })
            .cloned()
            .collect::<Vec<_>>()
    };
    for id in ids {
        refresh_one(app, state, id).await;
    }
    get_dashboard_inner(state)
}

fn get_dashboard_inner(state: &BackendState) -> DashboardSnapshot {
    let mut data = state.data.lock().expect("状態ロック");
    reconcile_providers(&mut data);
    for provider in &mut data.snapshot.providers {
        if !matches!(provider.status, ProviderStatus::Refreshing) {
            provider.status = classify(provider);
        }
    }
    data.snapshot.clone()
}

fn reposition_widget(window: &tauri::WebviewWindow) {
    let Ok(Some(monitor)) = window.primary_monitor() else {
        return;
    };
    let Ok(size) = window.inner_size() else {
        return;
    };
    let x = monitor.position().x + monitor.size().width as i32 - size.width as i32 - 8;
    let y = monitor.position().y + (monitor.size().height as i32 - size.height as i32) / 2;
    let _ = window.set_position(PhysicalPosition::new(x, y));
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
async fn save_settings(
    app: tauri::AppHandle,
    state: tauri::State<'_, BackendState>,
    settings: AppSettings,
) -> Result<(), String> {
    let settings = settings.clamped();
    state.save_settings(&settings)?;
    let auto_start = settings.auto_start;
    let snapshot = {
        let mut data = state.data.lock().expect("状態ロック");
        data.settings = settings;
        reconcile_providers(&mut data);
        data.snapshot.clone()
    };
    if auto_start {
        let _ = app.autolaunch().enable();
    } else {
        let _ = app.autolaunch().disable();
    }
    if let Some(widget) = app.get_webview_window("widget") {
        reposition_widget(&widget);
    }
    let _ = app.emit("usage-updated", snapshot);
    Ok(())
}

#[tauri::command]
async fn set_opencode_api_key(api_key: String) -> Result<(), String> {
    if api_key.trim().is_empty() {
        return Err("APIキーを入力してください".into());
    }
    let api_key = api_key.trim().to_string();
    fetch_opencode(api_key.clone()).await?;
    storage::save_opencode_key(&api_key)
}

#[tauri::command]
async fn disconnect_opencode() -> Result<(), String> {
    storage::delete_opencode_key()
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
    let settings = state.data.lock().expect("状態ロック").settings.clone();
    let (collapsed_width, expanded_width, base_height) = match settings.widget_size.as_str() {
        "s" => (56, 350, 268),
        "l" => (92, 430, 372),
        _ => (76, 388, 320),
    };
    let base_width = if expanded {
        expanded_width
    } else {
        collapsed_width
    };
    let scale = u32::from(settings.scale_percent);
    let width = (base_width * scale / 100).max(1);
    let height = (base_height * scale / 100).max(1);
    window
        .set_size(PhysicalSize::new(width, height))
        .map_err(|_| "ウィジェットサイズを変更できませんでした".to_string())?;
    reposition_widget(&window);
    Ok(())
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
        .setup(|app| {
            let config_dir = app
                .path()
                .app_config_dir()
                .map_err(|error| error.to_string())?;
            let state = BackendState {
                data: Arc::new(Mutex::new(BackendData {
                    settings: storage::load_settings(&config_dir.join("settings.json")),
                    snapshot: initial_snapshot(&config_dir.join("snapshot.json")),
                    refreshing: HashSet::new(),
                    notified: HashSet::new(),
                })),
                settings_path: config_dir.join("settings.json"),
                snapshot_path: config_dir.join("snapshot.json"),
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
                reposition_widget(&widget);
            }
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                loop {
                    let Some(state) = handle.try_state::<BackendState>() else {
                        break;
                    };
                    let _ = refresh_all(&handle, &state).await;
                    let interval = state
                        .data
                        .lock()
                        .expect("状態ロック")
                        .settings
                        .refresh_interval_seconds;
                    tokio::time::sleep(Duration::from_secs(interval)).await;
                }
            });
            Ok(())
        })
        .on_window_event(|window, event| {
            if matches!(event, WindowEvent::Resized(_))
                && window.label() == "widget"
                && let Some(webview) = window.app_handle().get_webview_window("widget")
            {
                reposition_widget(&webview);
            }
        })
        .invoke_handler(tauri::generate_handler![
            get_dashboard,
            refresh_usage,
            get_settings,
            save_settings,
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
    fn 残量に応じて最も強い通知閾値を選ぶ() {
        let thresholds = model::NotificationThresholds::default();
        assert_eq!(reached_notification_threshold(21.0, &thresholds), None);
        assert_eq!(reached_notification_threshold(20.0, &thresholds), Some(20));
        assert_eq!(reached_notification_threshold(9.0, &thresholds), Some(10));
        assert_eq!(reached_notification_threshold(0.0, &thresholds), Some(0));
    }
}
