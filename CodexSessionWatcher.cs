using System.Globalization;
using System.Text;
using System.Text.Json.Nodes;

namespace CodexBar;

/// <summary>
/// ~/.codex/sessions 配下のセッションJSONLを監視し、
/// token_count イベントに含まれるレートリミット情報を抽出する。
/// </summary>
public sealed class CodexSessionWatcher : IDisposable
{
    private const int InitialScanFileLimit = 20;

    private readonly string _sessionsDir;
    private readonly FileSystemWatcher _watcher;
    private readonly Dictionary<string, long> _offsets = new(StringComparer.OrdinalIgnoreCase);
    private readonly object _lock = new();
    private RateLimitInfo? _latest;

    /// <summary>新しいレートリミット情報が得られたときに発生。</summary>
    public event Action<RateLimitInfo>? RateLimitsUpdated;

    /// <summary>現時点で最も新しいレートリミット情報。</summary>
    public RateLimitInfo? Latest => _latest;

    public CodexSessionWatcher()
    {
        var home = Environment.GetFolderPath(Environment.SpecialFolder.UserProfile);
        _sessionsDir = Path.Combine(home, ".codex", "sessions");
        _watcher = new FileSystemWatcher(_sessionsDir)
        {
            IncludeSubdirectories = true,
            Filter = "*.jsonl",
            NotifyFilter = NotifyFilters.FileName | NotifyFilters.Size | NotifyFilters.LastWrite,
        };
        _watcher.Created += OnFileCreated;
        _watcher.Changed += OnFileChanged;
    }

    public void Start()
    {
        if (Directory.Exists(_sessionsDir))
        {
            _watcher.EnableRaisingEvents = true;
        }
        // 起動スキャンはUIをブロックしないようバックグラウンドで
        ThreadPool.QueueUserWorkItem(_ => InitialScan());
    }

    /// <summary>
    /// 最近更新されたファイルから token_count イベントを読み、
    /// 最新のレートリミット情報を探して通知する。
    /// </summary>
    private void InitialScan()
    {
        try
        {
            var files = Directory
                .EnumerateFiles(_sessionsDir, "*.jsonl", SearchOption.AllDirectories)
                .OrderByDescending(f => File.GetLastWriteTimeUtc(f))
                .Take(InitialScanFileLimit);

            foreach (var file in files)
            {
                var info = ReadFileForUpdates(file);
                if (info != null)
                {
                    PublishIfNewer(info);
                }
            }
        }
        catch (Exception ex) when (ex is IOException or UnauthorizedAccessException or DirectoryNotFoundException)
        {
            // ディレクトリが無い等は無視（Codex未導入環境）
        }

        // 1件でも見つかれば通知して表示を開始する
        lock (_lock)
        {
            if (_latest != null)
            {
                RaiseUpdated(_latest);
            }
        }
    }

    private void OnFileCreated(object sender, FileSystemEventArgs e) => ScheduleRead(e.FullPath);

    private void OnFileChanged(object sender, FileSystemEventArgs e) => ScheduleRead(e.FullPath);

    /// <summary>
    /// 書き込み中のチャンク読みを避けるため、デバウンスしてから増分を読む。
    /// </summary>
    private void ScheduleRead(string path)
    {
        // 書き込み中のチャンク読みを避ける簡易デバウンス
        _ = Task.Run(async () =>
        {
            await Task.Delay(250);
            var info = ReadFileForUpdates(path);
            if (info != null)
            {
                PublishIfNewer(info);
            }
        });
    }

    private RateLimitInfo? ReadFileForUpdates(string path)
    {
        long offset;
        lock (_lock)
        {
            _offsets.TryGetValue(path, out offset);
        }

        try
        {
            using var fs = new FileStream(path, FileMode.Open, FileAccess.Read,
                FileShare.ReadWrite | FileShare.Delete);
            if (offset > fs.Length)
            {
                offset = 0; // ファイルが切り詰められた場合
            }
            fs.Position = offset;

            using var reader = new StreamReader(fs, Encoding.UTF8);
            RateLimitInfo? newest = null;
            string? line;
            while ((line = reader.ReadLine()) != null)
            {
                if (line.Length == 0)
                {
                    continue;
                }
                if (TryParseTokenCount(line, out var info))
                {
                    newest = info;
                }
            }

            lock (_lock)
            {
                _offsets[path] = fs.Position;
            }
            return newest;
        }
        catch (Exception ex) when (ex is IOException or UnauthorizedAccessException)
        {
            return null;
        }
    }

    private void PublishIfNewer(RateLimitInfo info)
    {
        lock (_lock)
        {
            if (_latest != null
                && _latest.EventTimestamp != null
                && info.EventTimestamp != null
                && _latest.EventTimestamp >= info.EventTimestamp)
            {
                return; // 既に新しい情報を保持している
            }
            _latest = info;
            RaiseUpdated(info);
        }
    }

    private void RaiseUpdated(RateLimitInfo info)
    {
        RateLimitsUpdated?.Invoke(info);
    }

    /// <summary>
    /// token_count イベント1行をパースしてレートリミット情報を取り出す。
    /// </summary>
    internal static bool TryParseTokenCount(string line, out RateLimitInfo info)
    {
        info = null!;
        try
        {
            var node = JsonNode.Parse(line);
            if (node is not JsonObject root)
            {
                return false;
            }

            var payload = root["payload"] as JsonObject;
            if (payload?["type"]?.GetValue<string>() != "token_count")
            {
                return false;
            }

            var rateLimits = payload["rate_limits"] as JsonObject;
            var primary = rateLimits?["primary"] as JsonObject;
            if (rateLimits == null)
            {
                return false;
            }

            double? usedPercent = primary?["used_percent"]?.GetValue<double>();
            double? windowMinutes = primary?["window_minutes"]?.GetValue<double>();
            long? resetsAtEpoch = primary?["resets_at"]?.GetValue<long>();
            string? planType = rateLimits["plan_type"]?.GetValue<string>();

            if (usedPercent == null && resetsAtEpoch == null)
            {
                return false;
            }

            DateTime? eventTimestamp = null;
            if (root["timestamp"]?.GetValue<string>() is { } ts
                && DateTime.TryParse(ts, CultureInfo.InvariantCulture,
                    DateTimeStyles.AdjustToUniversal | DateTimeStyles.AssumeUniversal, out var parsed))
            {
                eventTimestamp = parsed.ToLocalTime();
            }

            info = new RateLimitInfo
            {
                UsedPercent = usedPercent,
                WindowMinutes = windowMinutes,
                ResetsAt = resetsAtEpoch is { } epoch ? DateTimeOffset.FromUnixTimeSeconds(epoch).LocalDateTime : null,
                PlanType = planType,
                EventTimestamp = eventTimestamp,
                LastUpdated = DateTime.Now,
            };
            return true;
        }
        catch (Exception ex) when (ex is System.Text.Json.JsonException or InvalidOperationException or FormatException)
        {
            return false;
        }
    }

    public void Dispose()
    {
        _watcher.Dispose();
    }
}
