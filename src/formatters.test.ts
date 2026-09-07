import { describe, expect, it } from "vitest";
import type { ProviderUsage } from "./contracts";
import { formatDateTime, formatRelativeTime, isResetPending, moveProviderOrder, primaryUsageWindow, visibleUsageWindows, remainingTone } from "./formatters";

describe("表示ロジック", () => {
  it("残量を閾値ごとの色に分類する", () => {
    expect(remainingTone(100)).toBe("good");
    expect(remainingTone(50)).toBe("good");
    expect(remainingTone(49)).toBe("warning");
    expect(remainingTone(20)).toBe("warning");
    expect(remainingTone(19)).toBe("critical");
    expect(remainingTone(0)).toBe("critical");
    expect(remainingTone(null)).toBe("unknown");
  });

  it("今日・明日・日付を表示する", () => {
    const now = new Date("2026-08-29T10:00:00+09:00");
    expect(formatDateTime("2026-08-29T15:30:00+09:00", now)).toContain("今日");
    expect(formatDateTime("2026-08-30T15:30:00+09:00", now)).toContain("明日");
    expect(formatDateTime("2026-09-02T15:30:00+09:00", now)).toContain("9月2日");
  });

  it("相対時間を分・時間で表示する", () => {
    const now = new Date("2026-08-29T10:00:00Z");
    expect(formatRelativeTime("2026-08-29T09:59:30Z", now)).toBe("たった今");
    expect(formatRelativeTime("2026-08-29T09:40:00Z", now)).toBe("20分前");
    expect(formatRelativeTime("2026-08-29T12:00:00Z", now)).toBe("2時間後");
  });

  it("プロバイダー順を安全に入れ替える", () => {
    expect(moveProviderOrder(["codex", "opencode-go"], 1, -1)).toEqual(["opencode-go", "codex"]);
    expect(moveProviderOrder(["codex", "opencode-go"], 0, -1)).toEqual(["codex", "opencode-go"]);
  });

  it("リセット時刻通過後はサーバー再取得まで確認中にする", () => {
    const now = new Date("2026-08-29T10:00:00Z");
    expect(isResetPending({ kind: "daily", label: "日次", usedPercent: null, remainingPercent: null, resetsAt: "2026-08-29T09:00:00Z", windowDurationMinutes: 1440 }, now)).toBe(true);
    expect(isResetPending({ kind: "daily", label: "日次", usedPercent: 20, remainingPercent: 80, resetsAt: "2026-08-29T09:00:00Z", windowDurationMinutes: 1440 }, now)).toBe(true);
    expect(isResetPending({ kind: "daily", label: "日次", usedPercent: 20, remainingPercent: 80, resetsAt: "2026-08-29T11:00:00Z", windowDurationMinutes: 1440 }, now)).toBe(false);
  });

  it("5時間枠がないCodexは内部枠より通常の週間usageを代表表示する", () => {
    const provider: ProviderUsage = {
      id: "codex",
      displayName: "Codex",
      planName: "ChatGPT Pro",
      hasFiveHourLimit: false,
      status: "fresh",
      source: "app-server",
      updatedAt: "2026-09-04T10:00:00Z",
      lastError: null,
      windows: [
        { kind: "weekly", label: "base_model_inference · 週間枠", usedPercent: 0, remainingPercent: 100, resetsAt: null, windowDurationMinutes: 10080 },
        { kind: "weekly", label: "週間枠", usedPercent: 47, remainingPercent: 53, resetsAt: null, windowDurationMinutes: 10080 },
      ],
    };

    expect(primaryUsageWindow(provider)?.remainingPercent).toBe(53);
    expect(visibleUsageWindows(provider).map((window) => window.label)).toEqual(["週間枠"]);
    provider.windows = provider.windows.slice(0, 1);
    expect(visibleUsageWindows(provider)).toEqual([]);
    expect(primaryUsageWindow(provider)).toBeNull();
  });

  it("5時間枠があるCodexは5時間枠を代表表示する", () => {
    const provider: ProviderUsage = {
      id: "codex",
      displayName: "Codex",
      planName: "ChatGPT Plus",
      hasFiveHourLimit: true,
      status: "fresh",
      source: "app-server",
      updatedAt: null,
      lastError: null,
      windows: [
        { kind: "weekly", label: "週間枠", usedPercent: 20, remainingPercent: 80, resetsAt: null, windowDurationMinutes: 10080 },
        { kind: "five-hour", label: "5時間枠", usedPercent: 60, remainingPercent: 40, resetsAt: null, windowDurationMinutes: 300 },
      ],
    };

    expect(primaryUsageWindow(provider)?.remainingPercent).toBe(40);
  });

  it("OpenCode Goは従来どおり先頭の利用枠を代表表示する", () => {
    const provider: ProviderUsage = {
      id: "opencode-go",
      displayName: "OpenCode Go",
      planName: null,
      hasFiveHourLimit: null,
      status: "fresh",
      source: "api",
      updatedAt: null,
      lastError: null,
      windows: [
        { kind: "five-hour", label: "rolling", usedPercent: 25, remainingPercent: 75, resetsAt: null, windowDurationMinutes: null },
        { kind: "weekly", label: "weekly", usedPercent: 10, remainingPercent: 90, resetsAt: null, windowDurationMinutes: null },
      ],
    };

    expect(primaryUsageWindow(provider)?.remainingPercent).toBe(75);
  });
});
