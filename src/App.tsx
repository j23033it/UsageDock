import { Component, type CSSProperties, type ErrorInfo, type FormEvent, type ReactNode, useCallback, useEffect, useRef, useState } from "react";
import { appAdapter, type AppAdapter } from "./adapter";
import openAiIconUrl from "./assets/providers/openai.svg";
import openCodeGoIconUrl from "./assets/providers/opencode-go.svg";
import { defaultSettings, type AppSettings, type AppUpdate, type CodexLoginMode, type ConnectionOverview, type DashboardSnapshot, type ProviderUsage, type UsageWindow } from "./contracts";
import { formatDateTime, formatPercent, formatRelativeTime, isResetPending, moveProviderOrder, remainingTone, sourceLabel, statusLabel } from "./formatters";

type AppProps = { adapter?: AppAdapter };
type IconProps = { size?: number };

const providerIconUrls: Record<string, string> = { codex: openAiIconUrl, "opencode-go": openCodeGoIconUrl };
const ProviderMark = ({ provider, size = 20 }: IconProps & { provider: ProviderUsage }) => <img className={`provider-mark provider-mark--${provider.id}`} src={providerIconUrls[provider.id] ?? openAiIconUrl} width={size} height={size} alt="" aria-hidden="true" />;
const RefreshGlyph = ({ size = 16 }: IconProps) => <svg width={size} height={size} viewBox="0 0 24 24" aria-hidden="true"><path d="M19 8.3A7.5 7.5 0 1 0 19.2 15M19 4.5v4.8h-4.8" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" /></svg>;
const SettingsGlyph = ({ size = 16 }: IconProps) => <svg width={size} height={size} viewBox="0 0 24 24" aria-hidden="true"><path d="m12 3 1.1 2.6 2.8.8 2.6-1.2 1.7 1.7-1.2 2.6.8 2.8L22.4 13v2l-2.6 1.1-.8 2.8 1.2 2.6-1.7 1.7-2.6-1.2-2.8.8L12 23l-2.1-2.2-2.8-.8-2.6 1.2-1.7-1.7L4 16.1 3.2 13 1 12V10l2.2-1.1L4 6.1 2.8 3.5 4.5 1.8l2.6 1.2L10 2.2 12 0" transform="scale(.9) translate(1.3 1.3)" fill="none" stroke="currentColor" strokeWidth="1.3" strokeLinejoin="round" /><circle cx="12" cy="12" r="3" fill="none" stroke="currentColor" strokeWidth="1.5" /></svg>;
const statusIcon = (status: ProviderUsage["status"]) => status === "fresh" ? "●" : status === "refreshing" ? "↻" : "!";
const planSummary = (provider: ProviderUsage) => {
  if (provider.id !== "codex") return null;
  const window = provider.hasFiveHourLimit === true ? "5時間枠あり" : provider.hasFiveHourLimit === false ? "5時間枠なし" : "5時間枠未取得";
  return `${provider.planName ?? "プラン未取得"} · ${window}`;
};

export const UsageRing = ({ value, provider, size = 44 }: { value: number | null; provider: ProviderUsage; size?: number }) => {
  const tone = remainingTone(value);
  const radius = 18;
  const circumference = 2 * Math.PI * radius;
  const dash = value === null ? 0 : Math.max(0, Math.min(100, value)) / 100 * circumference;
  return <span className={`usage-ring usage-ring--${tone}`} style={{ width: size, height: size }} aria-label={`残量 ${formatPercent(value)}`}><svg viewBox="0 0 44 44" aria-hidden="true"><circle className="usage-ring__track" cx="22" cy="22" r={radius} />{value !== null && <circle className="usage-ring__value" cx="22" cy="22" r={radius} strokeDasharray={`${dash} ${circumference - dash}`} />}</svg><span className="usage-ring__mark"><ProviderMark provider={provider} size={size > 38 ? 17 : 15} /></span></span>;
};
const LoadingState = ({ label = "使用量を読み込み中" }: { label?: string }) => <div className="loading-state" role="status"><span className="loading-state__bar" /><span>{label}…</span></div>;
const ErrorState = ({ message, onRetry }: { message: string; onRetry?: () => void }) => <div className="error-state" role="alert"><p>{message}</p>{onRetry && <button type="button" className="button button--ghost" onClick={onRetry}>再試行</button>}</div>;

