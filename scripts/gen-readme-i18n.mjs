import fs from 'fs';
import path from 'path';

const docsDir = 'd:/CyberNexus/docs';

const translations = {
    'zh-CN': {
        lang: '简体中文',
        whatIs: '✨ CyberNexus 是什么？',
        desc: 'CyberNexus 是一款桌面应用，为你的 AI 编程工具提供**可视化、统一的模型管理界面**。无需再手动翻配置文件 —— 点一下，就能切换。',
        problem: '痛点',
        p1: '😫 在 OpenClaw 等工具中切换模型需要手动编辑配置文件',
        p2: '🔄 每个工具都有自己的模型配置格式',
        p3: '🧩 没有方便的方式管理技能和扩展',
        solution: '解决方案',
        solutionDesc: 'CyberNexus 是你所有 AI 编程工具的**中央控制面板**：',
        f1: '🎯 **一键切换模型** — 可视化切换任何支持工具的 AI 模型',
        f2: '🔀 **双协议支持** — OpenAI & Anthropic API 支持，随时随地切换模型',
        f3: '🚇 **智能隧道代理** — 无需全局 VPN 即可访问受限 API，仅代理 API 流量',
        f4: '🧩 **技能浏览器** — 发现、安装和管理 AI 技能',
        f5: '🖥️ **本地模型服务器** — 通过 llama.cpp 本地运行开源模型（Qwen、DeepSeek、Llama）',
        f6: '🌍 **28 种语言** — 完整国际化支持',
        f7: '🎮 **内置 AI 应用** — 交互式 AI 游戏和工具，如 Reversi 和 AI 翻译',
        f8: '🌃 **赛博朋克 UI** — 炫酷的霓虹绿终端美学，让编程充满未来感',
        screenshots: '🖼️ 截图',
        ss1: 'Model Nexus — 在一处管理所有 AI 模型',
        ss2: 'App Manager — 一键为所有编程工具切换模型',
        ss3: 'Local Server — 通过 llama.cpp 本地运行开源模型',
        ss4: 'Skill Browser — 发现和安装 AI 技能',
        quickStart: '🚀 快速开始',
        download: '下载',
        downloadDesc: '获取适合你平台的最新版本：',
        platform: '平台',
        linuxNotes: 'Linux 说明',
        fuseNote: '如果遇到 FUSE 错误：',
        supportedTools: '🔧 支持的工具',
        tool: '工具', status: '状态', modelSwitching: '模型切换', protocol: '协议',
        supported: '✅ 已支持',
        techStack: '🏗️ 技术栈',
        t1: '**Electron** — 跨平台桌面框架',
        t2: '**React + TypeScript** — UI 框架',
        t3: '**Vanilla CSS** — 自定义赛博朋克设计系统',
        t4: '**Vite** — 构建工具',
        t5: '**llama.cpp** — 本地模型推理引擎',
        dev: '🛠️ 开发',
        contributing: '🤝 贡献',
        contribDesc: '欢迎贡献！随时提交 Issue 或 Pull Request。',
        contribHelp: 'We\'re especially looking for help with:\n- 🍎 **macOS 测试** — 我们还没有完全测试 macOS 构建\n- 🔧 **新工具集成** — 帮助我们支持更多 AI 编程工具\n- 🌐 **翻译改进** — 欢迎母语使用者！',
        support: '⭐ 支持',
        supportDesc: '如果 CyberNexus 对你有帮助，请在 GitHub 上给个 ⭐ —— 让更多人发现这个项目！',
        license: '📄 许可证',
        madeWith: '由 CyberNexus 团队用 💚 打造'
    },
    'zh-TW': {
        lang: '繁體中文',
        whatIs: '✨ CyberNexus 是什麼？',
        desc: 'CyberNexus 是一款桌面應用，為你的 AI 程式設計工具提供**視覺化、統一的模型管理介面**。不再需要手動翻設定檔 —— 點一下，就能切換。',
        problem: '痛點',
        p1: '😫 在 OpenClaw 等工具中切換模型需要手動編輯設定檔',
        p2: '🔄 每個工具都有自己的模型設定格式',
        p3: '🧩 沒有方便的方式管理技能和擴充',
        solution: '解決方案',
        solutionDesc: 'CyberNexus 是你所有 AI 程式設計工具的**中央控制面板**：',
        f1: '🎯 **一鍵切換模型** — 視覺化切換任何支援工具的 AI 模型',
        f2: '🔀 **雙協議支援** — OpenAI & Anthropic API 支援，隨時隨地切換模型',
        f3: '🚇 **智慧隧道代理** — 無需全域 VPN 即可存取受限 API，僅代理 API 流量',
        f4: '🧩 **技能瀏覽器** — 發現、安裝和管理 AI 技能',
        f5: '🖥️ **本地模型伺服器** — 透過 llama.cpp 本地執行開源模型（Qwen、DeepSeek、Llama）',
        f6: '🌍 **28 種語言** — 完整國際化支援',
        f7: '🎮 **內建 AI 應用** — 互動式 AI 遊戲和工具，如 Reversi 和 AI 翻譯',
        f8: '🌃 **賽博龐克 UI** — 炫酷的霓虹綠終端美學，讓程式設計充滿未來感',
        screenshots: '🖼️ 截圖',
        ss1: 'Model Nexus — 在一處管理所有 AI 模型',
        ss2: 'App Manager — 一鍵為所有程式設計工具切換模型',
        ss3: 'Local Server — 透過 llama.cpp 本地執行開源模型',
        ss4: 'Skill Browser — 發現和安裝 AI 技能',
        quickStart: '🚀 快速開始',
        download: '下載',
        downloadDesc: '取得適合你平台的最新版本：',
        platform: '平台',
        linuxNotes: 'Linux 說明',
        fuseNote: '如果遇到 FUSE 錯誤：',
        supportedTools: '🔧 支援的工具',
        tool: '工具', status: '狀態', modelSwitching: '模型切換', protocol: '協議',
        supported: '✅ 已支援',
        techStack: '🏗️ 技術棧',
        t1: '**Electron** — 跨平台桌面框架',
        t2: '**React + TypeScript** — UI 框架',
        t3: '**Vanilla CSS** — 自訂賽博龐克設計系統',
        t4: '**Vite** — 建置工具',
        t5: '**llama.cpp** — 本地模型推理引擎',
        dev: '🛠️ 開發',
        contributing: '🤝 貢獻',
        contribDesc: '歡迎貢獻！隨時提交 Issue 或 Pull Request。',
        contribHelp: 'We\'re especially looking for help with:\n- 🍎 **macOS 測試** — 我們還沒有完全測試 macOS 構建\n- 🔧 **新工具整合** — 幫助我們支援更多 AI 程式設計工具\n- 🌐 **翻譯改進** — 歡迎母語使用者！',
        support: '⭐ 支持',
        supportDesc: '如果 CyberNexus 對你有幫助，請在 GitHub 上給個 ⭐ —— 讓更多人發現這個專案！',
        license: '📄 授權條款',
        madeWith: '由 CyberNexus 團隊用 💚 打造'
    },
    'ja': {
        lang: '日本語',
        whatIs: '✨ CyberNexus とは？',
        desc: 'CyberNexus は、AIコーディングツール全体でモデルを管理するための**ビジュアルで統一されたインターフェース**を提供するデスクトップアプリです。設定ファイルを掘り返す必要はもうありません — クリックするだけで切り替え。',
        problem: '課題',
        p1: '😫 OpenClaw などのツールでAIモデルを切り替えるには設定ファイルの手動編集が必要',
        p2: '🔄 各ツールが独自のモデル設定形式を持っている',
        p3: '🧩 ツール間でスキルや拡張機能を管理する簡単な方法がない',
        solution: 'ソリューション',
        solutionDesc: 'CyberNexus はすべてのAIコーディングツールの**中央コントロールパネル**として機能します：',
        f1: '🎯 **ワンクリックモデル切替** — 対応ツールのAIモデルをビジュアルに切り替え',
        f2: '🔀 **デュアルプロトコル** — OpenAI & Anthropic API対応、いつでもどこでもモデル切替',
        f3: '🚇 **スマートトンネルプロキシ** — フルVPNなしで地域制限APIにアクセス、APIトラフィックのみをプロキシ',
        f4: '🧩 **スキルブラウザ** — AIスキルを発見、インストール、管理',
        f5: '🖥️ **ローカルモデルサーバー** — llama.cpp経由でオープンソースモデル（Qwen、DeepSeek、Llama）をローカル実行',
        f6: '🌍 **28言語対応** — グローバル対応の完全国際化',
        f7: '🎮 **内蔵AIアプリ** — Reversi やAI翻訳などのインタラクティブなAIゲームとユーティリティ',
        f8: '🌃 **サイバーパンク UI** — ネオングリーンのターミナル美学で近未来的コーディング体験',
        screenshots: '🖼️ スクリーンショット',
        ss1: 'Model Nexus — すべてのAIモデルを一箇所で管理',
        ss2: 'App Manager — すべてのコーディングツールをワンクリックでモデル切替',
        ss3: 'Local Server — llama.cppでオープンソースモデルをローカル実行',
        ss4: 'Skill Browser — AIスキルを発見・インストール',
        quickStart: '🚀 クイックスタート',
        download: 'ダウンロード',
        downloadDesc: 'お使いのプラットフォーム向けの最新リリースを入手：',
        platform: 'プラットフォーム',
        linuxNotes: 'Linux の注意事項',
        fuseNote: 'FUSE エラーが発生した場合：',
        supportedTools: '🔧 対応ツール',
        tool: 'ツール', status: 'ステータス', modelSwitching: 'モデル切替', protocol: 'プロトコル',
        supported: '✅ 対応済み',
        techStack: '🏗️ 技術スタック',
        t1: '**Electron** — クロスプラットフォームデスクトップフレームワーク',
        t2: '**React + TypeScript** — UIフレームワーク',
        t3: '**Vanilla CSS** — カスタムサイバーパンクデザインシステム',
        t4: '**Vite** — ビルドツール',
        t5: '**llama.cpp** — ローカルモデル推論エンジン',
        dev: '🛠️ 開発',
        contributing: '🤝 コントリビュート',
        contribDesc: 'コントリビュート大歓迎！Issue や Pull Request をお気軽にどうぞ。',
        contribHelp: 'We\'re especially looking for help with:\n- 🍎 **macOSテスト** — macOSビルドのテストがまだ完了していません\n- 🔧 **新ツール統合** — より多くのAIコーディングツールのサポート追加にご協力ください\n- 🌐 **翻訳改善** — ネイティブスピーカー歓迎！',
        support: '⭐ サポート',
        supportDesc: 'CyberNexus が役立ったら、GitHub で ⭐ をお願いします — プロジェクトの発見に繋がります！',
        license: '📄 ライセンス',
        madeWith: 'CyberNexus チームが 💚 を込めて制作'
    },
    'ko': {
        lang: '한국어',
        whatIs: '✨ CyberNexus란?',
        desc: 'CyberNexus는 AI 코딩 도구 전반에 걸쳐 모델을 관리하기 위한 **시각적이고 통합된 인터페이스**를 제공하는 데스크톱 애플리케이션입니다. 설정 파일을 뒤질 필요 없이 — 클릭 한 번으로 전환하세요.',
        problem: '문제점',
        p1: '😫 OpenClaw 같은 도구에서 AI 모델을 전환하려면 설정 파일을 수동으로 편집해야 함',
        p2: '🔄 각 도구마다 고유한 모델 구성 형식이 있음',
        p3: '🧩 도구 간 스킬과 확장 기능을 관리할 편리한 방법이 없음',
        solution: '솔루션',
        solutionDesc: 'CyberNexus는 모든 AI 코딩 도구의 **중앙 제어 패널** 역할을 합니다:',
        f1: '🎯 **원클릭 모델 전환** — 지원 도구의 AI 모델을 시각적으로 전환',
        f2: '🔀 **듀얼 프로토콜** — OpenAI & Anthropic API 지원, 언제 어디서나 모델 전환',
        f3: '🚇 **스마트 터널 프록시** — VPN 없이 지역 제한 API에 접근, API 트래픽만 프록시',
        f4: '🧩 **스킬 브라우저** — AI 스킬 검색, 설치, 관리',
        f5: '🖥️ **로컬 모델 서버** — llama.cpp로 오픈소스 모델(Qwen, DeepSeek, Llama) 로컬 실행',
        f6: '🌍 **28개 언어** — 글로벌 대응 완전 국제화',
        f7: '🎮 **내장 AI 앱** — Reversi, AI 번역 등 인터랙티브 AI 게임 및 유틸리티',
        f8: '🌃 **사이버펑크 UI** — 네온 그린 터미널 미학으로 미래지향적 코딩 경험',
        screenshots: '🖼️ 스크린샷',
        ss1: 'Model Nexus — 한 곳에서 모든 AI 모델 관리',
        ss2: 'App Manager — 모든 코딩 도구 원클릭 모델 전환',
        ss3: 'Local Server — llama.cpp로 오픈소스 모델 로컬 실행',
        ss4: 'Skill Browser — AI 스킬 검색 및 설치',
        quickStart: '🚀 빠른 시작',
        download: '다운로드',
        downloadDesc: '플랫폼에 맞는 최신 릴리스를 받으세요:',
        platform: '플랫폼',
        linuxNotes: 'Linux 참고사항',
        fuseNote: 'FUSE 오류가 발생하면:',
        supportedTools: '🔧 지원 도구',
        tool: '도구', status: '상태', modelSwitching: '모델 전환', protocol: '프로토콜',
        supported: '✅ 지원',
        techStack: '🏗️ 기술 스택',
        t1: '**Electron** — 크로스 플랫폼 데스크톱 프레임워크',
        t2: '**React + TypeScript** — UI 프레임워크',
        t3: '**Vanilla CSS** — 커스텀 사이버펑크 디자인 시스템',
        t4: '**Vite** — 빌드 도구',
        t5: '**llama.cpp** — 로컬 모델 추론 엔진',
        dev: '🛠️ 개발',
        contributing: '🤝 기여',
        contribDesc: '기여를 환영합니다! 이슈나 풀 리퀘스트를 자유롭게 제출해 주세요.',
        contribHelp: 'We\'re especially looking for help with:\n- 🍎 **macOS 테스트** — macOS 빌드를 아직 완전히 테스트하지 못했습니다\n- 🔧 **새로운 도구 통합** — 더 많은 AI 코딩 도구 지원 추가에 도움을 주세요\n- 🌐 **번역 개선** — 원어민 환영!',
        support: '⭐ 지원',
        supportDesc: 'CyberNexus가 유용하다면, GitHub에서 ⭐를 눌러주세요 — 더 많은 사람들이 프로젝트를 발견하는 데 도움이 됩니다!',
        license: '📄 라이선스',
        madeWith: 'CyberNexus 팀이 💚으로 제작'
    },
    'es': {
        lang: 'Español',
        whatIs: '✨ ¿Qué es CyberNexus?',
        desc: 'CyberNexus es una aplicación de escritorio que proporciona una **interfaz visual y unificada** para gestionar modelos de IA en tus herramientas de programación. Sin más archivos de configuración — solo haz clic y cambia.',
        problem: 'El Problema',
        p1: '😫 Cambiar modelos de IA en herramientas como OpenClaw requiere editar archivos de configuración manualmente',
        p2: '🔄 Cada herramienta tiene su propio formato de configuración de modelos',
        p3: '🧩 No hay forma fácil de gestionar habilidades y extensiones entre herramientas',
        solution: 'La Solución',
        solutionDesc: 'CyberNexus actúa como un **panel de control central** para todas tus herramientas de programación con IA:',
        f1: '🎯 **Cambio de Modelo con Un Clic** — Cambia visualmente modelos de IA para cualquier herramienta compatible',
        f2: '🔀 **Protocolo Dual** — Soporte OpenAI y Anthropic API, cambia modelos en cualquier momento',
        f3: '🚇 **Proxy Túnel Inteligente** — Accede a APIs con restricción geográfica sin VPN completa; solo se proxifica el tráfico API',
        f4: '🧩 **Explorador de Habilidades** — Descubre, instala y gestiona habilidades de IA',
        f5: '🖥️ **Servidor de Modelos Local** — Ejecuta modelos de código abierto (Qwen, DeepSeek, Llama) localmente vía llama.cpp',
        f6: '🌍 **28 Idiomas** — Soporte completo de internacionalización',
        f7: '🎮 **Apps de IA Integradas** — Juegos y utilidades de IA interactivos como Reversi y AI Translate',
        f8: '🌃 **UI Cyberpunk** — Estética de terminal neón verde que hace que programar se sienta futurista',
        screenshots: '🖼️ Capturas de pantalla',
        ss1: 'Model Nexus — Gestiona todos tus modelos de IA en un solo lugar',
        ss2: 'App Manager — Cambio de modelo con un clic para todas las herramientas',
        ss3: 'Local Server — Ejecuta modelos de código abierto localmente con llama.cpp',
        ss4: 'Skill Browser — Descubre e instala habilidades de IA',
        quickStart: '🚀 Inicio Rápido',
        download: 'Descargar',
        downloadDesc: 'Obtén la última versión para tu plataforma:',
        platform: 'Plataforma',
        linuxNotes: 'Notas de Linux',
        fuseNote: 'Si encuentras errores de FUSE:',
        supportedTools: '🔧 Herramientas Compatibles',
        tool: 'Herramienta', status: 'Estado', modelSwitching: 'Cambio de Modelo', protocol: 'Protocolo',
        supported: '✅ Compatible',
        techStack: '🏗️ Stack Tecnológico',
        t1: '**Electron** — Framework de escritorio multiplataforma',
        t2: '**React + TypeScript** — Framework de UI',
        t3: '**Vanilla CSS** — Sistema de diseño cyberpunk personalizado',
        t4: '**Vite** — Herramienta de compilación',
        t5: '**llama.cpp** — Motor de inferencia de modelos local',
        dev: '🛠️ Desarrollo',
        contributing: '🤝 Contribuir',
        contribDesc: '¡Las contribuciones son bienvenidas! No dudes en abrir issues o enviar pull requests.',
        contribHelp: 'We\'re especially looking for help with:\n- 🍎 **Pruebas en macOS** — Aún no hemos probado completamente las builds de macOS\n- 🔧 **Nuevas integraciones** — Ayúdanos a agregar soporte para más herramientas de IA\n- 🌐 **Mejoras de traducción** — ¡Hablantes nativos bienvenidos!',
        support: '⭐ Apoyo',
        supportDesc: 'Si CyberNexus te resulta útil, considera darle una ⭐ en GitHub — ¡ayuda a que otros descubran el proyecto!',
        license: '📄 Licencia',
        madeWith: 'Hecho con 💚 por el equipo de CyberNexus'
    },
    'fr': {
        lang: 'Français',
        whatIs: '✨ Qu\'est-ce que CyberNexus ?',
        desc: 'CyberNexus est une application de bureau qui fournit une **interface visuelle et unifiée** pour gérer les modèles d\'IA à travers vos outils de développement. Plus besoin de fouiller dans les fichiers de configuration — cliquez et basculez.',
        problem: 'Le Problème',
        p1: '😫 Changer de modèle IA dans des outils comme OpenClaw nécessite d\'éditer manuellement les fichiers de configuration',
        p2: '🔄 Chaque outil a son propre format de configuration de modèles',
        p3: '🧩 Pas de moyen facile de gérer les compétences et extensions entre les outils',
        solution: 'La Solution',
        solutionDesc: 'CyberNexus agit comme un **panneau de contrôle central** pour tous vos outils de développement IA :',
        f1: '🎯 **Changement de Modèle en Un Clic** — Basculez visuellement les modèles IA pour n\'importe quel outil compatible',
        f2: '🔀 **Double Protocole** — Support OpenAI et Anthropic API, changez de modèle à tout moment',
        f3: '🚇 **Proxy Tunnel Intelligent** — Accédez aux APIs géo-restreintes sans VPN complet ; seul le trafic API est proxifié',
        f4: '🧩 **Navigateur de Compétences** — Découvrez, installez et gérez des compétences IA',
        f5: '🖥️ **Serveur de Modèles Local** — Exécutez des modèles open-source (Qwen, DeepSeek, Llama) localement via llama.cpp',
        f6: '🌍 **28 Langues** — Support complet d\'internationalisation',
        f7: '🎮 **Apps IA Intégrées** — Jeux et utilitaires IA interactifs comme Reversi et AI Translate',
        f8: '🌃 **UI Cyberpunk** — Esthétique terminale néon vert qui rend le développement futuriste',
        screenshots: '🖼️ Captures d\'écran',
        ss1: 'Model Nexus — Gérez tous vos modèles IA en un seul endroit',
        ss2: 'App Manager — Changement de modèle en un clic pour tous les outils',
        ss3: 'Local Server — Exécutez des modèles open-source localement avec llama.cpp',
        ss4: 'Skill Browser — Découvrez et installez des compétences IA',
        quickStart: '🚀 Démarrage Rapide',
        download: 'Télécharger',
        downloadDesc: 'Obtenez la dernière version pour votre plateforme :',
        platform: 'Plateforme',
        linuxNotes: 'Notes Linux',
        fuseNote: 'Si vous rencontrez des erreurs FUSE :',
        supportedTools: '🔧 Outils Compatibles',
        tool: 'Outil', status: 'Statut', modelSwitching: 'Changement de Modèle', protocol: 'Protocole',
        supported: '✅ Compatible',
        techStack: '🏗️ Stack Technique',
        t1: '**Electron** — Framework de bureau multiplateforme',
        t2: '**React + TypeScript** — Framework UI',
        t3: '**Vanilla CSS** — Système de design cyberpunk personnalisé',
        t4: '**Vite** — Outil de build',
        t5: '**llama.cpp** — Moteur d\'inférence de modèles local',
        dev: '🛠️ Développement',
        contributing: '🤝 Contribuer',
        contribDesc: 'Les contributions sont les bienvenues ! N\'hésitez pas à ouvrir des issues ou soumettre des pull requests.',
        contribHelp: 'We\'re especially looking for help with:\n- 🍎 **Tests macOS** — Nous n\'avons pas encore entièrement testé les builds macOS\n- 🔧 **Nouvelles intégrations** — Aidez-nous à ajouter le support de plus d\'outils IA\n- 🌐 **Améliorations des traductions** — Locuteurs natifs bienvenus !',
        support: '⭐ Soutien',
        supportDesc: 'Si CyberNexus vous est utile, pensez à lui donner une ⭐ sur GitHub — cela aide les autres à découvrir le projet !',
        license: '📄 Licence',
        madeWith: 'Fait avec 💚 par l\'équipe CyberNexus'
    },
    'de': {
        lang: 'Deutsch',
        whatIs: '✨ Was ist CyberNexus?',
        desc: 'CyberNexus ist eine Desktop-Anwendung, die eine **visuelle, einheitliche Oberfläche** zur Verwaltung von KI-Modellen in deinen Programmier-Tools bietet. Kein Durchsuchen von Konfigurationsdateien mehr — einfach klicken und wechseln.',
        problem: 'Das Problem',
        p1: '😫 Das Wechseln von KI-Modellen in Tools wie OpenClaw erfordert manuelles Bearbeiten von Konfigurationsdateien',
        p2: '🔄 Jedes Tool hat sein eigenes Modell-Konfigurationsformat',
        p3: '🧩 Keine einfache Möglichkeit, Skills und Erweiterungen über Tools hinweg zu verwalten',
        solution: 'Die Lösung',
        solutionDesc: 'CyberNexus fungiert als **zentrale Steuerungszentrale** für alle deine KI-Programmier-Tools:',
        f1: '🎯 **Ein-Klick Modellwechsel** — Visuell KI-Modelle für jedes unterstützte Tool wechseln',
        f2: '🔀 **Dual-Protokoll** — OpenAI & Anthropic API-Unterstützung, jederzeit und überall Modelle wechseln',
        f3: '🚇 **Intelligenter Tunnel-Proxy** — Zugriff auf geo-beschränkte APIs ohne vollständiges VPN; nur API-Traffic wird proxied',
        f4: '🧩 **Skill-Browser** — KI-Skills entdecken, installieren und verwalten',
        f5: '🖥️ **Lokaler Modell-Server** — Open-Source-Modelle (Qwen, DeepSeek, Llama) lokal über llama.cpp ausführen',
        f6: '🌍 **28 Sprachen** — Vollständige Internationalisierung',
        f7: '🎮 **Integrierte KI-Apps** — Interaktive KI-Spiele und Werkzeuge wie Reversi und AI Translate',
        f8: '🌃 **Cyberpunk-UI** — Atemberaubende neongrüne Terminal-Ästhetik für futuristisches Programmieren',
        screenshots: '🖼️ Screenshots',
        ss1: 'Model Nexus — Alle KI-Modelle an einem Ort verwalten',
        ss2: 'App Manager — Ein-Klick Modellwechsel für alle Coding-Tools',
        ss3: 'Local Server — Open-Source-Modelle lokal mit llama.cpp ausführen',
        ss4: 'Skill Browser — KI-Skills entdecken und installieren',
        quickStart: '🚀 Schnellstart',
        download: 'Download',
        downloadDesc: 'Hol dir die neueste Version für deine Plattform:',
        platform: 'Plattform',
        linuxNotes: 'Linux-Hinweise',
        fuseNote: 'Bei FUSE-Fehlern:',
        supportedTools: '🔧 Unterstützte Tools',
        tool: 'Tool', status: 'Status', modelSwitching: 'Modellwechsel', protocol: 'Protokoll',
        supported: '✅ Unterstützt',
        techStack: '🏗️ Tech-Stack',
        t1: '**Electron** — Plattformübergreifendes Desktop-Framework',
        t2: '**React + TypeScript** — UI-Framework',
        t3: '**Vanilla CSS** — Benutzerdefiniertes Cyberpunk-Designsystem',
        t4: '**Vite** — Build-Tool',
        t5: '**llama.cpp** — Lokale Modell-Inferenz-Engine',
        dev: '🛠️ Entwicklung',
        contributing: '🤝 Mitwirken',
        contribDesc: 'Beiträge sind willkommen! Erstelle gerne Issues oder sende Pull Requests.',
        contribHelp: 'We\'re especially looking for help with:\n- 🍎 **macOS-Tests** — Wir haben die macOS-Builds noch nicht vollständig getestet\n- 🔧 **Neue Tool-Integrationen** — Hilf uns, mehr KI-Tools zu unterstützen\n- 🌐 **Übersetzungsverbesserungen** — Muttersprachler willkommen!',
        support: '⭐ Unterstützung',
        supportDesc: 'Wenn du CyberNexus nützlich findest, gib bitte einen ⭐ auf GitHub — das hilft anderen, das Projekt zu entdecken!',
        license: '📄 Lizenz',
        madeWith: 'Mit 💚 vom CyberNexus Team erstellt'
    },
    'pt': {
        lang: 'Português',
        whatIs: '✨ O que é CyberNexus?',
        desc: 'CyberNexus é um aplicativo de desktop que fornece uma **interface visual e unificada** para gerenciar modelos de IA nas suas ferramentas de programação. Sem mais edição de arquivos de configuração — apenas clique e troque.',
        problem: 'O Problema',
        p1: '😫 Trocar modelos de IA em ferramentas como OpenClaw requer edição manual de arquivos de configuração',
        p2: '🔄 Cada ferramenta tem seu próprio formato de configuração de modelos',
        p3: '🧩 Sem forma fácil de gerenciar habilidades e extensões entre ferramentas',
        solution: 'A Solução',
        solutionDesc: 'CyberNexus atua como um **painel de controle central** para todas as suas ferramentas de programação com IA:',
        f1: '🎯 **Troca de Modelo com Um Clique** — Troque visualmente modelos de IA para qualquer ferramenta compatível',
        f2: '🔀 **Protocolo Duplo** — Suporte OpenAI e Anthropic API, troque modelos a qualquer momento',
        f3: '🚇 **Proxy Túnel Inteligente** — Acesse APIs com restrição geográfica sem VPN completa; apenas o tráfego API é proxificado',
        f4: '🧩 **Navegador de Habilidades** — Descubra, instale e gerencie habilidades de IA',
        f5: '🖥️ **Servidor de Modelos Local** — Execute modelos de código aberto (Qwen, DeepSeek, Llama) localmente via llama.cpp',
        f6: '🌍 **28 Idiomas** — Suporte completo de internacionalização',
        f7: '🎮 **Apps de IA Integrados** — Jogos e utilitários de IA interativos como Reversi e AI Translate',
        f8: '🌃 **UI Cyberpunk** — Estética de terminal neon verde que faz programar parecer futurista',
        screenshots: '🖼️ Capturas de tela',
        ss1: 'Model Nexus — Gerencie todos os seus modelos de IA em um só lugar',
        ss2: 'App Manager — Troca de modelo com um clique para todas as ferramentas',
        ss3: 'Local Server — Execute modelos de código aberto localmente com llama.cpp',
        ss4: 'Skill Browser — Descubra e instale habilidades de IA',
        quickStart: '🚀 Início Rápido',
        download: 'Download',
        downloadDesc: 'Obtenha a versão mais recente para sua plataforma:',
        platform: 'Plataforma',
        linuxNotes: 'Notas do Linux',
        fuseNote: 'Se encontrar erros de FUSE:',
        supportedTools: '🔧 Ferramentas Compatíveis',
        tool: 'Ferramenta', status: 'Status', modelSwitching: 'Troca de Modelo', protocol: 'Protocolo',
        supported: '✅ Compatível',
        techStack: '🏗️ Stack Tecnológico',
        t1: '**Electron** — Framework de desktop multiplataforma',
        t2: '**React + TypeScript** — Framework de UI',
        t3: '**Vanilla CSS** — Sistema de design cyberpunk personalizado',
        t4: '**Vite** — Ferramenta de build',
        t5: '**llama.cpp** — Motor de inferência de modelos local',
        dev: '🛠️ Desenvolvimento',
        contributing: '🤝 Contribuir',
        contribDesc: 'Contribuições são bem-vindas! Fique à vontade para abrir issues ou enviar pull requests.',
        contribHelp: 'We\'re especially looking for help with:\n- 🍎 **Testes no macOS** — Ainda não testamos completamente as builds do macOS\n- 🔧 **Novas integrações** — Ajude-nos a adicionar suporte para mais ferramentas de IA\n- 🌐 **Melhorias de tradução** — Falantes nativos são bem-vindos!',
        support: '⭐ Apoio',
        supportDesc: 'Se CyberNexus é útil para você, considere dar uma ⭐ no GitHub — ajuda outros a descobrirem o projeto!',
        license: '📄 Licença',
        madeWith: 'Feito com 💚 pela equipe CyberNexus'
    },
    'ru': {
        lang: 'Русский',
        whatIs: '✨ Что такое CyberNexus?',
        desc: 'CyberNexus — это настольное приложение, предоставляющее **визуальный, унифицированный интерфейс** для управления ИИ-моделями во всех ваших инструментах разработки. Больше не нужно копаться в конфигурационных файлах — просто нажмите и переключите.',
        problem: 'Проблема',
        p1: '😫 Переключение ИИ-моделей в инструментах вроде OpenClaw требует ручного редактирования конфигурационных файлов',
        p2: '🔄 У каждого инструмента свой формат конфигурации моделей',
        p3: '🧩 Нет удобного способа управлять навыками и расширениями между инструментами',
        solution: 'Решение',
        solutionDesc: 'CyberNexus выступает **центральной панелью управления** для всех ваших ИИ-инструментов разработки:',
        f1: '🎯 **Переключение Модели в Один Клик** — Визуально переключайте ИИ-модели для любого поддерживаемого инструмента',
        f2: '🔀 **Двойной Протокол** — Поддержка OpenAI и Anthropic API, переключение моделей в любое время',
        f3: '🚇 **Умный Туннельный Прокси** — Доступ к гео-ограниченным API без полного VPN; проксируется только API-трафик',
        f4: '🧩 **Браузер Навыков** — Находите, устанавливайте и управляйте ИИ-навыками',
        f5: '🖥️ **Локальный Сервер Моделей** — Запускайте open-source модели (Qwen, DeepSeek, Llama) локально через llama.cpp',
        f6: '🌍 **28 Языков** — Полная поддержка интернационализации',
        f7: '🎮 **Встроенные ИИ-Приложения** — Интерактивные ИИ-игры и утилиты, такие как Reversi и AI Translate',
        f8: '🌃 **Киберпанк UI** — Потрясающая неоново-зелёная эстетика терминала для футуристического кодинга',
        screenshots: '🖼️ Скриншоты',
        ss1: 'Model Nexus — Управляйте всеми ИИ-моделями в одном месте',
        ss2: 'App Manager — Переключение модели в один клик для всех инструментов',
        ss3: 'Local Server — Запускайте open-source модели локально с llama.cpp',
        ss4: 'Skill Browser — Находите и устанавливайте ИИ-навыки',
        quickStart: '🚀 Быстрый Старт',
        download: 'Скачать',
        downloadDesc: 'Получите последний релиз для вашей платформы:',
        platform: 'Платформа',
        linuxNotes: 'Примечания для Linux',
        fuseNote: 'При ошибках FUSE:',
        supportedTools: '🔧 Поддерживаемые Инструменты',
        tool: 'Инструмент', status: 'Статус', modelSwitching: 'Смена Модели', protocol: 'Протокол',
        supported: '✅ Поддерживается',
        techStack: '🏗️ Технологический Стек',
        t1: '**Electron** — Кроссплатформенный десктопный фреймворк',
        t2: '**React + TypeScript** — UI фреймворк',
        t3: '**Vanilla CSS** — Пользовательская дизайн-система в стиле киберпанк',
        t4: '**Vite** — Инструмент сборки',
        t5: '**llama.cpp** — Локальный движок инференса моделей',
        dev: '🛠️ Разработка',
        contributing: '🤝 Вклад',
        contribDesc: 'Вклады приветствуются! Не стесняйтесь открывать issues или отправлять pull requests.',
        contribHelp: 'We\'re especially looking for help with:\n- 🍎 **Тестирование macOS** — Мы ещё не полностью протестировали сборки macOS\n- 🔧 **Новые интеграции** — Помогите добавить поддержку большего количества ИИ-инструментов\n- 🌐 **Улучшение переводов** — Приглашаем носителей языка!',
        support: '⭐ Поддержка',
        supportDesc: 'Если CyberNexus вам полезен, поставьте ⭐ на GitHub — это помогает другим найти проект!',
        license: '📄 Лицензия',
        madeWith: 'Сделано с 💚 командой CyberNexus'
    },
    'ar': {
        lang: 'العربية',
        whatIs: '✨ ما هو CyberNexus؟',
        desc: 'CyberNexus هو تطبيق سطح مكتب يوفر **واجهة مرئية وموحدة** لإدارة نماذج الذكاء الاصطناعي عبر أدوات البرمجة الخاصة بك. لا مزيد من البحث في ملفات التكوين — فقط انقر وبدّل.',
        problem: 'المشكلة',
        p1: '😫 تبديل نماذج الذكاء الاصطناعي في أدوات مثل OpenClaw يتطلب تعديل ملفات التكوين يدوياً',
        p2: '🔄 كل أداة لها تنسيق تكوين النماذج الخاص بها',
        p3: '🧩 لا توجد طريقة سهلة لإدارة المهارات والإضافات عبر الأدوات',
        solution: 'الحل',
        solutionDesc: 'CyberNexus يعمل كـ **لوحة تحكم مركزية** لجميع أدوات البرمجة بالذكاء الاصطناعي:',
        f1: '🎯 **تبديل النموذج بنقرة واحدة** — بدّل نماذج الذكاء الاصطناعي بصرياً لأي أداة مدعومة',
        f2: '🔀 **بروتوكول مزدوج** — دعم OpenAI و Anthropic API، بدّل النماذج في أي وقت',
        f3: '🚇 **وكيل نفق ذكي** — الوصول إلى واجهات برمجة التطبيقات المحظورة جغرافياً بدون VPN كامل',
        f4: '🧩 **متصفح المهارات** — اكتشف وثبّت وأدر مهارات الذكاء الاصطناعي',
        f5: '🖥️ **خادم النماذج المحلي** — شغّل نماذج مفتوحة المصدر (Qwen، DeepSeek، Llama) محلياً عبر llama.cpp',
        f6: '🌍 **28 لغة** — دعم كامل للتدويل',
        f7: '🎮 **تطبيقات ذكاء اصطناعي مدمجة** — ألعاب وأدوات تفاعلية مثل Reversi و AI Translate',
        f8: '🌃 **واجهة هاكر سايبربانك** — جمالية طرفية بالنيون الأخضر تجعل البرمجة مستقبلية',
        screenshots: '🖼️ لقطات الشاشة',
        ss1: 'Model Nexus — أدر جميع نماذج الذكاء الاصطناعي في مكان واحد',
        ss2: 'App Manager — تبديل النموذج بنقرة واحدة لجميع الأدوات',
        ss3: 'Local Server — شغّل نماذج مفتوحة المصدر محلياً مع llama.cpp',
        ss4: 'Skill Browser — اكتشف وثبّت مهارات الذكاء الاصطناعي',
        quickStart: '🚀 البداية السريعة',
        download: 'تحميل',
        downloadDesc: 'احصل على أحدث إصدار لمنصتك:',
        platform: 'المنصة',
        linuxNotes: 'ملاحظات Linux',
        fuseNote: 'إذا واجهت أخطاء FUSE:',
        supportedTools: '🔧 الأدوات المدعومة',
        tool: 'الأداة', status: 'الحالة', modelSwitching: 'تبديل النموذج', protocol: 'البروتوكول',
        supported: '✅ مدعوم',
        techStack: '🏗️ المكدس التقني',
        t1: '**Electron** — إطار عمل سطح مكتب متعدد المنصات',
        t2: '**React + TypeScript** — إطار عمل واجهة المستخدم',
        t3: '**Vanilla CSS** — نظام تصميم سايبربانك مخصص',
        t4: '**Vite** — أداة البناء',
        t5: '**llama.cpp** — محرك استدلال النماذج المحلي',
        dev: '🛠️ التطوير',
        contributing: '🤝 المساهمة',
        contribDesc: 'المساهمات مرحب بها! لا تتردد في فتح issues أو إرسال pull requests.',
        contribHelp: 'We\'re especially looking for help with:\n- 🍎 **اختبار macOS** — لم نختبر بناء macOS بالكامل بعد\n- 🔧 **تكاملات جديدة** — ساعدنا في إضافة دعم لمزيد من أدوات الذكاء الاصطناعي\n- 🌐 **تحسين الترجمات** — المتحدثون الأصليون مرحب بهم!',
        support: '⭐ الدعم',
        supportDesc: 'إذا وجدت CyberNexus مفيداً، فكّر في إعطائه ⭐ على GitHub — يساعد الآخرين على اكتشاف المشروع!',
        license: '📄 الرخصة',
        madeWith: 'صنع بـ 💚 من فريق CyberNexus'
    }
};

