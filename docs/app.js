(() => {
  // ===== 1. Starfield Canvas (inspired by ui/stars.slint) =====
  const canvas = document.getElementById("starfield");
  const ctx = canvas.getContext("2d");
  const reducedMotion = window.matchMedia("(prefers-reduced-motion: reduce)").matches;

  const stars = [];
  const STAR_COUNT = 115;

  function hash(i) {
    const x = Math.sin(i * 127.1 + 311.7) * 43758.5453;
    return x - Math.floor(x);
  }

  for (let i = 0; i < STAR_COUNT; i++) {
    const isHero = i >= STAR_COUNT - 6;
    stars.push({
      x: hash(i + 0.3),
      y: hash(i + 7.7),
      r: isHero ? 1.5 + hash(i + 67.1) * 0.9 : 0.6 + hash(i + 19.3) * 1.1,
      alpha: isHero ? 0.45 + 0.35 * hash(i + 67.1) : 0.14 + 0.42 * hash(i + 19.3),
      phase: hash(i + 41.7) * Math.PI * 2,
      speed: 0.4 + hash(i + 53.9) * 0.8,
      hero: isHero,
    });
  }

  function resizeCanvas() {
    const dpr = Math.min(window.devicePixelRatio || 1, 2);
    canvas.width = window.innerWidth * dpr;
    canvas.height = window.innerHeight * dpr;
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
  }

  function drawStars(time) {
    const w = window.innerWidth;
    const h = window.innerHeight;
    ctx.clearRect(0, 0, w, h);

    for (const s of stars) {
      const twinkle = reducedMotion ? 1 : 0.72 + 0.28 * Math.sin(time * 0.0012 * s.speed + s.phase);
      const a = s.alpha * twinkle;
      const sx = s.x * w;
      const sy = s.y * h;

      if (s.hero) {
        const isLav = document.documentElement.dataset.theme === "lavender";
        ctx.shadowBlur = 8;
        ctx.shadowColor = isLav ? "rgba(150, 166, 255, 0.55)" : "rgba(255, 255, 255, 0.55)";
        ctx.fillStyle = `rgba(255, 255, 255, ${a.toFixed(3)})`;
      } else {
        ctx.shadowBlur = 0;
        ctx.fillStyle = `rgba(255, 255, 255, ${a.toFixed(3)})`;
      }
      ctx.beginPath();
      ctx.arc(sx, sy, s.r, 0, Math.PI * 2);
      ctx.fill();
    }
    ctx.shadowBlur = 0;

    if (!reducedMotion) {
      requestAnimationFrame(drawStars);
    }
  }

  resizeCanvas();
  window.addEventListener("resize", () => {
    resizeCanvas();
    if (reducedMotion) drawStars(0);
  });
  if (reducedMotion) {
    drawStars(0);
  } else {
    requestAnimationFrame(drawStars);
  }

  // ===== 2. Translations (RU / EN) =====
  const i18n = {
    ru: {
      nav_preview: "Интерфейс",
      nav_features: "Функции",
      nav_servers: "Сервера",
      nav_download: "Скачать",
      nav_faq: "FAQ",
      btn_download_short: "Скачать",
      hero_badge: "Клиент готов · Rust + Slint · GPL-3.0",
      hero_title: 'ByteStrap<br /><span class="hero-subline">Тёмный лаунчер для Roblox</span>',
      hero_desc: "Нативный клиент на Rust с GPU-интерфейсом, редактором FastFlags, честным TCP-пингом серверов, менеджером модов, внутрисерверным чатом и цветовым оверлеем.",
      hero_cta_download: "Скачать для Windows",
      hero_cta_github: "GitHub",
      metric_stack: "Нативный стек без Electron",
      metric_threads: "Потоков загрузки клиента",
      metric_channels: "Поддержка каналов Roblox",
      tab_home: "Главная",
      tab_games: "Игры",
      tab_flags: "FastFlags",
      tab_mods: "Моды",
      tab_settings: "Настройки",
      shot_desc_home: "Быстрый старт клиента и Roblox Studio, запуск по ссылке или JobId, 14-дневный график активности и учёт времени по плейсам.",
      shot_desc_games: "Список недавних плейсов, поиск публичных серверов по регионам (EU / US / ASIA), замер TCP-пинга дата-центров и вход по JobId.",
      shot_desc_flags: "Визуальные пресеты (лимит FPS, MSAA, Vulkan / D3D11, зум камеры) и таблица FastFlags с индикатором allowlist и импортом JSON.",
      shot_desc_mods: "Управление папками модов с порядком загрузки и откатом, кастомный звук смерти (ouch.ogg), курсоры и шрифты интерфейса.",
      shot_desc_settings: "Выбор канала (LIVE / BETA / ZNext), 1–8 потоков загрузки, Discord Rich Presence, Anti-AFK, разрешение 4:3 и цветовой оверлей.",
      feat_eyebrow: "ВОЗМОЖНОСТИ",
      feat_title: "Всё нужное для Roblox — в одном окне",
      feat_sub: "Без телеметрии, рекламы и фоновых служб. Каждый инструмент работает локально и прозрачно.",
      f1_title: "FastFlags и умные пресеты",
      f1_desc: "Удобные выпадающие списки для лимита FPS, MSAA, API рендера (Vulkan / D3D11), дальности травы, зума камеры и отключения пост-эффектов. Встроенный редактор предупреждает о флагах вне официального allowlist Roblox, поддерживает импорт из буфера и профили.",
      f2_title: "Менеджер модов и кастомизация",
      f2_desc: "Включайте папки модов и меняйте порядок приоритета в пару кликов с безопасным откатом. Заменяйте звук смерти (ouch.ogg), курсоры (2006 / 2013 / Crosshair), шрифты и Shift-Lock.",
      f3_title: "Внутрисерверный чат",
      f3_desc: "Общайтесь с другими пользователями ByteStrap, находясь на одном сервере Roblox, через лёгкий WebSocket relay без нагрузки на игру.",
      f4_title: "Оверлей и окно 4:3",
      f4_desc: "Цветокоррекция поверх окна Roblox (яркость, насыщенность, контраст через Magnification API), фиксированное разрешение (включая 4:3 stretched) и Anti-AFK.",
      f5_title: "Галерея скриншотов и Discord RP",
      f5_desc: "Просмотр снимков Roblox с копированием картинки в буфер (Ctrl+V в Discord), Rich Presence с регионом сервера, режимом инкогнито и статистикой сессии.",
      f6_title: "Производительность нативного Rust",
      f6_desc: "Отрисовка на Slint (FemtoVG) с нулевой перерисовкой в простое, минимальным потреблением памяти и 1–8 потоками при загрузке обновлений клиента. Встроенная проверка целостности хешей версий, просмотр логов и авто-очистка кэша.",
      srv_label: "СЕРВЕР-БРАУЗЕР",
      srv_title: "Честный TCP-пинг до дата-центров",
      srv_ping_btn: "Проверить пинг",
      srv_desc: "Фильтруйте публичные серверы по региону (EU / US / ASIA), копируйте JobId или заходите на лучший сервер одним нажатием.",
      reg_all: "Все",
      ff_label: "FASTFLAGS · ПРЕСЕТЫ",
      ff_title: "Соберите конфиг за 3 клика",
      ff_allow: "Allowlist OK",
      ff_desc: "Переключайте готовые пресеты лаунчера и сразу смотрите итоговый JSON для ClientAppSettings.",
      ff_p1_name: "Разблокировка FPS (240)",
      ff_p2_name: "Рендер Vulkan",
      ff_p3_name: "Без пост-эффектов (Bloom)",
      ff_p4_name: "Без DPI-скейла Windows",
      dl_title: "Готовы попробовать ByteStrap?",
      dl_desc: "Один компактный бинарник для Windows 10/11 (x64). Открытый исходный код под лицензией GPL-3.0 — вы всегда можете проверить сборку на GitHub.",
      dl_btn: "Скачать ByteStrap.exe",
      dl_notes_title: "ЧТО НОВОГО В РЕЛИЗЕ",
      dl_notes_open: "Открыть на GitHub →",
      dl_notes_loading: "Загрузка списка изменений с GitHub…",
      dl_notes_empty: "Подробности релиза доступны на странице GitHub Releases.",
      dl_notes_commits_lbl: "Изменения в релизе:",
      dl_all_releases: "Все релизы на GitHub",
      dl_telegram: "Канал в Telegram",
      faq_title: "Частые вопросы",
      faq_q1: "Безопасно ли использовать ByteStrap?",
      faq_a1: "Да. ByteStrap не вмешивается в память процесса Roblox и не является читом — это кастомный bootstrapper-лаунчер на Rust. Весь исходный код открыт на GitHub под лицензией GPL-3.0.",
      faq_q2: "Почему часть FastFlags помечена предупреждением?",
      faq_a2: "Roblox игнорирует клиентские флаги, которые не входят в официальный allowlist. ByteStrap честно подсвечивает такие флаги жёлтым индикатором, чтобы вы знали, какие параметры реально применяются клиентом.",
      faq_q3: "Как работают моды и можно ли всё откатить?",
      faq_a3: "Каждая папка в каталоге Mods/ отображается в лаунчере отдельным модом. Вы задаёте порядок применения, включаете нужные паки и в любой момент можете нажать «Откатить», чтобы вернуть оригинальные файлы Roblox.",
      faq_q4: "Что нужно для работы на Windows?",
      faq_a4: "Windows 10 или Windows 11 (x64). Никаких дополнительных рантаймов вроде .NET или WebView2 устанавливать не требуется — интерфейс работает нативно через Slint и OpenGL/FemtoVG.",
    },
    en: {
      nav_preview: "Interface",
      nav_features: "Features",
      nav_servers: "Servers",
      nav_download: "Download",
      nav_faq: "FAQ",
      btn_download_short: "Download",
      hero_badge: "Client Ready · Rust + Slint · GPL-3.0",
      hero_title: 'ByteStrap<br /><span class="hero-subline">Dark Roblox Launcher</span>',
      hero_desc: "Native Rust launcher with GPU UI, FastFlags editor, honest TCP datacenter ping, mod manager, in-server chat, and a real-time color overlay.",
      hero_cta_download: "Download for Windows",
      hero_cta_github: "GitHub",
      metric_stack: "Native stack, zero Electron",
      metric_threads: "Parallel download threads",
      metric_channels: "Roblox release channels",
      tab_home: "Home",
      tab_games: "Games",
      tab_flags: "FastFlags",
      tab_mods: "Mods",
      tab_settings: "Settings",
      shot_desc_home: "One-click launch for Roblox and Studio, deep-link / JobId join, 14-day activity chart, and per-place playtime tracking.",
      shot_desc_games: "Recent places history, public server browser filtered by EU / US / ASIA, TCP datacenter ping tests, and direct JobId join.",
      shot_desc_flags: "Visual presets (FPS cap, MSAA, Vulkan / D3D11, camera zoom) plus FastFlags table with allowlist warnings and JSON import.",
      shot_desc_mods: "Priority-ordered mod folders with one-click Apply / Revert, custom death sound (ouch.ogg), cursors, and UI fonts.",
      shot_desc_settings: "Channel switcher (LIVE / BETA / ZNext), 1–8 download threads, Discord Rich Presence, Anti-AFK, 4:3 window sizing, and color overlay.",
      feat_eyebrow: "CAPABILITIES",
      feat_title: "Everything you need for Roblox — in one window",
      feat_sub: "No telemetry, no ads, no background bloat. Every tool runs locally and transparently.",
      f1_title: "FastFlags & Smart Presets",
      f1_desc: "Clean dropdown presets for FPS limit, MSAA, renderer API (Vulkan / D3D11), grass distance, camera zoom, and post-FX. Built-in editor flags non-allowlisted keys, supports clipboard import, and saves profiles.",
      f2_title: "Mod Manager & Customization",
      f2_desc: "Toggle mod folders and reorder priority in seconds with clean rollback. Swap death sounds (ouch.ogg), cursor styles (2006 / 2013 / Crosshair), UI fonts, and Shift-Lock.",
      f3_title: "In-Server Chat",
      f3_desc: "Chat with other ByteStrap players on the same Roblox server via a lightweight WebSocket relay with zero impact on game FPS.",
      f4_title: "Color Overlay & 4:3 Window",
      f4_desc: "Real-time brightness, saturation, and contrast overlay via Windows Magnification API, fixed window resolution (including 4:3 stretched), and Anti-AFK.",
      f5_title: "Screenshot Gallery & Discord RP",
      f5_desc: "Browse Roblox captures and copy straight to clipboard for Discord (Ctrl+V), plus Rich Presence with server region, incognito mode, and session timer.",
      f6_title: "Native Rust Performance",
      f6_desc: "Rendered with Slint (FemtoVG) with zero idle redraws, minimal RAM footprint, and 1–8 parallel download threads. Includes version hash verification, log viewer, and auto cache cleanup.",
      srv_label: "SERVER BROWSER",
      srv_title: "Honest TCP ping to datacenters",
      srv_ping_btn: "Ping servers",
      srv_desc: "Filter public servers by region (EU / US / ASIA), copy JobId, or join the lowest-ping server in one click.",
      reg_all: "All",
      ff_label: "FASTFLAGS · PRESETS",
      ff_title: "Build a config in 3 clicks",
      ff_allow: "Allowlist OK",
      ff_desc: "Toggle built-in presets and inspect the generated ClientAppSettings JSON live.",
      ff_p1_name: "Unlock FPS (240)",
      ff_p2_name: "Vulkan Renderer",
      ff_p3_name: "Disable Post-FX (Bloom)",
      ff_p4_name: "Disable Windows DPI Scale",
      dl_title: "Ready to try ByteStrap?",
      dl_desc: "A single compact binary for Windows 10/11 (x64). Open source under GPL-3.0 — inspect every line on GitHub.",
      dl_btn: "Download ByteStrap.exe",
      dl_notes_title: "RELEASE NOTES",
      dl_notes_open: "View on GitHub →",
      dl_notes_loading: "Loading release notes from GitHub…",
      dl_notes_empty: "Full release details are available on GitHub Releases.",
      dl_notes_commits_lbl: "Changes in this release:",
      dl_all_releases: "All GitHub Releases",
      dl_telegram: "Telegram Channel",
      faq_title: "Frequently Asked Questions",
      faq_q1: "Is ByteStrap safe to use?",
      faq_a1: "Yes. ByteStrap never injects into or modifies Roblox process memory — it is a native bootstrapper launcher written in Rust and fully open-source under GPL-3.0.",
      faq_q2: "Why do some FastFlags show a warning?",
      faq_a2: "Roblox ignores client flags outside its official allowlist. ByteStrap highlights non-allowlisted flags with a yellow warning so you always know what actually applies.",
      faq_q3: "How do mods work and can I revert them?",
      faq_a3: "Every folder in Mods/ becomes an independent toggleable mod with custom load priority. Click Revert at any time to restore clean Roblox client files.",
      faq_q4: "What does it require on Windows?",
      faq_a4: "Windows 10 or Windows 11 (x64). No extra runtimes like .NET or WebView2 are needed — the UI renders natively via Slint and FemtoVG.",
    },
  };

  let currentLang = "ru";
  let currentShot = "home";
  let cachedReleaseState = null;

  const shotMeta = {
    home: { src: "assets/shots/home.png", tag: "01 · HOME", key: "shot_desc_home" },
    games: { src: "assets/shots/games.png", tag: "02 · GAMES & SERVERS", key: "shot_desc_games" },
    flags: { src: "assets/shots/flags.png", tag: "03 · FASTFLAGS", key: "shot_desc_flags" },
    mods: { src: "assets/shots/mods.png", tag: "04 · MODS & CUSTOMS", key: "shot_desc_mods" },
    settings: { src: "assets/shots/settings.png", tag: "05 · SETTINGS", key: "shot_desc_settings" },
  };

  function applyLanguage(lang) {
    currentLang = lang;
    document.documentElement.lang = lang;
    const dict = i18n[lang];

    document.querySelectorAll("[data-i18n]").forEach((el) => {
      const k = el.getAttribute("data-i18n");
      if (!dict[k]) return;
      if (k === "hero_title") {
        el.innerHTML = dict[k];
      } else {
        el.textContent = dict[k];
      }
    });

    document.querySelectorAll(".lang-opt").forEach((opt) => {
      opt.classList.toggle("active", opt.dataset.lang === lang);
    });

    updateShotDescription();
    if (typeof renderLatestRelease === "function" && cachedReleaseState) {
      renderLatestRelease(cachedReleaseState);
    }
  }

  function updateShotDescription() {
    const meta = shotMeta[currentShot];
    const descEl = document.getElementById("shot-desc");
    if (meta && descEl) {
      descEl.textContent = i18n[currentLang][meta.key];
    }
  }

  document.getElementById("lang-toggle").addEventListener("click", () => {
    applyLanguage(currentLang === "ru" ? "en" : "ru");
  });

  // ===== 2b. Theme Switcher (EditingNews Mono / Saved Lavender) =====
  const themeToggle = document.getElementById("theme-toggle");
  function applyTheme(theme) {
    document.documentElement.dataset.theme = theme;
    localStorage.setItem("bs_site_theme", theme);
    document.querySelectorAll(".theme-opt").forEach((opt) => {
      opt.classList.toggle("active", opt.dataset.themeOpt === theme);
    });
    if (reducedMotion) drawStars(0);
  }
  const savedTheme = localStorage.getItem("bs_site_theme");
  if (savedTheme === "editingnews" || savedTheme === "lavender") {
    applyTheme(savedTheme);
  }
  if (themeToggle) {
    themeToggle.addEventListener("click", () => {
      const next = document.documentElement.dataset.theme === "lavender" ? "editingnews" : "lavender";
      applyTheme(next);
    });
  }

  // ===== 3. Interactive Launcher Showcase Tabs =====
  const shotImg = document.getElementById("shot-img");
  const shotTag = document.getElementById("shot-tag");
  const frameTabs = document.querySelectorAll(".frame-tab");

  // Preload screenshots for instant switching
  Object.values(shotMeta).forEach((m) => {
    const img = new Image();
    img.src = m.src;
  });

  frameTabs.forEach((btn) => {
    btn.addEventListener("click", () => {
      const target = btn.dataset.shot;
      if (!shotMeta[target] || target === currentShot) return;

      currentShot = target;
      frameTabs.forEach((b) => {
        const active = b.dataset.shot === target;
        b.classList.toggle("active", active);
        b.setAttribute("aria-selected", String(active));
      });

      shotImg.classList.add("switching");
      setTimeout(() => {
        shotImg.src = shotMeta[target].src;
        shotTag.textContent = shotMeta[target].tag;
        updateShotDescription();
        shotImg.classList.remove("switching");
      }, 110);
    });
  });

  // ===== 4. Interactive Server Browser Preview =====
  const servers = [
    { region: "EU", loc: "EU: Frankfurt", players: "38/50", ping: 24, job: "7a9c…4f12" },
    { region: "EU", loc: "EU: Amsterdam", players: "46/50", ping: 31, job: "3e11…8b09" },
    { region: "US", loc: "US: Ashburn", players: "12/50", ping: 78, job: "91d0…2c4a" },
    { region: "ASIA", loc: "ASIA: Singapore", players: "29/50", ping: 118, job: "5b42…9e71" },
  ];

  let activeRegion = "ALL";
  const serverListEl = document.getElementById("server-list");

  function renderServers() {
    const filtered = activeRegion === "ALL" ? servers : servers.filter((s) => s.region === activeRegion);
    serverListEl.innerHTML = filtered
      .map((s) => {
        const cls = s.ping <= 45 ? "ping-good" : s.ping <= 90 ? "ping-warn" : "ping-mid";
        return `
          <div class="server-row">
            <div class="server-left">
              <span class="ping-pill ${cls}">${s.ping} ms</span>
              <span class="server-loc">${s.loc}</span>
            </div>
            <div class="server-right">
              <span class="server-players mono">${s.players}</span>
              <span class="job-tag mono">${s.job}</span>
            </div>
          </div>
        `;
      })
      .join("");
  }

  document.querySelectorAll(".filter-pill").forEach((pill) => {
    pill.addEventListener("click", () => {
      activeRegion = pill.dataset.region;
      document.querySelectorAll(".filter-pill").forEach((p) => p.classList.toggle("active", p === pill));
      renderServers();
    });
  });

  document.getElementById("ping-refresh-btn").addEventListener("click", () => {
    servers.forEach((s) => {
      const base = s.region === "EU" ? 26 : s.region === "US" ? 79 : 116;
      s.ping = Math.max(14, base + Math.round((Math.random() - 0.5) * 10));
    });
    renderServers();
  });

  renderServers();

  // ===== 5. Interactive FastFlags Builder =====
  const switches = document.querySelectorAll(".bs-switch");
  const jsonPreview = document.getElementById("ff-json-preview");

  function renderFlagsJson() {
    const obj = {};
    switches.forEach((sw) => {
      if (sw.checked) {
        const raw = sw.dataset.val;
        obj[sw.dataset.flag] = raw === "true" ? true : Number(raw);
      }
    });
    jsonPreview.textContent = JSON.stringify(obj, null, 2);
  }

  switches.forEach((sw) => sw.addEventListener("change", renderFlagsJson));
  renderFlagsJson();

  // ===== 6. Fetch Latest GitHub Release + Notes Automatically =====
  const RELEASE_CACHE_KEY = "bs_latest_release_v1";
  const RELEASE_CACHE_TTL_MS = 10 * 60 * 1000;

  function escapeHtml(str) {
    return String(str)
      .replace(/&/g, "&amp;")
      .replace(/</g, "&lt;")
      .replace(/>/g, "&gt;")
      .replace(/"/g, "&quot;")
      .replace(/'/g, "&#39;");
  }

  function formatInlineMarkdown(escapedText) {
    let out = escapedText
      .replace(/`([^`]+)`/g, "<code>$1</code>")
      .replace(/\*\*([^*]+)\*\*/g, "<strong>$1</strong>")
      .replace(/\[([^\]]+)\]\((https?:\/\/[^\s)]+)\)/g, '<a href="$2" target="_blank" rel="noopener">$1</a>');

    // Convert bare GitHub URLs not already inside an anchor tag
    out = out.replace(/(^|[\s(>])(https?:\/\/[^\s<)]+)/g, (_, prefix, url) => {
      const shortMatch = url.match(/github\.com\/[^/]+\/[^/]+\/compare\/([^\s]+)/);
      const label = shortMatch ? shortMatch[1] : url;
      return `${prefix}<a href="${url}" target="_blank" rel="noopener">${label}</a>`;
    });
    return out;
  }

  function renderMarkdownToHtml(md) {
    const lines = String(md || "").replace(/\r\n/g, "\n").split("\n");
    const chunks = [];
    let inList = false;

    for (const rawLine of lines) {
      const line = rawLine.trim();
      if (!line) {
        if (inList) {
          chunks.push("</ul>");
          inList = false;
        }
        continue;
      }

      if (/^#{1,4}\s+/.test(line)) {
        if (inList) {
          chunks.push("</ul>");
          inList = false;
        }
        const heading = line.replace(/^#{1,4}\s+/, "");
        chunks.push(`<h4>${formatInlineMarkdown(escapeHtml(heading))}</h4>`);
        continue;
      }

      if (/^[-*•]\s+/.test(line)) {
        if (!inList) {
          chunks.push("<ul>");
          inList = true;
        }
        const item = line.replace(/^[-*•]\s+/, "");
        chunks.push(`<li>${formatInlineMarkdown(escapeHtml(item))}</li>`);
        continue;
      }

      if (inList) {
        chunks.push("</ul>");
        inList = false;
      }
      chunks.push(`<p>${formatInlineMarkdown(escapeHtml(line))}</p>`);
    }

    if (inList) chunks.push("</ul>");
    return chunks.join("");
  }

  function hasSubstantiveReleaseBody(body) {
    if (!body) return false;
    const stripped = body
      .replace(/\*\*Full Changelog\*\*:.*$/gim, "")
      .replace(/Full Changelog:.*$/gim, "")
      .trim();
    return stripped.length > 0;
  }

  function renderLatestRelease(state) {
    if (!state || !state.release) return;
    cachedReleaseState = state;
    const rel = state.release;
    const tag = rel.tag_name.startsWith("v") ? rel.tag_name : `v${rel.tag_name}`;

    const navVer = document.getElementById("nav-version");
    const relTag = document.getElementById("release-tag");
    const relLink = document.getElementById("release-html-link");
    if (navVer) navVer.textContent = tag;
    if (relTag) relTag.textContent = `ByteStrap ${tag} · ${rel.prerelease ? "Pre-release" : "Latest"}`;
    if (relLink && rel.html_url) relLink.href = rel.html_url;

    const exeAsset = Array.isArray(rel.assets)
      ? rel.assets.find((a) => /\.exe$/i.test(a.name))
      : null;

    const dlBtn = document.getElementById("direct-download-btn");
    const heroBtn = document.getElementById("hero-download-btn");
    const dlLabel = document.getElementById("direct-download-label");

    if (exeAsset) {
      const sizeMb = (exeAsset.size / (1024 * 1024)).toFixed(1);
      const sizeEl = document.getElementById("release-size");
      if (sizeEl) sizeEl.textContent = `${sizeMb} MB`;
      if (dlBtn) dlBtn.href = exeAsset.browser_download_url;
      if (heroBtn) heroBtn.href = exeAsset.browser_download_url;
      if (dlLabel) {
        dlLabel.textContent = currentLang === "ru" ? `Скачать ${tag} (.exe)` : `Download ${tag} (.exe)`;
      }

      const dlCountEl = document.getElementById("release-downloads");
      const dlSepEl = document.getElementById("release-dl-sep");
      if (dlCountEl && dlSepEl && typeof exeAsset.download_count === "number" && exeAsset.download_count > 0) {
        dlCountEl.textContent = currentLang === "ru"
          ? `${exeAsset.download_count} скач.`
          : `${exeAsset.download_count} downloads`;
        dlCountEl.hidden = false;
        dlSepEl.hidden = false;
      }
    } else if (rel.html_url) {
      if (dlBtn) dlBtn.href = rel.html_url;
      if (heroBtn) heroBtn.href = rel.html_url;
    }

    if (rel.published_at) {
      const dateEl = document.getElementById("release-date");
      const dateSepEl = document.getElementById("release-date-sep");
      if (dateEl && dateSepEl) {
        const dt = new Date(rel.published_at);
        if (!Number.isNaN(dt.getTime())) {
          dateEl.textContent = dt.toLocaleDateString(currentLang === "ru" ? "ru-RU" : "en-US", {
            year: "numeric",
            month: "short",
            day: "numeric",
          });
          dateEl.hidden = false;
          dateSepEl.hidden = false;
        }
      }
    }

    const notesBody = document.getElementById("release-notes-body");
    if (!notesBody) return;

    const bodyText = (rel.body || "").trim();
    const parts = [];

    if (hasSubstantiveReleaseBody(bodyText)) {
      parts.push(renderMarkdownToHtml(bodyText));
    } else {
      if (Array.isArray(state.commits) && state.commits.length > 0) {
        parts.push(`<h4>${escapeHtml(i18n[currentLang].dl_notes_commits_lbl)}</h4>`);
        const items = state.commits
          .map((c) => `<li><span class="commit-sha">${escapeHtml(c.sha)}</span>${escapeHtml(c.msg)}</li>`)
          .join("");
        parts.push(`<ul>${items}</ul>`);
      }
      if (bodyText) {
        parts.push(renderMarkdownToHtml(bodyText));
      }
    }

    notesBody.innerHTML = parts.length > 0
      ? parts.join("")
      : `<p>${escapeHtml(i18n[currentLang].dl_notes_empty)}</p>`;
  }

  async function loadLatestRelease() {
    try {
      const rawCache = localStorage.getItem(RELEASE_CACHE_KEY);
      if (rawCache) {
        const parsed = JSON.parse(rawCache);
        if (parsed && parsed.ts && Date.now() - parsed.ts < RELEASE_CACHE_TTL_MS && parsed.state) {
          renderLatestRelease(parsed.state);
          return;
        }
      }
    } catch (_) {
      // Ignore storage errors
    }

    try {
      const res = await fetch("https://api.github.com/repos/mesorg-dev/ByteStrap/releases?per_page=2");
      if (!res.ok) throw new Error(`HTTP ${res.status}`);
      const releases = await res.json();
      if (!Array.isArray(releases) || releases.length === 0) return;

      const latest = releases.find((r) => !r.draft) || releases[0];
      const prev = releases.find((r) => r.tag_name !== latest.tag_name && !r.draft);
      let commits = [];

      if (!hasSubstantiveReleaseBody(latest.body)) {
        try {
          if (prev && prev.tag_name) {
            const cmpRes = await fetch(
              `https://api.github.com/repos/mesorg-dev/ByteStrap/compare/${encodeURIComponent(prev.tag_name)}...${encodeURIComponent(latest.tag_name)}`
            );
            if (cmpRes.ok) {
              const cmpData = await cmpRes.json();
              if (Array.isArray(cmpData.commits)) {
                commits = cmpData.commits
                  .slice(-8)
                  .reverse()
                  .map((c) => ({
                    sha: (c.sha || "").slice(0, 7),
                    msg: ((c.commit && c.commit.message) || "").split("\n")[0],
                  }))
                  .filter((c) => c.msg);
              }
            }
          }
        } catch (_) {
          // Compare fallback is optional
        }
      }

      const state = { release: latest, commits };
      renderLatestRelease(state);

      try {
        localStorage.setItem(RELEASE_CACHE_KEY, JSON.stringify({ ts: Date.now(), state }));
      } catch (_) {
        // Ignore quota errors
      }
    } catch (_) {
      try {
        const rawCache = localStorage.getItem(RELEASE_CACHE_KEY);
        if (rawCache) {
          const parsed = JSON.parse(rawCache);
          if (parsed && parsed.state) {
            renderLatestRelease(parsed.state);
            return;
          }
        }
      } catch (_) {}

      const notesBody = document.getElementById("release-notes-body");
      if (notesBody) {
        notesBody.innerHTML = `<p>${escapeHtml(i18n[currentLang].dl_notes_empty)}</p>`;
      }
    }
  }

  loadLatestRelease();

  // ===== 7. Active Pill Nav on Scroll =====
  const sections = ["preview", "features", "servers", "download", "faq"]
    .map((id) => document.getElementById(id))
    .filter(Boolean);
  const navLinks = document.querySelectorAll(".pill-link");

  const observer = new IntersectionObserver(
    (entries) => {
      for (const entry of entries) {
        if (entry.isIntersecting) {
          navLinks.forEach((l) => {
            l.classList.toggle("active", l.getAttribute("href") === `#${entry.target.id}`);
          });
        }
      }
    },
    { rootMargin: "-35% 0px -55% 0px" }
  );
  sections.forEach((sec) => observer.observe(sec));
})();
