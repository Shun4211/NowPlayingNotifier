# 🎮 NowPlayingNotifier

[![CI](https://github.com/Shun4211/NowPlayingNotifier/actions/workflows/ci.yml/badge.svg)](https://github.com/Shun4211/NowPlayingNotifier/actions/workflows/ci.yml)

Windows で再生中の曲を、見た目を変えられるポップアップで知らせる常駐アプリです。Rust で書かれており、exe は約 0.3MB です。ランタイムをインストールする必要はなく、メモリもあまり使いません。

## ✨ 特徴

- 🎵 再生中の曲名・アーティスト名・アルバムアートをポップアップで通知
- 📌 タスクバーにも再生中の曲名を小さく表示（位置・行数・幅・文字サイズ・一時停止中の表示を変更可能）
- 🖱️ ポップアップをクリックすると再生中のアプリを前面に表示
- ⌨️ グローバルホットキーでメディア操作（キーは自由に変更可能）
- 🖥️ マルチモニター対応（表示するモニターを選択可能）
- 🎮 全画面のゲーム・動画・プレゼン中は通知を控える
- ⚙️ ポップアップの位置・サイズ・透明度・テーマをカスタマイズ（プレビュー付き）
- 🚀 Windows 起動時の自動起動
- 🌐 日本語 / 英語の切り替え
- 🔔 システムトレイに常駐（二重起動防止つき）
- 🔄 新しいバージョンが出たら通知し、ワンクリックで更新

Spotify、Apple Music、YouTube など、Windows のメディア操作に対応しているアプリで使えます。

## 📥 ダウンロード

[Releases](https://github.com/Shun4211/NowPlayingNotifier/releases) からインストーラーか zip をダウンロードしてください。

| ファイル | 説明 |
|------|------|
| `NowPlayingNotifier-x.y.z-win-x64-setup.exe` | インストーラー（おすすめ）。管理者権限は不要です。スタートメニューに登録され、アンインストールもできます |
| `NowPlayingNotifier-x.y.z-win-x64.zip` | exe だけが入っています。インストールせずにすぐ使えます |
| `NowPlayingNotifier-x.y.z-win-x64-symbols.zip` | 不具合調査用のシンボル (PDB)。普段は不要です |

**動作環境**: Windows 10 (19041 以降) / Windows 11、x64

バージョン 1.2.1 までは「OtherPlayingNotifier」という名前でした。旧名のバージョン（C# 版を含む）を使っていた場合も、設定と自動起動はそのまま引き継がれます。インストーラーは上書きでインストールでき、旧名のフォルダーやショートカットは自動で削除されます。

## ⌨️ ホットキー（初期設定）

| キー | 操作 |
|------|------|
| Ctrl+Alt+Space | 再生/一時停止 |
| Ctrl+Alt+→ | 次のトラック |
| Ctrl+Alt+← | 前のトラック |

設定画面で別のキーに変えたり、無効にしたりできます。他のアプリと重なって登録できなかったときは、トレイから通知されます。

## ⚙️ 設定

システムトレイのアイコンを右クリックし、「設定」を選ぶと次の項目を変えられます。

- 表示位置（画面の4隅）と表示するモニター
- サイズ（幅・高さ）、表示時間、透明度
- テーマ（ダーク/ライト）
- タスクバー表示（表示する/しない、位置、行数、一時停止中の表示、最大幅、文字サイズ、間隔、アーティスト名・アルバムアート）
- アルバムアートの表示、全画面表示中の通知抑制、自動起動、更新の自動確認
- ホットキー
- 言語（日本語/English）

「プレビュー」ボタンを押すと、保存する前に見た目を確認できます。設定は `%APPDATA%\NowPlayingNotifier\settings.json` に保存されます。

## 🔄 アップデート

起動後しばらくしてからと、その後1日1回、GitHub の Releases に新しいバージョンがあるか確認します（設定でオフにできます。設定画面の「更新を確認」ですぐに確認することもできます）。新しいバージョンがあるとトレイから通知され、クリックするか、トレイメニューの「バージョン x.y.z に更新...」を選ぶと更新できます。

- インストーラーで入れた場合: インストーラーを自動でダウンロードして入れ替え、終わるとアプリを起動し直します
- zip 版の場合: ダウンロードページを開きます

確認のために送るのは GitHub へのバージョン確認のリクエストだけで、個人の情報は送りません。

アプリが異常終了したときは、`%LOCALAPPDATA%\NowPlayingNotifier\crash.log` に記録が残ります。

## 🛠️ 開発

```sh
git clone https://github.com/Shun4211/NowPlayingNotifier.git
cd NowPlayingNotifier

# 実行
cargo run

# テスト・静的解析
cargo test
cargo clippy --all-targets -- -D warnings

# 配布用の exe を作成（target/release/NowPlayingNotifier.exe）
cargo build --release
```

**必要なもの**
- [Rust](https://rustup.rs/)（stable、`x86_64-pc-windows-msvc`）
- Visual Studio Build Tools の「C++ によるデスクトップ開発」。リンカーと、アイコンなどを埋め込むリソースコンパイラーに使います

### リリース手順

`v1.2.3` のようなタグを push すると、GitHub Actions がテストとビルドを行い、Releases にインストーラーと zip を公開します。インストーラーは [Inno Setup](https://jrsoftware.org/isinfo.php)（`installer/NowPlayingNotifier.iss`）で作っています。exe のバージョン情報はタグから付きます。

```sh
git tag v1.2.3
git push origin v1.2.3
```

## 📦 技術スタック

- Rust / [windows](https://github.com/microsoft/windows-rs) クレート
- Win32（トレイ・設定画面）、Direct2D / DirectWrite（ポップアップの描画）、WIC（アルバムアートの読み込み）
- Windows.Media.Control API（再生中の曲の取得・メディア操作）
- GitHub Actions（CI・リリース）

## 📂 プロジェクト構成

```
NowPlayingNotifier/
├── src/
│   ├── main.rs             # 起動処理
│   ├── app.rs              # 各機能の取りまとめ・メッセージ処理
│   ├── media.rs            # 再生中の曲の監視・メディア操作
│   ├── track.rs            # 曲の切り替わり判定
│   ├── imaging.rs          # アルバムアートのデコード
│   ├── popup.rs            # ポップアップ表示
│   ├── taskbar.rs          # タスクバー上の曲名表示
│   ├── position.rs         # 表示位置の計算
│   ├── main_window.rs      # 「再生中」ウィンドウ
│   ├── settings_window.rs  # 設定画面
│   ├── settings.rs         # 設定の保存・読み込み
│   ├── hotkey.rs           # ホットキーの設定値
│   ├── hotkeys.rs          # ホットキーの登録
│   ├── tray.rs             # トレイアイコン
│   ├── system.rs           # 二重起動防止・自動起動・全画面検出・アプリの前面化
│   ├── strings.rs          # 日本語/英語の文字列
│   ├── update.rs           # 更新の確認・インストール
│   ├── crash.rs            # 異常終了の記録
│   ├── debug_log.rs        # 開発用ログ（デバッグビルドのみ）
│   └── win.rs              # Win32 の共通処理
├── res/                    # アイコン・マニフェスト
├── build.rs                # リソース（アイコン・バージョン情報）の埋め込み
├── installer/              # インストーラー (Inno Setup)
└── .github/workflows/      # CI・リリース
```