const WindowBar = ({ window, now }: { window: UsageWindow; now: Date }) => {
  const tone = remainingTone(window.remainingPercent);
  const pending = isResetPending(window, now);
  return <div className={`window-row window-row--${tone}`}><div className="window-row__heading"><span>{window.label}</span><strong>{pending ? "リセット確認中" : `${formatPercent(window.remainingPercent)}残り`}</strong></div><div className="window-row__bar" aria-label={`${window.label} ${formatPercent(window.remainingPercent)}残り`}><span style={{ width: `${window.remainingPercent ?? 0}%` }} /></div><div className="window-row__meta"><span>使用済み {formatPercent(window.usedPercent)}</span><span>{pending ? "更新待ち" : `リセット ${formatDateTime(window.resetsAt, now)}`}</span></div></div>;
};
const ProviderDetail = ({ provider, now }: { provider: ProviderUsage; now: Date }) => { const summary = planSummary(provider); return <section className="widget-detail__content" aria-labelledby={`provider-detail-${provider.id}`}><div className="widget-detail__header"><div className="provider-heading"><ProviderMark provider={provider} size={25} /><div><h2 id={`provider-detail-${provider.id}`}>{provider.displayName}</h2>{summary && <p>{summary}</p>}</div></div><div className={`status status--${provider.status}`}><span aria-hidden="true">{statusIcon(provider.status)}</span>{statusLabel(provider.status)}</div></div><div className="provider-meta"><span>最終更新 {formatRelativeTime(provider.updatedAt, now)}</span><span>取得元 {sourceLabel(provider.source)}</span></div><div className="window-list">{provider.windows.length > 0 ? provider.windows.map((item, index) => <WindowBar key={`${provider.id}-${item.kind}-${item.label}-${item.resetsAt ?? "none"}-${index}`} window={item} now={now} />) : <p className="muted">利用できるウィンドウがありません。</p>}</div>{provider.lastError && <p className="inline-error">{provider.lastError}</p>}</section>; };

const WidgetApp = ({ adapter }: { adapter: AppAdapter }) => {
  const [snapshot, setSnapshot] = useState<DashboardSnapshot | null>(null);
  const [settings, setSettings] = useState<AppSettings>(defaultSettings);
  const [error, setError] = useState<string | null>(null);
  const [expandedProviderId, setExpandedProviderId] = useState<string | null>(null);
  const [pinned, setPinned] = useState(false);
  const [now, setNow] = useState(() => new Date());
  const closeTimer = useRef<number | undefined>(undefined);
  const openTimer = useRef<number | undefined>(undefined);
  const lastExpanded = useRef<boolean | undefined>(undefined);
  const notifyExpanded = useCallback((expanded: boolean) => { if (lastExpanded.current === expanded) return; lastExpanded.current = expanded; void adapter.setWidgetExpanded(expanded); }, [adapter]);
  const close = useCallback(() => { setExpandedProviderId(null); setPinned(false); notifyExpanded(false); }, [notifyExpanded]);
  const load = useCallback(async () => { try { setError(null); setSnapshot(await adapter.getDashboard()); } catch (caught) { setError(caught instanceof Error ? caught.message : "使用量を取得できませんでした。"); } }, [adapter]);
  useEffect(() => { void load(); void adapter.getSettings().then((value) => setSettings(settingsWithDefaults(value))); const cleanupUsage = adapter.onUsageUpdated(setSnapshot); const cleanupSettings = adapter.onSettingsUpdated((value) => setSettings(settingsWithDefaults(value))); const clock = window.setInterval(() => setNow(new Date()), 30_000); return () => { cleanupUsage(); cleanupSettings(); window.clearInterval(clock); window.clearTimeout(closeTimer.current); window.clearTimeout(openTimer.current); }; }, [adapter, load]);
  useEffect(() => { const handleWindowKeyDown = (event: KeyboardEvent) => { if (event.key === "Escape") close(); }; window.addEventListener("keydown", handleWindowKeyDown); return () => window.removeEventListener("keydown", handleWindowKeyDown); }, [close]);
  const providers = snapshot?.providers ?? [];
  const selected = providers.find((provider) => provider.id === expandedProviderId) ?? providers[0];
  const scheduleOpen = (providerId: string) => { window.clearTimeout(closeTimer.current); window.clearTimeout(openTimer.current); openTimer.current = window.setTimeout(() => { setExpandedProviderId(providerId); notifyExpanded(true); }, 150); };
  const scheduleClose = () => { window.clearTimeout(openTimer.current); if (!pinned) closeTimer.current = window.setTimeout(close, 300); };
  const selectProvider = (providerId: string, shouldPin = false) => { window.clearTimeout(openTimer.current); window.clearTimeout(closeTimer.current); setExpandedProviderId(providerId); if (shouldPin) setPinned((value) => !value); notifyExpanded(true); };
  const handleKeyDown = (event: React.KeyboardEvent<HTMLButtonElement>, providerId: string) => { if (event.key === "Escape") { event.preventDefault(); close(); } else if (event.key === "Enter" || event.key === " ") { event.preventDefault(); selectProvider(providerId, true); } else if (event.key === "ContextMenu" || (event.key === "F10" && event.shiftKey)) { event.preventDefault(); void adapter.openSettings(providerId); } };
  if (error) return <main className="widget-app"><ErrorState message={error} onRetry={() => void load()} /></main>;
  const widgetStyle = { opacity: settings.opacityPercent / 100, "--widget-scale": settings.scalePercent / 100 } as CSSProperties;
  const ringSize = settings.widgetSize === "s" ? 30 : settings.widgetSize === "l" ? 40 : 34;
  return <main className={`widget-app widget-app--${pinned ? "pinned" : "hover"} widget-app--size-${settings.widgetSize}`} style={widgetStyle} onMouseEnter={() => selected && scheduleOpen(selected.id)} onMouseLeave={scheduleClose}>{selected && expandedProviderId && <div className="widget-detail" onMouseEnter={() => window.clearTimeout(closeTimer.current)} onMouseLeave={scheduleClose}><ProviderDetail provider={selected} now={now} /></div>}<aside className="widget-rail" aria-label="使用量ウィジェット"><div className="widget-rail__top"><span className="widget-brand" aria-label="UsageDock">UD</span></div><div className="provider-stack">{snapshot ? providers.map((provider) => <button key={provider.id} type="button" className={`provider-button ${expandedProviderId === provider.id ? "provider-button--selected" : ""}`} aria-label={`${provider.displayName} ${formatPercent(provider.windows[0]?.remainingPercent ?? null)}残り`} aria-expanded={expandedProviderId === provider.id} onMouseEnter={() => scheduleOpen(provider.id)} onFocus={() => selectProvider(provider.id)} onClick={() => selectProvider(provider.id, true)} onContextMenu={(event) => { event.preventDefault(); void adapter.openSettings(provider.id); }} onKeyDown={(event) => handleKeyDown(event, provider.id)}><UsageRing value={provider.windows[0]?.remainingPercent ?? null} provider={provider} size={ringSize} /><span className={`provider-button__percent provider-button__percent--${remainingTone(provider.windows[0]?.remainingPercent ?? null)}`}>{formatPercent(provider.windows[0]?.remainingPercent ?? null)}</span><span className={`provider-button__status provider-button__status--${provider.status}`} aria-label={statusLabel(provider.status)}>{statusIcon(provider.status)}</span></button>) : <LoadingState label="読込" />}</div><div className="widget-rail__bottom"><button type="button" className="icon-button" aria-label="使用量を更新" onClick={() => void adapter.refreshUsage()}><RefreshGlyph size={14} /></button><button type="button" className="icon-button" aria-label="設定を開く" onClick={() => void adapter.openSettings()}><SettingsGlyph size={14} /></button></div></aside></main>;
};

