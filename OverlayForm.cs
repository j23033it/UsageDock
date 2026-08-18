using System.Drawing.Drawing2D;
using System.Runtime.InteropServices;

namespace CodexBar;

/// <summary>
/// タスクバー上に表示する小型オーバーレイウィンドウ。
/// 残りのリミット（%）バーと次のリセット日時を表示する。
/// ドラッグでタスクバーの帯の範囲内を自由に移動でき、トレイメニューから初期位置に戻せる。
/// </summary>
public sealed class OverlayForm : Form
{
    private const int BaseWidth = 320;
    private const int BaseHeight = 44;
    private const int MinWidth = 150;
    private const float TextFontSize = 6f; // デフォルトの2/3サイズ
    private const int ClockReservePx = 320;
    private const int AbmGetTaskbarPos = 0x00000005;
    private const uint AbeLeft = 0, AbeTop = 1, AbeRight = 2, AbeBottom = 3;
    private const uint SwpNoActivate = 0x0010;
    private const uint SwpShowWindow = 0x0040;
    private const uint SwpNoSize = 0x0001;

    private static readonly IntPtr HwndTopmost = new(-1);

    private readonly System.Windows.Forms.Timer _repositionTimer;
    private RateLimitInfo? _info;
    private bool _dragging;
    private Point _dragOffset;
    private bool _userPositioned;
    private float _displayFontSize = TextFontSize;

    public OverlayForm()
    {
        // 自前でOnPaint描画するため自動スケーリングは無効にし、DPI比を手動で適用する
        AutoScaleMode = AutoScaleMode.None;
        FormBorderStyle = FormBorderStyle.None;
        ShowInTaskbar = false;
        StartPosition = FormStartPosition.Manual;
        TopMost = true;
        Opacity = 0.96;
        DoubleBuffered = true;
        BackColor = Color.FromArgb(28, 28, 30);
        Font = new Font("Segoe UI", TextFontSize);
        Size = new Size(BaseWidth, BaseHeight);

        _repositionTimer = new System.Windows.Forms.Timer { Interval = 10000 };
        _repositionTimer.Tick += (_, _) => Reposition();
        _repositionTimer.Start();

        Reposition();
    }

    /// <summary>クリックしてもフォーカスを奪わないトップモストのツールウィンドウにする。</summary>
    protected override CreateParams CreateParams
    {
        get
        {
            var cp = base.CreateParams;
            cp.ExStyle |= 0x00000008; // WS_EX_TOPMOST
            cp.ExStyle |= 0x00000080; // WS_EX_TOOLWINDOW
            cp.ExStyle |= 0x08000000; // WS_EX_NOACTIVATE
            return cp;
        }
    }

    /// <summary>DPIに応じたウィンドウサイズと位置を適用する。</summary>
    protected override void OnHandleCreated(EventArgs e)
    {
        base.OnHandleCreated(e);
        ApplyDpiScale();
        Reposition();
    }

    /// <summary>DPI比（DeviceDpi / 96）。DPI 200%なら2.0。</summary>
    private float Scale => DeviceDpi / 96f;

    /// <summary>
    /// DPIに応じたウィンドウサイズを適用する。
    /// 枠の横幅は基準フォント（6pt）のテキスト幅で決めて固定し、
    /// その枠に収まる最大のフォントサイズを自動探索して表示に使う。
    /// </summary>
    private void ApplyDpiScale()
    {
        var s = Scale;
        var margin = (int)(8 * s);
        using var g = CreateGraphics();
        // 表示テキストの最大幅ケース（ラベルなし・100%・年末の日時）
        const string sample = "残り100% 12/31(土) 23:59";

        // 枠の幅: 基準フォントでの最大テキスト幅 + マージン（この枠サイズを維持する）
        int frameWidth;
        using (var font = new Font(Font.FontFamily, TextFontSize))
        {
            frameWidth = TextRenderer.MeasureText(g, sample, font).Width + margin * 2 + (int)(6 * s);
        }
        frameWidth = Math.Max(frameWidth, (int)(MinWidth * s));

        // 枠に収まる最大のフォントサイズを探す（横幅と縦幅の両方をチェック）
        var availWidth = frameWidth - (int)(4 * s);
        var textAreaHeight = (int)(28 * s);
        float best = TextFontSize;
        for (float f = TextFontSize; f <= 28f; f += 0.25f)
        {
            using var font = new Font(Font.FontFamily, f, FontStyle.Bold);
            var size = TextRenderer.MeasureText(g, sample, font);
            if (size.Width <= availWidth && size.Height <= textAreaHeight)
            {
                best = f;
            }
            else
            {
                break;
            }
        }
        _displayFontSize = best;

        Size = new Size(frameWidth, (int)(BaseHeight * s));
    }

