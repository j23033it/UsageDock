# UsageDock

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

## 対応プロバイダー

| プロバイダー | 取得方法 | 利用前の準備 |
| --- | --- | --- |
| Codex | Codex App Server。利用できない場合はローカルのセッションログへフォールバック | Codex CLIをインストールし、ChatGPTアカウントでログインする |
| OpenCode Go | OpenCode Go Usage API | 設定画面でAPIキーを登録する |

OpenCode GoのAPIキーはWindows Credential Managerに保存します。設定JSON、スナップショット、React側のWebViewには保存・返却しません。

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

設定画面では、更新間隔、ウィジェットの見た目、自動起動、通知閾値、プロバイダーの有効化と並べ替えを変更できます。OpenCode Goを使う場合も、ここでAPIキーを登録します。

### 終了する

通知領域のUsageDockアイコンを右クリックし、「終了」を選びます。ウィジェットを閉じるボタンは設けていません。

### 更新する

`release\UsageDock-x64-setup.exe` を実行します。インストーラーは起動中のUsageDockを確認して終了させてから更新するため、実行ファイルを手作業で上書きしないでください。保存済みの設定とAPIキーは引き継がれ、古いバージョンへの上書きは拒否されます。旧WinForms版の `CodexBar.exe` は使用しません。

## ローカルでビルドする

### 必要なもの

- Windows 11
- Node.jsとnpm
- Rustツールチェーン
- Tauri v2のWindows向け開発環境（Microsoft C++ Build Tools、WebView2を含む）
- Codexの残量を表示する場合は、ログイン済みのCodex CLI

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

### Windowsインストーラーを生成する

```powershell
$releaseVersion = Read-Host '新しいバージョン（例: 1.2.3）'
npm run release -- $releaseVersion
```

リリース前に、機能変更と検証をコミットして作業ツリーをクリーンにします。引数には現在より大きい配布バージョンを指定してください。

アプリのバージョンは `src-tauri\Cargo.toml` だけを編集元とし、`tauri.conf.json` はその値を自動的に使用します。機能追加のたびには採番せず、配布するときだけリリースコマンドで更新します。

リリースコマンドは一時フォルダーで検証とNSISビルドを行い、生成物を `release\UsageDock-x64-setup.exe` の1本へ集約します。`release\release.json` には配布バージョンとSHA-256を記録します。一時ビルドは完了時に削除されるため、`src-tauri\target` に過去バージョンの実行ファイルやインストーラーを積み上げません。

成功後は変更された `src-tauri\Cargo.toml` と `src-tauri\Cargo.lock` を確認してコミットし、配布版と同じ番号のローカルGitタグを付けます。過去版はGitで追跡し、ローカルに複数のインストーラーを保管しません。

## データ取得と状態表示

Codexは、まずローカルのCodex App Serverへ問い合わせます。起動できない場合や応答を解釈できない場合に限り、`%USERPROFILE%\.codex\sessions` の直近のセッションログから利用状況を探します。互換モードを有効にすると、最初からローカルログを使用します。

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
│     ├─ lib.rs          # ウィンドウ、トレイ、更新処理
│     ├─ model.rs        # 設定・利用状況のデータモデル
│     ├─ providers.rs    # Codex / OpenCode Goの取得処理
│     └─ storage.rs      # 設定、スナップショット、資格情報の保存
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
