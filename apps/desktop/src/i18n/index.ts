import { useSyncExternalStore } from "react";

import { zh } from "./zh";

export type Lang = "zh" | "en";

const STORAGE_KEY = "assetmesh-lang";

export const DEFAULT_LANG: Lang = "zh";

export const LANGUAGES: { id: Lang; label: string }[] = [
  { id: "zh", label: "中文" },
  { id: "en", label: "English" },
];

function readStoredLang(): Lang {
  try {
    const saved = localStorage.getItem(STORAGE_KEY);
    if (saved === "zh" || saved === "en") return saved;
  } catch {
    // localStorage can be unavailable; the default still renders.
  }
  return DEFAULT_LANG;
}

let current: Lang = readStoredLang();

const listeners = new Set<() => void>();

/** BCP-47 tag for `Date.toLocale*`, so dates follow the interface language. */
export function localeTag(): string {
  return current === 'zh' ? 'zh-CN' : 'en-US';
}

export function getLang(): Lang {
  return current;
}

export function setLang(lang: Lang): void {
  if (lang === current) return;
  current = lang;
  try {
    localStorage.setItem(STORAGE_KEY, lang);
  } catch {
    // The in-memory value already changed, so the UI is consistent either way.
  }
  for (const notify of Array.from(listeners)) notify();
}

function subscribe(notify: () => void): () => void {
  listeners.add(notify);
  return () => listeners.delete(notify);
}

export function useLang(): Lang {
  return useSyncExternalStore(subscribe, getLang, getLang);
}

export type Vars = Record<string, unknown>;

/**
 * Translate a piece of interface copy.
 *
 * The key is the English source text itself, so an entry missing from the
 * dictionary degrades to English instead of rendering nothing. `{name}`
 * placeholders are filled from `vars`.
 */
export function t(text: string, vars?: Vars): string {
  const template = current === "zh" ? (zh[text] ?? text) : text;
  if (!vars) return template;
  return template.replace(/\{(\w+)\}/g, (whole: string, key: string) =>
    Object.prototype.hasOwnProperty.call(vars, key) ? String(vars[key]) : whole,
  );
}

/** Subscribes the calling component to language changes. */
export function useT(): typeof t {
  useLang();
  return t;
}
