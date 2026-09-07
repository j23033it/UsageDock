import { getVersion } from "@tauri-apps/api/app";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import type { AppSettings, AppUpdate, CodexLoginMode, CodexLoginPrompt, ConnectionOverview, DashboardSnapshot, ProviderUsage } from "./contracts";
import { defaultSettings } from "./contracts";

export type WindowLabel = "widget" | "settings";

export type AppAdapter = {
  getWindowLabel: () => WindowLabel;
  getAppVersion: () => Promise<string>;
  getDashboard: () => Promise<DashboardSnapshot>;
  refreshUsage: () => Promise<DashboardSnapshot>;
  getSettings: () => Promise<AppSettings>;
  saveSettings: (settings: AppSettings) => Promise<void>;
  checkForUpdate: () => Promise<AppUpdate>;
  installUpdate: () => Promise<void>;
  getConnections: () => Promise<ConnectionOverview>;
  startCodexLogin: (mode: CodexLoginMode) => Promise<CodexLoginPrompt>;
  cancelCodexLogin: () => Promise<void>;
  setCodexApiKey: (apiKey: string) => Promise<void>;
  disconnectCodex: () => Promise<void>;
  setOpencodeApiKey: (apiKey: string) => Promise<void>;
  disconnectOpencode: () => Promise<void>;
  openSettings: (providerId?: string) => Promise<void>;
  setWidgetExpanded: (expanded: boolean) => Promise<void>;
  onUsageUpdated: (handler: (snapshot: DashboardSnapshot) => void) => () => void;
  onSettingsUpdated: (handler: (settings: AppSettings) => void) => () => void;
  onConnectionsUpdated: (handler: (connections: ConnectionOverview) => void) => () => void;
  onSettingsFocus: (handler: (providerId: string) => void) => () => void;
};

const isTauriRuntime = () => typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

const tauriAdapter: AppAdapter = {
  getWindowLabel: () => {
    const label = getCurrentWindow().label;
    return label === "settings" ? "settings" : "widget";
  },
  getAppVersion: getVersion,
  getDashboard: () => invoke<DashboardSnapshot>("get_dashboard"),
  refreshUsage: () => invoke<DashboardSnapshot>("refresh_usage"),
  getSettings: () => invoke<AppSettings>("get_settings"),
  saveSettings: (settings) => invoke("save_settings", { settings }),
  checkForUpdate: () => invoke<AppUpdate>("check_for_update"),
  installUpdate: () => invoke("install_update"),
  getConnections: () => invoke<ConnectionOverview>("get_connections"),
  startCodexLogin: (mode) => invoke<CodexLoginPrompt>("start_codex_login", { mode }),
  cancelCodexLogin: () => invoke("cancel_codex_login"),
  setCodexApiKey: (apiKey) => invoke("set_codex_api_key", { apiKey }),
  disconnectCodex: () => invoke("disconnect_codex"),
  setOpencodeApiKey: (apiKey) => invoke("set_opencode_api_key", { apiKey }),
  disconnectOpencode: () => invoke("disconnect_opencode"),
  openSettings: (providerId) => invoke("open_settings", { providerId: providerId ?? null }),
  setWidgetExpanded: (expanded) => invoke("set_widget_expanded", { expanded }),
  onUsageUpdated: (handler) => {
    let active = true;
    let unlisten: (() => void) | undefined;
    void listen<DashboardSnapshot>("usage-updated", (event) => {
      if (active) handler(event.payload);
    }).then((cleanup) => {
      unlisten = cleanup;
      if (!active) cleanup();
    });
    return () => {
      active = false;
      unlisten?.();
    };
  },
  onSettingsUpdated: (handler) => {
    let active = true;
    let unlisten: (() => void) | undefined;
    void listen<AppSettings>("settings-updated", (event) => {
      if (active) handler(event.payload);
    }).then((cleanup) => {
      unlisten = cleanup;
      if (!active) cleanup();
    });
    return () => {
      active = false;
      unlisten?.();
    };
  },
  onConnectionsUpdated: (handler) => {
    let active = true;
    let unlisten: (() => void) | undefined;
    void listen<ConnectionOverview>("connections-updated", (event) => {
      if (active) handler(event.payload);
    }).then((cleanup) => {
      unlisten = cleanup;
      if (!active) cleanup();
    });
    return () => {
      active = false;
      unlisten?.();
    };
  },
  onSettingsFocus: (handler) => {
    let active = true;
    let unlisten: (() => void) | undefined;
    void listen<string>("settings-focus-provider", (event) => {
      if (active) handler(event.payload);
    }).then((cleanup) => {
      unlisten = cleanup;
      if (!active) cleanup();
    });
    return () => {
      active = false;
      unlisten?.();
    };
  },
};

const nextHour = (hours: number) => new Date(Date.now() + hours * 60 * 60 * 1000).toISOString();

