<p align="center">
  <img src="../build/icon.png" alt="WhichClaw" width="120" />
</p>

<h1 align="center">WhichClaw</h1>

<p align="center">
  <strong>One Hub. All Models. Every Coding Tool.</strong><br/>
  <sub>WhichClaw est une application de bureau qui fournit une</sub>
</p>

<p align="center">
  <a href="https://github.com/ebenxp707-boop/WhichClaw/releases">
    <img src="https://img.shields.io/github/v/release/ebenxp707-boop/WhichClaw?style=flat-square&color=00FF9D" alt="Release" />
  </a>
  <img src="https://img.shields.io/badge/platform-Windows%20%7C%20macOS%20%7C%20Linux-blue?style=flat-square" alt="Platform" />
  <img src="https://img.shields.io/github/license/ebenxp707-boop/WhichClaw?style=flat-square" alt="License" />
</p>

<p align="center">
  <a href="../README.md">English</a> · <a href="./README.zh-CN.md">简体中文</a> · <a href="./README.zh-TW.md">繁體中文</a> · <a href="./README.ja.md">日本語</a> · <a href="./README.ko.md">한국어</a> · <a href="./README.es.md">Español</a> · **Français** · <a href="./README.de.md">Deutsch</a> · <a href="./README.pt.md">Português</a> · <a href="./README.ru.md">Русский</a> · <a href="./README.ar.md">العربية</a>
</p>

---

## ✨ Qu'est-ce que WhichClaw ?

WhichClaw est une application de bureau qui fournit une **interface visuelle et unifiée** pour gérer les modèles d'IA à travers vos outils de développement. Plus besoin de fouiller dans les fichiers de configuration — cliquez et basculez.

### Le Problème

- 😫 Changer de modèle IA dans des outils comme OpenClaw nécessite d'éditer manuellement les fichiers de configuration
- 🔄 Chaque outil a son propre format de configuration de modèles
- 🧩 Pas de moyen facile de gérer les compétences et extensions entre les outils

### La Solution

WhichClaw agit comme un **panneau de contrôle central** pour tous vos outils de développement IA :

- 🎯 **Changement de Modèle en Un Clic** — Basculez visuellement les modèles IA pour n'importe quel outil compatible
- 🔀 **Double Protocole** — Support OpenAI et Anthropic API, changez de modèle à tout moment
- 🚇 **Proxy Tunnel Intelligent** — Accédez aux APIs géo-restreintes sans VPN complet ; seul le trafic API est proxifié
- 🧩 **Navigateur de Compétences** — Découvrez, installez et gérez des compétences IA
- 🖥️ **Serveur de Modèles Local** — Exécutez des modèles open-source (Qwen, DeepSeek, Llama) localement via llama.cpp
- 🌍 **28 Langues** — Support complet d'internationalisation
- 🎮 **Apps IA Intégrées** — Jeux et utilitaires IA interactifs comme Reversi et AI Translate
- 🌃 **UI Hacker Cyberpunk** — Esthétique terminale néon vert qui rend le développement futuriste

## 🖼️ Captures d'écran

### Model Nexus — Gérez tous vos modèles IA en un seul endroit
![Model Nexus](1.png)

### App Manager — Changement de modèle en un clic pour tous les outils
![App Manager](2.png)

### Local Server — Exécutez des modèles open-source localement avec llama.cpp
![Local Server](3.png)

### Skill Browser — Découvrez et installez des compétences IA
![Skill Browser](4.png)

## 🚀 Démarrage Rapide

### Télécharger

Obtenez la dernière version pour votre plateforme :

| Plateforme | Télécharger |
|----------|----------|
| Windows  | [WhichClaw-Setup.exe](https://github.com/ebenxp707-boop/WhichClaw/releases/latest) |
| macOS    | [WhichClaw.dmg](https://github.com/ebenxp707-boop/WhichClaw/releases/latest) |
| Linux    | [WhichClaw.AppImage](https://github.com/ebenxp707-boop/WhichClaw/releases/latest) |

### Notes Linux

```bash
chmod +x WhichClaw-*.AppImage
./WhichClaw-*.AppImage
```

> Si vous rencontrez des erreurs FUSE : `sudo apt install libfuse2`

## 🔧 Outils Compatibles

| Outil | Statut | Changement de Modèle | Protocole |
|------|--------|----------------|----------|
| OpenClaw | ✅ Compatible | ✅ | OpenAI / Anthropic |
| Claude Code | ✅ Compatible | ✅ | Anthropic |
| Cline | ✅ Compatible | ✅ | OpenAI |
| Continue | ✅ Compatible | ✅ | OpenAI |
| OpenCode | ✅ Compatible | ✅ | OpenAI |
| Codex | ✅ Compatible | ✅ | OpenAI |
| Roo Code | ✅ Compatible | ✅ | OpenAI |

## 🏗️ Stack Technique

- **Electron** — Framework de bureau multiplateforme
- **React + TypeScript** — Framework UI
- **Vanilla CSS** — Système de design cyberpunk personnalisé
- **Vite** — Outil de build
- **llama.cpp** — Moteur d'inférence de modèles local

## 🛠️ Développement

```bash
npm install
npm run dev
npm run build
```

## 🤝 Contribuer

Les contributions sont les bienvenues ! N'hésitez pas à ouvrir des issues ou soumettre des pull requests.

We're especially looking for help with:
- 🍎 **Tests macOS** — Nous n'avons pas encore entièrement testé les builds macOS
- 🔧 **Nouvelles intégrations** — Aidez-nous à ajouter le support de plus d'outils IA
- 🌐 **Améliorations des traductions** — Locuteurs natifs bienvenus !

1. Fork the repository
2. Create your feature branch (`git checkout -b feature/amazing-feature`)
3. Commit your changes (`git commit -m 'feat: add amazing feature'`)
4. Push to the branch (`git push origin feature/amazing-feature`)
5. Open a Pull Request

## 📬 Contact

- 📧 Email: [hi@whichclaw.com](mailto:hi@whichclaw.com)
- 🐛 Bug Reports: [GitHub Issues](https://github.com/ebenxp707-boop/WhichClaw/issues)
- 💬 Discussions: [GitHub Discussions](https://github.com/ebenxp707-boop/WhichClaw/discussions)

## ⭐ Soutien

Si WhichClaw vous est utile, pensez à lui donner une ⭐ sur GitHub — cela aide les autres à découvrir le projet !

## 📄 Licence

[MIT](../LICENSE)

---

<p align="center">
  Fait avec 💚 par l'équipe WhichClaw<br/>
  <sub>📧 <a href="mailto:hi@whichclaw.com">hi@whichclaw.com</a></sub>
</p>
