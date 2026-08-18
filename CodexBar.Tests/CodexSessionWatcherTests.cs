using CodexBar;

namespace CodexBar.Tests;

public class CodexSessionWatcherTests
{
    // 実データの構造を再現した token_count イベント1行
    private const string RealisticTokenCountLine =
        "{\"timestamp\":\"2026-08-15T12:03:07.267Z\",\"type\":\"event_msg\"," +
        "\"payload\":{\"type\":\"token_count\",\"info\":{}," +
        "\"rate_limits\":{\"limit_id\":\"codex\",\"limit_name\":null," +
        "\"primary\":{\"used_percent\":25.0,\"window_minutes\":10080,\"resets_at\":1787212694}," +
        "\"secondary\":null,\"credits\":{\"has_credits\":false,\"unlimited\":false,\"balance\":\"0\"}," +
        "\"individual_limit\":null,\"spend_control_reached\":null," +
        "\"plan_type\":\"prolite\",\"rate_limit_reached_type\":null}}}";

    [Fact]
    public void ParsesRealisticTokenCountEvent()
    {
        var ok = CodexSessionWatcher.TryParseTokenCount(RealisticTokenCountLine, out var info);

        Assert.True(ok);
        Assert.NotNull(info);
        Assert.Equal(25.0, info.UsedPercent);
        Assert.Equal(10080.0, info.WindowMinutes);
        Assert.Equal(DateTimeOffset.FromUnixTimeSeconds(1787212694).LocalDateTime, info.ResetsAt);
        Assert.Equal("prolite", info.PlanType);
        Assert.NotNull(info.EventTimestamp);
    }

    [Fact]
    public void IgnoresNonTokenCountEvents()
    {
        const string line =
            "{\"timestamp\":\"2026-08-15T12:00:00.000Z\",\"type\":\"event_msg\"," +
            "\"payload\":{\"type\":\"agent_message\",\"message\":\"hello\"}}";

        var ok = CodexSessionWatcher.TryParseTokenCount(line, out var info);

        Assert.False(ok);
        Assert.Null(info);
    }

    [Fact]
    public void IgnoresMalformedJson()
    {
        var ok = CodexSessionWatcher.TryParseTokenCount("{not valid json", out var info);

        Assert.False(ok);
        Assert.Null(info);
    }

    [Fact]
    public void IgnoresEventWithoutRateLimits()
    {
        const string line =
            "{\"timestamp\":\"2026-08-15T12:00:00.000Z\",\"type\":\"event_msg\"," +
            "\"payload\":{\"type\":\"token_count\",\"info\":{}}}";

        var ok = CodexSessionWatcher.TryParseTokenCount(line, out var info);

        Assert.False(ok);
        Assert.Null(info);
    }

    [Fact]
    public void ParsesEventWithOnlyUsedPercent()
    {
        const string line =
            "{\"timestamp\":\"2026-08-15T12:00:00.000Z\",\"type\":\"event_msg\"," +
            "\"payload\":{\"type\":\"token_count\",\"info\":{}," +
            "\"rate_limits\":{\"primary\":{\"used_percent\":50.5,\"window_minutes\":null,\"resets_at\":null}}}}";

        var ok = CodexSessionWatcher.TryParseTokenCount(line, out var info);

        Assert.True(ok);
        Assert.Equal(50.5, info.UsedPercent);
        Assert.Null(info.WindowMinutes);
        Assert.Null(info.ResetsAt);
    }

    [Fact]
    public void ParsesEventWithOnlyResetsAt()
    {
        const string line =
            "{\"timestamp\":\"2026-08-15T12:00:00.000Z\",\"type\":\"event_msg\"," +
            "\"payload\":{\"type\":\"token_count\",\"info\":{}," +
            "\"rate_limits\":{\"primary\":{\"used_percent\":null,\"window_minutes\":60,\"resets_at\":1787000000}}}}";

        var ok = CodexSessionWatcher.TryParseTokenCount(line, out var info);

        Assert.True(ok);
        Assert.Null(info.UsedPercent);
        Assert.Equal(60.0, info.WindowMinutes);
        Assert.Equal(DateTimeOffset.FromUnixTimeSeconds(1787000000).LocalDateTime, info.ResetsAt);
    }
}
