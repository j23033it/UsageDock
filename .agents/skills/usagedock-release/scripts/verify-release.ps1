param(
    [Parameter(Mandatory = $true)]
    [ValidatePattern('^\d+\.\d+\.\d+$')]
    [string]$Version,
    [ValidateSet('Local', 'Draft', 'Published')]
    [string]$Stage = 'Local'
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$repositoryRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..\..\..\..'))
$releaseRoot = Join-Path $repositoryRoot 'release'
$repository = 'j23033it/UsageDock'
$endpoint = "https://github.com/$repository/releases/latest/download/latest.json"
$artifactUrl = "https://github.com/$repository/releases/download/v$Version/UsageDock-x64-setup.exe"
$utf8 = [Text.UTF8Encoding]::new($false)

function Read-JsonFile([string]$Path) {
    return ([IO.File]::ReadAllText($Path, $utf8) | ConvertFrom-Json)
}

function Assert-UpdateMetadata($Value) {
    if ($Value.version -cne $Version) { throw "更新情報の番号が一致しません: $($Value.version)" }
    $platform = $Value.platforms.'windows-x86_64'
    if ($platform.url -cne $artifactUrl) { throw '更新情報のダウンロード先が一致しません' }
    if ($platform.signature -cne $signature) { throw '更新情報と署名ファイルが一致しません' }
}

# 配置先のプロジェクトを確認してから、配布物の読み取りだけを行う。
$config = Read-JsonFile (Join-Path $repositoryRoot 'src-tauri/tauri.conf.json')
if ($config.identifier -cne 'app.usagedock.desktop') { throw 'UsageDock専用の検証です' }
$manifest = [IO.File]::ReadAllText((Join-Path $repositoryRoot 'src-tauri/Cargo.toml'), $utf8)
$manifestVersion = [regex]::Match($manifest, '(?m)^version\s*=\s*"([^"]+)"').Groups[1].Value
if ($manifestVersion -cne $Version) { throw "ソースの番号が一致しません: $manifestVersion" }
$lock = [IO.File]::ReadAllText((Join-Path $repositoryRoot 'src-tauri/Cargo.lock'), $utf8)
$lockVersion = [regex]::Match($lock, '(?m)^name = "usage-dock"\r?\nversion = "([^"]+)"').Groups[1].Value
if ($lockVersion -cne $Version) { throw 'Cargo.lockの番号が一致しません' }
$metadata = Read-JsonFile (Join-Path $releaseRoot 'release.json')
$latest = Read-JsonFile (Join-Path $releaseRoot 'latest.json')
$signature = [IO.File]::ReadAllText((Join-Path $releaseRoot 'UsageDock-x64-setup.exe.sig'), $utf8).Trim()
if ([string]::IsNullOrWhiteSpace($signature)) { throw '署名が空です' }
$null = [Convert]::FromBase64String($signature)
$artifactHash = (Get-FileHash -LiteralPath (Join-Path $releaseRoot 'UsageDock-x64-setup.exe') -Algorithm SHA256).Hash
if ($metadata.version -cne $Version -or $metadata.file -cne 'UsageDock-x64-setup.exe' -or
    $metadata.updaterFile -cne 'UsageDock-x64-setup.exe' -or $metadata.updateEndpoint -cne $endpoint) {
    throw 'ローカル配布記録が一致しません'
}
if ($metadata.sha256 -ine $artifactHash) { throw 'ローカルEXEのSHA-256が一致しません' }
Assert-UpdateMetadata $latest

if ($Stage -ne 'Local') {
    $releaseText = & gh release view "v$Version" --repo $repository --json tagName,isDraft,assets
    if ($LASTEXITCODE -ne 0) { throw 'GitHubのリリース状態を読み取れません' }
    $remote = ($releaseText -join "`n") | ConvertFrom-Json
    if ($remote.tagName -cne "v$Version" -or $remote.isDraft -ne ($Stage -eq 'Draft')) {
        throw 'リリースの番号または公開状態が一致しません'
    }
    $names = @('UsageDock-x64-setup.exe', 'UsageDock-x64-setup.exe.sig', 'latest.json')
    if (@($remote.assets).Count -ne $names.Count) { throw '配布ファイルは3つ必要です' }
    foreach ($name in $names) {
        $assets = @($remote.assets | Where-Object { $_.name -ceq $name })
        if ($assets.Count -ne 1) { throw "配布ファイルが一意ではありません: $name" }
        $localFile = Join-Path $releaseRoot $name
        $digest = 'sha256:' + (Get-FileHash -LiteralPath $localFile -Algorithm SHA256).Hash.ToLowerInvariant()
        if ($assets[0].state -cne 'uploaded' -or $assets[0].digest -cne $digest -or
            $assets[0].size -ne (Get-Item -LiteralPath $localFile).Length) {
            throw "GitHub上のファイルがローカルと一致しません: $name"
        }
    }
}

if ($Stage -eq 'Published') {
    # ユーザーのアプリが使うURLを確認する。タグAPIだけで代用しない。
    $published = Invoke-RestMethod -Uri $endpoint -Headers @{ 'Cache-Control' = 'no-cache' } -TimeoutSec 60
    Assert-UpdateMetadata $published
    Add-Type -AssemblyName System.Net.Http
    $client = [Net.Http.HttpClient]::new()
    try {
        $client.Timeout = [TimeSpan]::FromSeconds(60)
        $bytes = $client.GetByteArrayAsync($artifactUrl).GetAwaiter().GetResult()
        $sha = [Security.Cryptography.SHA256]::Create()
        try { $downloadHash = [BitConverter]::ToString($sha.ComputeHash($bytes)).Replace('-', '') }
        finally { $sha.Dispose() }
        if ($downloadHash -cne $artifactHash) { throw '公開URLから取得したEXEが一致しません' }
    }
    finally { $client.Dispose() }
}

[pscustomobject]@{
    version = $Version
    stage = $Stage
    sha256 = $artifactHash
    result = '成果物の整合性を確認しました。署名の暗号学的検証と実機更新は別途確認してください。'
} | ConvertTo-Json
