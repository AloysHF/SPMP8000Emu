const translatableElements = [...document.querySelectorAll("[data-i18n]")];
const chinese = Object.fromEntries(translatableElements.map((element) => [element.dataset.i18n, element.innerHTML]));
const english = {
  "skip": "Skip to content",
  "nav.overview": "Overview",
  "nav.features": "Features",
  "nav.compat": "Compatibility",
  "nav.devices": "Devices",
  "hero.title": "SPMP8000<br><span>Game Emulator</span>",
  "hero.lead": "SPMP8000Emu is a Rust emulator for <strong>native BIN games</strong> released on SPMP8000-family devices. The same emulation core runs in a standalone desktop app or through a Libretro frontend.",
  "hero.run": "Get started",
  "hero.source": "View source ↗",
  "hero.screen": "Captured from SPMP8000Emu",
  "stats.games": "supported games",
  "stats.devices": "emulated devices",
  "stats.frontends": "runtime frontends",
  "stats.platforms": "supported platforms",
  "platforms.title": "Multi-platform support",
  "platforms.desc": "Windows, macOS, and Linux include both the standalone app and Libretro core. Android, iOS, and webOS run through Libretro frontends.",
  "overview.title": "One core,<br>two frontends",
  "overview.desc": "Game loading, CPU, memory, graphics, audio, and input live in a shared core. The standalone app provides the desktop experience, while the Libretro core integrates with compatible frontends.",
  "overview.standalone.title": "Standalone emulator",
  "overview.standalone.desc": "Desktop window, keyboard and gamepad input, custom mappings, and multiple display filters.",
  "overview.libretro.title": "Libretro core",
  "overview.libretro.desc": "Core options, save states, cheats, memory descriptors, and standard frontend callbacks.",
  "overview.experience.title": "Modern play experience",
  "overview.experience.desc": "Save states, custom controls, display filters, and game cheats.",
  "features.title": "Implemented features",
  "features.desc": "The standalone app and Libretro core share the same set of implemented emulator capabilities.",
  "feature.loader.title": "Game loading",
  "feature.loader.desc": "Parses SPMP8000 native BIN containers and headers, handles DES data, and supports LZ77/LZSS and RLE decompression paths.",
  "feature.cpu.title": "ARM execution",
  "feature.cpu.desc": "Runs game code with an ARM-mode interpreter while maintaining registers, memory access, and runtime state.",
  "feature.graphics.title": "Graphics output",
  "feature.graphics.desc": "Supports RGB565, indexed palettes, color-key transparency, and eight image transformations for common 320×240 content.",
  "feature.audio.title": "Audio playback",
  "feature.audio.desc": "Provides 22050 Hz stereo mixing with the WAV decoding and MIDI synthesis paths used by games.",
  "feature.state.title": "Runtime state",
  "feature.state.desc": "Supports reset, versioned save states, content checks, and memory or ARM-register cheat rules.",
  "feature.input.title": "Input controls",
  "feature.input.desc": "The standalone app offers keyboard mapping and physical gamepad support, while the Libretro core receives controls through the standard gamepad interface.",
  "compat.title": "Game compatibility",
  "compat.desc": "20+ SPMP8000 native games are supported across action, racing, puzzle, board and card, and homebrew releases, with the compatibility list continuing to grow.",
  "compat.more": "More games +",
  "compat.link": "View supported games →",
  "build.title": "How to use",
  "build.desc": "The project builds with Cargo. Run a game directly in the standalone emulator, or follow the documentation to build and install the Libretro core.",
  "build.copy": "Copy clone command",
  "build.project": "Project overview →",
  "build.standalone": "Standalone guide →",
  "build.libretro": "Libretro guide →",
  "devices.title": "Supported devices",
  "devices.desc": "SPMP8000/80xx powered gaming PMPs, MP4/MP5 players, and handheld game devices. SPMP8000Emu runs their SPMP8000 native games.",
  "devices.phg": "Chuwi PHG",
  "footer.desc": "An open-source emulator written in Rust<br>Standalone · Libretro",
  "footer.top": "Back to top ↑"
};

const languageButton = document.querySelector(".lang-toggle");
const description = document.querySelector('meta[name="description"]');
const metadata = {
  zh: {
    title: "SPMP8000Emu — SPMP8000 原生游戏模拟器",
    description: "SPMP8000Emu 是使用 Rust 编写的 SPMP8000 原生 BIN 游戏模拟器，提供独立版与 Libretro 核心。"
  },
  en: {
    title: "SPMP8000Emu — SPMP8000 Native Game Emulator",
    description: "SPMP8000Emu is a Rust emulator for SPMP8000 native BIN games, available as a standalone app and a Libretro core."
  }
};

const readSavedLanguage = () => {
  try {
    return window.localStorage.getItem("spmp8000emu-language");
  } catch {
    return null;
  }
};

let currentLanguage = readSavedLanguage() || (navigator.language.toLowerCase().startsWith("zh") ? "zh" : "en");

const applyLanguage = (language) => {
  currentLanguage = language;
  const translations = language === "en" ? english : chinese;
  translatableElements.forEach((element) => {
    const translated = translations[element.dataset.i18n];
    if (translated !== undefined) element.innerHTML = translated;
  });
  document.documentElement.lang = language === "en" ? "en" : "zh-CN";
  document.title = metadata[language].title;
  description.content = metadata[language].description;
  languageButton.textContent = language === "en" ? "中文" : "EN";
  languageButton.setAttribute("aria-label", language === "en" ? "切换到中文" : "Switch to English");
  try {
    window.localStorage.setItem("spmp8000emu-language", language);
  } catch {
    // Language persistence is optional in restricted browsing contexts.
  }
};

