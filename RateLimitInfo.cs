using System.Text.Json.Nodes;

namespace CodexBar;

/// <summary>
/// Codex セッションJSONL から抽出したレートリミット情報。
/// </summary>
public sealed class RateLimitInfo
{
    /// <summary>使用率（%）。サーバーが返した値。</summary>
    public double? UsedPercent { get; init; }

    /// <summary>制限ウィンドウの長さ（分）。例: 10080 = 7日。</summary>
    public double? WindowMinutes { get; init; }

    /// <summary>次のリセット日時（ローカル時刻）。</summary>
    public DateTime? ResetsAt { get; init; }

    /// <summary>プラン種別。例: prolite, plus, pro。</summary>
    public string? PlanType { get; init; }

    /// <summary>イベントが記録された日時（UTC→ローカル）。</summary>
    public DateTime? EventTimestamp { get; init; }

    /// <summary>この情報をファイルから読み取った日時。</summary>
    public DateTime LastUpdated { get; init; }
}
