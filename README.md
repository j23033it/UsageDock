# UsageDock

CodexとOpenCode Goの残量を、Windows 11の画面右端に固定表示するTauri v2アプリです。

旧WinForms版を置き換え、React + TypeScript + Rust構成で開発しています。確定したMVP仕様は [docs/product-contract.md](docs/product-contract.md) にあります。

## 起動と更新

- 通常の起動は、Windowsのスタートメニューにある「UsageDock」を使用します。
- `src-tauri\target\debug` 内の開発用EXEは、普段の起動には使用しません。
- 更新時はUsageDockを終了し、新しく生成したNSISインストーラーを実行します。保存済みの設定は引き継がれます。
- 旧アプリの`CodexBar.exe`は使用しません。

## 開発

```powershell
npm install
npm run tauri dev
```

Windows用インストーラーは次のコマンドで生成します。

```powershell
npm run tauri build -- --bundles nsis
```

検証は次のコマンドで実行します。

```powershell
npm run check
```

## 権利表記

本アプリはOpenAIおよびOpenCodeの公式製品ではなく、提携・承認を受けたものではありません。各名称・ロゴは各権利者に帰属します。