    public void UpdateInfo(RateLimitInfo info)
    {
        _info = info;
        Invalidate();
    }

    /// <summary>
    /// タスクバーの位置に合わせて初期位置へ配置する。
    /// ユーザーがドラッグで移動済みの場合は何もしない。
    /// </summary>
    public void Reposition()
    {
        if (_userPositioned || !IsHandleCreated)
        {
            return;
        }

        var screen = Screen.FromHandle(Handle);
        var bar = GetTaskbarRect();
        int x, y;
        if (bar is { } b)
        {
            switch (b.Edge)
            {
                case AbeBottom:
                    x = b.Rect.Right - Width - ClockReservePx;
                    y = b.Rect.Top - Height - 2;
                    break;
                case AbeTop:
                    x = b.Rect.Right - Width - ClockReservePx;
                    y = b.Rect.Bottom + 2;
                    break;
                case AbeLeft:
                    x = b.Rect.Right + 2;
                    y = b.Rect.Bottom - Height - 2;
                    break;
                default: // AbeRight
                    x = b.Rect.Left - Width - 2;
                    y = b.Rect.Bottom - Height - 2;
                    break;
            }
        }
        else
        {
            x = screen.WorkingArea.Right - Width - 20;
            y = screen.WorkingArea.Bottom - Height - 4;
        }

        // 画面内にクランプ（タスクバー上部の領域は許容する）
        x = Math.Clamp(x, screen.WorkingArea.Left, screen.WorkingArea.Right - Width);
        y = Math.Clamp(y, screen.WorkingArea.Top - Height, screen.WorkingArea.Bottom - Height);

        SetWindowPos(Handle, HwndTopmost, x, y, Width, Height, SwpNoActivate | SwpShowWindow | SwpNoSize);
    }

    /// <summary>ユーザーによる移動を解除して初期位置へ戻す。</summary>
    public void ResetPosition()
    {
        _userPositioned = false;
        Reposition();
    }

    protected override void OnPaint(PaintEventArgs e)
    {
        base.OnPaint(e);
        var g = e.Graphics;
        g.SmoothingMode = SmoothingMode.AntiAlias;
        g.TextRenderingHint = System.Drawing.Text.TextRenderingHint.ClearTypeGridFit;

        var rect = ClientRectangle;
        var s = Scale;
        var margin = (int)(8 * s);

        // 背景と枠
        using (var bg = new SolidBrush(Color.FromArgb(236, 28, 28, 30)))
        {
            g.FillRectangle(bg, rect);
        }
        using (var border = new Pen(Color.FromArgb(255, 88, 88, 94)))
        {
            g.DrawRectangle(border, 0, 0, rect.Width - 1, rect.Height - 1);
        }

        var info = _info;
        if (info == null)
        {
            using var font = new Font(Font.FontFamily, _displayFontSize, FontStyle.Bold);
            var waitingRect = new Rectangle((int)(2 * s), (int)(12 * s), rect.Width - (int)(4 * s), (int)(20 * s));
            TextRenderer.DrawText(g, "CodexBar: データ待機中…",
                font, waitingRect, Color.FromArgb(255, 180, 180, 180),
                TextFormatFlags.Left | TextFormatFlags.VerticalCenter);
            return;
        }

        double remaining = Math.Clamp(100.0 - (info.UsedPercent ?? 0.0), 0.0, 100.0);

        // 使用率バー（残り分を色付きで表示）: 上寄せでコンパクトにし、テキスト領域を広く取る
        var barRect = new Rectangle(margin, (int)(3 * s), rect.Width - margin * 2, (int)(6 * s));
        using (var barBg = new SolidBrush(Color.FromArgb(255, 60, 60, 66)))
        {
            g.FillRectangle(barBg, barRect);
        }
        var fillWidth = (int)(barRect.Width * remaining / 100.0);
        if (fillWidth > 0)
        {
            var color = remaining >= 50 ? Color.FromArgb(255, 52, 199, 89)
                : remaining >= 25 ? Color.FromArgb(255, 255, 179, 0)
                : Color.FromArgb(255, 255, 59, 48);
            using (var fill = new SolidBrush(color))
            {
                g.FillRectangle(fill, barRect.X, barRect.Y, fillWidth, barRect.Height);
            }
        }
        using (var barBorder = new Pen(Color.FromArgb(255, 120, 120, 126)))
        {
            g.DrawRectangle(barBorder, barRect.X, barRect.Y, barRect.Width - 1, barRect.Height - 1);
        }

        // 目盛り（25% / 50% / 75%位置）
        using (var tick = new Pen(Color.Black, 3f))
        {
            foreach (var pct in new[] { 0.25, 0.5, 0.75 })
            {
                var x = barRect.X + (int)(barRect.Width * pct);
                g.DrawLine(tick, x, barRect.Y + 1, x, barRect.Bottom - 2);
            }
        }

        // テキスト: 残り% と リセット日時（ラベルなし・枠いっぱいに詰めて最大サイズで描画）
        string resetText = info.ResetsAt is { } reset
            ? reset.ToString("M/d(ddd) HH:mm")
            : "不明";
        string text = $"残り{remaining:0}% {resetText}";
        using var textFont = new Font(Font.FontFamily, _displayFontSize, FontStyle.Bold);
        var textRect = new Rectangle((int)(2 * s), (int)(12 * s), rect.Width - (int)(4 * s), (int)(28 * s));
        TextRenderer.DrawText(g, text, textFont, textRect,
            Color.FromArgb(255, 240, 240, 245), TextFormatFlags.Left | TextFormatFlags.VerticalCenter);
    }

