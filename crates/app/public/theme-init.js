// theme-init.js — применение сохранённой темы до первого рендера (anti-FOUC).
//
// Подключён в <head> index.html как обычный (не defer/async) скрипт: он
// блокирует парсинг и выполняется до первого краска, поэтому data-theme на
// <html> выставляется раньше, чем браузер рисует страницу в тёмной теме.
// Внешний файл, а не inline: CSP приложения — script-src 'self' без
// 'unsafe-inline', inline-скрипты заблокированы.
//
// Логика переключения и синхронизация тумблера — в main.ts (applyTheme);
// здесь только раннее применение + синхронизация theme-color нативной панели.
(function () {
  "use strict";
  try {
    var theme = localStorage.getItem("darmark.theme");
    var isLight = theme === "light";
    document.documentElement.setAttribute("data-theme", isLight ? "light" : "dark");
    // theme-color: иначе панель WebView на миг останется тёмной при светлой теме.
    var meta = document.querySelector('meta[name="theme-color"]');
    if (meta) meta.setAttribute("content", isLight ? "#fafbfc" : "#0d0e12");
  } catch (e) {
    // localStorage недоступен (приватный режим) — остаёмся на тёмной по умолчанию.
  }
})();
