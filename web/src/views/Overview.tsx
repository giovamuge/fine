import { useState } from "react";
import { ArrowRight, ChevronDown, Clock, OctagonAlert, RefreshCw } from "lucide-react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardAction, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "@/components/ui/collapsible";
import { Alerts } from "@/components/Alerts";
import { BudgetRow, risk } from "@/components/BudgetCard";
import { Meter } from "@/components/Meter";
import { PageHeader } from "@/components/PageHeader";
import { Empty, Loaded } from "@/components/States";
import { Sparkline } from "@/components/charts/Sparkline";
import { api } from "@/lib/api";
import { useApp } from "@/lib/app-state";
import { euro, fmtMonth, inEuro, integer, money, percent, relativeTime, signed } from "@/lib/format";
import { Link } from "@/lib/router";
import type { AccountView, BudgetState, Status } from "@/lib/types";
import { useAsync } from "@/lib/useAsync";
import { cn } from "@/lib/utils";
import { t, tn } from "@/lib/i18n";

export const CHANGE_LABEL: Record<number, string> = { 7: t("7 giorni"), 30: t("30 giorni"), 90: t("3 mesi"), 365: t("1 anno") };

export function ChangeBadges({ changes }: { changes: { days: number; change: number }[] }) {
  return (
    <div className="flex flex-wrap gap-1.5">
      {changes.map((c) => (
        <Badge
          key={c.days}
          variant="outline"
          className={cn("tabular font-normal", c.change > 0 && "border-positive/30 text-positive", c.change < 0 && "border-destructive/30 text-destructive")}
        >
          {CHANGE_LABEL[c.days] ?? t("{n} giorni", { n: c.days })} {signed(c.change)}
        </Badge>
      ))}
    </div>
  );
}

