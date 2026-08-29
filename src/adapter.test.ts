import { describe, expect, it } from "vitest";
import { createMockAdapter } from "./adapter";

describe("ブラウザ用mock adapter", () => {
  it("ダッシュボードを返し、更新イベントを通知する", async () => {
    const adapter = createMockAdapter();
    const snapshots = [] as string[];
    const cleanup = adapter.onUsageUpdated((snapshot) => snapshots.push(snapshot.refreshedAt ?? ""));
    const snapshot = await adapter.getDashboard();
    expect(snapshot.providers.length).toBeGreaterThan(0);
    await adapter.refreshUsage();
    expect(snapshots).toHaveLength(1);
    cleanup();
    await adapter.refreshUsage();
    expect(snapshots).toHaveLength(1);
  });

  it("設定をmock内に保存できる", async () => {
    const adapter = createMockAdapter();
    const settings = await adapter.getSettings();
    settings.widgetSize = "l";
    await adapter.saveSettings(settings);
    expect((await adapter.getSettings()).widgetSize).toBe("l");
  });
});