const mockProviders = (): ProviderUsage[] => [
  {
    id: "codex",
    displayName: "Codex",
    planName: "ChatGPT Pro",
    hasFiveHourLimit: true,
    status: "fresh",
    source: "app-server",
    updatedAt: new Date().toISOString(),
    lastError: null,
    windows: [
      { kind: "short", label: "5時間", usedPercent: 27, remainingPercent: 73, resetsAt: nextHour(2.2), windowDurationMinutes: 300 },
      { kind: "weekly", label: "週次", usedPercent: 58, remainingPercent: 42, resetsAt: nextHour(42), windowDurationMinutes: 10080 },
    ],
  },
  {
    id: "opencode-go",
    displayName: "OpenCode Go",
    planName: null,
    hasFiveHourLimit: null,
    status: "stale",
    source: "api",
    updatedAt: new Date(Date.now() - 1000 * 60 * 18).toISOString(),
    lastError: null,
    windows: [
      { kind: "monthly", label: "月次", usedPercent: 82, remainingPercent: 18, resetsAt: nextHour(112), windowDurationMinutes: 43200 },
    ],
  },
];

const mockSnapshot = (): DashboardSnapshot => ({
  providers: mockProviders(),
  refreshedAt: new Date().toISOString(),
});

export const createMockAdapter = (): AppAdapter => {
  let snapshot = mockSnapshot();
  let settings = structuredClone(defaultSettings);
  const listeners = new Set<(value: DashboardSnapshot) => void>();
  const settingsListeners = new Set<(value: AppSettings) => void>();
  let connections: ConnectionOverview = {
    codex: {
      status: "connected",
      authType: "chatgpt",
      email: "user@example.com",
      planName: "ChatGPT Pro",
      executablePath: "C:\\Program Files\\Codex\\codex.exe",
      pendingLogin: null,
      error: null,
    },
    openCodeGo: { status: "connected" },
  };
  const connectionListeners = new Set<(value: ConnectionOverview) => void>();
  const connectedSnapshot = () => ({ ...snapshot, providers: snapshot.providers.filter((provider) => provider.id === "codex" ? connections.codex.status === "connected" : connections.openCodeGo.status === "connected") });
  const publishConnections = () => {
    connectionListeners.forEach((listener) => listener(structuredClone(connections)));
    listeners.forEach((listener) => listener(connectedSnapshot()));
  };
  return {
    getWindowLabel: () => (typeof document !== "undefined" && document.body.dataset.view === "settings" ? "settings" : "widget"),
    getAppVersion: async () => "開発版",
    getDashboard: async () => connectedSnapshot(),
    refreshUsage: async () => {
      snapshot = mockSnapshot();
      listeners.forEach((listener) => listener(connectedSnapshot()));
      return connectedSnapshot();
    },
    getSettings: async () => structuredClone(settings),
    saveSettings: async (value) => {
      settings = structuredClone(value);
      settingsListeners.forEach((listener) => listener(structuredClone(settings)));
    },
    checkForUpdate: async () => ({ status: "available", currentVersion: "開発版", availableVersion: "次の配布版", notes: "アカウント管理と自動更新を追加しました。" }),
    installUpdate: async () => undefined,
    getConnections: async () => structuredClone(connections),
    startCodexLogin: async (mode) => {
      const prompt: CodexLoginPrompt = {
        loginId: "mock-login",
        mode,
        verificationUrl: "https://auth.openai.com/codex/device",
        userCode: mode === "device-code" ? "ABCD-1234" : null,
      };
      connections.codex = { ...connections.codex, status: "connecting", pendingLogin: prompt };
      publishConnections();
      return prompt;
    },
    cancelCodexLogin: async () => {
      connections.codex = { ...connections.codex, status: "disconnected", pendingLogin: null };
      publishConnections();
    },
    setCodexApiKey: async () => {
      connections.codex = { ...connections.codex, status: "connected", authType: "api-key", email: null, planName: null, pendingLogin: null };
      publishConnections();
    },
    disconnectCodex: async () => {
      connections.codex = { ...connections.codex, status: "disconnected", authType: null, email: null, planName: null, pendingLogin: null };
      publishConnections();
    },
    setOpencodeApiKey: async () => {
      connections.openCodeGo.status = "connected";
      publishConnections();
    },
    disconnectOpencode: async () => {
      connections.openCodeGo.status = "disconnected";
      publishConnections();
    },
    openSettings: async () => undefined,
    setWidgetExpanded: async () => undefined,
    onUsageUpdated: (handler) => {
      listeners.add(handler);
      return () => listeners.delete(handler);
    },
    onSettingsUpdated: (handler) => {
      settingsListeners.add(handler);
      return () => settingsListeners.delete(handler);
    },
    onConnectionsUpdated: (handler) => {
      connectionListeners.add(handler);
      return () => connectionListeners.delete(handler);
    },
    onSettingsFocus: () => () => undefined,
  };
};

export const appAdapter: AppAdapter = isTauriRuntime() ? tauriAdapter : createMockAdapter();