const navItems = [
  { id: "general", label: "一般", description: "起動方法とデータの更新間隔を設定します。" },
  { id: "widget", label: "ウィジェット", description: "画面右端に表示するウィジェットの見た目を調整します。" },
  { id: "providers", label: "プロバイダー", description: "アカウントの接続とウィジェットへの表示を管理します。" },
  { id: "notifications", label: "通知", description: "利用枠が少なくなったときの通知条件を設定します。" },
  { id: "advanced", label: "詳細設定", description: "Codexの実行環境と旧環境向けの互換設定です。" },
  { id: "about", label: "アプリ情報", description: "現在のバージョンを確認し、UsageDockを更新します。" },
] as const;
type SettingsSection = typeof navItems[number]["id"];
type ActionMessage = { tone: "success" | "error"; text: string };
const settingsWithDefaults = (value: AppSettings): AppSettings => ({ ...defaultSettings, ...value, notificationThresholds: { ...defaultSettings.notificationThresholds, ...value.notificationThresholds }, providerOrder: value.providerOrder?.length ? value.providerOrder : defaultSettings.providerOrder });
const settingsAreEqual = (left: AppSettings, right: AppSettings) => JSON.stringify(left) === JSON.stringify(right);

const SettingsApp = ({ adapter }: { adapter: AppAdapter }) => {
  const [settings, setSettings] = useState<AppSettings>(defaultSettings);
  const [savedSettings, setSavedSettings] = useState<AppSettings>(defaultSettings);
  const [appVersion, setAppVersion] = useState("取得中…");
  const [section, setSection] = useState<SettingsSection>(() => new URLSearchParams(window.location.search).has("provider") ? "providers" : "general");
  const [status, setStatus] = useState<"idle" | "loading" | "saving" | "success" | "error">("loading");
  const [saveMessage, setSaveMessage] = useState<ActionMessage | null>(null);
  const [connectionMessage, setConnectionMessage] = useState<ActionMessage | null>(null);
  const [connections, setConnections] = useState<ConnectionOverview | null>(null);
  const [codexApiKey, setCodexApiKey] = useState("");
  const [openCodeApiKey, setOpenCodeApiKey] = useState("");
  const [connectionTask, setConnectionTask] = useState<string | null>(null);
  const [confirmDisconnect, setConfirmDisconnect] = useState<"codex" | "opencode" | null>(null);
  useEffect(() => { let active = true; void adapter.getSettings().then((value) => { if (active) { const normalized = settingsWithDefaults(value); setSettings(normalized); setSavedSettings(normalized); setStatus("idle"); } }).catch((caught) => { if (active) { setStatus("error"); setSaveMessage({ tone: "error", text: caught instanceof Error ? caught.message : "設定を読み込めませんでした。" }); } }); return () => { active = false; }; }, [adapter]);
  useEffect(() => { let active = true; void adapter.getAppVersion().then((version) => { if (active) setAppVersion(version); }).catch(() => { if (active) setAppVersion("不明"); }); return () => { active = false; }; }, [adapter]);
  useEffect(() => {
    let active = true;
    void adapter.getConnections()
      .then((value) => { if (active) setConnections(value); })
      .catch((caught) => {
        if (active) {
          setConnectionMessage({ tone: "error", text: caught instanceof Error ? caught.message : "接続状態を読み込めませんでした。" });
        }
      });
    const cleanup = adapter.onConnectionsUpdated((value) => { if (active) setConnections(value); });
    return () => { active = false; cleanup(); };
  }, [adapter]);
  useEffect(() => adapter.onSettingsFocus(() => { setSection("providers"); setSaveMessage(null); }), [adapter]);
  const update = <K extends keyof AppSettings>(key: K, value: AppSettings[K]) => {
    setSettings((current) => ({ ...current, [key]: value }));
    setStatus("idle");
    setSaveMessage(null);
  };
  const isDirty = !settingsAreEqual(settings, savedSettings);
  const save = async (event: FormEvent) => {
    event.preventDefault();
    if (status === "saving" || !isDirty) return;
    setStatus("saving");
    setSaveMessage(null);
    try {
      await adapter.saveSettings(settings);
      setSavedSettings(structuredClone(settings));
      setStatus("success");
      setSaveMessage({ tone: "success", text: "変更を保存しました。" });
    } catch (caught) {
      setStatus("error");
      setSaveMessage({ tone: "error", text: caught instanceof Error ? caught.message : "設定を保存できませんでした。" });
    }
  };
  const runConnectionTask = async (task: string, action: () => Promise<void>, successMessage: string) => {
    if (connectionTask) return;
    setConnectionTask(task);
    setConnectionMessage(null);
    setConfirmDisconnect(null);
    try {
      await action();
      setConnectionMessage({ tone: "success", text: successMessage });
      setConnections(await adapter.getConnections());
    } catch (caught) {
      setConnectionMessage({ tone: "error", text: caught instanceof Error ? caught.message : "アカウント操作を完了できませんでした。" });
    } finally {
      setConnectionTask(null);
    }
  };
  const startCodexLogin = (mode: CodexLoginMode) => runConnectionTask(
    `codex-${mode}`,
    async () => { await adapter.startCodexLogin(mode); },
    mode === "browser" ? "ブラウザーで認証を続けてください。" : "ブラウザーでコードを入力してください。",
  );
  const connectCodexApiKey = () => runConnectionTask("codex-api", async () => {
    await adapter.setCodexApiKey(codexApiKey);
    setCodexApiKey("");
  }, "CodexをAPIキーで接続しました。");
  const connectOpenCode = () => runConnectionTask("opencode-api", async () => {
    await adapter.setOpencodeApiKey(openCodeApiKey);
    setOpenCodeApiKey("");
  }, "OpenCode Goに接続しました。");
  const disconnectProvider = (provider: "codex" | "opencode") => {
    if (confirmDisconnect !== provider) {
      setConfirmDisconnect(provider);
      return;
    }
    const action = provider === "codex" ? adapter.disconnectCodex : adapter.disconnectOpencode;
    void runConnectionTask(`disconnect-${provider}`, action, provider === "codex" ? "Codexの接続を解除しました。" : "OpenCode Goの接続を解除しました。");
  };
  const moveProvider = (index: number, direction: -1 | 1) => update("providerOrder", moveProviderOrder(settings.providerOrder, index, direction));
  const currentSection = navItems.find((item) => item.id === section) ?? navItems[0];
  const selectSection = (nextSection: SettingsSection) => {
    setSection(nextSection);
    setSaveMessage(null);
    setConfirmDisconnect(null);
  };
  return <main className="settings-app">
    <nav className="settings-nav" aria-label="設定カテゴリー">
      <div className="settings-nav__brand"><span className="widget-brand" aria-hidden="true">UD</span><div><strong>UsageDock</strong><small>設定</small></div></div>
      <div className="settings-nav__items">{navItems.map(({ id, label }) => <button type="button" key={id} className={section === id ? "settings-nav__item settings-nav__item--active" : "settings-nav__item"} aria-current={section === id ? "page" : undefined} onClick={() => selectSection(id)}>{label}</button>)}</div>
    </nav>
    <form className="settings-content" onSubmit={(event) => void save(event)}>
      <header className="settings-header">
        <div className="settings-header__copy"><h1 id="settings-page-title">{currentSection.label}</h1><p>{currentSection.description}</p></div>
        <div className="settings-actions">
          {saveMessage && <p className={`action-message action-message--${saveMessage.tone}`} role={saveMessage.tone === "error" ? "alert" : "status"}>{saveMessage.text}</p>}
          {isDirty && <><span className="unsaved-indicator">未保存の変更</span><button type="submit" className="button button--primary" disabled={status === "loading" || status === "saving"}>{status === "saving" ? "保存中…" : "変更を保存"}</button></>}
        </div>
      </header>
      {status === "loading" ? <LoadingState label="設定を読み込み中" /> : <SettingsSectionContent adapter={adapter} section={section} settings={settings} appVersion={appVersion} connections={connections} connectionMessage={connectionMessage} codexApiKey={codexApiKey} setCodexApiKey={setCodexApiKey} openCodeApiKey={openCodeApiKey} setOpenCodeApiKey={setOpenCodeApiKey} connectionTask={connectionTask} confirmDisconnect={confirmDisconnect} update={update} startCodexLogin={(mode) => void startCodexLogin(mode)} cancelCodexLogin={() => void runConnectionTask("codex-cancel", adapter.cancelCodexLogin, "Codex認証をキャンセルしました。")} connectCodexApiKey={() => void connectCodexApiKey()} connectOpenCode={() => void connectOpenCode()} moveProvider={moveProvider} disconnectProvider={disconnectProvider} />}
    </form>
  </main>;
};

