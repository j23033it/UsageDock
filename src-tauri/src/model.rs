use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "kebab-case")]
pub enum WindowKind {
    FiveHour,
    Weekly,
    Monthly,
    Custom,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "kebab-case")]
pub enum ProviderId {
    Codex,
    #[serde(rename = "opencode-go")]
    OpenCodeGo,
}

impl ProviderId {
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Codex => "Codex",
            Self::OpenCodeGo => "OpenCode Go",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum ProviderStatus {
    Fresh,
    Stale,
    Outdated,
    Unavailable,
    Refreshing,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum UsageSource {
    AppServer,
    LocalLog,
    Api,
    None,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct UsageWindow {
    pub kind: WindowKind,
    pub label: String,
    pub used_percent: Option<f64>,
    pub remaining_percent: Option<f64>,
    pub resets_at: Option<String>,
    pub window_duration_minutes: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProviderUsage {
    pub id: ProviderId,
    pub display_name: String,
    pub plan_name: Option<String>,
    pub status: ProviderStatus,
    pub source: UsageSource,
    pub updated_at: Option<String>,
    pub last_error: Option<String>,
    pub windows: Vec<UsageWindow>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DashboardSnapshot {
    pub providers: Vec<ProviderUsage>,
    pub refreshed_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct NotificationThresholds {
    pub enabled: bool,
    pub warning_percent: u8,
    pub critical_percent: u8,
    pub exhausted_percent: u8,
}

impl Default for NotificationThresholds {
    fn default() -> Self {
        Self {
            enabled: true,
            warning_percent: 20,
            critical_percent: 10,
            exhausted_percent: 0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
#[serde(default)]
pub struct AppSettings {
    pub refresh_interval_seconds: u64,
    pub widget_size: String,
    pub scale_percent: u16,
    pub opacity_percent: u8,
    pub auto_start: bool,
    pub start_in_background: bool,
    pub provider_order: Vec<ProviderId>,
    pub codex_enabled: bool,
    pub open_code_go_enabled: bool,
    pub notification_thresholds: NotificationThresholds,
    pub codex_path: Option<String>,
    pub force_compatibility_mode: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            refresh_interval_seconds: 60,
            widget_size: "m".to_string(),
            scale_percent: 100,
            opacity_percent: 96,
            auto_start: false,
            start_in_background: true,
            provider_order: vec![ProviderId::Codex, ProviderId::OpenCodeGo],
            codex_enabled: true,
            open_code_go_enabled: false,
            notification_thresholds: NotificationThresholds::default(),
            codex_path: None,
            force_compatibility_mode: false,
        }
    }
}

impl AppSettings {
    pub fn clamped(mut self) -> Self {
        self.refresh_interval_seconds = self.refresh_interval_seconds.clamp(30, 900);
        self.scale_percent = self.scale_percent.clamp(75, 150);
        self.opacity_percent = self.opacity_percent.clamp(75, 100);
        if !matches!(self.widget_size.as_str(), "s" | "m" | "l") {
            self.widget_size = "m".to_string();
        }
        self.notification_thresholds.warning_percent =
            self.notification_thresholds.warning_percent.clamp(1, 100);
        self.notification_thresholds.critical_percent =
            self.notification_thresholds.critical_percent.clamp(1, 99);
        self.notification_thresholds.exhausted_percent = 0;
        if self.notification_thresholds.critical_percent
            >= self.notification_thresholds.warning_percent
        {
            self.notification_thresholds.critical_percent = self
                .notification_thresholds
                .warning_percent
                .saturating_sub(1);
        }
        self
    }
}

pub fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

pub fn unix_seconds_to_iso(value: &serde_json::Value) -> Option<String> {
    let seconds = value
        .as_i64()
        .or_else(|| value.as_u64().and_then(|v| i64::try_from(v).ok()))?;
    chrono::DateTime::<chrono::Utc>::from_timestamp(seconds, 0)
        .map(|date| date.to_rfc3339_opts(chrono::SecondsFormat::Millis, true))
}

pub fn clamp_percent(value: f64) -> f64 {
    value.clamp(0.0, 100.0)
}

pub fn kind_for_duration(minutes: Option<u64>) -> WindowKind {
    match minutes {
        Some(240..=360) => WindowKind::FiveHour,
        Some(6_000..=12_000) => WindowKind::Weekly,
        Some(value) if value >= 20_000 => WindowKind::Monthly,
        _ => WindowKind::Custom,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 残量を安全に変換する() {
        assert_eq!(clamp_percent(-1.0), 0.0);
        assert_eq!(clamp_percent(42.0), 42.0);
        assert_eq!(clamp_percent(101.0), 100.0);
    }

    #[test]
    fn 時間枠を判定する() {
        assert_eq!(kind_for_duration(Some(300)), WindowKind::FiveHour);
        assert_eq!(kind_for_duration(Some(10_080)), WindowKind::Weekly);
        assert_eq!(kind_for_duration(Some(43_200)), WindowKind::Monthly);
        assert_eq!(kind_for_duration(Some(17)), WindowKind::Custom);
    }

    #[test]
    fn 設定を範囲内へ補正する() {
        let settings = AppSettings {
            refresh_interval_seconds: 1,
            scale_percent: 200,
            opacity_percent: 1,
            widget_size: "x".into(),
            notification_thresholds: NotificationThresholds {
                enabled: true,
                warning_percent: 0,
                critical_percent: 100,
                exhausted_percent: 50,
            },
            ..Default::default()
        }
        .clamped();
        assert_eq!(settings.refresh_interval_seconds, 30);
        assert_eq!(settings.scale_percent, 150);
        assert_eq!(settings.opacity_percent, 75);
        assert_eq!(settings.widget_size, "m");
        assert_eq!(settings.notification_thresholds.warning_percent, 1);
        assert_eq!(settings.notification_thresholds.critical_percent, 0);
        assert_eq!(settings.notification_thresholds.exhausted_percent, 0);
    }
}