export function Overview() {
  const { version, sync, runSync } = useApp();
  const status = useAsync(() => api.status(), [version]);
  const balance = useAsync(() => api.balance(365), [version]);
  const accounts = useAsync(() => api.accounts(), [version]);
  const [allAlerts, setAllAlerts] = useState(false);

  return (
    <>
      <PageHeader
        title={t("Panoramica")}
        description={status.data ? `${fmtMonth(status.data.month)} · ${tn(status.data.days_left_in_month, "{n} giorno alla fine del mese", "{n} giorni alla fine del mese", { n: integer(status.data.days_left_in_month) })}` : " "}
        actions={<SyncLine accounts={accounts.data} running={sync.running} onSync={runSync} />}
      />

      <Card>
        <CardContent>
          <Loaded {...status} rows={4}>
            {(st) => <Hero st={st} />}
          </Loaded>
        </CardContent>
      </Card>

      {status.data && status.data.alerts.length > 0 && (() => {
        const st = status.data;
        const important = st.alerts.filter((a) => a.level !== "info");
        const info = st.alerts.length - important.length;
        return (
          <section className="flex flex-col gap-2" aria-label="Avvisi">
            <Alerts alerts={important.length && !allAlerts ? important : st.alerts} />
            {info > 0 && important.length > 0 && (
              <Button variant="ghost" size="sm" className="self-start text-muted-foreground" onClick={() => setAllAlerts((v) => !v)} aria-expanded={allAlerts}>
                {allAlerts ? t("Nascondi le informazioni") : tn(info, "Mostra {n} informazione", "Mostra {n} informazioni", { n: integer(info) })}
              </Button>
            )}
          </section>
        );
      })()}

      <div className="grid gap-6 lg:grid-cols-2">
        <Card>
          <CardHeader>
            <CardTitle>{t("Saldo")}</CardTitle>
            <CardDescription>{t("Totale dei conti in euro")}</CardDescription>
            <CardAction>
              <Button asChild variant="ghost" size="sm">
                <Link to="/analisi#saldo">
                  {t("Andamento")} <ArrowRight />
                </Link>
              </Button>
            </CardAction>
          </CardHeader>
          <CardContent>
            <Loaded {...balance} rows={3}>
              {(b) =>
                b.current === null ? (
                  <Empty title={t("Nessun saldo ancora.")}>{t("Arriva con la prima sincronizzazione dopo aver collegato un conto.")}</Empty>
                ) : (
                  <div className="flex flex-col gap-4">
                    <div className="flex items-end justify-between gap-4">
                      <span className="tabular text-3xl font-semibold tracking-tight">{euro(b.current)}</span>
                      <Sparkline values={b.points.slice(-30).map((p) => p.balance)} />
                    </div>
                    {b.changes.length > 0 && <ChangeBadges changes={b.changes} />}
                    {b.accounts.length > 1 && (
                      <ul className="flex flex-col divide-y text-sm">
                        {b.accounts.map((a) => (
                          <li key={a.account} className="flex justify-between gap-3 py-2">
                            <span className="min-w-0 truncate text-muted-foreground">
                              {a.account}
                              {!a.in_total && <span> · {t("escluso dal totale")}</span>}
                            </span>
                            <span className="flex flex-col items-end">
                              <span className="tabular">{money(a.balance, a.currency)}</span>
                              {a.balance !== 0 && inEuro(a.currency, a.balance_eur) && <span className="tabular text-xs text-muted-foreground">{inEuro(a.currency, a.balance_eur)}</span>}
                            </span>
                          </li>
                        ))}
                      </ul>
                    )}
                  </div>
                )
              }
            </Loaded>
          </CardContent>
        </Card>

        <Card>
          <CardHeader>
            <CardTitle>{t("Budget")}</CardTitle>
            <CardDescription>{t("I più vicini al limite")}</CardDescription>
            <CardAction>
              <Button asChild variant="ghost" size="sm">
                <Link to="/budget">
                  {t("Tutti")} <ArrowRight />
                </Link>
              </Button>
            </CardAction>
          </CardHeader>
          <CardContent>
            <Loaded {...status} rows={3}>
              {(st) => {
                const active = st.budgets.filter((b) => b.state !== "sospeso" && b.phase === "active");
                return active.length ? (
                  <ul className="flex flex-col gap-4">
                    {[...active]
                      .sort((a, b) => risk(b) - risk(a))
                      .slice(0, 4)
                      .map((b) => (
                        <li key={b.id}>
                          <Link to="/budget" className="flex rounded-md focus-visible:ring-3 focus-visible:ring-ring/50 focus-visible:outline-none">
                            <BudgetRow b={b} />
                          </Link>
                        </li>
                      ))}
                  </ul>
                ) : (
                  <Empty title={t("Nessun budget attivo.")}>
                    {t("Creane uno dalla sezione")} <Link to="/budget" className="underline underline-offset-4">{t("Budget")}</Link> {t("o chiedi alla tua AI: «aiutami a impostare il piano mensile».")}
                  </Empty>
                );
              }}
            </Loaded>
          </CardContent>
        </Card>
      </div>
    </>
  );
}

function Tile({ label, value, note, negative, to }: { label: string; value: string; note: string; negative?: boolean; to?: string }) {
  const body = (
    <>
      <span className="text-xs font-medium text-muted-foreground">{label}</span>
      <span className={cn("tabular text-xl font-semibold tracking-tight", negative && "text-destructive")}>{value}</span>
      <span className="flex items-center gap-1 text-xs text-muted-foreground">{note}</span>
    </>
  );
  const cls = "flex flex-col gap-1 rounded-xl border bg-muted/30 p-3";
  return to ? (
    <Link to={to} className={cn(cls, "transition-colors hover:bg-muted")}>
      {body}
    </Link>
  ) : (
    <div className={cls}>{body}</div>
  );
}