type SettingsContentProps = {
  adapter: AppAdapter;
  section: SettingsSection;
  settings: AppSettings;
  appVersion: string;
  connections: ConnectionOverview | null;
  connectionMessage: ActionMessage | null;
  codexApiKey: string;
  setCodexApiKey: (value: string) => void;
  openCodeApiKey: string;
  setOpenCodeApiKey: (value: string) => void;
  connectionTask: string | null;
  confirmDisconnect: "codex" | "opencode" | null;
  update: <K extends keyof AppSettings>(key: K, value: AppSettings[K]) => void;
  startCodexLogin: (mode: CodexLoginMode) => void;
  cancelCodexLogin: () => void;
  connectCodexApiKey: () => void;
  connectOpenCode: () => void;
  moveProvider: (index: number, direction: -1 | 1) => void;
  disconnectProvider: (provider: "codex" | "opencode") => void;
};

const ConnectionBadge = ({ status }: { status: ConnectionOverview["codex"]["status"] }) => {
  const label = { connected: "接続済み", disconnected: "未接続", connecting: "認証中", unavailable: "利用不可", error: "要確認" }[status];
  return <span className={`connection-badge connection-badge--${status}`}><span aria-hidden="true" />{label}</span>;
};

const ProviderConnections = (props: Omit<SettingsContentProps, "section" | "settings" | "appVersion" | "update" | "moveProvider">) => {
  const { connections, connectionMessage, codexApiKey, setCodexApiKey, openCodeApiKey, setOpenCodeApiKey, connectionTask, confirmDisconnect, startCodexLogin, cancelCodexLogin, connectCodexApiKey, connectOpenCode, disconnectProvider } = props;
  if (!connections) return <LoadingState label="接続状態を確認中" />;
  const codex = connections.codex;
  const codexConnected = codex.status === "connected";
  const codexBusy = connectionTask?.startsWith("codex") || codex.status === "connecting";
  const openCodeConnected = connections.openCodeGo.status === "connected";
  return <>
    {connectionMessage && <p className={`section-message section-message--${connectionMessage.tone}`} role={connectionMessage.tone === "error" ? "alert" : "status"}>{connectionMessage.text}</p>}
    <div className="connection-list">
    <article className="connection-card" aria-labelledby="codex-connection-title">
      <div className="connection-card__header"><div className="connection-card__provider"><img src={openAiIconUrl} width="24" height="24" alt="" /><div><h3 id="codex-connection-title">Codex</h3><p>ChatGPTサブスクリプション / OpenAI API</p></div></div><ConnectionBadge status={codex.status} /></div>
      {codex.status === "connecting" && codex.pendingLogin ? <div className="auth-progress" role="status"><strong>ブラウザーで認証を完了してください</strong>{codex.pendingLogin.userCode && <div className="device-code"><span>認証コード</span><code>{codex.pendingLogin.userCode}</code></div>}<p>完了すると、この画面へ自動で反映されます。</p><button type="button" className="button button--ghost" disabled={connectionTask === "codex-cancel"} onClick={cancelCodexLogin}>認証をキャンセル</button></div> : <>
        {codexConnected ? <div className="connection-summary"><div><span>認証方法</span><strong>{codex.authType === "chatgpt" ? "ChatGPTアカウント" : "OpenAI APIキー"}</strong></div>{codex.email && <div><span>アカウント</span><strong>{codex.email}</strong></div>}{codex.planName && <div><span>プラン</span><strong>{codex.planName}</strong></div>}</div> : <p className="connection-description">ブラウザーでChatGPTへサインインすると、サブスクリプションの利用枠を取得できます。認証情報の保存と更新はCodexが管理します。</p>}
        {codex.error && <p className="inline-error">{codex.error}</p>}
        <div className="connection-actions"><button type="button" className="button button--primary" disabled={Boolean(connectionTask)} onClick={() => startCodexLogin("browser")}>{codexConnected ? "ChatGPTで再認証" : "ChatGPTで認証"}</button><button type="button" className="button button--secondary" disabled={Boolean(connectionTask)} onClick={() => startCodexLogin("device-code")}>認証コードを使う</button>{codexConnected && <button type="button" className={`button ${confirmDisconnect === "codex" ? "button--danger" : "button--ghost"}`} disabled={Boolean(connectionTask)} onClick={() => disconnectProvider("codex")}>{confirmDisconnect === "codex" ? "もう一度押して解除" : "接続を解除"}</button>}</div>
        <details className="alternative-auth"><summary>OpenAI APIキーで接続</summary><p>API利用料が別途発生します。サブスクリプションの利用枠表示にはChatGPT認証を使用してください。</p><label className="field"><span>OpenAI APIキー</span><input type="password" autoComplete="off" value={codexApiKey} onChange={(event) => setCodexApiKey(event.target.value)} placeholder="sk-…" /></label><button type="button" className="button button--secondary" disabled={!codexApiKey || Boolean(connectionTask)} onClick={connectCodexApiKey}>{connectionTask === "codex-api" ? "確認中…" : codex.authType === "api-key" ? "キーを更新" : "APIキーで接続"}</button></details>
      </>}
      {codex.executablePath && <p className="connection-runtime" title={codex.executablePath}>Codex実行環境: {codex.executablePath}</p>}
    </article>
    <article className="connection-card" aria-labelledby="opencode-connection-title">
      <div className="connection-card__header"><div className="connection-card__provider"><img src={openCodeGoIconUrl} width="24" height="24" alt="" /><div><h3 id="opencode-connection-title">OpenCode Go</h3><p>APIキー</p></div></div><ConnectionBadge status={connections.openCodeGo.status} /></div>
      <p className="connection-description">APIキーはWindows資格情報マネージャーへ保存され、画面へ再表示されません。</p>
      <label className="field"><span>{openCodeConnected ? "新しいAPIキー" : "APIキー"}</span><input type="password" autoComplete="off" value={openCodeApiKey} onChange={(event) => setOpenCodeApiKey(event.target.value)} placeholder={openCodeConnected ? "変更するときだけ入力" : "APIキーを入力"} /></label>
      <div className="connection-actions"><button type="button" className="button button--secondary" disabled={!openCodeApiKey || Boolean(connectionTask)} onClick={connectOpenCode}>{connectionTask === "opencode-api" ? "確認中…" : openCodeConnected ? "キーを更新" : "接続"}</button>{openCodeConnected && <button type="button" className={`button ${confirmDisconnect === "opencode" ? "button--danger" : "button--ghost"}`} disabled={Boolean(connectionTask)} onClick={() => disconnectProvider("opencode")}>{confirmDisconnect === "opencode" ? "もう一度押して解除" : "接続を解除"}</button>}</div>
    </article>
    </div>
  </>;
};

