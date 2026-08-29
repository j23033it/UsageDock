import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import type { AppSettings, DashboardSnapshot, ProviderUsage } from "./contracts";
import { defaultSettings } from "./contracts";

export type WindowLabel = "widget" | "settings";

export type AppAdapter = {
  getWindowLabel: () => WindowLabel;
  getDashboard: () => Promise<DashboardSnapshot>;
  refreshUsage: () => Promise<DashboardSnapshot>;
  getSettings: () => Promise<AppSettings>;
  saveSettings: (settings: AppSettings) => Promise<void>;
  setOpencodeApiKey: (apiKey: string) => Promise<void>;
  disconnectOpencode: () => Promise<void>;
  openSettings: (providerId?: string) => Promise<void>;
  setWidgetExpanded: (expanded: boolean) => Promise<void>;
  onUsageUpdated: (handler: (snapshot: DashboardSnapshot) => void) => () => void;
  onSettingsUpdated: (handler: (settings: AppSettings) => void) => () => void;
  onSettingsFocus: (handler: (providerId: string) => void) => () => void;
};

const isTauriRuntime = () => typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

const tauriAdapter: AppAdapter = {
  getWindowLabel: () => {
    const label = getCurrentWindow().label;
    return label === "settings" ? "settings" : "widget";
  },
  getDashboard: () => invoke<DashboardSnapshot>("get_dashboard"),
  refreshUsage: () => invoke<DashboardSnapshot>("refresh_usage"),
  getSettings: () => invoke<AppSettings>("get_settings"),
  saveSettings: (settings) => invoke("save_settings", { settings }),
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
  return {
    getWindowLabel: () => (typeof document !== "undefined" && document.body.dataset.view === "settings" ? "settings" : "widget"),
    getDashboard: async () => snapshot,
    refreshUsage: async () => {
      snapshot = mockSnapshot();
      listeners.forEach((listener) => listener(snapshot));
      return snapshot;
    },
    getSettings: async () => structuredClone(settings),
    saveSettings: async (value) => {
      settings = structuredClone(value);
      settingsListeners.forEach((listener) => listener(structuredClone(settings)));
    },
    setOpencodeApiKey: async () => undefined,
    disconnectOpencode: async () => undefined,
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
    onSettingsFocus: () => () => undefined,
  };
};

export const appAdapter: AppAdapter = isTauriRuntime() ? tauriAdapter : createMockAdapter();

