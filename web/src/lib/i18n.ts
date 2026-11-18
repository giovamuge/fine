// UI language. The Italian text is the key: `t("Sincronizza ora")` returns it as is in Italian
// and its English translation (lib/en.ts) in English. Placeholders are written {name}.
// Changing the language reloads the page, so every module (dates, numbers) reads it once.
import { EN } from "./en";

export type Lang = "it" | "en";
const KEY = "fine.lang";

function readLang(): Lang {
  try {
    const saved = localStorage.getItem(KEY);
    if (saved === "it" || saved === "en") return saved;
  } catch {
    /* private mode */
  }
  return navigator.language.toLowerCase().startsWith("it") ? "it" : "en";
}

export const LANG: Lang = readLang();
/** Locale for Intl: Italian, or English with day-month dates and euro-style grouping. */
export const LOCALE = LANG === "it" ? "it-IT" : navigator.language.toLowerCase().startsWith("en-us") ? "en-US" : "en-GB";

document.documentElement.lang = LANG;

const missing = new Set<string>();

export function t(it: string, vars?: Record<string, string | number>): string {
  let s = it;
  if (LANG === "en") {
    const en = EN[it];
    if (en !== undefined) s = en;
    else if (import.meta.env.DEV && !missing.has(it)) {
      missing.add(it);
      console.warn(`[i18n] missing English text: ${JSON.stringify(it)}`);
    }
  }
  return vars ? s.replace(/\{(\w+)\}/g, (m, k: string) => (k in vars ? String(vars[k]) : m)) : s;
}

/** For a word whose English depends on where it is used: the key in en.ts is "context|text". */
export function tc(context: string, it: string): string {
  if (LANG !== "en") return it;
  return EN[`${context}|${it}`] ?? t(it);
}

/** `n` with the singular or plural noun: tn(3, "{n} movimento", "{n} movimenti"). */
export function tn(n: number, one: string, many: string, vars?: Record<string, string | number>): string {
  return t(n === 1 ? one : many, { n, ...vars });
}

export function setLang(lang: Lang) {
  try {
    localStorage.setItem(KEY, lang);
  } catch {
    /* private mode: the browser language stays in use */
  }
  window.location.reload();
}