const AppUpdatePanel = ({ adapter, appVersion }: { adapter: AppAdapter; appVersion: string }) => {
  const [update, setUpdate] = useState<AppUpdate | null>(null);
  const [updateState, setUpdateState] = useState<"idle" | "checking" | "installing" | "error">("idle");
  const [updateError, setUpdateError] = useState<string | null>(null);
  const check = async () => {
    if (updateState !== "idle" && updateState !== "error") return;
    setUpdateState("checking");
    setUpdateError(null);
    try {
      setUpdate(await adapter.checkForUpdate());
      setUpdateState("idle");
    } catch (caught) {
      setUpdateError(caught instanceof Error ? caught.message : "更新情報を確認できませんでした。");
      setUpdateState("error");
    }
  };
  const install = async () => {
    if (updateState === "installing") return;
    setUpdateState("installing");
    setUpdateError(null);
    try {
      await adapter.installUpdate();
      setUpdateState("idle");
    } catch (caught) {
      setUpdateError(caught instanceof Error ? caught.message : "更新をインストールできませんでした。");
      setUpdateState("error");
    }
  };
  const updateMessage = update?.status === "current"
    ? "最新バージョンを使用しています。"
    : update?.status === "unconfigured"
      ? "この開発ビルドには更新配信先が設定されていません。"
      : "更新プログラムが公開されているか確認できます。";
  return <section className="settings-card update-card" aria-labelledby="app-update-title">
    <div className="update-card__heading">
      <div className="update-card__identity">
        <span className="app-mark" aria-hidden="true"><span className="widget-brand">UD</span></span>
        <div><h2 id="app-update-title">UsageDock</h2><p>Windows用デスクトップアプリ</p></div>
      </div>
      <span className="version-badge">バージョン {appVersion}</span>
    </div>
    {update?.status === "available" ? <div className="update-available"><div><span className="connection-badge connection-badge--connecting">新しいバージョン</span><strong>バージョン {update.availableVersion}</strong></div>{update.notes && <p>{update.notes}</p>}</div> : <p className="update-card__status" role={update?.status === "current" ? "status" : undefined}>{updateMessage}</p>}
    <div className="update-card__footer">
      <p>設定と認証情報は更新後も保持されます。</p>
      {update?.status === "available"
        ? <button type="button" className="button button--primary" disabled={updateState === "installing"} onClick={() => void install()}>{updateState === "installing" ? "更新を準備中…" : "更新して再起動"}</button>
        : <button type="button" className="button button--secondary" disabled={updateState === "checking"} onClick={() => void check()}>{updateState === "checking" ? "確認中…" : "更新を確認"}</button>}
    </div>
    {updateError && <p className="inline-error" role="alert">{updateError}</p>}
  </section>;
};

