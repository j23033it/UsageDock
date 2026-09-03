import { describe, expect, it } from "vitest";
import { createMockAdapter } from "./adapter";

describe("ブラウザ用mock adapter", () => {
  it("ブラウザ開発時は開発版と表示する", async () => {
    expect(await createMockAdapter().getAppVersion()).toBe("開発版");
  });

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

  it("Codex認証の開始とキャンセルを接続状態へ通知する", async () => {
    const adapter = createMockAdapter();
    const statuses: string[] = [];
    const cleanup = adapter.onConnectionsUpdated((connections) => statuses.push(connections.codex.status));

    const prompt = await adapter.startCodexLogin("device-code");
    expect(prompt.userCode).toBe("ABCD-1234");
    expect((await adapter.getConnections()).codex.status).toBe("connecting");
    await adapter.cancelCodexLogin();
    expect(statuses).toEqual(["connecting", "disconnected"]);
    cleanup();
  });

  it("OpenCode Goのキー有無だけを公開する", async () => {
    const adapter = createMockAdapter();
    await adapter.disconnectOpencode();
    expect((await adapter.getConnections()).openCodeGo.status).toBe("disconnected");
    await adapter.setOpencodeApiKey("secret-that-must-not-be-returned");
    expect(await adapter.getConnections()).toEqual(expect.objectContaining({
      openCodeGo: { status: "connected" },
    }));
  });
});
