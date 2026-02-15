<p align="center">
  <img src="build/icon.png" alt="WhichClaw" width="120" />
</p>

<h1 align="center">WhichClaw</h1>

<p align="center">
  <strong>Visual AI Model Switching & Configuration Hub for Coding Tools</strong>
</p>

<p align="center">
  <a href="https://github.com/ebenxp707-boop/WhichClaw/releases">
    <img src="https://img.shields.io/github/v/release/ebenxp707-boop/WhichClaw?style=flat-square&color=00FF9D" alt="Release" />
  </a>
  <img src="https://img.shields.io/badge/platform-Windows%20%7C%20macOS%20%7C%20Linux-blue?style=flat-square" alt="Platform" />
  <img src="https://img.shields.io/github/license/ebenxp707-boop/WhichClaw?style=flat-square" alt="License" />
</p>

<p align="center">
  <a href="./README.md">English</a> ·
  <a href="./docs/README.zh-CN.md">简体中文</a> ·
  <a href="./docs/README.ja.md">日本語</a> ·
  <a href="./docs/README.ko.md">한국어</a>
</p>

---

## ✨ What is WhichClaw?

WhichClaw is a desktop application that provides a **visual, unified interface** for managing AI models across your coding tools. No more digging through config files or worrying about token usage — just point, click, and switch.

### The Problem

- 😫 Switching AI models in tools like OpenClaw requires editing config files manually
- 💸 No visibility into token consumption across different tools
- 🔄 Each tool has its own model configuration format
- 🔑 API keys scattered across multiple config files

### The Solution

WhichClaw acts as a **central control panel** for all your AI-powered coding tools:

- 🎯 **One-Click Model Switching** — Visually switch AI models for any supported tool
- 📊 **Token Usage Monitoring** — Track consumption and costs in real-time
- 🔐 **Secure Key Management** — Encrypted API key storage with hardware binding
- 🖥️ **Local Model Support** — Run open-source models (Llama, Mistral) locally via llama.cpp
- 🎮 **Built-in AI Playground** — Test models with interactive games like AI Reversi

## 🖼️ Screenshots

### Model Nexus — Manage all your AI models in one place
![Model Nexus](docs/1.png)

### App Manager — One-click model switching for all coding tools
![App Manager](docs/2.png)

### Local Server — Run open-source models locally with llama.cpp
![Local Server](docs/3.png)

### Skill Browser — Discover and install AI skills
![Skill Browser](docs/4.png)

## 🚀 Quick Start

### Download

Get the latest release for your platform:

| Platform | Download |
|----------|----------|
| Windows  | [WhichClaw-Setup.exe](https://github.com/ebenxp707-boop/WhichClaw/releases/latest) |
| macOS    | [WhichClaw.dmg](https://github.com/ebenxp707-boop/WhichClaw/releases/latest) |
| Linux    | [WhichClaw.AppImage](https://github.com/ebenxp707-boop/WhichClaw/releases/latest) |

### Linux Notes

```bash
chmod +x WhichClaw-*.AppImage
./WhichClaw-*.AppImage
```

> If you encounter FUSE errors: `sudo apt install libfuse2`

## 🔧 Supported Tools

| Tool | Status | Model Switching | Protocol |
|------|--------|----------------|----------|
| OpenClaw | ✅ Supported | ✅ | OpenAI / Anthropic |
| Claude Code | ✅ Supported | ✅ | Anthropic |
| Cline | ✅ Supported | ✅ | OpenAI / Anthropic |
| Continue | ✅ Supported | ✅ | OpenAI |
| Aider | ✅ Supported | ✅ | OpenAI |
| OpenCode | ✅ Supported | ✅ | OpenAI |
| Codex | ✅ Supported | ✅ | OpenAI |
| Roo Code | 🔜 Coming | — | — |

## 🏗️ Tech Stack

- **Electron** — Cross-platform desktop framework
- **React + TypeScript** — UI framework
- **Tailwind CSS** — Styling
- **Vite** — Build tool
- **llama.cpp** — Local model inference

## 🛠️ Development

```bash
# Install dependencies
npm install

# Start development server
npm run dev

# Build for production
npm run build
```

## 📄 License

[MIT](LICENSE)

---

<p align="center">
  Made with 💚 by the WhichClaw Team
</p>
