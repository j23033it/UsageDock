param(
    [Parameter(Mandatory = $true, Position = 0)]
    [ValidatePattern('^\d+\.\d+\.\d+$')]
    [string]$Version,

    [Parameter(Mandatory = $true)]
    [ValidatePattern('^[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+$')]
    [string]$GitHubRepository
)

$ErrorActionPreference = 'Stop'
$projectRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$cargoManifest = Join-Path $projectRoot 'src-tauri\Cargo.toml'
$releaseDirectory = Join-Path $projectRoot 'release'
$releaseInstaller = Join-Path $releaseDirectory 'UsageDock-x64-setup.exe'
$releaseUpdaterSignature = $releaseInstaller + '.sig'
$releaseMetadata = Join-Path $releaseDirectory 'release.json'
$updaterMetadata = Join-Path $releaseDirectory 'latest.json'
$signingKey = Join-Path $env:USERPROFILE '.tauri\usagedock.key'
$updateEndpoint = "https://github.com/$GitHubRepository/releases/latest/download/latest.json"
$updateArtifactUrl = "https://github.com/$GitHubRepository/releases/download/v$Version/UsageDock-x64-setup.exe"
$temporaryRoot = [IO.Path]::GetFullPath([IO.Path]::GetTempPath())
$temporaryTarget = [IO.Path]::GetFullPath((Join-Path $temporaryRoot "UsageDock-release-$([guid]::NewGuid())"))

