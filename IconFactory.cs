using System.Drawing.Drawing2D;

namespace CodexBar;

/// <summary>
/// トレイアイコン用の簡易アイコン生成。
/// </summary>
internal static class IconFactory
{
    public static Icon Create()
    {
        using var bmp = new Bitmap(16, 16);
        using (var g = Graphics.FromImage(bmp))
        {
            g.SmoothingMode = SmoothingMode.AntiAlias;
            g.Clear(Color.Transparent);
            using var bg = new SolidBrush(Color.FromArgb(255, 45, 45, 50));
            using var ring = new Pen(Color.FromArgb(255, 120, 120, 126), 1.5f);
            g.FillEllipse(bg, 1, 1, 14, 14);
            g.DrawEllipse(ring, 1.5f, 1.5f, 13, 13);
            using var textBrush = new SolidBrush(Color.FromArgb(255, 52, 199, 89));
            using var font = new Font("Segoe UI", 8.5f, FontStyle.Bold, GraphicsUnit.Pixel);
            var sf = new StringFormat { Alignment = StringAlignment.Center, LineAlignment = StringAlignment.Center };
            g.DrawString("C", font, textBrush, new RectangleF(0, 0, 16, 16), sf);
        }
        return Icon.FromHandle(bmp.GetHicon());
    }
}
