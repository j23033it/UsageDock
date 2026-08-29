export type WidgetSize = "s" | "m" | "l";

export type ProviderStatus = "fresh" | "stale" | "outdated" | "unavailable" | "refreshing";

export type UsageSource = "app-server" | "local-log" | "api" | "none";

export type UsageWindow = {
  kind: string;
  label: string;
  usedPercent: number | null;
  remainingPercent: number | null;
  resetsAt: string | null;
  windowDurationMinutes: number | null;
};

export type ProviderUsage = {
  id: string;
  displayName: string;
  planName: string | null;
  hasFiveHourLimit: boolean | null;
  status: ProviderStatus;
  source: UsageSource;
  updatedAt: string | null;
  lastError: string | null;
  windows: UsageWindow[];
};

export type DashboardSnapshot = {
  providers: ProviderUsage[];
  refreshedAt: string | null;
};

export type NotificationThresholds = {
  enabled: boolean;
  warningPercent: number;
  criticalPercent: number;
  exhaustedPercent: 0;
};

export type AppSettings = {
  refreshIntervalSeconds: number;
  widgetSize: WidgetSize;
  scalePercent: number;
  opacityPercent: number;
  autoStart: boolean;
  startInBackground: boolean;
  providerOrder: string[];
  codexEnabled: boolean;
  openCodeGoEnabled: boolean;
  notificationThresholds: NotificationThresholds;
  codexPath: string | null;
  forceCompatibilityMode: boolean;
};

export const defaultSettings: AppSettings = {
  refreshIntervalSeconds: 60,
  widgetSize: "m",
  scalePercent: 100,
  opacityPercent: 96,
  autoStart: false,
  startInBackground: true,
  providerOrder: ["codex", "opencode-go"],
  codexEnabled: true,
  openCodeGoEnabled: true,
  notificationThresholds: {
    enabled: true,
    warningPercent: 20,
    criticalPercent: 10,
    exhaustedPercent: 0,
  },
  codexPath: null,
  forceCompatibilityMode: false,
};

