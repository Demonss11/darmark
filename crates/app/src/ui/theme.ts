

const THEME_STORAGE_KEY = "darmark.theme";

type Theme = "dark" | "light";

function currentTheme(): Theme {
  return document.documentElement.getAttribute("data-theme") === "light" ? "light" : "dark";
}

function applyTheme(theme: Theme): void {
  document.documentElement.setAttribute("data-theme", theme);
  try {
    localStorage.setItem(THEME_STORAGE_KEY, theme);
  } catch {
  }

  const btn = document.getElementById("sb-theme");
  if (btn) {
    const isLight = theme === "light";
    const label = document.getElementById("sb-theme-label");
    const sun = document.getElementById("sb-theme-icon-sun");
    const moon = document.getElementById("sb-theme-icon-moon");
    if (label) label.textContent = isLight ? "Светлая" : "Тёмная";
    if (sun) sun.hidden = !isLight;
    if (moon) moon.hidden = isLight;
    btn.setAttribute("aria-label", isLight ? "Тема: светлая" : "Тема: тёмная");
    btn.setAttribute("aria-pressed", String(isLight));
  }
  const meta = document.querySelector('meta[name="theme-color"]');
  if (meta) meta.setAttribute("content", theme === "light" ? "#fafbfc" : "#0d0e12");
}

export function initTheme(): void {
  applyTheme(currentTheme());
  const themeBtn = document.getElementById("sb-theme");
  themeBtn?.addEventListener("click", () => {
    applyTheme(currentTheme() === "light" ? "dark" : "light");
  });
}
