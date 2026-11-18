// Budgets: named, rule-based limits and goals, created here or by the AI.
import { useState } from "react";
import { Pause, Pencil, Play, Plus, Sparkles, Target } from "lucide-react";
import { Button } from "@/components/ui/button";
import { DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuLabel, DropdownMenuSeparator, DropdownMenuTrigger } from "@/components/ui/dropdown-menu";
import { Separator } from "@/components/ui/separator";
import { BudgetCard, remainingText, risk } from "@/components/BudgetCard";
import { BudgetEditor } from "@/components/BudgetEditor";
import { ConfirmDialog } from "@/components/ConfirmDialog";
import { Meter } from "@/components/Meter";
import { PageHeader } from "@/components/PageHeader";
import { ResponsiveDialog } from "@/components/ResponsiveDialog";
import { Empty, Loaded } from "@/components/States";
import { api, errorMessage } from "@/lib/api";
import { useApp } from "@/lib/app-state";
import { GOAL_LABEL, STATE_LABEL, TEMPLATES, newBudget } from "@/lib/budget-rules";
import { CategoryLabel } from "@/components/CategoryPicker";
import { euro, fmtDateShort, percent } from "@/lib/format";
import { useToast } from "@/lib/toast";
import type { Budget, BudgetStatus } from "@/lib/types";
import { useAsync } from "@/lib/useAsync";
import { t } from "@/lib/i18n";

function NewMenu({ onPick }: { onPick: (b: Budget) => void }) {
  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button size="sm">
          <Plus /> {t("Nuovo budget")}
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end" className="w-72">
        <DropdownMenuItem onSelect={() => onPick(newBudget())}>
          <Plus /> {t("Da zero")}
        </DropdownMenuItem>
        <DropdownMenuSeparator />
        <DropdownMenuLabel className="text-xs font-normal text-muted-foreground">{t("Parti da un esempio")}</DropdownMenuLabel>
        {TEMPLATES.map((tpl) => (
          <DropdownMenuItem key={tpl.label} onSelect={() => onPick(tpl.budget())} className="flex flex-col items-start gap-0.5">
            <span>{tpl.label}</span>
            <span className="text-xs text-muted-foreground">{tpl.hint}</span>
          </DropdownMenuItem>
        ))}
      </DropdownMenuContent>
    </DropdownMenu>
  );
}

function Detail({ name, onClose, onEdit, onToggle }: { name: string | null; onClose: () => void; onEdit: (b: BudgetStatus) => void; onToggle: (b: BudgetStatus) => void }) {
  const { version } = useApp();
  const detail = useAsync(() => (name ? api.budget(name) : Promise.resolve(null)), [name, version]);
  const b = detail.data;
  return (
    <ResponsiveDialog
      open={!!name}
      onClose={onClose}
      title={name ?? ""}
      description={b ? `${GOAL_LABEL[b.goal]} ${euro(b.amount)} · ${b.period_label}` : undefined}
      footer={
        b && (
          <>
            <Button variant="outline" className="sm:mr-auto" onClick={() => onToggle(b)}>
              {b.budget.enabled === false ? <Play /> : <Pause />} {b.budget.enabled === false ? t("Riattiva") : t("Metti in pausa")}
            </Button>
            <Button onClick={() => onEdit(b)}>
              <Pencil /> {t("Modifica")}
            </Button>
          </>
        )
      }
    >
      <Loaded {...detail} rows={4}>
        {(b) =>
          b && (
            <div className="flex flex-col gap-4 pb-2">
              <div className="flex flex-col gap-2">
                <div className="flex items-baseline justify-between gap-2">
                  <span className="tabular text-3xl font-semibold tracking-tight">{euro(b.value)}</span>
                  <span className="tabular text-sm text-muted-foreground">
                    {t("su {limit}", { limit: euro(b.limit) })} · {STATE_LABEL[b.state]}
                  </span>
                </div>
                <Meter value={b.used} marker={b.phase === "active" ? b.elapsed : undefined} markerLabel={b.phase === "active" ? t("oggi") : undefined} state={b.state} label={t("{pct} usato", { pct: percent(b.used) })} />
                <span className="text-sm text-muted-foreground">
                  {remainingText(b)} · {fmtDateShort(b.start)} – {fmtDateShort(b.end)}
                </span>
              </div>
              <dl className="grid grid-cols-[auto_1fr] gap-x-4 gap-y-1.5 text-sm">
                <dt className="text-muted-foreground">{t("Conta")}</dt>
                <dd>{b.rule_summary}</dd>
                {b.carried !== 0 && (
                  <>
                    <dt className="text-muted-foreground">{t("Riporto")}</dt>
                    <dd className="tabular">
                      {b.carried > 0 ? "+" : "−"}
                      {t("{amount} dai periodi precedenti", { amount: euro(Math.abs(b.carried)) })}
                    </dd>
                  </>
                )}
                {b.projected !== null && b.goal === "max_spend" && (
                  <>
                    <dt className="text-muted-foreground">{t("Proiezione")}</dt>
                    <dd className="tabular">{t("~{amount} a fine periodo, al ritmo attuale", { amount: euro(b.projected) })}</dd>
                  </>
                )}
                {b.reserved > 0 && (
                  <>
                    <dt className="text-muted-foreground">{t("Da parte")}</dt>
                    <dd className="tabular">{t("{amount} tolti dal disponibile del mese", { amount: euro(b.reserved) })}</dd>
                  </>
                )}
                {b.budget.note && (
                  <>
                    <dt className="text-muted-foreground">{t("Nota")}</dt>
                    <dd>{b.budget.note}</dd>
                  </>
                )}
              </dl>
              {b.problems.map((p) => (
                <p key={p} className="text-sm text-warning">
                  {p}
                </p>
              ))}
              <Separator />
              <section className="flex flex-col gap-2">
                <h3 className="text-sm font-semibold">{t("Movimenti che contano in questo periodo")}</h3>
                {b.transactions?.length ? (
                  <ul className="flex flex-col divide-y rounded-lg border text-sm">
                    {b.transactions.map((tx) => (
                      <li key={`${tx.id}-${tx.date}`} className="flex items-center gap-3 px-3 py-2">
                        <span className="w-14 shrink-0 text-xs text-muted-foreground">{fmtDateShort(tx.date)}</span>
                        <span className="flex min-w-0 flex-1 flex-col">
                          <span className="truncate">{tx.merchant}</span>
                          <CategoryLabel name={tx.category} className="text-xs" />
                        </span>
                        <span className="tabular shrink-0">{euro(tx.amount)}</span>
                      </li>
                    ))}
                  </ul>
                ) : (
                  <p className="text-sm text-muted-foreground">{t("Nessuno, per ora.")}</p>
                )}
              </section>
            </div>
          )
        }
      </Loaded>
    </ResponsiveDialog>
  );
}

