// Create or edit a budget: goal, amount, period, rule (visual or JSON), options, live preview.
import { useEffect, useMemo, useState } from "react";
import { ChevronDown, Code2, Loader2, Trash2, Wand2 } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "@/components/ui/collapsible";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { Separator } from "@/components/ui/separator";
import { Switch } from "@/components/ui/switch";
import { Textarea } from "@/components/ui/textarea";
import { Meter } from "@/components/Meter";
import { ResponsiveDialog } from "@/components/ResponsiveDialog";
import { RuleBuilder } from "@/components/RuleBuilder";
import { api, errorMessage } from "@/lib/api";
import { GOAL_LABEL, PERIOD_TYPES, STATE_LABEL, defaultPeriod, formToRule, ruleToForm, type RuleForm } from "@/lib/budget-rules";
import { euro, fmtDateShort, integer } from "@/lib/format";
import { useToast } from "@/lib/toast";
import type { AccountView, Budget, BudgetGoal, BudgetPeriod, BudgetStatus } from "@/lib/types";
import { useApp } from "@/lib/app-state";
import { cn } from "@/lib/utils";
import { LOCALE, t } from "@/lib/i18n";

const MONTHS = Array.from({ length: 12 }, (_, i) => new Intl.DateTimeFormat(LOCALE, { month: "long" }).format(new Date(2024, i, 1)));
// Monday first; 1 Jan 2024 was a Monday.
const DAYS = Array.from({ length: 7 }, (_, i) => new Intl.DateTimeFormat(LOCALE, { weekday: "long" }).format(new Date(2024, 0, 1 + i)));

const EMPTY_HINT: Record<BudgetGoal, string> = {
  max_spend: t("Senza condizioni contano tutte le spese."),
  min_save: t("Senza condizioni contano tutti i movimenti verso risparmi e investimenti (al netto dei prelievi)."),
  min_income: t("Senza condizioni contano tutte le entrate."),
};

function Field({ label, htmlFor, hint, children, className }: { label: string; htmlFor?: string; hint?: string; children: React.ReactNode; className?: string }) {
  return (
    <div className={cn("flex flex-col gap-1.5", className)}>
      <Label htmlFor={htmlFor}>{label}</Label>
      {children}
      {hint && <span className="text-xs text-muted-foreground">{hint}</span>}
    </div>
  );
}

function SwitchRow({ id, checked, onChange, title, hint, disabled }: { id: string; checked: boolean; onChange: (v: boolean) => void; title: string; hint: string; disabled?: boolean }) {
  return (
    <div className={cn("flex items-start justify-between gap-3", disabled && "opacity-50")}>
      <Label htmlFor={id} className="flex flex-col items-start gap-1 font-normal">
        <span className="font-medium">{title}</span>
        <span className="text-xs text-muted-foreground">{hint}</span>
      </Label>
      <Switch id={id} checked={checked} onCheckedChange={onChange} disabled={disabled} />
    </div>
  );
}