function Invoke-TauriSigner {
    param(
        [Parameter(Mandatory = $true)]
        [string]$FilePath
    )

    $node = (Get-Command node.exe -ErrorAction Stop).Source
    $tauriCli = Join-Path $projectRoot 'node_modules\@tauri-apps\cli\tauri.js'
    $startInfo = [Diagnostics.ProcessStartInfo]::new()
    $startInfo.FileName = $node
    $startInfo.Arguments = "`"$tauriCli`" signer sign --private-key-path `"$signingKey`" --password `"`" `"$FilePath`""
    $startInfo.WorkingDirectory = $projectRoot
    $startInfo.UseShellExecute = $false
    $startInfo.RedirectStandardOutput = $true
    $startInfo.RedirectStandardError = $true
    $startInfo.CreateNoWindow = $true

    $process = [Diagnostics.Process]::new()
    $process.StartInfo = $startInfo
    try {
        if (-not $process.Start()) {
            throw '更新署名プロセスを開始できませんでした。'
        }
        $process.WaitForExit()
        $stdout = $process.StandardOutput.ReadToEnd()
        $stderr = $process.StandardError.ReadToEnd()
        if ($stdout) {
            [Console]::Out.WriteLine($stdout.TrimEnd())
        }
        if ($stderr) {
            [Console]::Error.WriteLine($stderr.TrimEnd())
        }
        return $process.ExitCode
    }
    finally {
        $process.Dispose()
    }
}

if (-not $temporaryTarget.StartsWith($temporaryRoot, [StringComparison]::OrdinalIgnoreCase) -or $temporaryTarget -eq $temporaryRoot) {
    throw '一時ビルド先を安全に決定できませんでした。'
}

Push-Location $projectRoot
$releaseSucceeded = $false
$originalManifest = $null
$originalLock = $null
try {
    if (git status --porcelain) {
        throw 'リリース前に変更をコミットし、作業ツリーをクリーンにしてください。'
    }

    $utf8 = [Text.UTF8Encoding]::new($false)
    if (-not (Test-Path -LiteralPath $signingKey -PathType Leaf)) {
        throw "更新署名鍵が見つかりません: $signingKey"
    }
    $packageMetadata = [IO.File]::ReadAllText((Join-Path $projectRoot 'package.json'), $utf8) | ConvertFrom-Json
    $tauriConfig = [IO.File]::ReadAllText((Join-Path $projectRoot 'src-tauri\tauri.conf.json'), $utf8) | ConvertFrom-Json
    if ($packageMetadata.PSObject.Properties.Name -contains 'version' -or $tauriConfig.PSObject.Properties.Name -contains 'version') {
        throw 'アプリのバージョンはsrc-tauri\Cargo.tomlだけで管理してください。'
    }

    $cargoLock = Join-Path $projectRoot 'src-tauri\Cargo.lock'
    $manifestText = [IO.File]::ReadAllText($cargoManifest, $utf8)
    $originalManifest = $manifestText
    $originalLock = [IO.File]::ReadAllText($cargoLock, $utf8)
    $versionMatch = [regex]::Match($manifestText, '(?m)^version\s*=\s*"([^"]+)"')
    if (-not $versionMatch.Success) {
        throw 'Cargo.tomlから現在のバージョンを取得できませんでした。'
    }
    if ([version]$Version -le [version]$versionMatch.Groups[1].Value) {
        throw "新しいバージョンは現在の$($versionMatch.Groups[1].Value)より大きくしてください。"
    }

    $updatedManifest = [regex]::Replace(
        $manifestText,
        '(?m)^(version\s*=\s*")[^"]+("\s*)$',
        { param($match) $match.Groups[1].Value + $Version + $match.Groups[2].Value },
        1
    )
    [IO.File]::WriteAllText($cargoManifest, $updatedManifest, $utf8)

    $previousCargoTarget = $env:CARGO_TARGET_DIR
    $previousUpdateEndpoint = $env:USAGEDOCK_UPDATE_ENDPOINT
    $env:CARGO_TARGET_DIR = $temporaryTarget
    $env:USAGEDOCK_UPDATE_ENDPOINT = $updateEndpoint
    try {
        npm run test
        if ($LASTEXITCODE -ne 0) {
            throw 'フロントエンドテストに失敗しました。'
        }
        npm run build
        if ($LASTEXITCODE -ne 0) {
            throw 'フロントエンドビルドに失敗しました。'
        }
        cargo test --manifest-path src-tauri/Cargo.toml --release
        if ($LASTEXITCODE -ne 0) {
            throw 'Rustテストに失敗しました。'
        }
        $bundleConfig = Join-Path $temporaryTarget 'release-config.json'
        [IO.File]::WriteAllText($bundleConfig, '{"bundle":{"createUpdaterArtifacts":false}}', $utf8)
        npm run tauri -- build --bundles nsis --config $bundleConfig
        if ($LASTEXITCODE -ne 0) {
            throw 'インストーラーの生成に失敗しました。'
        }

        $generatedInstallers = @(Get-ChildItem -LiteralPath (Join-Path $temporaryTarget 'release\bundle\nsis') -File -Filter "UsageDock_${Version}_x64-setup.exe")
        if ($generatedInstallers.Count -ne 1) {
            throw '生成済みインストーラーが見つかりませんでした。'
        }
        $generatedInstaller = $generatedInstallers[0]
        $generatedSignature = $generatedInstaller.FullName + '.sig'
        # 空パスワードはWindowsの環境変数では未設定扱いになるため、CLI引数として明示します。
        $signerExitCode = Invoke-TauriSigner -FilePath $generatedInstaller.FullName
        if ($signerExitCode -ne 0) {
            throw 'インストーラーの更新署名に失敗しました。'
        }
        if (-not (Test-Path -LiteralPath $generatedSignature -PathType Leaf)) {
            throw 'インストーラーの更新署名が見つかりませんでした。'
        }
        $previousVerifyArtifact = $env:USAGEDOCK_VERIFY_ARTIFACT
        $previousVerifySignature = $env:USAGEDOCK_VERIFY_SIGNATURE
        $env:USAGEDOCK_VERIFY_ARTIFACT = $generatedInstaller.FullName
        $env:USAGEDOCK_VERIFY_SIGNATURE = $generatedSignature
        try {
            cargo test --manifest-path src-tauri/Cargo.toml --release リリース時に更新成果物の署名を検証する
            if ($LASTEXITCODE -ne 0) {
                throw '更新成果物の署名検証に失敗しました。'
            }
        }
        finally {
            if ($null -eq $previousVerifyArtifact) {
                Remove-Item Env:USAGEDOCK_VERIFY_ARTIFACT -ErrorAction SilentlyContinue
            }
            else {
                $env:USAGEDOCK_VERIFY_ARTIFACT = $previousVerifyArtifact
            }
            if ($null -eq $previousVerifySignature) {
                Remove-Item Env:USAGEDOCK_VERIFY_SIGNATURE -ErrorAction SilentlyContinue
            }
            else {
                $env:USAGEDOCK_VERIFY_SIGNATURE = $previousVerifySignature
            }
        }

        New-Item -ItemType Directory -Path $releaseDirectory -Force | Out-Null
        Copy-Item -LiteralPath $generatedInstaller.FullName -Destination $releaseInstaller -Force
        Copy-Item -LiteralPath $generatedSignature -Destination $releaseUpdaterSignature -Force
        $installerStream = [IO.File]::OpenRead($releaseInstaller)
        try {
            $sha256 = [Security.Cryptography.SHA256]::Create()
            try {
                $hashBytes = $sha256.ComputeHash($installerStream)
                $hash = ([BitConverter]::ToString($hashBytes)).Replace('-', '')
            }
            finally {
                $sha256.Dispose()
            }
        }
        finally {
            $installerStream.Dispose()
        }
        $metadata = [ordered]@{
            version = $Version
            file = 'UsageDock-x64-setup.exe'
            sha256 = $hash
            updaterFile = 'UsageDock-x64-setup.exe'
            updateEndpoint = $updateEndpoint
            generatedAt = (Get-Date).ToUniversalTime().ToString('o')
        } | ConvertTo-Json
        [IO.File]::WriteAllText($releaseMetadata, $metadata, $utf8)
        $signature = [IO.File]::ReadAllText($releaseUpdaterSignature, $utf8).Trim()
        $latest = [ordered]@{
            version = $Version
            notes = "UsageDock $Version"
            pub_date = (Get-Date).ToUniversalTime().ToString('o')
            platforms = [ordered]@{
                'windows-x86_64' = [ordered]@{
                    signature = $signature
                    url = $updateArtifactUrl
                }
            }
        } | ConvertTo-Json -Depth 5
        [IO.File]::WriteAllText($updaterMetadata, $latest, $utf8)
        $releaseSucceeded = $true
    }
    finally {
        if ($null -eq $previousCargoTarget) {
            Remove-Item Env:CARGO_TARGET_DIR -ErrorAction SilentlyContinue
        }
        else {
            $env:CARGO_TARGET_DIR = $previousCargoTarget
        }
        if ($null -eq $previousUpdateEndpoint) {
            Remove-Item Env:USAGEDOCK_UPDATE_ENDPOINT -ErrorAction SilentlyContinue
        }
        else {
            $env:USAGEDOCK_UPDATE_ENDPOINT = $previousUpdateEndpoint
        }
    }

    Write-Host "UsageDock $Version を $releaseInstaller に生成しました。"
    Write-Host 'Cargo.tomlとCargo.lockのバージョン変更を確認してコミットしてください。'
}
finally {
    Pop-Location
    if (-not $releaseSucceeded -and $null -ne $originalManifest) {
        [IO.File]::WriteAllText($cargoManifest, $originalManifest, [Text.UTF8Encoding]::new($false))
        [IO.File]::WriteAllText((Join-Path $projectRoot 'src-tauri\Cargo.lock'), $originalLock, [Text.UTF8Encoding]::new($false))
    }
    if (Test-Path -LiteralPath $temporaryTarget) {
        Remove-Item -LiteralPath $temporaryTarget -Recurse -Force
    }
}
