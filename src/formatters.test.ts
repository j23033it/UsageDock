import { describe, expect, it } from "vitest";
import { formatDateTime, formatRelativeTime, isResetPending, moveProviderOrder, remainingTone } from "./formatters";

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
    expect(moveProviderOrder(["codex", "opencode"], 1, -1)).toEqual(["opencode", "codex"]);
    expect(moveProviderOrder(["codex", "opencode"], 0, -1)).toEqual(["codex", "opencode"]);
  });

  it("リセット時刻通過後に残量不明なら確認中にする", () => {
    const now = new Date("2026-08-29T10:00:00Z");
    expect(isResetPending({ kind: "daily", label: "日次", usedPercent: null, remainingPercent: null, resetsAt: "2026-08-29T09:00:00Z", windowDurationMinutes: 1440 }, now)).toBe(true);
    expect(isResetPending({ kind: "daily", label: "日次", usedPercent: 20, remainingPercent: 80, resetsAt: "2026-08-29T09:00:00Z", windowDurationMinutes: 1440 }, now)).toBe(false);
  });
});
