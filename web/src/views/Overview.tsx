import { useState } from "react";
import { api } from "../lib/api";
import { useApp } from "../lib/app-state";
import { useAsync } from "../lib/useAsync";
import { euro, fmtMonth, integer, percent, relativeTime, signed } from "../lib/format";
import { Link } from "../lib/router";
import { Alerts } from "../components/Alerts";
import { Meter } from "../components/Meter";
import { Empty, Loaded } from "../components/States";
import { Sparkline } from "../components/charts/Sparkline";
import { Icon } from "../components/Icon";
import type { AccountView, Status } from "../lib/types";

const CHANGE_LABEL: Record<number, string> = { 7: "7 giorni", 30: "30 giorni", 90: "3 mesi", 365: "1 anno" };

export function Overview() {
  const { version, sync, runSync } = useApp();
  const status = useAsync(() => api.status(), [version]);
  const balance = useAsync(() => api.balance(365), [version]);
  const accounts = useAsync(() => api.accounts(), [version]);
  const [allAlerts, setAllAlerts] = useState(false);

  return (
    <>
      <header className="page-head">
        <div>
          <h1>Panoramica</h1>
          <p className="sub">{status.data ? `${fmtMonth(status.data.month)} · ${integer(status.data.days_left_in_month)} giorni alla fine del mese` : " "}</p>
        </div>
        <SyncLine accounts={accounts.data} running={sync.running} onSync={runSync} />
      </header>

      <section className="card hero" aria-labelledby="hero-title">
        <Loaded {...status} rows={4}>
          {(st) => <Hero st={st} />}
        </Loaded>
      </section>

      <Loaded {...status} rows={2}>
        {(st) => {
          const critical = st.alerts.filter((a) => a.level !== "info");
          const info = st.alerts.filter((a) => a.level === "info");
          if (!st.alerts.length) return null;
          return (
            <section className="card" aria-labelledby="alerts-title">
              <div className="card-head">
                <h2 id="alerts-title">Avvisi</h2>
                {info.length > 0 && critical.length > 0 && (
                  <button type="button" className="btn sm ghost" onClick={() => setAllAlerts((v) => !v)} aria-expanded={allAlerts}>
                    {allAlerts ? "Nascondi le informazioni" : `${integer(info.length)} ${info.length === 1 ? "informazione" : "informazioni"}`}
                  </button>
                )}
              </div>
              <Alerts alerts={critical.length && !allAlerts ? critical : st.alerts} />
            </section>
          );
        }}
      </Loaded>

      <div className="grid-2">
        <section className="card" aria-labelledby="bal-title">
          <div className="card-head">
            <h2 id="bal-title">Saldo</h2>
            <Link to="/analisi#saldo" className="btn sm ghost">
              Andamento <Icon name="chevronR" size={14} />
            </Link>
          </div>
          <Loaded {...balance} rows={3}>
            {(b) => (
              <div className="stack">
                <div className="row-between" style={{ alignItems: "flex-end" }}>
                  <div className="stat">
                    <span className="label">Totale conti in euro</span>
                    <span className="value">{b.current === null ? "—" : euro(b.current)}</span>
                  </div>
                  <Sparkline values={b.points.slice(-30).map((p) => p.balance)} />
                </div>
                {b.changes.length > 0 && (
                  <div className="deltas">
                    {b.changes.map((c) => (
                      <span key={c.days} className={`badge ${c.change > 0 ? "ok" : c.change < 0 ? "bad" : "neutral"}`}>
                        {CHANGE_LABEL[c.days] ?? `${c.days} giorni`}: {signed(c.change)}
                      </span>
                    ))}
                  </div>
                )}
                {b.current === null && (
                  <Empty title="Nessun saldo ancora.">Arriva con la prima sincronizzazione dopo aver collegato un conto.</Empty>
                )}
                {b.accounts.length > 0 && (
                  <div className="list small">
                    {b.accounts.map((a) => (
                      <div key={a.account} className="kv">
                        <span className="k">
                          {a.account}
                          {!a.in_total && <span className="muted"> · non in euro, escluso dal totale</span>}
                        </span>
                        <span className="v">{a.currency === "EUR" ? euro(a.balance) : `${a.balance.toLocaleString("it-IT", { minimumFractionDigits: 2 })} ${a.currency}`}</span>
                      </div>
                    ))}
                  </div>
                )}
              </div>
            )}
          </Loaded>
        </section>

        <section className="card" aria-labelledby="budget-title">
          <div className="card-head">
            <h2 id="budget-title">Budget</h2>
            <Link to="/analisi#budget" className="btn sm ghost">
              Tutti <Icon name="chevronR" size={14} />
            </Link>
          </div>
          <Loaded {...status} rows={3}>
            {(st) =>
              st.budgets.length ? (
                <div className="bars">
                  {[...st.budgets]
                    .sort((a, b) => b.used - a.used)
                    .slice(0, 4)
                    .map((b) => (
                      <div key={`${b.category}-${b.period}`} className="bar-row">
                        <span className="name">{b.category}</span>
                        <span className="amt">
                          {euro(b.spent)} <span className="share">su {euro(b.limit)}</span>
                        </span>
                        <span style={{ gridColumn: "1 / -1" }}>
                          <Meter value={b.used} marker={b.elapsed} state={b.state} thin label={`${b.category}: ${percent(b.used)} del budget usato`} />
                        </span>
                      </div>
                    ))}
                </div>
              ) : (
                <Empty title="Nessun budget impostato.">
                  Chiedi alla tua AI: <code>aiutami a impostare il piano mensile</code>
                </Empty>
              )
            }
          </Loaded>
        </section>
      </div>
    </>
  );
}

