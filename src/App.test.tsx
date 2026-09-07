// @vitest-environment jsdom

import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { createMockAdapter, type AppAdapter } from "./adapter";
import { messageFromError, SettingsApp, WidgetApp } from "./App";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let container: HTMLDivElement;
let root: Root;

const settleEffects = async () => {
  await act(async () => {
    await Promise.resolve();
    await Promise.resolve();
  });
};

const renderSettings = async (adapter: AppAdapter) => {
  root = createRoot(container);
  await act(async () => {
    root.render(<SettingsApp adapter={adapter} />);
  });
  await settleEffects();
};

const buttonWithText = (text: string) => Array.from(container.querySelectorAll("button")).find((button) => button.textContent === text);

beforeEach(() => {
  window.history.replaceState(null, "", "/");
  container = document.createElement("div");
  document.body.append(container);
});

afterEach(() => {
  act(() => root?.unmount());
  container.remove();
  vi.useRealTimers();
});

describe("設定画面", () => {
  it("Tauriが文字列で返すエラーをそのまま表示できる", () => {
    expect(messageFromError("署名を検証できませんでした。", "失敗しました。")).toBe("署名を検証できませんでした。");
    expect(messageFromError(new Error("通信に失敗しました。"), "失敗しました。")).toBe("通信に失敗しました。");
    expect(messageFromError(null, "失敗しました。")).toBe("失敗しました。");
  });

  it("アプリ情報では重複見出しと無関係な保存操作を表示しない", async () => {
    await renderSettings(createMockAdapter());

    act(() => buttonWithText("アプリ情報")?.click());

    const appInfoHeadings = Array.from(container.querySelectorAll("h1, h2"))
      .filter((heading) => heading.textContent === "アプリ情報");
    expect(appInfoHeadings).toHaveLength(1);
    expect(buttonWithText("変更を保存")).toBeUndefined();
    expect(buttonWithText("更新を確認")).toBeDefined();
    expect(container.textContent).not.toContain("常駐ウィジェット");
  });

  it("設定を変更した時だけ保存操作を表示し、保存後に完了状態へ戻る", async () => {
    const adapter = createMockAdapter();
    await renderSettings(adapter);

    expect(buttonWithText("変更を保存")).toBeUndefined();
    const startupLabel = Array.from(container.querySelectorAll("label"))
      .find((label) => label.textContent?.includes("Windows起動時に起動"));
    const startupCheckbox = startupLabel?.querySelector<HTMLInputElement>('input[type="checkbox"]');
    expect(startupCheckbox).toBeDefined();

    act(() => startupCheckbox?.click());
    expect(buttonWithText("変更を保存")).toBeDefined();

    await act(async () => {
      buttonWithText("変更を保存")?.click();
      await Promise.resolve();
    });

    expect(buttonWithText("変更を保存")).toBeUndefined();
    expect(container.textContent).toContain("変更を保存しました。");
    expect((await adapter.getSettings()).autoStart).toBe(true);
  });

  it("更新失敗の詳細を表示し、再試行できる", async () => {
    const adapter = createMockAdapter();
    adapter.installUpdate = async () => Promise.reject("更新ファイルの署名を検証できませんでした。");
    await renderSettings(adapter);

    act(() => buttonWithText("アプリ情報")?.click());
    await act(async () => {
      buttonWithText("更新を確認")?.click();
      await Promise.resolve();
    });
    await act(async () => {
      buttonWithText("更新して再起動")?.click();
      await Promise.resolve();
    });

    expect(container.textContent).toContain("更新ファイルの署名を検証できませんでした。");
    expect(buttonWithText("更新を再試行")).toBeDefined();
    expect(container.querySelector('[role="alert"]')).not.toBeNull();
  });
});


describe("常駐ウィジェット", () => {
  const renderWidget = async (adapter: AppAdapter) => {
    root = createRoot(container);
    await act(async () => { root.render(<WidgetApp adapter={adapter} />); });
    await settleEffects();
  };

  it("接続解除で詳細とリングが消え、再接続で再表示する", async () => {
    const adapter = createMockAdapter();
    const expanded = vi.fn(async () => undefined);
    adapter.setWidgetExpanded = expanded;
    await renderWidget(adapter);
    expect(container.querySelectorAll(".provider-button")).toHaveLength(2);
    act(() => container.querySelector<HTMLButtonElement>(".provider-button")?.click());
    expect(container.querySelector(".widget-detail")).not.toBeNull();
    await act(async () => { await adapter.disconnectCodex(); });
    expect(container.querySelectorAll(".provider-button")).toHaveLength(1);
    expect(container.querySelector(".widget-detail")).toBeNull();
    expect(expanded).toHaveBeenLastCalledWith(false);
    await act(async () => { await adapter.disconnectOpencode(); await adapter.refreshUsage(); });
    expect(container.querySelectorAll(".provider-button")).toHaveLength(0);
    expect(container.querySelector('[aria-label="設定を開く"]')).not.toBeNull();
    await act(async () => { await adapter.setCodexApiKey("test"); });
    expect(container.querySelectorAll(".provider-button")).toHaveLength(1);
  });

  it("詳細から外へ出てすぐ戻っても古いタイマーで閉じない", async () => {
    vi.useFakeTimers();
    const adapter = createMockAdapter();
    const expanded = vi.fn(async () => undefined);
    adapter.setWidgetExpanded = expanded;
    await renderWidget(adapter);
    const button = container.querySelector<HTMLButtonElement>(".provider-button")!;
    act(() => { button.dispatchEvent(new MouseEvent("mouseover", { bubbles: true })); });
    act(() => { vi.advanceTimersByTime(150); });
    const detail = container.querySelector(".widget-detail")!;
    expect(detail).not.toBeNull();
    act(() => { detail.dispatchEvent(new MouseEvent("mouseout", { bubbles: true, relatedTarget: document.body })); });
    act(() => { vi.advanceTimersByTime(100); });
    act(() => { button.dispatchEvent(new MouseEvent("mouseover", { bubbles: true, relatedTarget: document.body })); });
    act(() => { vi.advanceTimersByTime(500); });
    expect(container.querySelector(".widget-detail")).not.toBeNull();
    expect(expanded).toHaveBeenCalledTimes(1);
    act(() => { button.dispatchEvent(new MouseEvent("mouseout", { bubbles: true, relatedTarget: document.body })); });
    act(() => { vi.advanceTimersByTime(300); });
    expect(container.querySelector(".widget-detail")).toBeNull();
    expect(expanded).toHaveBeenLastCalledWith(false);
  });

  it("プロバイダー設定から表示チェックを取り除く", async () => {
    await renderSettings(createMockAdapter());
    act(() => buttonWithText("プロバイダー")?.click());
    await settleEffects();
    expect(container.querySelectorAll('input[type="checkbox"]')).toHaveLength(0);
    expect(container.textContent).toContain("接続中のプロバイダーが自動で表示されます");
  });
});
