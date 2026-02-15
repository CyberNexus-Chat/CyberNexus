<p align="center">
  <img src="../build/icon.png" alt="WhichClaw" width="120" />
</p>

<h1 align="center">WhichClaw</h1>

<p align="center">
  <strong>One Hub. All Models. Every Coding Tool.</strong><br/>
  <sub>WhichClaw�?AI 코딩 도구 전반�?걸쳐 모델�?관리하�?위한</sub>
</p>

<p align="center">
  <a href="https://github.com/ebenxp707-boop/WhichClaw/releases">
    <img src="https://img.shields.io/github/v/release/ebenxp707-boop/WhichClaw?style=flat-square&color=00FF9D" alt="Release" />
  </a>
  <img src="https://img.shields.io/badge/platform-Windows%20%7C%20macOS%20%7C%20Linux-blue?style=flat-square" alt="Platform" />
  <img src="https://img.shields.io/github/license/ebenxp707-boop/WhichClaw?style=flat-square" alt="License" />
</p>

<p align="center">
  <a href="../README.md">English</a> · <a href="./README.zh-CN.md">简体中�?/a> · <a href="./README.zh-TW.md">繁體中文</a> · <a href="./README.ja.md">日本�?/a> · **한국�?* · <a href="./README.es.md">Español</a> · <a href="./README.fr.md">Français</a> · <a href="./README.de.md">Deutsch</a> · <a href="./README.pt.md">Português</a> · <a href="./README.ru.md">Русский</a> · <a href="./README.ar.md">العربية</a>
</p>

---

## �?WhichClaw란?

WhichClaw�?AI 코딩 도구 전반�?걸쳐 모델�?관리하�?위한 **시각적이�?통합�?인터페이�?*�?제공하는 데스크톱 애플리케이션입니�? 설정 파일�?뒤질 필요 없이 �?클릭 �?번으�?전환하세�?

### 문제�?
- 😫 OpenClaw 같은 도구에서 AI 모델�?전환하려�?설정 파일�?수동으로 편집해야 �?- 🔄 �?도구마다 고유�?모델 구성 형식�?있음
- 🧩 도구 �?스킬�?확장 기능�?관리할 편리�?방법�?없음

### 솔루�?
WhichClaw�?모든 AI 코딩 도구�?**중앙 제어 패널** 역할�?합니�?

- 🎯 **원클�?모델 전환** �?지�?도구�?AI 모델�?시각적으�?전환
- 🔀 **듀�?프로토콜** �?OpenAI & Anthropic API 지�? 언제 어디서나 모델 전환
- 🚇 **스마�?터널 프록�?* �?VPN 없이 지�?제한 API�?접근, API 트래픽만 프록�?- 🧩 **스킬 브라우저** �?AI 스킬 검�? 설치, 관�?- 🖥�?**로컬 모델 서버** �?llama.cpp�?오픈소스 모델(Qwen, DeepSeek, Llama) 로컬 실행
- 🌍 **28�?언어** �?글로벌 대�?완전 국제�?- 🎮 **내장 AI �?* �?Reversi, AI 번역 �?인터랙티�?AI 게임 �?유틸리티
- 🌃 **사이버펑�?해커 UI** �?네온 그린 터미�?미학으로 미래지향적 코딩 경험

## 🖼�?스크린샷

### Model Nexus �?�?곳에�?모든 AI 모델 관�?![Model Nexus](1.png)

### App Manager �?모든 코딩 도구 원클�?모델 전환
![App Manager](2.png)

### Local Server �?llama.cpp�?오픈소스 모델 로컬 실행
![Local Server](3.png)

### Skill Browser �?AI 스킬 검�?�?설치
![Skill Browser](4.png)

## 🚀 빠른 시작

### 다운로드

플랫폼에 맞는 최신 릴리스를 받으세요:

| 플랫�?| 다운로드 |
|----------|----------|
| Windows  | [WhichClaw-Setup.exe](https://github.com/ebenxp707-boop/WhichClaw/releases/latest) |
| macOS    | [WhichClaw.dmg](https://github.com/ebenxp707-boop/WhichClaw/releases/latest) |
| Linux    | [WhichClaw.AppImage](https://github.com/ebenxp707-boop/WhichClaw/releases/latest) |

### Linux 참고사항

```bash
chmod +x WhichClaw-*.AppImage
./WhichClaw-*.AppImage
```

> FUSE 오류가 발생하면: `sudo apt install libfuse2`

## 🔧 지�?도구

| 도구 | 상태 | 모델 전환 | 프로토콜 |
|------|--------|----------------|----------|
| OpenClaw | �?지�?| �?| OpenAI / Anthropic |
| Claude Code | �?지�?| �?| Anthropic |
| Cline | �?지�?| �?| OpenAI |
| Continue | �?지�?| �?| OpenAI |
| OpenCode | �?지�?| �?| OpenAI |
| Codex | �?지�?| �?| OpenAI |
| Roo Code | �?지�?| �?| OpenAI |

## 🏗�?기술 스택

- **Electron** �?크로�?플랫�?데스크톱 프레임워�?- **React + TypeScript** �?UI 프레임워�?- **Vanilla CSS** �?커스텀 사이버펑�?디자�?시스�?- **Vite** �?빌드 도구
- **llama.cpp** �?로컬 모델 추론 엔진

## 🛠�?개발

```bash
npm install
npm run dev
npm run build
```

## 🤝 기여

기여�?환영합니�? 이슈�?풀 리퀘스트를 자유롭게 제출�?주세�?

We're especially looking for help with:
- 🍎 **macOS 테스�?* �?macOS 빌드�?아직 완전�?테스트하지 못했습니�?- 🔧 **새로�?도구 통합** �?�?많은 AI 코딩 도구 지�?추가�?도움�?주세�?- 🌐 **번역 개선** �?원어�?환영!

1. Fork the repository
2. Create your feature branch (`git checkout -b feature/amazing-feature`)
3. Commit your changes (`git commit -m 'feat: add amazing feature'`)
4. Push to the branch (`git push origin feature/amazing-feature`)
5. Open a Pull Request

## 📬 Contact

- 📧 Email: [hi@whichclaw.com](mailto:hi@whichclaw.com)
- 🐛 Bug Reports: [GitHub Issues](https://github.com/ebenxp707-boop/WhichClaw/issues)
- 💬 Discussions: [GitHub Discussions](https://github.com/ebenxp707-boop/WhichClaw/discussions)

## �?지�?
WhichClaw가 유용하다�? GitHub에서 ⭐를 눌러주세�?�?�?많은 사람들이 프로젝트�?발견하는 �?도움�?됩니�?

## 📄 라이선스

[MIT](../LICENSE)

---

<p align="center">
  WhichClaw 팀�?💚으로 제작<br/>
  <sub>📧 <a href="mailto:hi@whichclaw.com">hi@whichclaw.com</a></sub>
</p>