languageButton.addEventListener("click", () => applyLanguage(currentLanguage === "zh" ? "en" : "zh"));
applyLanguage(currentLanguage);

const reducedMotion = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
const revealItems = document.querySelectorAll(".reveal");

if (reducedMotion || !("IntersectionObserver" in window)) {
  revealItems.forEach((item) => item.classList.add("visible"));
} else {
  const revealObserver = new IntersectionObserver((entries, observer) => {
    entries.forEach((entry) => {
      if (!entry.isIntersecting) return;
      entry.target.classList.add("visible");
      observer.unobserve(entry.target);
    });
  }, { threshold: 0.12 });
  revealItems.forEach((item) => revealObserver.observe(item));
}

const shots = [...document.querySelectorAll(".screen-shot")];
let activeShot = 0;
if (!reducedMotion && shots.length > 1) {
  window.setInterval(() => {
    shots[activeShot].classList.remove("active");
    activeShot = (activeShot + 1) % shots.length;
    shots[activeShot].classList.add("active");
  }, 3200);
}

const featureViewport = document.querySelector(".feature-viewport");
const featureTrack = document.querySelector(".feature-cards");
const featureCards = [...document.querySelectorAll(".feature-cards > .device-card")];
const featurePrevious = document.querySelector("[data-feature-prev]");
const featureNext = document.querySelector("[data-feature-next]");
const featureCounter = document.querySelector(".feature-carousel-counter");
const featureProgress = document.querySelector(".feature-carousel-progress i");

if (featureViewport && featureTrack && featureCards.length) {
  let featureIndex = 0;
  let featureTimer;
  let featureScrollFrame;

  const visibleFeatureCount = () => {
    if (window.innerWidth <= 600) return 1;
    if (window.innerWidth <= 900) return 2;
    return 3;
  };

  const maximumFeatureIndex = () => Math.max(0, featureCards.length - visibleFeatureCount());

  const updateFeatureControls = () => {
    featureIndex = Math.min(featureIndex, maximumFeatureIndex());
    featureCounter.textContent = `${String(featureIndex + 1).padStart(2, "0")} / ${String(featureCards.length).padStart(2, "0")}`;
    const visibleEnd = Math.min(featureCards.length, featureIndex + visibleFeatureCount());
    featureProgress.style.width = `${(visibleEnd / featureCards.length) * 100}%`;
  };

  const showFeature = (requestedIndex) => {
    const maximumIndex = maximumFeatureIndex();
    featureIndex = requestedIndex < 0 ? maximumIndex : requestedIndex > maximumIndex ? 0 : requestedIndex;
    const left = featureCards[featureIndex].offsetLeft - featureTrack.offsetLeft;
    featureViewport.scrollTo({ left, behavior: reducedMotion ? "auto" : "smooth" });
    updateFeatureControls();
  };

  const stopFeatureAutoplay = () => window.clearInterval(featureTimer);
  const startFeatureAutoplay = () => {
    stopFeatureAutoplay();
    if (!reducedMotion) featureTimer = window.setInterval(() => showFeature(featureIndex + 1), 4200);
  };

  featurePrevious?.addEventListener("click", () => {
    showFeature(featureIndex - 1);
    startFeatureAutoplay();
  });
  featureNext?.addEventListener("click", () => {
    showFeature(featureIndex + 1);
    startFeatureAutoplay();
  });
  featureViewport.addEventListener("keydown", (event) => {
    if (event.key !== "ArrowLeft" && event.key !== "ArrowRight") return;
    event.preventDefault();
    showFeature(featureIndex + (event.key === "ArrowRight" ? 1 : -1));
    startFeatureAutoplay();
  });
  featureViewport.addEventListener("scroll", () => {
    window.cancelAnimationFrame(featureScrollFrame);
    featureScrollFrame = window.requestAnimationFrame(() => {
      const cardWidth = featureCards[0].getBoundingClientRect().width;
      if (cardWidth > 0) featureIndex = Math.round(featureViewport.scrollLeft / cardWidth);
      updateFeatureControls();
    });
  }, { passive: true });

  const featureCarousel = featureViewport.closest(".feature-carousel");
  featureCarousel?.addEventListener("pointerenter", stopFeatureAutoplay);
  featureCarousel?.addEventListener("pointerleave", startFeatureAutoplay);
  featureCarousel?.addEventListener("focusin", stopFeatureAutoplay);
  featureCarousel?.addEventListener("focusout", startFeatureAutoplay);
  window.addEventListener("resize", () => showFeature(Math.min(featureIndex, maximumFeatureIndex())));
  document.addEventListener("visibilitychange", () => document.hidden ? stopFeatureAutoplay() : startFeatureAutoplay());

  updateFeatureControls();
  startFeatureAutoplay();
}

const copyButton = document.querySelector(".copy");
copyButton?.addEventListener("click", async () => {
  try {
    await navigator.clipboard.writeText(copyButton.dataset.copy);
    copyButton.textContent = currentLanguage === "en" ? "Copied ✓" : "已复制 ✓";
  } catch {
    copyButton.textContent = currentLanguage === "en" ? "Copy manually" : "请手动复制";
  }
  window.setTimeout(() => {
    copyButton.innerHTML = (currentLanguage === "en" ? english : chinese)["build.copy"];
  }, 1800);
});
