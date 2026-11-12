import { useEffect, useState } from "react";
import { api } from "../lib/api";
import { useApp } from "../lib/app-state";
import { useAsync } from "../lib/useAsync";
import { href, navigate, useRoute } from "../lib/router";
import { CADENCE_SHORT, PERIOD_LABEL, addMonths, euro, fmtDateShortYear, fmtMonth, integer, iso, percent, signed, ym } from "../lib/format";
import { BarList } from "../components/BarList";
import { BalanceChart } from "../components/charts/BalanceChart";
import { TrendChart } from "../components/charts/TrendChart";
import { Meter } from "../components/Meter";
import { Empty, Loaded } from "../components/States";
import { Icon } from "../components/Icon";

type TagPeriod = "month" | "30" | "year" | "all";
const TAG_PERIODS: { id: TagPeriod; label: string }[] = [
  { id: "month", label: "Questo mese" },
  { id: "30", label: "30 giorni" },
  { id: "year", label: "Anno" },
  { id: "all", label: "Sempre" },
];
const RANGES: { days: number; label: string }[] = [
  { days: 30, label: "30 giorni" },
  { days: 90, label: "3 mesi" },
  { days: 365, label: "1 anno" },
  { days: 3650, label: "Tutto" },
];
const CHANGE_LABEL: Record<number, string> = { 7: "7 giorni", 30: "30 giorni", 90: "3 mesi", 365: "1 anno" };

function tagRange(p: TagPeriod): { from?: string } {
  const now = new Date();
  if (p === "month") return { from: iso(new Date(now.getFullYear(), now.getMonth(), 1)) };
  if (p === "30") return { from: iso(new Date(now.getFullYear(), now.getMonth(), now.getDate() - 30)) };
  if (p === "year") return { from: iso(new Date(now.getFullYear(), 0, 1)) };
  return {};
}

