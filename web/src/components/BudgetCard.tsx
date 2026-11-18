// How a budget is shown: full card (Budget page) and compact row (Overview, Analisi).
import { CalendarClock, PauseCircle, PiggyBank, Repeat, TriangleAlert } from "lucide-react";
import { Badge } from "@/components/ui/badge";
import { Meter } from "@/components/Meter";
import { GOAL_SHORT, STATE_LABEL } from "@/lib/budget-rules";
import { euro, fmtDateShort, percent } from "@/lib/format";
import type { BudgetState, BudgetStatus } from "@/lib/types";
import { cn } from "@/lib/utils";
import { t } from "@/lib/i18n";

const STATE_BADGE: Record<BudgetState, string> = {
  ok: "border-positive/30 text-positive",
  in_linea: "",
  raggiunto: "border-positive/30 text-positive",
  attenzione: "border-warning/40 text-warning",
  indietro: "border-warning/40 text-warning",
  sforato: "border-destructive/30 text-destructive",
  mancato: "border-destructive/30 text-destructive",
  sospeso: "text-muted-foreground",
};

/** Risk ordering for "closest to the limit" lists. */
export function risk(b: BudgetStatus): number {
  const base = { sforato: 4, mancato: 3, attenzione: 3, indietro: 2, ok: 1, in_linea: 1, raggiunto: 0, sospeso: -1 }[b.state];
  return base * 10 + b.used;
}

export function remainingText(b: BudgetStatus): string {
  if (b.phase === "upcoming") return t("Inizia il {date}", { date: fmtDateShort(b.start) });
  if (b.goal === "max_spend") {
    if (b.remaining < 0) return t("Sforato di {amount}", { amount: euro(-b.remaining) });
    const left = t("Restano {amount}", { amount: euro(b.remaining) });
    return b.per_day !== null && b.phase === "active" ? `${left} · ${t("{amount} al giorno", { amount: euro(b.per_day) })}` : left;
  }
  return b.remaining > 0 ? t("Mancano {amount}", { amount: euro(b.remaining) }) : t("Obiettivo raggiunto");
}

function History({ b }: { b: BudgetStatus }) {
  if (b.history.length < 2) return null;
  const max = Math.max(...b.history.map((h) => Math.max(h.value, h.limit)), 1);
  return (
    <div className="flex h-8 items-end gap-1" aria-label={t("Periodi precedenti")} role="img">
      {b.history.map((h) => {
        const over = b.goal === "max_spend" && h.value > h.limit;
        return (
          <div
            key={h.start}
            title={`${fmtDateShort(h.start)} – ${fmtDateShort(h.end)}: ${t("{value} su {limit}", { value: euro(h.value), limit: euro(h.limit) })}`}
            className={cn("w-3 rounded-sm", over ? "bg-destructive/60" : "bg-muted-foreground/30")}
            style={{ height: `${Math.max(6, (h.value / max) * 100)}%` }}
          />
        );
      })}
    </div>
  );
}

export function BudgetCard({ b, onOpen }: { b: BudgetStatus; onOpen: () => void }) {
  const marker = b.phase === "active" && b.budget.period.type !== "rolling" ? b.elapsed : undefined;
  return (
    <button type="button" onClick={onOpen} className="flex w-full flex-col gap-3 rounded-xl border bg-card p-4 text-left transition-colors hover:bg-muted/50 focus-visible:ring-3 focus-visible:ring-ring/50 focus-visible:outline-none">
      <div className="flex items-start justify-between gap-3">
        <div className="flex min-w-0 flex-col gap-0.5">
          <span className="truncate font-semibold">{b.name}</span>
          <span className="text-xs text-muted-foreground">
            {GOAL_SHORT[b.goal]} · {b.period_label}
          </span>
        </div>
        <Badge variant="outline" className={cn("shrink-0 font-normal", STATE_BADGE[b.state])}>
          {b.state === "sospeso" && <PauseCircle />}
          {b.phase === "upcoming" && b.state !== "sospeso" ? t("In arrivo") : STATE_LABEL[b.state]}
        </Badge>
      </div>
      <div className="flex items-baseline justify-between gap-2">
        <span className="tabular text-2xl font-semibold tracking-tight">{euro(b.value)}</span>
        <span className="tabular text-sm text-muted-foreground">{t("su {limit}", { limit: euro(b.limit) })}</span>
      </div>
      <Meter value={b.used} marker={marker} state={b.state} label={`${b.name}: ${b.goal === "max_spend" ? t("{pct} speso", { pct: percent(b.used) }) : t("{pct} raggiunto", { pct: percent(b.used) })}`} />
      <span className={cn("text-sm", b.remaining < 0 && b.goal === "max_spend" ? "text-destructive" : "text-muted-foreground")}>{remainingText(b)}</span>
      <p className="line-clamp-2 text-xs text-muted-foreground">{b.rule_summary}</p>
      <div className="flex flex-wrap items-end justify-between gap-2">
        <div className="flex flex-wrap gap-1.5">
          {b.projected !== null && b.goal === "max_spend" && (
            <Badge variant="secondary" className={cn("font-normal", b.projected > b.limit && "text-destructive")}>
              <CalendarClock /> {t("a fine periodo ~{amount}", { amount: euro(b.projected) })}
            </Badge>
          )}
          {b.carried !== 0 && (
            <Badge variant="secondary" className="font-normal">
              <Repeat /> {t("riporto")} {b.carried > 0 ? "+" : "−"}
              {euro(Math.abs(b.carried))}
            </Badge>
          )}
          {b.reserved > 0 && (
            <Badge variant="secondary" className="font-normal">
              <PiggyBank /> {t("{amount} da parte", { amount: euro(b.reserved) })}
            </Badge>
          )}
        </div>
        <History b={b} />
      </div>
      {b.problems.map((p) => (
        <p key={p} className="flex items-start gap-1.5 text-xs text-warning">
          <TriangleAlert className="mt-0.5 size-3.5 shrink-0" /> {p}
        </p>
      ))}
    </button>
  );
}

/** Compact row for the Overview and Analisi cards. */
export function BudgetRow({ b }: { b: BudgetStatus }) {
  return (
    <div className="flex min-w-0 flex-1 flex-col gap-1.5">
      <div className="flex justify-between gap-2 text-sm">
        <span className="truncate font-medium">
          {b.name} <span className="font-normal text-muted-foreground">· {b.period_label}</span>
        </span>
        <span className="tabular shrink-0">
          {euro(b.value)} <span className="text-muted-foreground">/ {euro(b.limit)}</span>
        </span>
      </div>
      <Meter value={b.used} marker={b.phase === "active" ? b.elapsed : undefined} state={b.state} label={`${b.name}: ${t("{pct} usato", { pct: percent(b.used) })}`} />
      <span className={cn("text-xs", b.remaining < 0 && b.goal === "max_spend" ? "text-destructive" : "text-muted-foreground")}>{remainingText(b)}</span>
    </div>
  );
}
