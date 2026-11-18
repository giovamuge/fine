// Budget rules as an editable form: "include if all/any of …, exclude if any of …".
// That shape covers what people write by hand; any other tree (e.g. written by the AI)
// stays editable as JSON. The server validates every rule anyway.
import type { Budget, BudgetGoal, BudgetPeriod, BudgetRule, BudgetState, CategoryKind } from "./types";
import { t, tc } from "./i18n";

type Node = Extract<BudgetRule, { all: unknown } | { any: unknown } | { not: unknown }>;
export type Leaf = Exclude<BudgetRule, Node>;
type KeysOf<T> = T extends unknown ? keyof T : never;
export type CondType = KeysOf<Leaf>;

export interface RuleForm {
  mode: "all" | "any";
  include: Leaf[];
  exclude: Leaf[];
}

export const COND_TYPES: CondType[] = ["category", "tag", "text", "account", "amount", "direction", "weekday", "day_of_month", "category_kind", "uncategorized", "fixed"];

export const COND_LABEL: Record<CondType, string> = {
  category: t("Categoria"),
  tag: t("Tag"),
  text: t("Esercente o descrizione contiene"),
  account: t("Conto"),
  amount: t("Importo"),
  direction: t("Direzione"),
  weekday: t("Giorno della settimana"),
  day_of_month: t("Giorno del mese"),
  category_kind: t("Tipo di categoria"),
  uncategorized: t("Categorizzazione"),
  fixed: t("Spese fisse pianificate"),
};

export function condType(l: Leaf): CondType {
  return Object.keys(l)[0] as CondType;
}

export function isLeaf(r: BudgetRule): r is Leaf {
  return !("all" in r) && !("any" in r) && !("not" in r);
}

export function emptyLeaf(t: CondType): Leaf {
  switch (t) {
    case "category":
      return { category: [] };
    case "tag":
      return { tag: [] };
    case "text":
      return { text: [] };
    case "account":
      return { account: [] };
    case "amount":
      return { amount: {} };
    case "direction":
      return { direction: "out" };
    case "weekday":
      return { weekday: [6, 7] };
    case "day_of_month":
      return { day_of_month: { from: 1, to: 15 } };
    case "category_kind":
      return { category_kind: ["expense" as CategoryKind] };
    case "uncategorized":
      return { uncategorized: true };
    case "fixed":
      return { fixed: false };
  }
}

/** Does the leaf have a value yet (lists not empty, amount with min or max)? */
export function leafComplete(l: Leaf): boolean {
  const v = Object.values(l)[0] as unknown;
  if (Array.isArray(v)) return v.length > 0;
  if ("amount" in l) return l.amount.min !== undefined || l.amount.max !== undefined;
  return true;
}

function excludedLeaves(r: BudgetRule): Leaf[] | null {
  if (!("not" in r)) return null;
  const inner = r.not;
  if (isLeaf(inner)) return [inner];
  if ("any" in inner && inner.any.every(isLeaf)) return inner.any as Leaf[];
  return null;
}

/** The form for a rule, or null when the rule has a shape the form can't show. */
export function ruleToForm(r: BudgetRule | undefined): RuleForm | null {
  if (!r) return { mode: "all", include: [], exclude: [] };
  if (isLeaf(r)) return { mode: "all", include: [r], exclude: [] };
  const ex = excludedLeaves(r);
  if (ex) return { mode: "all", include: [], exclude: ex };
  if ("any" in r) return r.any.every(isLeaf) ? { mode: "any", include: r.any as Leaf[], exclude: [] } : null;
  if (!("all" in r)) return null;
  const form: RuleForm = { mode: "all", include: [], exclude: [] };
  let anyGroup: Leaf[] | null = null;
  for (const item of r.all) {
    const e = excludedLeaves(item);
    if (e) form.exclude.push(...e);
    else if (isLeaf(item)) form.include.push(item);
    else if ("any" in item && item.any.every(isLeaf) && !anyGroup) anyGroup = item.any as Leaf[];
    else return null;
  }
  if (anyGroup) {
    if (form.include.length) return null;
    return { mode: "any", include: anyGroup, exclude: form.exclude };
  }
  return form;
}

