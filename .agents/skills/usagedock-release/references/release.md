# 公開手順

コマンドはリポジトリルートのPowerShellで実行する。`$releaseVersion` は今回決めた番号。例の番号をそのまま再公開しない。

## 1. 公開前

- `src-tauri/Cargo.toml` が番号の唯一の編集元。`Cargo.lock` は生成に追従する。`package.json` と `tauri.conf.json` に番号を追加しない。
- `gh release list --repo j23033it/UsageDock --limit 10` と `git tag --list` で現状を確認する。現在のローカル版と公開版の両方より大きい未使用番号を選ぶ。
- 同じ番号のリリースが存在したら状態を読む。自分の今回のドラフトなら内容を照合して再開し、公開済みなら上書きしない。タイムアウトだけで作成を再試行しない。
- `scripts/release.ps1`、`src-tauri/src/updates.rs` の現行実装を確認する。署名鍵はリポジトリ外の `%USERPROFILE%/.tauri/usagedock.key`。鍵を表示・コピー・再生成しない。
- 実装をコミットして作業ツリーをクリーンにする。リリーススクリプトがフロントエンドテスト、型検査・ビルド、Rustのreleaseテスト、NSIS生成、署名と実成果物の署名検証を行う。直前に同じ検証を重複実行する必要はない。

## 2. 配布物を作る

```powershell
npm run release -- $releaseVersion -GitHubRepository j23033it/UsageDock
if ($LASTEXITCODE -ne 0) { throw '配布物の生成に失敗しました' }
powershell -NoProfile -ExecutionPolicy Bypass -File .agents/skills/usagedock-release/scripts/verify-release.ps1 -Version $releaseVersion -Stage Local
if ($LASTEXITCODE -ne 0) { throw 'ローカル成果物が一致しません' }
```

一時ビルドは数分かかる。進行中を失敗扱いせず、終了コードを待つ。失敗時はスクリプトによる番号の復元と一時フォルダーの後片付けを確認する。生成に失敗したのに `release/` の前回成果物をアップロードしない。

生成物は `release/UsageDock-x64-setup.exe`、同名の `.sig`、`latest.json`、照合用 `release.json`。同じEXEを通常インストールとアプリ内更新に使う。

変更内容と確認方法を `release/release-notes.md` に書く。必要なら `latest.json` の `notes` だけをユーザー向けに更新する。その後にLocal照合を行う。EXE、署名、版番号、URLを書き換えない。

## 3. 下書きへ配置して公開する

```powershell
gh release create "v$releaseVersion" --repo j23033it/UsageDock --draft --title "UsageDock $releaseVersion" --notes-file release/release-notes.md
if ($LASTEXITCODE -ne 0) { throw '下書きの状態を確認してください' }
gh release upload "v$releaseVersion" release/UsageDock-x64-setup.exe release/UsageDock-x64-setup.exe.sig release/latest.json --repo j23033it/UsageDock
if ($LASTEXITCODE -ne 0) { throw 'アップロードの状態を確認してください' }
powershell -NoProfile -ExecutionPolicy Bypass -File .agents/skills/usagedock-release/scripts/verify-release.ps1 -Version $releaseVersion -Stage Draft
if ($LASTEXITCODE -ne 0) { throw '公開前の照合に失敗しました' }
```

`release.json` とソースコードはアップロードしない。確認済みのドラフト再開時には、作成やアップロードを無条件に繰り返さない。既存ファイルのdigestを照合して不足分だけ追加する。違う内容があれば公開せず理由を調べる。

成功した生成処理が変更した `Cargo.toml` と `Cargo.lock` の差分を確認し、日本語メッセージでコミットする。同じ番号のローカル注釈付きタグをそのコミットへ付ける。ソース用リモートへのpushは不要。

```powershell
gh release edit "v$releaseVersion" --repo j23033it/UsageDock --draft=false --latest
if ($LASTEXITCODE -ne 0) { throw '公開結果を確認してください' }
powershell -NoProfile -ExecutionPolicy Bypass -File .agents/skills/usagedock-release/scripts/verify-release.ps1 -Version $releaseVersion -Stage Published
if ($LASTEXITCODE -ne 0) { throw '公開先の照合に失敗しました' }
```

Published照合はアプリと同じ `/releases/latest/download/latest.json` と、そのURLのEXEを取得する。単にタグページが開くことを成功条件にしない。直後に古い情報が返った場合は公開状態を確認し、少し待って読み取りだけを再試行する。古い情報が続く場合は未検証と報告し、別の公開や削除でごまかさない。

検証スクリプトは成果物の整合性を確認するもので、署名の暗号学的検証は生成スクリプトのRustテストが担当する。どちらも実機の更新成功を代替しない。続いて [Windows実機確認](windows-verification.md) を行う。
