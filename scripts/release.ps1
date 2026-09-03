param(
    [Parameter(Mandatory = $true, Position = 0)]
    [ValidatePattern('^\d+\.\d+\.\d+$')]
    [string]$Version
)

$ErrorActionPreference = 'Stop'
$projectRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$cargoManifest = Join-Path $projectRoot 'src-tauri\Cargo.toml'
$releaseDirectory = Join-Path $projectRoot 'release'
$releaseInstaller = Join-Path $releaseDirectory 'UsageDock-x64-setup.exe'
$releaseMetadata = Join-Path $releaseDirectory 'release.json'
$temporaryRoot = [IO.Path]::GetFullPath([IO.Path]::GetTempPath())
$temporaryTarget = [IO.Path]::GetFullPath((Join-Path $temporaryRoot "UsageDock-release-$([guid]::NewGuid())"))

if (-not $temporaryTarget.StartsWith($temporaryRoot, [StringComparison]::OrdinalIgnoreCase) -or $temporaryTarget -eq $temporaryRoot) {
    throw '一時ビルド先を安全に決定できませんでした。'
}

Push-Location $projectRoot
try {
    if (git status --porcelain) {
        throw 'リリース前に変更をコミットし、作業ツリーをクリーンにしてください。'
    }

    $utf8 = [Text.UTF8Encoding]::new($false)
    $packageMetadata = [IO.File]::ReadAllText((Join-Path $projectRoot 'package.json'), $utf8) | ConvertFrom-Json
    $tauriConfig = [IO.File]::ReadAllText((Join-Path $projectRoot 'src-tauri\tauri.conf.json'), $utf8) | ConvertFrom-Json
    if ($packageMetadata.PSObject.Properties.Name -contains 'version' -or $tauriConfig.PSObject.Properties.Name -contains 'version') {
        throw 'アプリのバージョンはsrc-tauri\Cargo.tomlだけで管理してください。'
    }

    $manifestText = [IO.File]::ReadAllText($cargoManifest, $utf8)
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
    $env:CARGO_TARGET_DIR = $temporaryTarget
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
        npm run tauri build -- --bundles nsis
        if ($LASTEXITCODE -ne 0) {
            throw 'インストーラーの生成に失敗しました。'
        }

        $generatedInstallers = @(Get-ChildItem -LiteralPath (Join-Path $temporaryTarget 'release\bundle\nsis') -File -Filter "UsageDock_${Version}_x64-setup.exe")
        if ($generatedInstallers.Count -ne 1) {
            throw '生成済みインストーラーが見つかりませんでした。'
        }
        $generatedInstaller = $generatedInstallers[0]

        New-Item -ItemType Directory -Path $releaseDirectory -Force | Out-Null
        Copy-Item -LiteralPath $generatedInstaller.FullName -Destination $releaseInstaller -Force
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
            generatedAt = (Get-Date).ToUniversalTime().ToString('o')
        } | ConvertTo-Json
        [IO.File]::WriteAllText($releaseMetadata, $metadata, $utf8)
    }
    finally {
        if ($null -eq $previousCargoTarget) {
            Remove-Item Env:CARGO_TARGET_DIR -ErrorAction SilentlyContinue
        }
        else {
            $env:CARGO_TARGET_DIR = $previousCargoTarget
        }
    }

    Write-Host "UsageDock $Version を $releaseInstaller に生成しました。"
    Write-Host 'Cargo.tomlとCargo.lockのバージョン変更を確認してコミットしてください。'
}
finally {
    Pop-Location
    if (Test-Path -LiteralPath $temporaryTarget) {
        Remove-Item -LiteralPath $temporaryTarget -Recurse -Force
    }
}
