# UsageDock

この公開リポジトリではUsageDockのソースコードと署名済みWindows更新ファイルを管理しています。配布ファイルは[GitHub Releases](https://github.com/j23033it/UsageDock/releases)から取得できます。

CodexとOpenCode Goの利用可能な残量を、Windows 11の画面端でいつでも確認できる常駐ウィジェットです。

細いレールには各サービスの残量だけを表示し、ホバーまたはクリックすると利用枠・リセット時刻・データの取得状態を展開します。作業中にダッシュボードを開き直さなくても、AIサービスの残量を視界の端で把握できます。

> [!NOTE]
> UsageDockは個人利用を目的に開発中のWindows専用アプリです。OpenAIおよびOpenCodeの公式製品ではありません。

## 主な機能

- CodexとOpenCode Goの残量をリングとパーセントで常時表示
- 5時間枠・週間枠・月間枠など、サービスが返す複数の利用枠に対応
- ホバーで詳細を展開し、クリックで表示を固定
- 最終更新時刻、取得元、次回リセット時刻を表示
- 一時的な取得失敗時も、最後に取得できた値を古さとともに保持
- 残量20%・10%・0%を初期値とするWindows通知
- 更新間隔、サイズ、表示倍率、不透明度、プロバイダーの表示順を設定可能
- Windowsログイン時の自動起動、システムトレイからの更新・設定・終了に対応
- アプリ内でCodex / OpenCode Goの認証を追加・更新・解除
- 署名付き更新の確認とワンクリックインストール

## 対応プロバイダー

| プロバイダー | 取得方法 | 利用前の準備 |
| --- | --- | --- |
| Codex | Codex App Server | 設定画面からChatGPT認証またはOpenAI APIキーを追加する |
| OpenCode Go | OpenCode Go Usage API | 設定画面でAPIキーを登録する |

CodexのChatGPT認証は公式App Serverに任せ、資格情報はWindows Credential Managerへ保存します。UsageDockはトークンを読み取りません。OpenCode GoのAPIキーもWindows Credential Managerに保存し、設定JSON、スナップショット、React側のWebViewには保存・返却しません。

## 使い方

### 起動する

インストール後は、Windowsのスタートメニューから「UsageDock」を起動します。開発用の `src-tauri\target\debug\UsageDock.exe` は普段の利用には使用しません。

ウィジェットはプライマリモニターの右端に表示されます。

| 操作 | 動作 |
| --- | --- |
| プロバイダーへマウスを重ねる | 詳細パネルを展開する |
| 左クリック | 詳細パネルを固定／固定解除する |
| `Esc` | 展開中の詳細パネルを閉じる |
| 右クリック | そのプロバイダーの設定を開く |
| 更新アイコン | すべての残量を今すぐ取得する |
| 設定アイコン | 設定画面を開く |

設定画面では、更新間隔、ウィジェットの見た目、自動起動、通知閾値、プロバイダーの有効化と並べ替えを変更できます。「プロバイダー」ではCodexのブラウザー認証・認証コード・APIキーと、OpenCode GoのAPIキーを追加、更新、解除できます。

### 終了する

通知領域のUsageDockアイコンを右クリックし、「終了」を選びます。ウィジェットを閉じるボタンは設けていません。

### 更新する

設定画面の「アプリ情報」で「更新を確認」を押します。新しい署名済みバージョンがあれば、そのままダウンロードしてインストールできます。Windowsではインストール開始時にUsageDockが終了し、設定と資格情報は更新後も引き継がれます。初回導入だけは `release\UsageDock-x64-setup.exe` を使用します。

## ローカルでビルドする

### 必要なもの

- Windows 11
- Node.jsとnpm
- Rustツールチェーン
- Tauri v2のWindows向け開発環境（Microsoft C++ Build Tools、WebView2を含む）
- Codexデスクトップアプリ、またはApp Server対応のCodex CLI

### セットアップ

```powershell
npm install
npm run tauri dev
```

フロントエンドだけを確認する場合は `npm run dev` を使えます。ただし、Tauriのバックエンドを介する残量取得や設定保存は動作しません。

### 検証

```powershell
npm run check
```

`npm run check` は、Reactのユニットテスト、TypeScriptの型検査、Viteの本番ビルド、Rustのユニットテストを順番に実行します。

個別に実行できるコマンドは次のとおりです。

| コマンド | 内容 |
| --- | --- |
| `npm run test` | Vitestのユニットテスト |
| `npm run test:watch` | Vitestの監視モード |
| `npm run build` | TypeScriptの型検査とViteビルド |
| `cargo test --manifest-path src-tauri/Cargo.toml` | Rustのユニットテスト |

### 更新版を公開する

```powershell
$releaseVersion = Read-Host '新しいバージョン（例: 1.2.3）'
npm run release -- $releaseVersion -GitHubRepository j23033it/UsageDock
```

今後の更新はGitHub Releaseへの公開に統一します。同じ番号の単体EXEを渡す運用は行いません。

リリース前に、機能変更と検証をコミットして作業ツリーをクリーンにします。引数には現在より大きい配布バージョンと、配布に使うGitHubリポジトリの `owner/repository` を指定してください。

アプリのバージョンは `src-tauri\Cargo.toml` だけを編集元とし、`tauri.conf.json` はその値を自動的に使用します。機能追加のたびには採番せず、配布するときだけリリースコマンドで更新します。

リリースコマンドは一時フォルダーで検証とNSISビルドを行い、通常インストールとアプリ内更新を兼ねる署名付きセットアップEXE、署名、`latest.json` を `release` へ固定名で集約します。ビルド失敗時はバージョン変更を自動で戻し、一時ビルドも削除するため、`src-tauri\target` に過去版を積み上げません。

更新パッケージは `%USERPROFILE%\.tauri\usagedock.key` の秘密鍵で署名します。この鍵はリポジトリ外で安全に保管し、紛失しないでください。別の開発環境では次のコマンドで一度だけ生成します。

```powershell
npm run tauri signer generate -- --ci --write-keys "$env:USERPROFILE\.tauri\usagedock.key"
```

[UsageDock](https://github.com/j23033it/UsageDock) のGitHub Release `v<version>` へ `UsageDock-x64-setup.exe`、同名の `.sig`、`latest.json` を配置すると、アプリの更新ボタンから取得できます。ソースコードは同じリポジトリの `main` で管理します。

アップロードはドラフトで行い、セットアップEXE・署名・`latest.json` の3ファイルを揃えてから最新版として公開します。公開後はアプリの更新先にある `latest.json` のバージョンと、公開EXEのSHA-256がローカルの `release/release.json` と一致することを確認します。

成功後は変更された `src-tauri\Cargo.toml` と `src-tauri\Cargo.lock` を確認してコミットし、配布版と同じ番号のローカルGitタグを付けます。過去版はGitで追跡し、ローカルに複数のインストーラーを保管しません。

## データ取得と状態表示

Codexは、Codexデスクトップアプリに同梱された新しい実行ファイルを優先してApp Serverへ問い合わせます。UsageDock専用の `CODEX_HOME` を使うため、個人用の古い `config.toml` は読み込みません。互換モードを明示的に有効にした場合だけ、`%USERPROFILE%\.codex\sessions` の直近ログを使用します。

OpenCode Goは `https://opencode.ai/zen/go/v1/usage` へBearer認証で問い合わせます。取得処理はいずれもRust側で行い、秘密情報をUI側へ渡しません。

取得状態は最終成功時刻から次のように判定します。

| 状態 | 条件 | 表示 |
| --- | --- | --- |
| Fresh | 2分未満 | 最新の値を表示 |
| Stale | 2分以上10分未満 | 最後の値と警告を表示 |
| Outdated | 10分以上 | 最後の値と強い警告を表示 |
| Unavailable | 正常値を一度も取得できていない | 数値を `—` で表示 |

取得に失敗しても残量を0%とは扱いません。また、表示上のリセット時刻を過ぎただけでは100%へ戻さず、次の取得結果を待ちます。

## ディレクトリ構成

```text
.
├─ docs/                 # 確定した製品仕様
├─ src/                  # React / TypeScriptのUIとテスト
│  └─ assets/providers/  # プロバイダーのアイコン
├─ src-tauri/            # RustバックエンドとTauri設定
│  └─ src/
│     ├─ codex.rs        # Codex実行環境、認証、App Server接続
│     ├─ lib.rs          # ウィンドウ、トレイ、更新処理
│     ├─ model.rs        # 設定・利用状況のデータモデル
│     ├─ providers.rs    # Codex / OpenCode Goの取得処理
│     ├─ storage.rs      # 設定、スナップショット、資格情報の保存
│     └─ updates.rs      # 署名付きアプリ更新
├─ package.json          # フロントエンドの依存関係とコマンド
└─ vite.config.ts        # Vite設定
```

MVPで守る仕様と、実装時の判断基準は [製品契約](docs/product-contract.md) にまとめています。

## 技術スタック

- Tauri v2
- React 19
- TypeScript
- Rust 2024 Edition
- Vite
- Vitest

## プライバシーと制約

- テレメトリや利用状況の独自収集は行いません。
- 任意のURLを指定するプロバイダーや、自動的な有料フォールバックは実装していません。
- 設定と最後の正常な取得結果は、Tauriのアプリ設定フォルダーへローカル保存します。
- 外部サービス側のAPIやCodexのローカル形式が変わると、取得できなくなる可能性があります。

## 権利表記

本アプリはOpenAIおよびOpenCodeの公式製品ではなく、提携・承認を受けたものではありません。各名称・ロゴは各権利者に帰属します。
