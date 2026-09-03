export type WidgetSize = "s" | "m" | "l";

export type ProviderStatus = "fresh" | "stale" | "outdated" | "unavailable" | "refreshing";

export type UsageSource = "app-server" | "local-log" | "api" | "none";

export type ConnectionStatus = "connected" | "disconnected" | "connecting" | "unavailable" | "error";
export type CodexAuthType = "chatgpt" | "api-key";
export type CodexLoginMode = "browser" | "device-code";

export type CodexLoginPrompt = {
  loginId: string;
  mode: CodexLoginMode;
  verificationUrl: string;
  userCode: string | null;
};

export type CodexConnection = {
  status: ConnectionStatus;
  authType: CodexAuthType | null;
  email: string | null;
  planName: string | null;
  executablePath: string | null;
  pendingLogin: CodexLoginPrompt | null;
  error: string | null;
};

export type ConnectionOverview = {
  codex: CodexConnection;
  openCodeGo: { status: ConnectionStatus };
};

export type AppUpdate = {
  status: "current" | "available" | "unconfigured";
  currentVersion: string;
  availableVersion: string | null;
  notes: string | null;
};

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

