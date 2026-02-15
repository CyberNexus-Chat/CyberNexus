<p align="center">
  <img src="../build/icon.png" alt="WhichClaw" width="120" />
</p>

<h1 align="center">WhichClaw</h1>

<p align="center">
  <strong>One Hub. All Models. Every Coding Tool.</strong><br/>
  <sub>WhichClaw es una aplicaci贸n de escritorio que proporciona una</sub>
</p>

<p align="center">
  <a href="https://github.com/ebenxp707-boop/WhichClaw/releases">
    <img src="https://img.shields.io/github/v/release/ebenxp707-boop/WhichClaw?style=flat-square&color=00FF9D" alt="Release" />
  </a>
  <img src="https://img.shields.io/badge/platform-Windows%20%7C%20macOS%20%7C%20Linux-blue?style=flat-square" alt="Platform" />
  <img src="https://img.shields.io/github/license/ebenxp707-boop/WhichClaw?style=flat-square" alt="License" />
</p>

<p align="center">
  <a href="../README.md">English</a> 路 <a href="./README.zh-CN.md">绠�浣撲腑鏂?/a> 路 <a href="./README.zh-TW.md">绻侀珨涓枃</a> 路 <a href="./README.ja.md">鏃ユ湰瑾?/a> 路 <a href="./README.ko.md">頃滉淡鞏?/a> 路 **Espa帽ol** 路 <a href="./README.fr.md">Fran莽ais</a> 路 <a href="./README.de.md">Deutsch</a> 路 <a href="./README.pt.md">Portugu锚s</a> 路 <a href="./README.ru.md">袪褍褋褋泻懈泄</a> 路 <a href="./README.ar.md">丕賱毓乇亘賷丞</a>
</p>

---

## 鉁?驴Qu茅 es WhichClaw?

WhichClaw es una aplicaci贸n de escritorio que proporciona una **interfaz visual y unificada** para gestionar modelos de IA en tus herramientas de programaci贸n. Sin m谩s archivos de configuraci贸n 鈥?solo haz clic y cambia.

### El Problema

- 馃槴 Cambiar modelos de IA en herramientas como OpenClaw requiere editar archivos de configuraci贸n manualmente
- 馃攧 Cada herramienta tiene su propio formato de configuraci贸n de modelos
- 馃З No hay forma f谩cil de gestionar habilidades y extensiones entre herramientas

### La Soluci贸n

WhichClaw act煤a como un **panel de control central** para todas tus herramientas de programaci贸n con IA:

- 馃幆 **Cambio de Modelo con Un Clic** 鈥?Cambia visualmente modelos de IA para cualquier herramienta compatible
- 馃攢 **Protocolo Dual** 鈥?Soporte OpenAI y Anthropic API, cambia modelos en cualquier momento
- 馃殗 **Proxy T煤nel Inteligente** 鈥?Accede a APIs con restricci贸n geogr谩fica sin VPN completa; solo se proxifica el tr谩fico API
- 馃З **Explorador de Habilidades** 鈥?Descubre, instala y gestiona habilidades de IA
- 馃枼锔?**Servidor de Modelos Local** 鈥?Ejecuta modelos de c贸digo abierto (Qwen, DeepSeek, Llama) localmente v铆a llama.cpp
- 馃實 **28 Idiomas** 鈥?Soporte completo de internacionalizaci贸n
- 馃幃 **Apps de IA Integradas** 鈥?Juegos y utilidades de IA interactivos como Reversi y AI Translate
- 馃寖 **UI Hacker Cyberpunk** 鈥?Est茅tica de terminal ne贸n verde que hace que programar se sienta futurista

## 馃柤锔?Capturas de pantalla

### Model Nexus 鈥?Gestiona todos tus modelos de IA en un solo lugar
![Model Nexus](1.png)

### App Manager 鈥?Cambio de modelo con un clic para todas las herramientas
![App Manager](2.png)

### Local Server 鈥?Ejecuta modelos de c贸digo abierto localmente con llama.cpp
![Local Server](3.png)

### Skill Browser 鈥?Descubre e instala habilidades de IA
![Skill Browser](4.png)

## 馃殌 Inicio R谩pido

### Descargar

Obt茅n la 煤ltima versi贸n para tu plataforma:

| Plataforma | Descargar |
|----------|----------|
| Windows  | [WhichClaw-Setup.exe](https://github.com/ebenxp707-boop/WhichClaw/releases/latest) |
| macOS    | [WhichClaw.dmg](https://github.com/ebenxp707-boop/WhichClaw/releases/latest) |
| Linux    | [WhichClaw.AppImage](https://github.com/ebenxp707-boop/WhichClaw/releases/latest) |

### Notas de Linux

```bash
chmod +x WhichClaw-*.AppImage
./WhichClaw-*.AppImage
```

> Si encuentras errores de FUSE: `sudo apt install libfuse2`

## 馃敡 Herramientas Compatibles

| Herramienta | Estado | Cambio de Modelo | Protocolo |
|------|--------|----------------|----------|
| OpenClaw | 鉁?Compatible | 鉁?| OpenAI / Anthropic |
| Claude Code | 鉁?Compatible | 鉁?| Anthropic |
| Cline | 鉁?Compatible | 鉁?| OpenAI |
| Continue | 鉁?Compatible | 鉁?| OpenAI |
| OpenCode | 鉁?Compatible | 鉁?| OpenAI |
| Codex | 鉁?Compatible | 鉁?| OpenAI |
| Roo Code | 鉁?Compatible | 鉁?| OpenAI |

## 馃彈锔?Stack Tecnol贸gico

- **Electron** 鈥?Framework de escritorio multiplataforma
- **React + TypeScript** 鈥?Framework de UI
- **Vanilla CSS** 鈥?Sistema de dise帽o cyberpunk personalizado
- **Vite** 鈥?Herramienta de compilaci贸n
- **llama.cpp** 鈥?Motor de inferencia de modelos local

## 馃洜锔?Desarrollo

```bash
npm install
npm run dev
npm run build
```

## 馃 Contribuir

隆Las contribuciones son bienvenidas! No dudes en abrir issues o enviar pull requests.

We're especially looking for help with:
- 馃崕 **Pruebas en macOS** 鈥?A煤n no hemos probado completamente las builds de macOS
- 馃敡 **Nuevas integraciones** 鈥?Ay煤danos a agregar soporte para m谩s herramientas de IA
- 馃寪 **Mejoras de traducci贸n** 鈥?隆Hablantes nativos bienvenidos!

1. Fork the repository
2. Create your feature branch (`git checkout -b feature/amazing-feature`)
3. Commit your changes (`git commit -m 'feat: add amazing feature'`)
4. Push to the branch (`git push origin feature/amazing-feature`)
5. Open a Pull Request

## 馃摤 Contact

- 馃摟 Email: [hi@whichclaw.com](mailto:hi@whichclaw.com)
- 馃悰 Bug Reports: [GitHub Issues](https://github.com/ebenxp707-boop/WhichClaw/issues)
- 馃挰 Discussions: [GitHub Discussions](https://github.com/ebenxp707-boop/WhichClaw/discussions)

## 猸?Apoyo

Si WhichClaw te resulta 煤til, considera darle una 猸?en GitHub 鈥?隆ayuda a que otros descubran el proyecto!

## 馃搫 Licencia

[MIT](../LICENSE)

---

<p align="center">
  Hecho con 馃挌 por el equipo de WhichClaw<br/>
  <sub>馃摟 <a href="mailto:hi@whichclaw.com">hi@whichclaw.com</a></sub>
</p>
