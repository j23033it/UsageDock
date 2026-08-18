using Microsoft.Win32;

namespace CodexBar;

/// <summary>
/// Windows 起動時の自動起動をレジストリ（HKCU\...\Run）で管理する。
/// </summary>
internal static class AutoStart
{
    private const string RunKeyPath = @"Software\Microsoft\Windows\CurrentVersion\Run";
    private const string ValueName = "CodexBar";

    public static bool IsRegistered
    {
        get
        {
            using var key = Registry.CurrentUser.OpenSubKey(RunKeyPath, writable: false);
            return key?.GetValue(ValueName) is string;
        }
    }

    public static void RegisterIfMissing()
    {
        if (!IsRegistered)
        {
            Register();
        }
    }

    public static void Register()
    {
        var exe = Environment.ProcessPath;
        if (string.IsNullOrEmpty(exe))
        {
            return;
        }
        using var key = Registry.CurrentUser.CreateSubKey(RunKeyPath);
        key?.SetValue(ValueName, $"\"{exe}\"");
    }

    public static void Unregister()
    {
        using var key = Registry.CurrentUser.CreateSubKey(RunKeyPath);
        key?.DeleteValue(ValueName, throwOnMissingValue: false);
    }
}