function Hero({ st }: { st: Status }) {
  const neg = st.available_month < 0;
  const pace: BudgetState = st.budget_used > 1 ? "sforato" : st.budget_used > st.month_elapsed + 0.15 ? "attenzione" : "ok";
  const rows: [string, string, boolean?][] = [
    [st.income_source === "planned" ? t("Entrate pianificate: ricevute + attese") : t("Entrate (media ultimi 3 mesi): ricevute + attese"), euro(st.income_received + st.income_expected_remaining)],
    [t("Spese fisse: pagate + ancora da pagare"), `− ${euro(st.fixed_paid + st.fixed_still_due)}`],
    [t("Accantonamenti per le spese annuali"), `− ${euro(st.sinking_fund_quota)}`],
    [t("Obiettivo di risparmio"), `− ${euro(st.savings_goal)}`],
    [t("Budget variabile"), euro(st.variable_budget), true],
    [t("Spese variabili già fatte"), `− ${euro(st.variable_spent)}`],
    ...(st.reserved > 0 ? ([[t("Messo da parte per i budget"), `− ${euro(st.reserved)}`]] as [string, string][]) : []),
  ];
  return (
    <div className="flex flex-col gap-5">
      <div className="flex flex-col gap-1">
        <span className="text-sm font-medium text-muted-foreground">{t("Puoi ancora spendere questo mese")}</span>
        <span className={cn("tabular text-5xl font-semibold tracking-tighter", neg && "text-destructive")}>{euro(st.available_month)}</span>
        <span className="text-sm text-muted-foreground">
          {neg
            ? t("Hai superato il budget variabile di {amount}: le prossime spese pesano sul risparmio.", { amount: euro(-st.available_month) })
            : t("Spese variabili {spent} su {budget}", { spent: euro(st.variable_spent), budget: euro(st.variable_budget) })}
        </span>
      </div>
      <Meter
        value={st.budget_used}
        marker={st.month_elapsed}
        markerLabel={t("oggi")}
        state={pace}
        label={t("Budget variabile usato: {used}, mese trascorso al {elapsed}", { used: percent(st.budget_used), elapsed: percent(st.month_elapsed) })}
      />
      <div className="grid grid-cols-2 gap-3 sm:grid-cols-3">
        <Tile label={t("Questa settimana")} value={euro(st.available_week)} note={t("già spesi {amount}", { amount: euro(st.spent_this_week) })} negative={st.available_week < 0} />
        <Tile label={t("Al giorno")} value={euro(st.available_per_day)} note={tn(st.days_left_in_month, "per {n} giorno", "per {n} giorni", { n: integer(st.days_left_in_month) })} negative={st.available_per_day < 0} />
        {st.uncategorized > 0 && <Tile label={t("Da categorizzare")} value={integer(st.uncategorized)} note={t("Sistemali ora →")} to="/movimenti?uncategorized=1" />}
      </div>
      <Collapsible>
        <CollapsibleTrigger asChild>
          <Button variant="link" size="sm" className="group px-0 text-muted-foreground">
            {t("Come è calcolato")} <ChevronDown className="transition-transform group-data-[state=open]:rotate-180" />
          </Button>
        </CollapsibleTrigger>
        <CollapsibleContent>
          <dl className="mt-2 flex flex-col divide-y rounded-lg border text-sm">
            {rows.map(([k, v, strong]) => (
              <div key={k} className={cn("flex justify-between gap-4 px-3 py-2", strong && "bg-muted/50 font-semibold")}>
                <dt className={cn(!strong && "text-muted-foreground")}>{k}</dt>
                <dd className="tabular shrink-0">{v}</dd>
              </div>
            ))}
          </dl>
        </CollapsibleContent>
      </Collapsible>
    </div>
  );
}

function SyncLine({ accounts, running, onSync }: { accounts: AccountView[] | null; running: boolean; onSync: () => void }) {
  if (!accounts) return null;
  const last = accounts.map((a) => a.last_sync_at).filter((x): x is string => !!x).sort().at(-1);
  const failed = accounts.some((a) => a.last_sync_ok === false);
  return (
    <>
      {accounts.length > 0 && (
        <span className={cn("flex items-center gap-1.5 text-sm", failed ? "text-destructive" : "text-muted-foreground")}>
          {failed ? <OctagonAlert className="size-4" /> : <Clock className="size-4" />}
          {last ? t("Aggiornato {when}", { when: relativeTime(last) }) : t("Mai sincronizzato")}
          {failed && ` · ${t("errori")}`}
        </span>
      )}
      <Button variant="outline" size="sm" onClick={onSync} disabled={running || accounts.length === 0}>
        <RefreshCw className={cn(running && "animate-spin")} />
        {running ? t("Sincronizzo…") : t("Sincronizza")}
      </Button>
    </>
  );
}

