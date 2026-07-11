/**
 * Clotho color-scheme preference.
 *
 * `preference` is what the user chose (system | dark | light).
 * `resolved` is the concrete mode applied to `<html data-mode>`.
 * Current Belweave dark tokens are the product default when unset.
 */

export const THEME_STORAGE_KEY = "clotho-color-scheme";

export type ThemePreference = "system" | "dark" | "light";
export type ResolvedMode = "dark" | "light";

export const THEME_PREFERENCES: readonly ThemePreference[] = [
  "system",
  "dark",
  "light",
] as const;

export function isThemePreference(value: unknown): value is ThemePreference {
  return value === "system" || value === "dark" || value === "light";
}

export function readStoredPreference(): ThemePreference {
  if (typeof window === "undefined") return "dark";
  try {
    const raw = window.localStorage.getItem(THEME_STORAGE_KEY);
    return isThemePreference(raw) ? raw : "dark";
  } catch {
    return "dark";
  }
}

export function storePreference(preference: ThemePreference): void {
  try {
    window.localStorage.setItem(THEME_STORAGE_KEY, preference);
  } catch {
    // private mode / quota — still apply in-session via DOM
  }
}

export function systemPrefersDark(): boolean {
  if (typeof window === "undefined") return true;
  return window.matchMedia("(prefers-color-scheme: dark)").matches;
}

export function resolveMode(preference: ThemePreference): ResolvedMode {
  if (preference === "system") {
    return systemPrefersDark() ? "dark" : "light";
  }
  return preference;
}

/** Apply resolved mode to the document root (Kumo + Belweave tokens). */
export function applyResolvedMode(mode: ResolvedMode): void {
  const root = document.documentElement;
  root.setAttribute("data-mode", mode);
  root.style.colorScheme = mode;
}

/**
 * Inline bootstrap for `<head>` — must stay string-serializable and free of
 * imports so it can run before paint and avoid a flash of the wrong theme.
 */
export const THEME_INIT_SCRIPT = `(function(){try{var k=${JSON.stringify(
  THEME_STORAGE_KEY,
)};var p=localStorage.getItem(k);if(p!=="system"&&p!=="dark"&&p!=="light")p="dark";var m=p==="system"?(window.matchMedia("(prefers-color-scheme: dark)").matches?"dark":"light"):p;var r=document.documentElement;r.setAttribute("data-mode",m);r.style.colorScheme=m;}catch(e){document.documentElement.setAttribute("data-mode","dark");document.documentElement.style.colorScheme="dark";}})();`;