export function BudgetView() {
  const { version, refresh } = useApp();
  const toast = useToast();
  const list = useAsync(() => api.budgets(), [version]);
  const extras = useAsync(() => Promise.all([api.tags(), api.accounts()]), [version]);
  const [editing, setEditing] = useState<{ budget: Budget; original?: string } | null>(null);
  const [detail, setDetail] = useState<string | null>(null);
  const [deleting, setDeleting] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const tags = extras.data?.[0].map((tag) => tag.name) ?? [];
  const accounts = extras.data?.[1] ?? [];

  const toggle = async (b: BudgetStatus) => {
    try {
      await api.saveBudget({ ...b.budget, enabled: b.budget.enabled === false }, b.name);
      refresh();
      toast.success(b.budget.enabled === false ? t("«{name}» riattivato.", { name: b.name }) : t("«{name}» in pausa.", { name: b.name }));
    } catch (e) {
      toast.error(errorMessage(e));
    }
  };

  const remove = async () => {
    if (!deleting) return;
    setBusy(true);
    try {
      await api.deleteBudget(deleting);
      refresh();
      toast.success(t("Budget «{name}» eliminato.", { name: deleting }));
      setDeleting(null);
      setEditing(null);
      setDetail(null);
    } catch (e) {
      toast.error(errorMessage(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <>
      <PageHeader title={t("Budget")} description={t("Limiti e obiettivi con regole tue: categorie, tag, esercenti, conti, periodi")} actions={<NewMenu onPick={(budget) => setEditing({ budget })} />} />
      <Loaded {...list} rows={4}>
        {(budgets) =>
          budgets.length ? (
            <div className="grid gap-4 md:grid-cols-2 xl:grid-cols-3">
              {[...budgets]
                .sort((a, b) => risk(b) - risk(a))
                .map((b) => (
                  <BudgetCard key={b.id} b={b} onOpen={() => setDetail(b.name)} />
                ))}
            </div>
          ) : (
            <Empty title={t("Nessun budget, per ora.")} icon={<Target />} action={<NewMenu onPick={(budget) => setEditing({ budget })} />}>
              {t("Un budget può essere semplice («Ristoranti, 200 € al mese») o su misura: più categorie insieme, esclusioni per tag, un esercente, un viaggio tra due date, un obiettivo di risparmio, il mese che parte dal giorno dello stipendio.")}
              <span className="mt-2 flex items-center justify-center gap-1.5">
                <Sparkles className="size-4" /> {t("Oppure chiedi alla tua AI: «crea un budget per lo svago».")}
              </span>
            </Empty>
          )
        }
      </Loaded>

      <Detail
        name={detail}
        onClose={() => setDetail(null)}
        onEdit={(b) => {
          setDetail(null);
          setEditing({ budget: b.budget, original: b.name });
        }}
        onToggle={toggle}
      />
      <BudgetEditor
        open={!!editing}
        initial={editing?.budget ?? null}
        original={editing?.original}
        onClose={() => setEditing(null)}
        onDelete={editing?.original ? () => setDeleting(editing.original ?? null) : undefined}
        tags={tags}
        accounts={accounts}
      />
      <ConfirmDialog open={!!deleting} onClose={() => setDeleting(null)} onConfirm={remove} title={t("Eliminare «{name}»?", { name: deleting ?? "" })} confirmLabel={t("Elimina")} busy={busy} danger>
        {t("Il budget e le sue regole vengono eliminati. I movimenti non cambiano.")}
      </ConfirmDialog>
    </>
  );
}
