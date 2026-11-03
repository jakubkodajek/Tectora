// Colour scheme: "system" follows the OS, "light"/"dark" force one via
// `<html data-theme>` (see app.css). `ui.dark` is reactive so canvases
// (3D view) can repaint when it changes.

export type Theme = "system" | "light" | "dark";

const media = typeof window !== "undefined" ? window.matchMedia("(prefers-color-scheme: dark)") : null;

export const ui = $state({ theme: readCached(), dark: false });

function readCached(): Theme {
  try {
    const t = localStorage.getItem("tectora.theme");
    if (t === "light" || t === "dark" || t === "system") return t;
  } catch {
    // Storage unavailable.
  }
  return "system";
}

function resolve() {
  ui.dark = ui.theme === "dark" || (ui.theme === "system" && !!media?.matches);
}

export function applyTheme(theme: Theme) {
  ui.theme = theme;
  if (theme === "system") delete document.documentElement.dataset.theme;
  else document.documentElement.dataset.theme = theme;
  try {
    // Cache so the next start paints in the right theme before settings load.
    localStorage.setItem("tectora.theme", theme);
  } catch {
    // Not cached; the theme still applies now.
  }
  resolve();
}

media?.addEventListener("change", resolve);
applyTheme(ui.theme);

export const isDark = () => ui.dark;
