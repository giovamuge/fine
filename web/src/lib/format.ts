// Formatting helpers in the UI language (Intl, EUR by default).
import { LOCALE, t, tn } from "./i18n";

// useGrouping "always": Italian would otherwise write 2560,00 € but 15.391,44 €
// (supported by every current browser; the TS lib types still say boolean).
const ALWAYS = "always" as unknown as boolean;
const eur = new Intl.NumberFormat(LOCALE, { style: "currency", currency: "EUR", useGrouping: ALWAYS });
const eur0 = new Intl.NumberFormat(LOCALE, { style: "currency", currency: "EUR", maximumFractionDigits: 0, useGrouping: ALWAYS });
const currencyCache = new Map<string, Intl.NumberFormat>();
const int = new Intl.NumberFormat(LOCALE, { maximumFractionDigits: 0, useGrouping: ALWAYS });
const pct = new Intl.NumberFormat(LOCALE, { style: "percent", maximumFractionDigits: 0 });

export function euro(n: number): string {
  return eur.format(n);
}

export function euroRound(n: number): string {
  return eur0.format(n);
}

export function money(n: number, currency: string | null | undefined): string {
  const c = (currency || "EUR").toUpperCase();
  if (c === "EUR") return eur.format(n);
  let f = currencyCache.get(c);
  if (!f) {
    try {
      f = new Intl.NumberFormat(LOCALE, { style: "currency", currency: c, useGrouping: ALWAYS });
    } catch {
      f = new Intl.NumberFormat(LOCALE, { minimumFractionDigits: 2, maximumFractionDigits: 2, useGrouping: ALWAYS });
    }
    currencyCache.set(c, f);
  }
  return f.format(n);
}

/** "+12,30 €" / "−4,00 €" (true minus sign). */
export function signed(n: number, currency?: string | null): string {
  const s = money(Math.abs(n), currency);
  if (n > 0) return `+${s}`;
  if (n < 0) return `−${s}`;
  return s;
}

export function percent(frac: number): string {
  return pct.format(frac);
}

export function integer(n: number): string {
  return int.format(n);
}

/** Axis tick: full under 10.000, compact "12 k" above. */
export function compact(n: number): string {
  const a = Math.abs(n);
  if (a >= 10_000) {
    const k = n / 1000;
    return `${new Intl.NumberFormat(LOCALE, { maximumFractionDigits: k % 1 === 0 ? 0 : 1 }).format(k)} k`;
  }
  return int.format(n);
}

export function parseDate(iso: string): Date {
  // Dates are YYYY-MM-DD (local) or RFC 3339 timestamps.
  if (/^\d{4}-\d{2}-\d{2}$/.test(iso)) {
    const [y, m, d] = iso.split("-").map(Number);
    return new Date(y ?? 1970, (m ?? 1) - 1, d ?? 1);
  }
  return new Date(iso);
}

const dLong = new Intl.DateTimeFormat(LOCALE, { day: "numeric", month: "long", year: "numeric" });
const dShort = new Intl.DateTimeFormat(LOCALE, { day: "numeric", month: "short" });
const dShortYear = new Intl.DateTimeFormat(LOCALE, { day: "numeric", month: "short", year: "numeric" });
const dNum = new Intl.DateTimeFormat(LOCALE, { dateStyle: "short" });
const dDay = new Intl.DateTimeFormat(LOCALE, { weekday: "long", day: "numeric", month: "long" });
const dTime = new Intl.DateTimeFormat(LOCALE, { dateStyle: "short", timeStyle: "short" });
const mLong = new Intl.DateTimeFormat(LOCALE, { month: "long", year: "numeric" });
const mShort = new Intl.DateTimeFormat(LOCALE, { month: "short" });

export function fmtDate(iso: string): string {
  return dLong.format(parseDate(iso));
}
export function fmtDateShort(iso: string): string {
  return dShort.format(parseDate(iso));
}
export function fmtDateShortYear(iso: string): string {
  return dShortYear.format(parseDate(iso));
}
export function fmtDateNum(iso: string): string {
  return dNum.format(parseDate(iso));
}
export function fmtDateTime(iso: string): string {
  return dTime.format(parseDate(iso));
}
/** "martedì 11 novembre", with t("Oggi")/t("Ieri") when it applies. */
export function fmtDay(iso: string, today: Date = new Date()): string {
  const d = parseDate(iso);
  const diff = Math.round((startOfDay(today).getTime() - startOfDay(d).getTime()) / 86_400_000);
  if (diff === 0) return t("Oggi");
  if (diff === 1) return t("Ieri");
  const s = dDay.format(d);
  return s.charAt(0).toUpperCase() + s.slice(1);
}
export function fmtMonth(ym: string): string {
  const s = mLong.format(parseDate(`${ym}-01`));
  return s.charAt(0).toUpperCase() + s.slice(1);
}
export function fmtMonthShort(ym: string): string {
  const d = parseDate(`${ym}-01`);
  const s = mShort.format(d).replace(".", "");
  return d.getMonth() === 0 ? `${s} ${String(d.getFullYear()).slice(2)}` : s;
}

export function startOfDay(d: Date): Date {
  return new Date(d.getFullYear(), d.getMonth(), d.getDate());
}

export function iso(d: Date): string {
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;
}

export function ym(d: Date): string {
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}`;
}

export function addMonths(ymStr: string, n: number): string {
  const [y, m] = ymStr.split("-").map(Number);
  return ym(new Date(y ?? 1970, (m ?? 1) - 1 + n, 1));
}

export function daysUntil(iso: string, today: Date = new Date()): number {
  return Math.floor((parseDate(iso).getTime() - startOfDay(today).getTime()) / 86_400_000);
}

/** "3 ore fa", "ieri alle 07:07" ... */
export function relativeTime(iso: string, now: Date = new Date()): string {
  const d = parseDate(iso);
  const mins = Math.round((now.getTime() - d.getTime()) / 60_000);
  if (mins < 1) return t("adesso");
  if (mins < 60) return t("{n} min fa", { n: mins });
  const hours = Math.round(mins / 60);
  if (hours < 24) return tn(hours, "{n} ora fa", "{n} ore fa");
  const days = Math.round(hours / 24);
  if (days === 1) return t("ieri");
  if (days < 7) return t("{n} giorni fa", { n: days });
  return dTime.format(d);
}

export const CADENCE_SHORT: Record<"week" | "month" | "year", string> = {
  week: t("sett."),
  month: t("mese"),
  year: t("anno"),
};

export function plural(n: number, one: string, many: string): string {
  return `${integer(n)} ${t(n === 1 ? one : many)}`;
}