const SettingsSectionContent = (props: SettingsContentProps) => {
  const { section, settings, appVersion, update, moveProvider } = props;
  if (section === "general") return <section className="settings-section settings-stack" aria-labelledby="settings-page-title">
    <section className="settings-card" aria-labelledby="refresh-settings-title"><h2 className="settings-card__title" id="refresh-settings-title">データ更新</h2><label className="field"><span>更新間隔</span><span className="field-inline"><input type="number" min="30" max="900" step="1" value={settings.refreshIntervalSeconds} onChange={(event) => update("refreshIntervalSeconds", Number(event.target.value))} /><small>秒（30〜900）</small></span></label></section>
    <section className="settings-card" aria-labelledby="startup-settings-title"><h2 className="settings-card__title" id="startup-settings-title">起動</h2><div className="toggle-list"><label className="toggle-field"><input type="checkbox" checked={settings.autoStart} onChange={(event) => update("autoStart", event.target.checked)} /><span><strong>Windows起動時に起動</strong><small>サインイン後にUsageDockを自動で起動します。</small></span></label><label className="toggle-field"><input type="checkbox" checked={settings.startInBackground} onChange={(event) => update("startInBackground", event.target.checked)} /><span><strong>バックグラウンドで開始</strong><small>起動時は設定画面を開かず、ウィジェットだけを表示します。</small></span></label></div></section>
  </section>;
  if (section === "widget") return <section className="settings-section" aria-labelledby="settings-page-title"><div className="settings-card settings-grid"><label className="field"><span>サイズ</span><select value={settings.widgetSize} onChange={(event) => update("widgetSize", event.target.value as AppSettings["widgetSize"])}><option value="s">S — コンパクト</option><option value="m">M — 標準</option><option value="l">L — 詳細</option></select></label><label className="field"><span>表示倍率</span><span className="field-inline"><input type="number" min="75" max="150" value={settings.scalePercent} onChange={(event) => update("scalePercent", Number(event.target.value))} /><small>%（75〜150）</small></span></label><label className="field"><span>不透明度</span><span className="field-inline"><input type="number" min="75" max="100" value={settings.opacityPercent} onChange={(event) => update("opacityPercent", Number(event.target.value))} /><small>%（75〜100）</small></span></label></div></section>;
  if (section === "providers") return <section className="settings-section" aria-labelledby="settings-page-title"><ProviderConnections {...props} /><section className="settings-card provider-display-settings" aria-labelledby="provider-display-title"><h2 className="settings-card__title" id="provider-display-title">ウィジェット表示</h2><label className="toggle-field"><input type="checkbox" checked={settings.codexEnabled} onChange={(event) => update("codexEnabled", event.target.checked)} /><span><strong>Codexを表示</strong><small>認証済みアカウントの利用枠を表示します。</small></span></label><label className="toggle-field"><input type="checkbox" checked={settings.openCodeGoEnabled} onChange={(event) => update("openCodeGoEnabled", event.target.checked)} /><span><strong>OpenCode Goを表示</strong><small>登録済みAPIキーの利用枠を表示します。</small></span></label><div className="provider-order"><h3>表示順</h3>{settings.providerOrder.map((id, index) => { const providerName = id === "codex" ? "Codex" : "OpenCode Go"; return <div className="provider-order__row" key={id}><span>{index + 1}</span><strong>{providerName}</strong><button type="button" className="button button--small" aria-label={`${providerName}を上へ`} disabled={index === 0} onClick={() => moveProvider(index, -1)}>上へ</button><button type="button" className="button button--small" aria-label={`${providerName}を下へ`} disabled={index === settings.providerOrder.length - 1} onClick={() => moveProvider(index, 1)}>下へ</button></div>; })}</div></section></section>;
  if (section === "notifications") return <section className="settings-section" aria-labelledby="settings-page-title"><div className="settings-card settings-grid"><label className="toggle-field field--wide"><input type="checkbox" checked={settings.notificationThresholds.enabled} onChange={(event) => update("notificationThresholds", { ...settings.notificationThresholds, enabled: event.target.checked })} /><span><strong>残量通知を有効にする</strong><small>設定した閾値を下回ったときに一度だけ通知します。</small></span></label><label className="field"><span>注意の閾値</span><span className="field-inline"><input type="number" min="2" max="100" disabled={!settings.notificationThresholds.enabled} value={settings.notificationThresholds.warningPercent} onChange={(event) => update("notificationThresholds", { ...settings.notificationThresholds, warningPercent: Number(event.target.value) })} /><small>%以下</small></span></label><label className="field"><span>危険の閾値</span><span className="field-inline"><input type="number" min="1" max="99" disabled={!settings.notificationThresholds.enabled} value={settings.notificationThresholds.criticalPercent} onChange={(event) => update("notificationThresholds", { ...settings.notificationThresholds, criticalPercent: Number(event.target.value) })} /><small>%以下</small></span></label><p className="settings-note field--wide">残量が0%になった場合は、上の閾値にかかわらず使い切りとして通知します。</p></div></section>;
  if (section === "advanced") return <section className="settings-section" aria-labelledby="settings-page-title"><div className="settings-card settings-grid"><label className="field field--wide"><span>Codex実行ファイルのパス</span><input type="text" value={settings.codexPath ?? ""} onChange={(event) => update("codexPath", event.target.value || null)} placeholder="自動検出（推奨）" /><small>空欄の場合はCodexデスクトップ同梱版を自動検出します。</small></label><label className="toggle-field field--wide"><input type="checkbox" checked={settings.forceCompatibilityMode} onChange={(event) => update("forceCompatibilityMode", event.target.checked)} /><span><strong>ローカルログ互換モード</strong><small>App Serverを使えない旧環境だけで有効にします。アプリ内認証は利用できません。</small></span></label></div></section>;
  return <section className="settings-section" aria-labelledby="settings-page-title"><AppUpdatePanel adapter={props.adapter} appVersion={appVersion} /></section>;
};

class RootErrorBoundary extends Component<{ children: ReactNode }, { error: Error | null }> { state: { error: Error | null } = { error: null }; static getDerivedStateFromError(error: Error) { return { error }; } componentDidCatch(error: Error, info: ErrorInfo) { console.error("UsageDock UI error", error, info); } render() { return this.state.error ? <main className="root-error" role="alert"><h1>画面を表示できません</h1><p>{this.state.error.message}</p><button type="button" className="button button--primary" onClick={() => window.location.reload()}>再読み込み</button></main> : this.props.children; } }
export function App({ adapter = appAdapter }: AppProps) { const [label, setLabel] = useState<"widget" | "settings" | null>(null); useEffect(() => { setLabel(adapter.getWindowLabel()); }, [adapter]); if (!label) return <main className="bootstrap"><LoadingState /></main>; return <RootErrorBoundary>{label === "settings" ? <SettingsApp adapter={adapter} /> : <WidgetApp adapter={adapter} />}</RootErrorBoundary>; }
export { SettingsApp, WidgetApp };