export function Analisi() {
  const { version } = useApp();
  const route = useRoute();
  const thisMonth = ym(new Date());
  const [month, setMonth] = useState(thisMonth);
  const [tagPeriod, setTagPeriod] = useState<TagPeriod>("month");
  const [range, setRange] = useState(365);
  const [trendTable, setTrendTable] = useState(false);
  const [balTable, setBalTable] = useState(false);

  const summary = useAsync(() => api.summary(month), [version, month]);
  const trend = useAsync(() => api.trend(), [version]);
  const balance = useAsync(() => api.balance(range), [version, range]);
  const status = useAsync(() => api.status(), [version]);
  const tr = tagRange(tagPeriod);
  const tags = useAsync(() => api.tags(tr.from), [version, tr.from]);
  const recurring = useAsync(() => api.recurring(), [version]);
  const sinking = useAsync(() => api.sinking(), [version]);

  // Deep links from the overview (#saldo, #budget, #ricorrenti).
  useEffect(() => {
    const id = window.location.hash.slice(1);
    if (!id) return;
    const h = window.setTimeout(() => document.getElementById(id)?.scrollIntoView({ block: "start" }), 150);
    return () => window.clearTimeout(h);
  }, [route.path]);

  const monthFrom = `${month}-01`;
  const monthTo = iso(new Date(Number(month.slice(0, 4)), Number(month.slice(5, 7)), 0));

  return (
    <>
      <header className="page-head">
        <div>
          <h1>Analisi</h1>
          <p className="sub">Categorie, andamento, saldo, tag e spese ricorrenti</p>
        </div>
      </header>

      <section className="card" aria-labelledby="cat-title">
        <div className="card-head">
          <h2 id="cat-title">Spese per categoria</h2>
          <div className="inline" style={{ gap: 4 }}>
            <button type="button" className="btn sm icon ghost" onClick={() => setMonth(addMonths(month, -1))} aria-label="Mese precedente">
              <Icon name="chevronL" size={18} />
            </button>
            <strong style={{ minWidth: 120, textAlign: "center" }} aria-live="polite">{fmtMonth(month)}</strong>
            <button type="button" className="btn sm icon ghost" onClick={() => setMonth(addMonths(month, 1))} disabled={month >= thisMonth} aria-label="Mese successivo">
              <Icon name="chevronR" size={18} />
            </button>
          </div>
        </div>
        <Loaded {...summary} rows={5}>
          {(s) => (
            <div className="stack" style={{ gap: 14 }}>
              <div className="tiles" style={{ display: "grid", gridTemplateColumns: "repeat(auto-fit, minmax(120px, 1fr))", gap: 10 }}>
                <div className="stat">
                  <span className="label">Entrate</span>
                  <span className="value">{euro(s.income)}</span>
                </div>
                <div className="stat">
                  <span className="label">Uscite</span>
                  <span className="value">{euro(s.expenses)}</span>
                </div>
                <div className="stat">
                  <span className="label">Risparmio</span>
                  <span className={`value${s.saved < 0 ? " neg-text" : ""}`}>{signed(s.saved)}</span>
                  <span className="delta">
                    {s.savings_rate !== null && s.income > 0 && Math.abs(s.savings_rate) <= 1 ? `${percent(s.savings_rate)} delle entrate` : "entrate − uscite"}
                    {s.moved_to_savings > 0 && ` · ${euro(s.moved_to_savings)} messi da parte`}
                  </span>
                </div>
              </div>
              {s.by_category.length ? (
                <BarList
                  items={s.by_category.map((c) => ({
                    key: c.category,
                    name: c.category,
                    value: c.amount,
                    valueLabel: euro(c.amount),
                    share: percent(c.share),
                    title: "Mostra i movimenti",
                    onClick: () =>
                      navigate(
                        c.category === "Da categorizzare"
                          ? href("/movimenti", { uncategorized: "1", period: "custom", from: monthFrom, to: monthTo })
                          : href("/movimenti", { category: c.category, period: "custom", from: monthFrom, to: monthTo }),
                      ),
                  }))}
                />
              ) : (
                <Empty title="Nessuna spesa in questo mese." />
              )}
              {s.uncategorized > 0 && (
                <p className="small muted">
                  {integer(s.uncategorized)} movimenti ancora da categorizzare in questo mese.
                </p>
              )}
            </div>
          )}
        </Loaded>
      </section>

      <section className="card" aria-labelledby="trend-title">
        <div className="card-head">
          <h2 id="trend-title">Ultimi 12 mesi</h2>
          <button type="button" className="btn sm ghost" aria-pressed={trendTable} onClick={() => setTrendTable((v) => !v)}>
            <Icon name="table" size={14} /> Tabella
          </button>
        </div>
        <div className="legend" style={{ marginBottom: 8 }}>
          <span><i style={{ background: "var(--income)" }} />Entrate</span>
          <span><i style={{ background: "var(--expense)" }} />Uscite</span>
        </div>
        <Loaded {...trend} rows={5}>
          {(d) => (d.length ? <TrendChart data={d} showTable={trendTable} /> : <Empty title="Ancora nessun dato." />)}
        </Loaded>
      </section>

      <section className="card" id="saldo" aria-labelledby="saldo-title" style={{ scrollMarginTop: 80 }}>
        <div className="card-head">
          <h2 id="saldo-title">Saldo nel tempo</h2>
          <button type="button" className="btn sm ghost" aria-pressed={balTable} onClick={() => setBalTable((v) => !v)}>
            <Icon name="table" size={14} /> Tabella
          </button>
        </div>
        <div className="row-between" style={{ flexWrap: "wrap", marginBottom: 12 }}>
          <div className="stat">
            <span className="label">Totale conti in euro</span>
            <span className="value">{balance.data?.current != null ? euro(balance.data.current) : "—"}</span>
          </div>
          <div className="segmented" role="group" aria-label="Intervallo">
            {RANGES.map((r) => (
              <button key={r.days} type="button" aria-pressed={range === r.days} onClick={() => setRange(r.days)}>
                {r.label}
              </button>
            ))}
          </div>
        </div>
        <Loaded {...balance} rows={5}>
          {(b) => (
            <div className="stack" style={{ gap: 12 }}>
              {b.points.length >= 2 ? (
                <BalanceChart points={b.points} showTable={balTable} />
              ) : (
                <Empty title={b.current === null ? "Nessun saldo ancora." : "Servono movimenti di più giorni per vedere l'andamento."}>
                  {b.current === null ? "Arriva con la prima sincronizzazione." : undefined}
                </Empty>
              )}
              {b.changes.length > 0 && (
                <div className="deltas">
                  {b.changes.map((c) => (
                    <span key={c.days} className={`badge ${c.change > 0 ? "ok" : c.change < 0 ? "bad" : "neutral"}`}>
                      {CHANGE_LABEL[c.days] ?? `${c.days} giorni`}: {signed(c.change)}
                    </span>
                  ))}
                </div>
              )}
              <p className="small muted">Ricostruito dai movimenti salvati a partire dal saldo comunicato dalla banca (solo conti in euro).</p>
            </div>
          )}
        </Loaded>
      </section>

      <div className="grid-2">
        <section className="card" id="budget" aria-labelledby="bud-title" style={{ scrollMarginTop: 80 }}>
          <h2 id="bud-title">Budget</h2>
          <Loaded {...status} rows={4}>
            {(st) =>
              st.budgets.length ? (
                <div className="bars">
                  {st.budgets.map((b) => (
                    <div key={`${b.category}-${b.period}`} className="bar-row">
                      <span className="name">
                        {b.category} <span className="muted small">· {PERIOD_LABEL[b.period]}</span>
                      </span>
                      <span className="amt">
                        {euro(b.spent)} <span className="share">su {euro(b.limit)}</span>
                      </span>
                      <span style={{ gridColumn: "1 / -1" }}>
                        <Meter value={b.used} marker={b.elapsed} state={b.state} thin label={`${b.category}: ${percent(b.used)} del budget ${PERIOD_LABEL[b.period]} usato`} />
                      </span>
                      <span className="sub">
                        {b.remaining >= 0 ? `Restano ${euro(b.remaining)}` : `Sforato di ${euro(-b.remaining)}`} · periodo al {percent(b.elapsed)}
                      </span>
                    </div>
                  ))}
                </div>
              ) : (
                <Empty title="Nessun budget impostato.">
                  Chiedi alla tua AI: <code>imposta un budget mensile di 300 € per Ristoranti e bar</code>
                </Empty>
              )
            }
          </Loaded>
        </section>

        <section className="card" aria-labelledby="sink-title">
          <h2 id="sink-title">Accantonamenti per le spese annuali</h2>
          <Loaded {...sinking} rows={3}>
            {(list) =>
              list.length ? (
                <div className="bars">
                  {list.map((f) => (
                    <div key={f.name} className="bar-row">
                      <span className="name">
                        {f.name} <span className="muted small">· {fmtDateShortYear(f.due)}</span>
                      </span>
                      <span className="amt">{euro(f.monthly_quota)}/mese</span>
                      <span style={{ gridColumn: "1 / -1" }}>
                        <Meter value={f.amount ? f.should_have_saved / f.amount : 0} thin label={`${f.name}: da avere da parte ${euro(f.should_have_saved)} di ${euro(f.amount)}`} />
                      </span>
                      <span className="sub">
                        Da avere da parte oggi: {euro(f.should_have_saved)} di {euro(f.amount)}
                        {f.months_to_due > 0 ? ` · tra ${integer(f.months_to_due)} ${f.months_to_due === 1 ? "mese" : "mesi"}` : " · in scadenza"}
                      </span>
                    </div>
                  ))}
                </div>
              ) : (
                <Empty title="Nessuna spesa annuale prevista.">
                  Assicurazioni, bollo, tasse… Chiedi alla tua AI: <code>aggiungi la RC auto da 540 € a dicembre</code>
                </Empty>
              )
            }
          </Loaded>
        </section>
      </div>

      <section className="card" aria-labelledby="tags-title">
        <div className="card-head">
          <h2 id="tags-title">Tag</h2>
          <div className="segmented" role="group" aria-label="Periodo dei tag">
            {TAG_PERIODS.map((p) => (
              <button key={p.id} type="button" aria-pressed={tagPeriod === p.id} onClick={() => setTagPeriod(p.id)}>
                {p.label}
              </button>
            ))}
          </div>
        </div>
        <Loaded {...tags} rows={3}>
          {(list) => {
            const active = list.filter((t) => t.transactions > 0);
            return active.length ? (
              <BarList
                items={active.map((t) => ({
                  key: t.name,
                  name: `#${t.name}`,
                  value: t.spent,
                  valueLabel: euro(t.spent),
                  share: t.received ? `+${euro(t.received)}` : undefined,
                  sub: `${integer(t.transactions)} movimenti${t.rules.length ? ` · automatico per: ${t.rules.join(", ")}` : ""}`,
                  title: "Mostra i movimenti con questo tag",
                  onClick: () => navigate(href("/movimenti", { tag: t.name, period: tagPeriod === "all" ? undefined : tagPeriod === "year" ? "year" : tagPeriod })),
                }))}
              />
            ) : (
              <Empty title={list.length ? "Nessun movimento con tag in questo periodo." : "Nessun tag ancora."}>
                {list.length ? undefined : (
                  <>
                    Aggiungili aprendo un movimento, oppure chiedi all'AI: <code>tagga come vacanza-londra le spese dal 1 al 7 agosto</code>
                  </>
                )}
              </Empty>
            );
          }}
        </Loaded>
      </section>

      <section className="card" id="ricorrenti" aria-labelledby="rec-title" style={{ scrollMarginTop: 80 }}>
        <h2 id="rec-title">Spese ricorrenti</h2>
        <Loaded {...recurring} rows={4}>
          {(list) => {
            const active = list.filter((r) => r.active);
            const total = active.reduce((a, r) => a + r.monthly_equivalent, 0);
            return active.length ? (
              <div className="stack" style={{ gap: 12 }}>
                <div className="list">
                  {active.map((r) => (
                    <div key={`${r.merchant}-${r.cadence}`} className="kv" style={{ alignItems: "flex-start" }}>
                      <span className="k">
                        <span style={{ fontWeight: 600, color: "var(--text)" }}>{r.merchant}</span>
                        {!r.planned && <span className="badge warn" style={{ marginLeft: 8 }}>non pianificata</span>}
                        <br />
                        <span className="small muted">
                          {r.category ?? "senza categoria"} · prossima {fmtDateShortYear(r.next_expected)} · {integer(r.occurrences)} volte
                        </span>
                      </span>
                      <span className="v">
                        {euro(r.typical_amount)}
                        <span className="muted small">/{CADENCE_SHORT[r.cadence]}</span>
                      </span>
                    </div>
                  ))}
                </div>
                <p className="small muted">Totale ricorrente ≈ {euro(total)} al mese. Le voci «non pianificate» non sono ancora tra le spese previste: chiedi all'AI di aggiungerle o valuta se disdirle.</p>
              </div>
            ) : (
              <Empty title="Nessuna spesa ricorrente rilevata.">Servono almeno 3 mesi di movimenti.</Empty>
            );
          }}
        </Loaded>
      </section>
    </>
  );
}