// 所有语言列表（用于语言切换链接）
const allLangs = { en: 'English', ...Object.fromEntries(Object.entries(translations).map(([k, v]) => [k, v.lang])) };

function buildLangLinks(currentLang) {
    const links = Object.entries(allLangs).map(([code, name]) => {
        const href = code === 'en' ? '../README.md' : `./README.${code}.md`;
        if (code === currentLang) return `**${name}**`;
        return `<a href="${href}">${name}</a>`;
    });
    return links.join(' · ');
}

function generateReadme(lang, t) {
    const langLinks = buildLangLinks(lang);
    return `<p align="center">
  <img src="../build/icon.png" alt="CyberNexus" width="120" />
</p>

<h1 align="center">CyberNexus</h1>

<p align="center">
  <strong>The Nexus for Models, Agents & Vibe Coding.</strong><br/>
  <sub>${t.desc.split('**')[0].trim()}</sub>
</p>

<p align="center">
  <a href="https://github.com/CyberNexus-Chat/CyberNexus/releases">
    <img src="https://img.shields.io/github/v/release/CyberNexus-Chat/CyberNexus?style=flat-square&color=00FF9D" alt="Release" />
  </a>
  <img src="https://img.shields.io/badge/platform-Windows%20%7C%20macOS%20%7C%20Linux-blue?style=flat-square" alt="Platform" />
  <img src="https://img.shields.io/github/license/CyberNexus-Chat/CyberNexus?style=flat-square" alt="License" />
</p>

<p align="center">
  ${langLinks}
</p>

---

## ${t.whatIs}

${t.desc}

### ${t.problem}

- ${t.p1}
- ${t.p2}
- ${t.p3}

### ${t.solution}

${t.solutionDesc}

- ${t.f1}
- ${t.f2}
- ${t.f3}
- ${t.f4}
- ${t.f5}
- ${t.f6}
- ${t.f7}
- ${t.f8}

## ${t.screenshots}

### ${t.ss1}
![Model Nexus](1.png)

### ${t.ss2}
![App Manager](2.png)

### ${t.ss3}
![Local Server](3.png)

### ${t.ss4}
![Skill Browser](4.png)

## ${t.quickStart}

### ${t.download}

${t.downloadDesc}

| ${t.platform} | ${t.download} |
|----------|----------|
| Windows  | [CyberNexus-Setup.exe](https://github.com/CyberNexus-Chat/CyberNexus/releases/latest) |
| macOS    | [CyberNexus.dmg](https://github.com/CyberNexus-Chat/CyberNexus/releases/latest) |
| Linux    | [CyberNexus.AppImage](https://github.com/CyberNexus-Chat/CyberNexus/releases/latest) |

### ${t.linuxNotes}

\`\`\`bash
chmod +x CyberNexus-*.AppImage
./CyberNexus-*.AppImage
\`\`\`

> ${t.fuseNote} \`sudo apt install libfuse2\`

## ${t.supportedTools}

| ${t.tool} | ${t.status} | ${t.modelSwitching} | ${t.protocol} |
|------|--------|----------------|----------|
| OpenClaw | ${t.supported} | ✅ | OpenAI / Anthropic |
| Claude Code | ${t.supported} | ✅ | Anthropic |
| Cline | ${t.supported} | ✅ | OpenAI |
| Continue | ${t.supported} | ✅ | OpenAI |
| OpenCode | ${t.supported} | ✅ | OpenAI |
| Codex | ${t.supported} | ✅ | OpenAI |
| Roo Code | ${t.supported} | ✅ | OpenAI |

## ${t.techStack}

- ${t.t1}
- ${t.t2}
- ${t.t3}
- ${t.t4}
- ${t.t5}

## ${t.dev}

\`\`\`bash
npm install
npm run dev
npm run build
\`\`\`

## ${t.contributing}

${t.contribDesc}

${t.contribHelp || ''}

1. Fork the repository
2. Create your feature branch (\`git checkout -b feature/amazing-feature\`)
3. Commit your changes (\`git commit -m 'feat: add amazing feature'\`)
4. Push to the branch (\`git push origin feature/amazing-feature\`)
5. Open a Pull Request

## 📬 Contact

- 📧 Email: [hi@cybernexus.chat](mailto:hi@cybernexus.chat)
- 🐛 Bug Reports: [GitHub Issues](https://github.com/CyberNexus-Chat/CyberNexus/issues)
- 💬 Discussions: [GitHub Discussions](https://github.com/CyberNexus-Chat/CyberNexus/discussions)

## ${t.support}

${t.supportDesc}

## ${t.license}

[MIT](../LICENSE)

---

<p align="center">
  ${t.madeWith}<br/>
  <sub>📧 <a href="mailto:hi@cybernexus.chat">hi@cybernexus.chat</a></sub>
</p>
`;
}

// 生成所有文件
let count = 0;
for (const [lang, t] of Object.entries(translations)) {
    const filePath = path.join(docsDir, `README.${lang}.md`);
    const content = generateReadme(lang, t);
    fs.writeFileSync(filePath, content, 'utf-8');
    console.log(`OK: README.${lang}.md`);
    count++;
}
console.log(`\nGenerated ${count} README files`);