    protected override void OnMouseDown(MouseEventArgs e)
    {
        base.OnMouseDown(e);
        if (e.Button == MouseButtons.Left)
        {
            _dragging = true;
            _dragOffset = Cursor.Position - new Size(Location);
        }
    }

    protected override void OnMouseMove(MouseEventArgs e)
    {
        base.OnMouseMove(e);
        if (_dragging)
        {
            var target = Cursor.Position - (Size)_dragOffset;
            Location = ClampToTaskbarBand(target);
        }
    }

    protected override void OnMouseUp(MouseEventArgs e)
    {
        base.OnMouseUp(e);
        if (_dragging)
        {
            _dragging = false;
            _userPositioned = true;
        }
    }

    /// <summary>
    /// タスクバーの帯（タスクバーに重なる位置〜そのすぐ外側）の範囲にクランプする。
    /// タスクバー以外には配置できないようにする。
    /// </summary>
    private Point ClampToTaskbarBand(Point location)
    {
        if (GetTaskbarRect() is not { } bar)
        {
            return location;
        }
        var r = bar.Rect;
        int x, y;
        switch (bar.Edge)
        {
            case AbeBottom:
                x = Math.Clamp(location.X, r.Left, r.Right - Width);
                y = Math.Clamp(location.Y, r.Top - Height, r.Bottom - Height);
                break;
            case AbeTop:
                x = Math.Clamp(location.X, r.Left, r.Right - Width);
                y = Math.Clamp(location.Y, r.Top, r.Bottom);
                break;
            case AbeLeft:
                x = Math.Clamp(location.X, r.Left, r.Right);
                y = Math.Clamp(location.Y, r.Top, r.Bottom - Height);
                break;
            default: // AbeRight
                x = Math.Clamp(location.X, r.Left - Width, r.Right);
                y = Math.Clamp(location.Y, r.Top, r.Bottom - Height);
                break;
        }
        return new Point(x, y);
    }

    protected override void Dispose(bool disposing)
    {
        if (disposing)
        {
            _repositionTimer?.Dispose();
        }
        base.Dispose(disposing);
    }

    private static (Rectangle Rect, uint Edge)? GetTaskbarRect()
    {
        var data = new AppBarData { cbSize = Marshal.SizeOf<AppBarData>() };
        SHAppBarMessage(AbmGetTaskbarPos, ref data);
        if (data.rc.Right == 0 && data.rc.Bottom == 0)
        {
            return null;
        }
        return (new Rectangle(data.rc.Left, data.rc.Top,
            data.rc.Right - data.rc.Left, data.rc.Bottom - data.rc.Top), data.uEdge);
    }

    [StructLayout(LayoutKind.Sequential)]
    private struct AppBarData
    {
        public int cbSize;
        public IntPtr hWnd;
        public uint uCallbackMessage;
        public uint uEdge;
        public RECT rc;
        public IntPtr lParam;
    }

    [StructLayout(LayoutKind.Sequential)]
    private struct RECT
    {
        public int Left, Top, Right, Bottom;
    }

    [DllImport("shell32.dll", SetLastError = true)]
    private static extern IntPtr SHAppBarMessage(uint dwMessage, ref AppBarData pData);

    [DllImport("user32.dll")]
    private static extern bool SetWindowPos(IntPtr hWnd, IntPtr hWndInsertAfter, int x, int y, int cx, int cy, uint uFlags);
}
