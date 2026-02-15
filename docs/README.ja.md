<p align="center">
  <img src="../build/icon.png" alt="WhichClaw" width="120" />
</p>

<h1 align="center">WhichClaw</h1>

<p align="center">
  <strong>One Hub. All Models. Every Coding Tool.</strong><br/>
  <sub>WhichClaw は、AIコーディングツール全体でモデルを管理するための</sub>
</p>

<p align="center">
  <a href="https://github.com/ebenxp707-boop/WhichClaw/releases">
    <img src="https://img.shields.io/github/v/release/ebenxp707-boop/WhichClaw?style=flat-square&color=00FF9D" alt="Release" />
  </a>
  <img src="https://img.shields.io/badge/platform-Windows%20%7C%20macOS%20%7C%20Linux-blue?style=flat-square" alt="Platform" />
  <img src="https://img.shields.io/github/license/ebenxp707-boop/WhichClaw?style=flat-square" alt="License" />
</p>

<p align="center">
  <a href="../README.md">English</a> · <a href="./README.zh-CN.md">简体中文</a> · <a href="./README.zh-TW.md">繁體中文</a> · **日本語** · <a href="./README.ko.md">한국어</a> · <a href="./README.es.md">Español</a> · <a href="./README.fr.md">Français</a> · <a href="./README.de.md">Deutsch</a> · <a href="./README.pt.md">Português</a> · <a href="./README.ru.md">Русский</a> · <a href="./README.ar.md">العربية</a>
</p>

---

## ✨ WhichClaw とは？

WhichClaw は、AIコーディングツール全体でモデルを管理するための**ビジュアルで統一されたインターフェース**を提供するデスクトップアプリです。設定ファイルを掘り返す必要はもうありません — クリックするだけで切り替え。

### 課題

- 😫 OpenClaw などのツールでAIモデルを切り替えるには設定ファイルの手動編集が必要
- 🔄 各ツールが独自のモデル設定形式を持っている
- 🧩 ツール間でスキルや拡張機能を管理する簡単な方法がない

### ソリューション

WhichClaw はすべてのAIコーディングツールの**中央コントロールパネル**として機能します：

- 🎯 **ワンクリックモデル切替** — 対応ツールのAIモデルをビジュアルに切り替え
- 🔀 **デュアルプロトコル** — OpenAI & Anthropic API対応、いつでもどこでもモデル切替
- 🚇 **スマートトンネルプロキシ** — フルVPNなしで地域制限APIにアクセス、APIトラフィックのみをプロキシ
- 🧩 **スキルブラウザ** — AIスキルを発見、インストール、管理
- 🖥️ **ローカルモデルサーバー** — llama.cpp経由でオープンソースモデル（Qwen、DeepSeek、Llama）をローカル実行
- 🌍 **28言語対応** — グローバル対応の完全国際化
- 🎮 **内蔵AIアプリ** — Reversi やAI翻訳などのインタラクティブなAIゲームとユーティリティ
- 🌃 **サイバーパンクハッカーUI** — ネオングリーンのターミナル美学で近未来的コーディング体験

## 🖼️ スクリーンショット

### Model Nexus — すべてのAIモデルを一箇所で管理
![Model Nexus](1.png)

### App Manager — すべてのコーディングツールをワンクリックでモデル切替
![App Manager](2.png)

### Local Server — llama.cppでオープンソースモデルをローカル実行
![Local Server](3.png)

### Skill Browser — AIスキルを発見・インストール
![Skill Browser](4.png)

## 🚀 クイックスタート

### ダウンロード

お使いのプラットフォーム向けの最新リリースを入手：

| プラットフォーム | ダウンロード |
|----------|----------|
| Windows  | [WhichClaw-Setup.exe](https://github.com/ebenxp707-boop/WhichClaw/releases/latest) |
| macOS    | [WhichClaw.dmg](https://github.com/ebenxp707-boop/WhichClaw/releases/latest) |
| Linux    | [WhichClaw.AppImage](https://github.com/ebenxp707-boop/WhichClaw/releases/latest) |

### Linux の注意事項

```bash
chmod +x WhichClaw-*.AppImage
./WhichClaw-*.AppImage
```

> FUSE エラーが発生した場合： `sudo apt install libfuse2`

## 🔧 対応ツール

| ツール | ステータス | モデル切替 | プロトコル |
|------|--------|----------------|----------|
| OpenClaw | ✅ 対応済み | ✅ | OpenAI / Anthropic |
| Claude Code | ✅ 対応済み | ✅ | Anthropic |
| Cline | ✅ 対応済み | ✅ | OpenAI |
| Continue | ✅ 対応済み | ✅ | OpenAI |
| OpenCode | ✅ 対応済み | ✅ | OpenAI |
| Codex | ✅ 対応済み | ✅ | OpenAI |
| Roo Code | ✅ 対応済み | ✅ | OpenAI |

## 🏗️ 技術スタック

- **Electron** — クロスプラットフォームデスクトップフレームワーク
- **React + TypeScript** — UIフレームワーク
- **Vanilla CSS** — カスタムサイバーパンクデザインシステム
- **Vite** — ビルドツール
- **llama.cpp** — ローカルモデル推論エンジン

## 🛠️ 開発

```bash
npm install
npm run dev
npm run build
```

## 🤝 コントリビュート

コントリビュート大歓迎！Issue や Pull Request をお気軽にどうぞ。

We're especially looking for help with:
- 🍎 **macOSテスト** — macOSビルドのテストがまだ完了していません
- 🔧 **新ツール統合** — より多くのAIコーディングツールのサポート追加にご協力ください
- 🌐 **翻訳改善** — ネイティブスピーカー歓迎！

1. Fork the repository
2. Create your feature branch (`git checkout -b feature/amazing-feature`)
3. Commit your changes (`git commit -m 'feat: add amazing feature'`)
4. Push to the branch (`git push origin feature/amazing-feature`)
5. Open a Pull Request

## 📬 Contact

- 📧 Email: [hi@whichclaw.com](mailto:hi@whichclaw.com)
- 🐛 Bug Reports: [GitHub Issues](https://github.com/ebenxp707-boop/WhichClaw/issues)
- 💬 Discussions: [GitHub Discussions](https://github.com/ebenxp707-boop/WhichClaw/discussions)

## ⭐ サポート

WhichClaw が役立ったら、GitHub で ⭐ をお願いします — プロジェクトの発見に繋がります！

## 📄 ライセンス

[MIT](../LICENSE)

---

<p align="center">
  WhichClaw チームが 💚 を込めて制作<br/>
  <sub>📧 <a href="mailto:hi@whichclaw.com">hi@whichclaw.com</a></sub>
</p>
