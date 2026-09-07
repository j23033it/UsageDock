import type { ProviderStatus, ProviderUsage, UsageSource, UsageWindow } from "./contracts";

export type RemainingTone = "good" | "warning" | "critical" | "unknown";

export const remainingTone = (remainingPercent: number | null): RemainingTone => {
  if (remainingPercent === null || Number.isNaN(remainingPercent)) return "unknown";
  if (remainingPercent >= 50) return "good";
  if (remainingPercent >= 20) return "warning";
  return "critical";
};

export const formatPercent = (value: number | null) => value === null ? "—" : `${Math.round(value)}%`;

export const formatDateTime = (value: string | null, now = new Date()) => {
  if (!value) return "未取得";
  const date = new Date(value);
  if (Number.isNaN(date.valueOf())) return "未取得";
  const today = new Date(now);
  const tomorrow = new Date(now);
  tomorrow.setDate(tomorrow.getDate() + 1);
  const dateKey = (item: Date) => `${item.getFullYear()}-${item.getMonth()}-${item.getDate()}`;
  const time = new Intl.DateTimeFormat("ja-JP", { hour: "2-digit", minute: "2-digit" }).format(date);
  if (dateKey(date) === dateKey(today)) return `今日 ${time}`;
  if (dateKey(date) === dateKey(tomorrow)) return `明日 ${time}`;
  return `${date.getMonth() + 1}月${date.getDate()}日 ${time}`;
};

export const formatRelativeTime = (value: string | null, now = new Date()) => {
  if (!value) return "更新なし";
  const deltaSeconds = Math.round((new Date(value).valueOf() - now.valueOf()) / 1000);
  if (!Number.isFinite(deltaSeconds)) return "更新なし";
  if (Math.abs(deltaSeconds) < 60) return "たった今";
  const minutes = Math.round(Math.abs(deltaSeconds) / 60);
  if (minutes < 60) return deltaSeconds < 0 ? `${minutes}分前` : `${minutes}分後`;
  const hours = Math.round(minutes / 60);
  return deltaSeconds < 0 ? `${hours}時間前` : `${hours}時間後`;
};

export const sourceLabel = (source: UsageSource) => ({
  "app-server": "App Server",
  "local-log": "ローカルログ",
  api: "API",
  none: "取得元なし",
}[source]);

export const statusLabel = (status: ProviderStatus) => ({
  fresh: "最新",
  stale: "更新遅延",
  outdated: "古いデータ",
  unavailable: "利用不可",
  refreshing: "更新中",
}[status]);

export const isResetPending = (usageWindow: UsageWindow, now = new Date()) => {
  if (!usageWindow.resetsAt) return false;
  const resetAt = new Date(usageWindow.resetsAt);
  if (Number.isNaN(resetAt.valueOf()) || resetAt > now) return false;
  return true;
};

export const visibleUsageWindows = (provider: ProviderUsage): UsageWindow[] => provider.id === "codex"
  ? provider.windows.filter((window) => !/base_model_inference|gpt-reserve/i.test(window.label))
  : provider.windows;

export const primaryUsageWindow = (provider: ProviderUsage): UsageWindow | null => {
  const windows = visibleUsageWindows(provider);
  if (windows.length === 0) return null;
  if (provider.id !== "codex") return windows[0];

  const publicWindows = windows.filter((window) => !window.label.includes(" · "));
  const findPublicWindow = (kind: string) => publicWindows.find((window) => window.kind === kind);
  if (provider.hasFiveHourLimit === false) {
    return findPublicWindow("weekly") ?? publicWindows[0] ?? windows[0];
  }
  if (provider.hasFiveHourLimit === true) {
    return findPublicWindow("five-hour") ?? findPublicWindow("weekly") ?? publicWindows[0] ?? windows[0];
  }
  return findPublicWindow("five-hour") ?? findPublicWindow("weekly") ?? publicWindows[0] ?? windows[0];
};

export const moveProviderOrder = (order: string[], index: number, direction: -1 | 1) => {
  const next = [...order];
  const target = index + direction;
  if (target < 0 || target >= next.length) return next;
  [next[index], next[target]] = [next[target], next[index]];
  return next;
};
