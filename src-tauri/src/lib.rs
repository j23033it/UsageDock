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

struct BackendData {
    settings: AppSettings,
    snapshot: DashboardSnapshot,
    refreshing: HashSet<ProviderId>,
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
    let snapshot = {
        let mut data = state.data.lock().expect("状態ロック");
        data.refreshing.remove(&id);
        let provider = data
            .snapshot
            .providers
            .iter_mut()
            .find(|provider| provider.id == id)
            .expect("プロバイダー");
        match result {
            Ok(value) => {
                provider.source = value.source;
                provider.plan_name = value.plan_name;
                provider.windows = value.windows;
                provider.updated_at = Some(now_iso());
                provider.last_error = None;
                provider.status = ProviderStatus::Fresh;
            }
            Err(error) => {
                provider.last_error = Some(error);
                provider.status = classify(provider);
            }
        }
        data.snapshot.refreshed_at = now_iso();
        let codex_enabled = data.settings.codex_enabled;
        let open_code_go_enabled = data.settings.open_code_go_enabled;
        let provider_order = data.settings.provider_order.clone();
        data.snapshot
            .providers
            .retain(|provider| match provider.id {
                ProviderId::Codex => codex_enabled,
                ProviderId::OpenCodeGo => open_code_go_enabled,
            });
        for id in [ProviderId::Codex, ProviderId::OpenCodeGo] {
            if !data
                .snapshot
                .providers
                .iter()
                .any(|provider| provider.id == id)
            {
                data.snapshot.providers.push(empty_provider(id));
            }
        }
        data.snapshot.providers.sort_by_key(|provider| {
            provider_order
                .iter()
                .position(|id| id == &provider.id)
                .unwrap_or(usize::MAX)
        });
        state.save_snapshot(&data.snapshot);
        data.snapshot.clone()
    };
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
    {
        state.data.lock().expect("状態ロック").settings = settings;
    }
    if auto_start {
        let _ = app.autolaunch().enable();
    } else {
        let _ = app.autolaunch().disable();
    }
    if let Some(widget) = app.get_webview_window("widget") {
        reposition_widget(&widget);
    }
    Ok(())
}

#[tauri::command]
async fn set_opencode_api_key(api_key: String) -> Result<(), String> {
    if api_key.trim().is_empty() {
        return Err("APIキーを入力してください".into());
    }
    storage::save_opencode_key(api_key.trim())
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
        return Ok(());
    }
    let url = match provider_id {
        Some(id) => format!("index.html?provider={id:?}"),
        None => "index.html".to_string(),
    };
    tauri::WebviewWindowBuilder::new(&app, "settings", tauri::WebviewUrl::App(url.into()))
        .title("UsageDock 設定")
        .inner_size(560.0, 640.0)
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
    let base = if expanded { 388 } else { 76 };
    let width = (base * u32::from(settings.scale_percent) / 100).max(1);
    window
        .set_size(PhysicalSize::new(width, 196))
        .map_err(|_| "ウィジェットサイズを変更できませんでした".to_string())?;
    reposition_widget(&window);
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|_app, _argv, _cwd| {}))
        .plugin(tauri_plugin_autostart::Builder::new().build())
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
                })
                .build(app)?;
            if let Some(widget) = app.get_webview_window("widget") {
                reposition_widget(&widget);
                if settings.start_in_background {
                    let _ = widget.hide();
                }
            }
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
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
                    tokio::time::sleep(Duration::from_secs(interval)).await;
                    let _ = refresh_all(&handle, &state).await;
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
