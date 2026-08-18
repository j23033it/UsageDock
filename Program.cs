namespace CodexBar;

static class Program
{
    [STAThread]
    static void Main()
    {
        ApplicationConfiguration.Initialize();
        Application.Run(new AppContext());
    }
}

/// <summary>
/// アプリ全体の寿命を管理するコンテキスト。
/// トレイアイコン・JSONL監視・オーバーレイを束ねる。
/// </summary>
internal sealed class AppContext : ApplicationContext
{
    private readonly NotifyIcon _tray;
    private readonly CodexSessionWatcher _watcher;
    private readonly OverlayForm _overlay;

    public AppContext()
    {
        _overlay = new OverlayForm();
        _overlay.Show();

        _watcher = new CodexSessionWatcher();
        _watcher.RateLimitsUpdated += OnRateLimitsUpdated;
        _watcher.Start();

        _tray = new NotifyIcon
        {
            Icon = IconFactory.Create(),
            Visible = true,
            Text = "CodexBar",
            ContextMenuStrip = BuildMenu(),
        };

        // 初回起動時は自動起動を登録する（トレイメニューで解除可能）
        AutoStart.RegisterIfMissing();

        Application.ApplicationExit += (_, _) => Cleanup();
    }

    private ContextMenuStrip BuildMenu()
    {
        var menu = new ContextMenuStrip();
        var autoStartItem = new ToolStripMenuItem("Windows起動時に自動起動")
        {
            Checked = AutoStart.IsRegistered,
        };
        autoStartItem.Click += (_, _) =>
        {
            if (AutoStart.IsRegistered)
            {
                AutoStart.Unregister();
            }
            else
            {
                AutoStart.Register();
            }
            autoStartItem.Checked = AutoStart.IsRegistered;
        };
        menu.Items.Add(autoStartItem);
        menu.Items.Add("オーバーレイを初期位置へ", null, (_, _) => _overlay.ResetPosition());
        menu.Items.Add(new ToolStripSeparator());
        menu.Items.Add("終了", null, (_, _) =>
        {
            Cleanup();
            Application.Exit();
        });
        return menu;
    }

    private void OnRateLimitsUpdated(RateLimitInfo info)
    {
        _overlay.UpdateInfo(info);

        var remaining = Math.Max(0, 100 - (info.UsedPercent ?? 0));
        var reset = info.ResetsAt?.ToString("M/dd HH:mm") ?? "不明";
        _tray.Text = $"CodexBar: 残り{remaining:0}% | リセット {reset} | 更新 {info.LastUpdated:HH:mm}";
    }

    private void Cleanup()
    {
        _tray?.Dispose();
        _watcher?.Dispose();
        _overlay?.Dispose();
    }
}