export function formToRule(f: RuleForm): BudgetRule | undefined {
  const include = f.include.filter(leafComplete);
  const exclude = f.exclude.filter(leafComplete);
  const inc: BudgetRule | null = include.length === 0 ? null : include.length === 1 ? include[0]! : f.mode === "all" ? { all: include } : { any: include };
  const exc: BudgetRule | null = exclude.length === 0 ? null : { not: exclude.length === 1 ? exclude[0]! : { any: exclude } };
  if (inc && exc) return { all: [...(inc && "all" in inc ? inc.all : [inc]), exc] };
  return inc ?? exc ?? undefined;
}

export const GOAL_LABEL: Record<BudgetGoal, string> = {
  max_spend: t("Spendi al massimo"),
  min_save: t("Metti da parte almeno"),
  min_income: t("Incassa almeno"),
};

export const GOAL_SHORT: Record<BudgetGoal, string> = {
  max_spend: t("Spesa"),
  min_save: t("Risparmio"),
  min_income: t("Entrate"),
};

export const STATE_LABEL: Record<BudgetState, string> = {
  ok: t("In regola"),
  attenzione: t("Attenzione"),
  sforato: t("Sforato"),
  in_linea: t("In linea"),
  indietro: tc("budget", "Indietro"),
  raggiunto: t("Raggiunto"),
  mancato: t("Non raggiunto"),
  sospeso: t("In pausa"),
};

export const PERIOD_TYPES: { type: BudgetPeriod["type"]; label: string }[] = [
  { type: "month", label: t("Ogni mese") },
  { type: "week", label: t("Ogni settimana") },
  { type: "quarter", label: t("Ogni trimestre") },
  { type: "year", label: t("Ogni anno") },
  { type: "rolling", label: t("Ultimi N giorni") },
  { type: "custom", label: t("Date fisse") },
];

export function defaultPeriod(type: BudgetPeriod["type"], today = new Date()): BudgetPeriod {
  const iso = (d: Date) => `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;
  switch (type) {
    case "week":
      return { type };
    case "month":
      return { type };
    case "quarter":
      return { type };
    case "year":
      return { type };
    case "rolling":
      return { type, days: 30 };
    case "custom":
      return { type, from: iso(today), to: iso(new Date(today.getFullYear(), today.getMonth(), today.getDate() + 7)) };
  }
}

export function newBudget(): Budget {
  return { name: "", goal: "max_spend", period: { type: "month" }, amount: 0, alerts: [80, 100] };
}

/** Ready-made starting points for the empty state and the "new" menu. */
export const TEMPLATES: { label: string; hint: string; budget: () => Budget }[] = [
  {
    label: t("Svago del mese"),
    hint: t("Ristoranti, bar e svago, esclusi i pasti di lavoro, con riporto"),
    budget: () => ({
      ...newBudget(),
      name: t("Svago"),
      amount: 300,
      rollover: "surplus",
      rule: { all: [{ category: ["Ristoranti e bar", "Svago"] }, { not: { tag: ["lavoro"] } }] },
    }),
  },
  {
    label: t("Un viaggio"),
    hint: t("Tutto quello che ha un tag, tra due date, messo da parte dal disponibile"),
    budget: () => ({ ...newBudget(), name: t("Viaggio"), amount: 800, basis: "cash", reserve: true, period: defaultPeriod("custom"), rule: { tag: ["viaggio"] } }),
  },
  {
    label: t("Un esercente"),
    hint: t("Es. Amazon: tutte le spese che contengono un testo"),
    budget: () => ({ ...newBudget(), name: "Amazon", amount: 50, rule: { text: ["amazon"] } }),
  },
  {
    label: t("Spesa settimanale"),
    hint: t("La spesa alimentare, ogni settimana"),
    budget: () => ({ ...newBudget(), name: t("Spesa"), amount: 100, period: { type: "week" }, rule: { category: ["Spesa alimentare"] } }),
  },
  {
    label: t("Risparmio minimo"),
    hint: t("Almeno una cifra al mese verso risparmi e investimenti"),
    budget: () => ({ ...newBudget(), name: t("Risparmio"), goal: "min_save", amount: 200, alerts: [100] }),
  },
];
