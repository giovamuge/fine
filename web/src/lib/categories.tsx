// How a category looks: its emoji (or a neutral dot) on a tinted chip of its color.
// The class strings are spelled out in full so Tailwind sees them.
import type { CategoryKind, CategoryView } from "./types";
import { cn } from "./utils";

export const COLORS = ["slate", "red", "orange", "amber", "lime", "green", "teal", "sky", "blue", "indigo", "violet", "pink"] as const;
export type CategoryColor = (typeof COLORS)[number];

const CHIP: Record<CategoryColor, string> = {
  slate: "bg-slate-100 text-slate-700 dark:bg-slate-500/20 dark:text-slate-200",
  red: "bg-red-100 text-red-700 dark:bg-red-500/20 dark:text-red-200",
  orange: "bg-orange-100 text-orange-700 dark:bg-orange-500/20 dark:text-orange-200",
  amber: "bg-amber-100 text-amber-800 dark:bg-amber-500/20 dark:text-amber-200",
  lime: "bg-lime-100 text-lime-800 dark:bg-lime-500/20 dark:text-lime-200",
  green: "bg-green-100 text-green-700 dark:bg-green-500/20 dark:text-green-200",
  teal: "bg-teal-100 text-teal-700 dark:bg-teal-500/20 dark:text-teal-200",
  sky: "bg-sky-100 text-sky-700 dark:bg-sky-500/20 dark:text-sky-200",
  blue: "bg-blue-100 text-blue-700 dark:bg-blue-500/20 dark:text-blue-200",
  indigo: "bg-indigo-100 text-indigo-700 dark:bg-indigo-500/20 dark:text-indigo-200",
  violet: "bg-violet-100 text-violet-700 dark:bg-violet-500/20 dark:text-violet-200",
  pink: "bg-pink-100 text-pink-700 dark:bg-pink-500/20 dark:text-pink-200",
};

/** Solid swatch, for the color picker and chart bars. */
export const SWATCH: Record<CategoryColor, string> = {
  slate: "bg-slate-500",
  red: "bg-red-500",
  orange: "bg-orange-500",
  amber: "bg-amber-500",
  lime: "bg-lime-500",
  green: "bg-green-500",
  teal: "bg-teal-500",
  sky: "bg-sky-500",
  blue: "bg-blue-500",
  indigo: "bg-indigo-500",
  violet: "bg-violet-500",
  pink: "bg-pink-500",
};

export const COLOR_LABEL: Record<CategoryColor, string> = {
  slate: "Grigio",
  red: "Rosso",
  orange: "Arancione",
  amber: "Ambra",
  lime: "Lime",
  green: "Verde",
  teal: "Verde acqua",
  sky: "Celeste",
  blue: "Blu",
  indigo: "Indaco",
  violet: "Viola",
  pink: "Rosa",
};

export const KIND_LABEL: Record<CategoryKind, string> = {
  expense: "Spese",
  income: "Entrate",
  transfer: "Giroconti",
  saving: "Risparmio",
};

export const KIND_HINT: Record<CategoryKind, string> = {
  expense: "Contano nel budget e nel «disponibile».",
  income: "Stipendio, rimborsi e altre entrate.",
  transfer: "Movimenti tra i tuoi conti: esclusi da entrate e uscite.",
  saving: "Soldi messi da parte o investiti: non sono spese.",
};

export const KINDS: CategoryKind[] = ["expense", "income", "transfer", "saving"];

/** A stable color for categories without one, so every category is recognizable. */
export function colorOf(c: Pick<CategoryView, "name" | "color"> | undefined): CategoryColor {
  if (c?.color && (COLORS as readonly string[]).includes(c.color)) return c.color as CategoryColor;
  const name = c?.name ?? "";
  let h = 0;
  for (const ch of name) h = (h * 31 + ch.charCodeAt(0)) >>> 0;
  return COLORS[h % COLORS.length] ?? "slate";
}

export function chipClass(c: Pick<CategoryView, "name" | "color"> | undefined): string {
  return CHIP[colorOf(c)];
}

export function CategoryIcon({ category, className }: { category: Pick<CategoryView, "name" | "color" | "icon"> | undefined; className?: string }) {
  return (
    <span aria-hidden="true" className={cn("inline-flex size-8 shrink-0 items-center justify-center rounded-lg text-base leading-none", category ? chipClass(category) : "bg-muted text-muted-foreground", className)}>
      {category?.icon ?? (category ? category.name.slice(0, 1).toUpperCase() : "?")}
    </span>
  );
}

/** Suggested emojis for the icon picker (any emoji can be typed too). */
export const EMOJI = [
  "🛒", "🍽️", "☕", "🍕", "🚆", "🚗", "⛽", "🅿️", "✈️", "🏠", "🔑", "💡", "📱", "🔁", "💊", "🏋️", "💇", "👕", "💻", "🛍️",
  "🎬", "🎮", "🎵", "📚", "🧸", "🐾", "🎁", "🤝", "🛡️", "🧾", "🏦", "💶", "💼", "📦", "💰", "↩️", "📈", "➕", "🔄", "🐷",
  "🚲", "🏖️", "⚽", "🍷", "🧹", "🔧", "🌱", "❤️", "⭐", "🎓",
];
