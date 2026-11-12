import { useState } from "react";
import { ArrowRight, ChevronDown, Clock, OctagonAlert, RefreshCw } from "lucide-react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardAction, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "@/components/ui/collapsible";
import { Alerts } from "@/components/Alerts";
import { Meter } from "@/components/Meter";
import { PageHeader } from "@/components/PageHeader";
import { Empty, Loaded } from "@/components/States";
import { Sparkline } from "@/components/charts/Sparkline";
import { api } from "@/lib/api";
import { useApp } from "@/lib/app-state";
import { CategoryIcon } from "@/lib/categories";
import { euro, fmtMonth, integer, money, percent, relativeTime, signed } from "@/lib/format";
import { Link } from "@/lib/router";
import type { AccountView, BudgetState, Status } from "@/lib/types";
import { useAsync } from "@/lib/useAsync";
import { cn } from "@/lib/utils";

export const CHANGE_LABEL: Record<number, string> = { 7: "7 giorni", 30: "30 giorni", 90: "3 mesi", 365: "1 anno" };

export function ChangeBadges({ changes }: { changes: { days: number; change: number }[] }) {
  return (
    <div className="flex flex-wrap gap-1.5">
      {changes.map((c) => (
        <Badge
          key={c.days}
          variant="outline"
          className={cn("tabular font-normal", c.change > 0 && "border-positive/30 text-positive", c.change < 0 && "border-destructive/30 text-destructive")}
        >
          {CHANGE_LABEL[c.days] ?? `${c.days} giorni`} {signed(c.change)}
        </Badge>
      ))}
    </div>
  );
}

export function Overview() {
  const { version, sync, runSync, categoryByName } = useApp();
  const status = useAsync(() => api.status(), [version]);
  const balance = useAsync(() => api.balance(365), [version]);
  const accounts = useAsync(() => api.accounts(), [version]);
  const [allAlerts, setAllAlerts] = useState(false);

  return (
    <>
      <PageHeader
        title="Panoramica"
        description={status.data ? `${fmtMonth(status.data.month)} · ${integer(status.data.days_left_in_month)} giorni alla fine del mese` : " "}
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
                {allAlerts ? "Nascondi le informazioni" : `Mostra ${integer(info)} ${info === 1 ? "informazione" : "informazioni"}`}
              </Button>
            )}
          </section>
        );
      })()}

      <div className="grid gap-6 lg:grid-cols-2">
        <Card>
          <CardHeader>
            <CardTitle>Saldo</CardTitle>
            <CardDescription>Totale dei conti in euro</CardDescription>
            <CardAction>
              <Button asChild variant="ghost" size="sm">
                <Link to="/analisi#saldo">
                  Andamento <ArrowRight />
                </Link>
              </Button>
            </CardAction>
          </CardHeader>
          <CardContent>
            <Loaded {...balance} rows={3}>
              {(b) =>
                b.current === null ? (
                  <Empty title="Nessun saldo ancora.">Arriva con la prima sincronizzazione dopo aver collegato un conto.</Empty>
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
                              {!a.in_total && <span> · escluso dal totale</span>}
                            </span>
                            <span className="tabular">{money(a.balance, a.currency)}</span>
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
            <CardTitle>Budget</CardTitle>
            <CardDescription>I più vicini al limite</CardDescription>
            <CardAction>
              <Button asChild variant="ghost" size="sm">
                <Link to="/analisi#budget">
                  Tutti <ArrowRight />
                </Link>
              </Button>
            </CardAction>
          </CardHeader>
          <CardContent>
            <Loaded {...status} rows={3}>
              {(st) =>
                st.budgets.length ? (
                  <ul className="flex flex-col gap-4">
                    {[...st.budgets]
                      .sort((a, b) => b.used - a.used)
                      .slice(0, 4)
                      .map((b) => (
                        <li key={`${b.category}-${b.period}`} className="flex items-center gap-3">
                          <CategoryIcon category={categoryByName.get(b.category.toLowerCase())} />
                          <div className="flex min-w-0 flex-1 flex-col gap-1.5">
                            <div className="flex justify-between gap-2 text-sm">
                              <span className="truncate font-medium">{b.category}</span>
                              <span className="tabular shrink-0">
                                {euro(b.spent)} <span className="text-muted-foreground">/ {euro(b.limit)}</span>
                              </span>
                            </div>
                            <Meter value={b.used} marker={b.elapsed} state={b.state} label={`${b.category}: ${percent(b.used)} del budget usato`} />
                          </div>
                        </li>
                      ))}
                  </ul>
                ) : (
                  <Empty title="Nessun budget impostato.">
                    Chiedi alla tua AI: «aiutami a impostare il piano mensile».
                  </Empty>
                )
              }
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
    [`Entrate ${st.income_source === "planned" ? "pianificate" : "(media ultimi 3 mesi)"}: ricevute + attese`, euro(st.income_received + st.income_expected_remaining)],
    ["Spese fisse: pagate + ancora da pagare", `− ${euro(st.fixed_paid + st.fixed_still_due)}`],
    ["Accantonamenti per le spese annuali", `− ${euro(st.sinking_fund_quota)}`],
    ["Obiettivo di risparmio", `− ${euro(st.savings_goal)}`],
    ["Budget variabile", euro(st.variable_budget), true],
    ["Spese variabili già fatte", `− ${euro(st.variable_spent)}`],
  ];
  return (
    <div className="flex flex-col gap-5">
      <div className="flex flex-col gap-1">
        <span className="text-sm font-medium text-muted-foreground">Puoi ancora spendere questo mese</span>
        <span className={cn("tabular text-5xl font-semibold tracking-tighter", neg && "text-destructive")}>{euro(st.available_month)}</span>
        <span className="text-sm text-muted-foreground">
          {neg
            ? `Hai superato il budget variabile di ${euro(-st.available_month)}: le prossime spese pesano sul risparmio.`
            : `Spese variabili ${euro(st.variable_spent)} su ${euro(st.variable_budget)}`}
        </span>
      </div>
      <Meter
        value={st.budget_used}
        marker={st.month_elapsed}
        markerLabel="oggi"
        state={pace}
        label={`Budget variabile usato: ${percent(st.budget_used)}, mese trascorso al ${percent(st.month_elapsed)}`}
      />
      <div className="grid grid-cols-2 gap-3 sm:grid-cols-3">
        <Tile label="Questa settimana" value={euro(st.available_week)} note={`già spesi ${euro(st.spent_this_week)}`} negative={st.available_week < 0} />
        <Tile label="Al giorno" value={euro(st.available_per_day)} note={`per ${integer(st.days_left_in_month)} giorni`} negative={st.available_per_day < 0} />
        {st.uncategorized > 0 && <Tile label="Da categorizzare" value={integer(st.uncategorized)} note="Sistemali ora →" to="/movimenti?uncategorized=1" />}
      </div>
      <Collapsible>
        <CollapsibleTrigger asChild>
          <Button variant="link" size="sm" className="group px-0 text-muted-foreground">
            Come è calcolato <ChevronDown className="transition-transform group-data-[state=open]:rotate-180" />
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
          {last ? `Aggiornato ${relativeTime(last)}` : "Mai sincronizzato"}
          {failed && " · errori"}
        </span>
      )}
      <Button variant="outline" size="sm" onClick={onSync} disabled={running || accounts.length === 0}>
        <RefreshCw className={cn(running && "animate-spin")} />
        {running ? "Sincronizzo…" : "Sincronizza"}
      </Button>
    </>
  );
}