function Hero({ st }: { st: Status }) {
  const neg = st.available_month < 0;
  const paceState = st.budget_used > 1 ? "sforato" : st.budget_used > st.month_elapsed + 0.15 ? "attenzione" : "ok";
  return (
    <>
      <div className="label" id="hero-title">
        Puoi ancora spendere questo mese
      </div>
      <div className={`value${neg ? " neg" : ""}`}>{euro(st.available_month)}</div>
      <p className="small muted">
        {neg
          ? `Hai superato il budget variabile di ${euro(-st.available_month)}: le prossime spese pesano sul risparmio.`
          : `Spese variabili ${euro(st.variable_spent)} su ${euro(st.variable_budget)} · la tacca indica dove dovresti essere oggi`}
      </p>
      <div style={{ marginTop: 14 }}>
        <Meter value={st.budget_used} marker={st.month_elapsed} markerLabel="oggi" state={paceState} label={`Budget variabile usato: ${percent(st.budget_used)}, mese trascorso al ${percent(st.month_elapsed)}`} />
      </div>
      <div className="tiles">
        <div className="tile">
          <div className="label">Questa settimana</div>
          <div className={`value${st.available_week < 0 ? " neg-text" : ""}`}>{euro(st.available_week)}</div>
          <div className="note">già spesi {euro(st.spent_this_week)}</div>
        </div>
        <div className="tile">
          <div className="label">Al giorno</div>
          <div className={`value${st.available_per_day < 0 ? " neg-text" : ""}`}>{euro(st.available_per_day)}</div>
          <div className="note">per {integer(st.days_left_in_month)} giorni</div>
        </div>
        {st.uncategorized > 0 && (
          <Link to="/movimenti?uncategorized=1" className="tile" style={{ textDecoration: "none", color: "inherit" }}>
            <div className="label">Da categorizzare</div>
            <div className="value">{integer(st.uncategorized)}</div>
            <div className="note" style={{ color: "var(--accent-ink)" }}>
              Sistemali ora <Icon name="chevronR" size={12} />
            </div>
          </Link>
        )}
      </div>
      <details style={{ marginTop: 14 }}>
        <summary className="small" style={{ cursor: "pointer", color: "var(--accent-ink)" }}>
          Come è calcolato
        </summary>
        <div className="list small" style={{ marginTop: 8 }}>
          <div className="kv">
            <span className="k">Entrate {st.income_source === "planned" ? "(pianificate)" : "(media ultimi 3 mesi)"}: ricevute + attese</span>
            <span className="v">{euro(st.income_received + st.income_expected_remaining)}</span>
          </div>
          <div className="kv">
            <span className="k">Spese fisse: pagate + ancora da pagare</span>
            <span className="v">− {euro(st.fixed_paid + st.fixed_still_due)}</span>
          </div>
          <div className="kv">
            <span className="k">Accantonamenti per le spese annuali</span>
            <span className="v">− {euro(st.sinking_fund_quota)}</span>
          </div>
          <div className="kv">
            <span className="k">Obiettivo di risparmio</span>
            <span className="v">− {euro(st.savings_goal)}</span>
          </div>
          <div className="kv">
            <span className="k">
              <strong>Budget variabile</strong>
            </span>
            <span className="v">
              <strong>{euro(st.variable_budget)}</strong>
            </span>
          </div>
          <div className="kv">
            <span className="k">Spese variabili già fatte</span>
            <span className="v">− {euro(st.variable_spent)}</span>
          </div>
        </div>
      </details>
    </>
  );
}

function SyncLine({ accounts, running, onSync }: { accounts: AccountView[] | null; running: boolean; onSync: () => void }) {
  if (!accounts) return null;
  const last = accounts.map((a) => a.last_sync_at).filter((x): x is string => !!x).sort().at(-1);
  const failed = accounts.some((a) => a.last_sync_ok === false);
  return (
    <div className="inline small">
      {accounts.length > 0 && (
        <span className={failed ? "neg-text" : "muted"}>
          {failed ? <Icon name="alert" size={14} /> : <Icon name="clock" size={14} />}{" "}
          {last ? `Aggiornato ${relativeTime(last)}` : "Mai sincronizzato"}
          {failed ? " · errori" : ""}
        </span>
      )}
      <button type="button" className="btn sm" onClick={onSync} disabled={running || accounts.length === 0} title="Scarica i nuovi movimenti (max 4 volte al giorno per conto)">
        <Icon name="sync" size={15} className={running ? "spin" : undefined} />
        {running ? "Sincronizzo…" : "Sincronizza"}
      </button>
    </div>
  );
}