function PeriodEditor({ value, onChange }: { value: BudgetPeriod; onChange: (p: BudgetPeriod) => void }) {
  return (
    <div className="flex flex-col gap-2">
      <Select value={value.type} onValueChange={(type) => onChange(defaultPeriod(type as BudgetPeriod["type"]))}>
        <SelectTrigger className="w-full" aria-label={t("Periodo")}>
          <SelectValue />
        </SelectTrigger>
        <SelectContent>
          {PERIOD_TYPES.map((p) => (
            <SelectItem key={p.type} value={p.type}>
              {p.label}
            </SelectItem>
          ))}
        </SelectContent>
      </Select>
      {value.type === "week" && (
        <Select value={String(value.start_weekday ?? 1)} onValueChange={(v) => onChange({ type: "week", start_weekday: Number(v) === 1 ? undefined : Number(v) })}>
          <SelectTrigger className="w-full" aria-label={t("La settimana parte di")}>
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            {DAYS.map((d, i) => (
              <SelectItem key={d} value={String(i + 1)}>
                {t("Parte di {day}", { day: d })}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      )}
      {value.type === "month" && (
        <div className="flex items-center gap-2 text-sm">
          <span className="text-muted-foreground">{t("Parte il giorno")}</span>
          <Input
            inputMode="numeric"
            className="w-16"
            defaultValue={value.start_day ?? 1}
            aria-label={t("Il mese parte il giorno")}
            onChange={(e) => {
              const n = Math.min(28, Math.max(1, Number(e.target.value) || 1));
              onChange({ type: "month", start_day: n === 1 ? undefined : n });
            }}
          />
          <span className="text-xs text-muted-foreground">{t("es. 27, il giorno dello stipendio")}</span>
        </div>
      )}
      {value.type === "year" && (
        <Select value={String(value.start_month ?? 1)} onValueChange={(v) => onChange({ type: "year", start_month: Number(v) === 1 ? undefined : Number(v) })}>
          <SelectTrigger className="w-full" aria-label={t("L'anno parte da")}>
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            {MONTHS.map((m, i) => (
              <SelectItem key={m} value={String(i + 1)}>
                {t("Parte da {month}", { month: m })}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      )}
      {value.type === "rolling" && (
        <div className="flex items-center gap-2 text-sm">
          <span className="text-muted-foreground">{t("Gli ultimi")}</span>
          <Input inputMode="numeric" className="w-20" defaultValue={value.days} aria-label={t("Numero di giorni")} onChange={(e) => onChange({ type: "rolling", days: Math.min(366, Math.max(1, Number(e.target.value) || 1)) })} />
          <span className="text-muted-foreground">{t("giorni, fino a oggi")}</span>
        </div>
      )}
      {value.type === "custom" && (
        <div className="grid grid-cols-2 gap-2">
          <Input type="date" value={value.from} onChange={(e) => onChange({ ...value, from: e.target.value })} aria-label={t("Dal")} />
          <Input type="date" value={value.to} onChange={(e) => onChange({ ...value, to: e.target.value })} aria-label={t("Al")} />
        </div>
      )}
    </div>
  );
}

function Preview({ state }: { state: { data: BudgetStatus | null; error: string | null; loading: boolean } }) {
  const p = state.data;
  return (
    <section className="flex flex-col gap-3 rounded-xl border bg-muted/30 p-3" aria-live="polite">
      <div className="flex items-center justify-between">
        <h3 className="text-sm font-semibold">{t("Anteprima")}</h3>
        {state.loading && <Loader2 className="size-4 animate-spin text-muted-foreground" />}
      </div>
      {state.error ? (
        <p className="text-sm text-destructive">{state.error}</p>
      ) : !p ? (
        <p className="text-sm text-muted-foreground">{t("Indica un importo per vedere quanto risulta oggi.")}</p>
      ) : (
        <>
          <div className="flex items-baseline justify-between gap-2 text-sm">
            <span className="text-muted-foreground">
              {p.phase === "upcoming" ? t("Non ancora iniziato") : `${fmtDateShort(p.start)} – ${fmtDateShort(p.end)}`} · {STATE_LABEL[p.state]}
            </span>
            <span className="tabular font-medium">
              {euro(p.value)} <span className="text-muted-foreground">/ {euro(p.limit)}</span>
            </span>
          </div>
          <Meter value={p.used} marker={p.phase === "active" ? p.elapsed : undefined} state={p.state} label={t("Anteprima: {pct}% usato", { pct: Math.round(p.used * 100) })} />
          <p className="text-xs text-muted-foreground">{t("Conta: {rule}.", { rule: p.rule_summary })}</p>
          {p.transactions && p.transactions.length > 0 ? (
            <ul className="flex flex-col divide-y rounded-lg border bg-background text-sm">
              {p.transactions.slice(0, 6).map((tx) => (
                <li key={`${tx.id}-${tx.date}`} className="flex items-center justify-between gap-2 px-3 py-1.5">
                  <span className="min-w-0 truncate">
                    <span className="text-muted-foreground">{fmtDateShort(tx.date)}</span> {tx.merchant}
                  </span>
                  <span className="tabular shrink-0">{euro(tx.amount)}</span>
                </li>
              ))}
              {p.transactions.length > 6 && <li className="px-3 py-1.5 text-xs text-muted-foreground">{t("…e altri {n} movimenti", { n: integer(p.transactions.length - 6) })}</li>}
            </ul>
          ) : (
            <p className="text-xs text-muted-foreground">{t("Nessun movimento corrisponde nel periodo attuale.")}</p>
          )}
        </>
      )}
    </section>
  );
}

export function BudgetEditor({
  open,
  initial,
  original,
  onClose,
  onDelete,
  tags,
  accounts,
}: {
  open: boolean;
  initial: Budget | null;
  /** Name of the budget being edited (absent for a new one). */
  original?: string;
  onClose: () => void;
  onDelete?: () => void;
  tags: string[];
  accounts: AccountView[];
}) {
  const { refresh } = useApp();
  const toast = useToast();
  const [b, setB] = useState<Budget | null>(initial);
  const [amountText, setAmountText] = useState("");
  const [form, setForm] = useState<RuleForm | null>(null);
  const [json, setJson] = useState<string | null>(null);
  const [alertsText, setAlertsText] = useState("");
  const [busy, setBusy] = useState(false);
  const [preview, setPreview] = useState<{ data: BudgetStatus | null; error: string | null; loading: boolean }>({ data: null, error: null, loading: false });

  useEffect(() => {
    if (!open || !initial) return;
    setB(initial);
    setAmountText(initial.amount ? String(initial.amount).replace(".", ",") : "");
    const f = ruleToForm(initial.rule);
    setForm(f);
    setJson(f ? null : JSON.stringify(initial.rule, null, 2));
    setAlertsText((initial.alerts ?? [80, 100]).join(", "));
    setPreview({ data: null, error: null, loading: false });
  }, [open, initial]);

  // The budget as it would be saved, or an error message.
  const candidate = useMemo((): { budget?: Budget; error?: string } => {
    if (!b) return {};
    const raw = amountText.trim();
    const amount = Number(raw.includes(",") ? raw.replace(/\./g, "").replace(",", ".") : raw);
    let rule = b.rule;
    if (json !== null) {
      if (!json.trim()) rule = undefined;
      else
        try {
          rule = JSON.parse(json);
        } catch {
          return { error: t("JSON della regola non valido.") };
        }
    } else if (form) {
      rule = formToRule(form);
    }
    const alerts = alertsText
      .split(/[,\s]+/)
      .map((s) => Number(s))
      .filter((n) => Number.isFinite(n) && n > 0);
    return { budget: { ...b, amount: Number.isFinite(amount) ? amount : 0, rule, alerts } };
  }, [b, amountText, form, json, alertsText]);

  const key = JSON.stringify(candidate);
  useEffect(() => {
    if (!open) return;
    if (candidate.error) {
      setPreview({ data: null, error: candidate.error, loading: false });
      return;
    }
    const budget = candidate.budget;
    if (!budget || !(budget.amount > 0)) {
      setPreview({ data: null, error: null, loading: false });
      return;
    }
    let cancelled = false;
    setPreview((p) => ({ ...p, loading: true }));
    const timer = setTimeout(() => {
      api.previewBudget({ ...budget, name: budget.name.trim() || t("Anteprima") }).then(
        (data) => !cancelled && setPreview({ data, error: null, loading: false }),
        (e: unknown) => !cancelled && setPreview({ data: null, error: errorMessage(e), loading: false }),
      );
    }, 400);
    return () => {
      cancelled = true;
      clearTimeout(timer);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [key, open]);

  if (!b) return null;
  const set = (patch: Partial<Budget>) => setB({ ...b, ...patch });
  const goal = b.goal ?? "max_spend";
  const spend = goal === "max_spend";
  const recurring = ["week", "month", "quarter", "year"].includes(b.period.type);

  const save = async () => {
    if (candidate.error || !candidate.budget) return;
    setBusy(true);
    try {
      const saved = await api.saveBudget(candidate.budget, original);
      refresh();
      toast.success(original ? t("Budget «{name}» aggiornato.", { name: saved.name }) : t("Budget «{name}» creato.", { name: saved.name }));
      onClose();
    } catch (e) {
      toast.error(errorMessage(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <ResponsiveDialog
      open={open}
      onClose={onClose}
      title={original ? t("Modifica budget") : t("Nuovo budget")}
      description={t("Scegli cosa conta, quanto e per quale periodo. L'anteprima mostra il risultato di oggi.")}
      className="sm:max-w-2xl"
      footer={
        <>
          {original && onDelete && (
            <Button variant="destructive" className="sm:mr-auto" onClick={onDelete} disabled={busy}>
              <Trash2 /> {t("Elimina")}
            </Button>
          )}
          <Button variant="outline" onClick={onClose} disabled={busy}>
            {t("Annulla")}
          </Button>
          <Button onClick={save} disabled={busy || !b.name.trim() || !candidate.budget || !(candidate.budget.amount > 0)}>
            {busy && <Loader2 className="animate-spin" />}
            {original ? t("Salva") : t("Crea")}
          </Button>
        </>
      }
    >
      <div className="flex flex-col gap-5 pb-2">
        <Field label={t("Nome")} htmlFor="b-name">
          <Input id="b-name" value={b.name} onChange={(e) => set({ name: e.target.value })} maxLength={60} placeholder={t("Es. Svago, Viaggio Londra, Amazon")} autoFocus={!original} />
        </Field>

        <div className="grid gap-4 sm:grid-cols-2">
          <Field label={t("Obiettivo")}>
            <Select
              value={goal}
              onValueChange={(g) => set({ goal: g as BudgetGoal, ...(g !== "max_spend" ? { rollover: "none", reserve: false } : {}) })}
            >
              <SelectTrigger className="w-full">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                {(Object.keys(GOAL_LABEL) as BudgetGoal[]).map((g) => (
                  <SelectItem key={g} value={g}>
                    {GOAL_LABEL[g]}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </Field>
          <Field label={t("Importo")} htmlFor="b-amount">
            <div className="relative">
              <Input id="b-amount" inputMode="decimal" value={amountText} onChange={(e) => setAmountText(e.target.value)} placeholder="0" className="pr-8" />
              <span className="pointer-events-none absolute top-1/2 right-3 -translate-y-1/2 text-sm text-muted-foreground">€</span>
            </div>
          </Field>
        </div>

        <Field label={t("Periodo")}>
          <PeriodEditor value={b.period} onChange={(period) => set({ period, ...(["rolling", "custom"].includes(period.type) ? { rollover: "none" } : {}) })} />
        </Field>

        <Separator />
        <section className="flex flex-col gap-3">
          <div className="flex items-center justify-between gap-2">
            <h3 className="text-sm font-semibold">{t("Cosa conta")}</h3>
            {json === null ? (
              <Button type="button" variant="ghost" size="sm" onClick={() => setJson(JSON.stringify(form ? formToRule(form) ?? null : b.rule ?? null, null, 2).replace(/^null$/, ""))}>
                <Code2 /> {t("Modifica come JSON")}
              </Button>
            ) : (
              <Button
                type="button"
                variant="ghost"
                size="sm"
                disabled={(() => {
                  try {
                    return ruleToForm(json.trim() ? JSON.parse(json) : undefined) === null;
                  } catch {
                    return true;
                  }
                })()}
                onClick={() => {
                  setForm(ruleToForm(json.trim() ? JSON.parse(json) : undefined));
                  setJson(null);
                }}
              >
                <Wand2 /> {t("Editor visuale")}
              </Button>
            )}
          </div>
          {json !== null ? (
            <Field label={t("Regola (JSON)")} htmlFor="b-json" hint={t('Combina condizioni con "all", "any" e "not": es. {example}', { example: '{"all": [{"category": ["Ristoranti e bar"]}, {"not": {"tag": ["lavoro"]}}]}' })}>
              <Textarea id="b-json" value={json} onChange={(e) => setJson(e.target.value)} rows={8} className="font-mono text-xs" spellCheck={false} />
            </Field>
          ) : (
            form && <RuleBuilder value={form} onChange={setForm} emptyHint={EMPTY_HINT[goal]} tags={tags} accounts={accounts} />
          )}
        </section>

        <Collapsible>
          <CollapsibleTrigger asChild>
            <Button variant="link" size="sm" className="group self-start px-0 text-muted-foreground">
              {t("Opzioni avanzate")} <ChevronDown className="transition-transform group-data-[state=open]:rotate-180" />
            </Button>
          </CollapsibleTrigger>
          <CollapsibleContent className="flex flex-col gap-4 pt-2">
            {spend && (
              <div className="grid gap-4 sm:grid-cols-2">
                <Field label={t("Riporto")} hint={recurring ? t("Cosa passa al periodo successivo.") : t("Solo per i periodi che si ripetono.")}>
                  <Select value={b.rollover ?? "none"} onValueChange={(v) => set({ rollover: v as Budget["rollover"] })} disabled={!recurring}>
                    <SelectTrigger className="w-full">
                      <SelectValue />
                    </SelectTrigger>
                    <SelectContent>
                      <SelectItem value="none">{t("Nessuno")}</SelectItem>
                      <SelectItem value="surplus">{t("Il non speso si aggiunge")}</SelectItem>
                      <SelectItem value="deficit">{t("Lo sforamento si toglie")}</SelectItem>
                      <SelectItem value="both">{t("Entrambi")}</SelectItem>
                    </SelectContent>
                  </Select>
                </Field>
                <Field label={t("Tetto del riporto")} htmlFor="b-cap" hint={t("Facoltativo, in euro.")}>
                  <Input
                    id="b-cap"
                    inputMode="decimal"
                    disabled={!recurring || (b.rollover ?? "none") === "none"}
                    defaultValue={b.rollover_cap ?? ""}
                    onChange={(e) => {
                      const n = Number(e.target.value.replace(",", "."));
                      set({ rollover_cap: e.target.value.trim() && n > 0 ? n : undefined });
                    }}
                  />
                </Field>
                <Field label={t("Come contare le spese")} className="sm:col-span-2">
                  <Select value={b.basis ?? "amortized"} onValueChange={(v) => set({ basis: v as Budget["basis"] })}>
                    <SelectTrigger className="w-full">
                      <SelectValue />
                    </SelectTrigger>
                    <SelectContent>
                      <SelectItem value="amortized">{t("Come il resto di fine: ammortamenti a quote, spese annuali pianificate escluse")}</SelectItem>
                      <SelectItem value="cash">{t("Per intero, nel giorno del movimento")}</SelectItem>
                    </SelectContent>
                  </Select>
                </Field>
              </div>
            )}
            <Field label={t("Avvisi alle soglie (%)")} htmlFor="b-alerts" hint={t("Es. 50, 80, 100: un avviso quando li superi.")}>
              <Input id="b-alerts" value={alertsText} onChange={(e) => setAlertsText(e.target.value)} inputMode="numeric" />
            </Field>
            <SwitchRow id="b-pace" checked={b.pace_alert ?? true} onChange={(v) => set({ pace_alert: v })} title={t("Avviso sul ritmo")} hint={spend ? t("Se spendi più in fretta del tempo che passa.") : t("Se sei indietro rispetto al tempo che passa.")} />
            {spend && (
              <SwitchRow
                id="b-reserve"
                checked={b.reserve ?? false}
                onChange={(v) => set({ reserve: v })}
                title={t("Metti da parte dal disponibile")}
                hint={t("La parte non ancora spesa (di questo mese) viene tolta da quanto puoi spendere: utile per un viaggio o una spesa prevista.")}
              />
            )}
            <SwitchRow id="b-enabled" checked={b.enabled ?? true} onChange={(v) => set({ enabled: v })} title={t("Attivo")} hint={t("In pausa resta salvato ma non conta e non avvisa.")} />
            <Field label={t("Nota")} htmlFor="b-note">
              <Textarea id="b-note" value={b.note ?? ""} onChange={(e) => set({ note: e.target.value })} rows={2} maxLength={500} />
            </Field>
          </CollapsibleContent>
        </Collapsible>

        <Preview state={preview} />
      </div>
    </ResponsiveDialog>
  );
}
