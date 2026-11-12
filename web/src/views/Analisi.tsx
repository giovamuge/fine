import { useEffect, useState, type ReactNode } from "react";
import { ChevronLeft, ChevronRight, Table2 } from "lucide-react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardAction, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Toggle } from "@/components/ui/toggle";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";
import { BarList } from "@/components/BarList";
import { Meter } from "@/components/Meter";
import { PageHeader } from "@/components/PageHeader";
import { Empty, Loaded } from "@/components/States";
import { BalanceChart } from "@/components/charts/BalanceChart";
import { TrendChart } from "@/components/charts/TrendChart";
import { api } from "@/lib/api";
import { useApp } from "@/lib/app-state";
import { CategoryIcon, SWATCH, colorOf } from "@/lib/categories";
import { CADENCE_SHORT, PERIOD_LABEL, addMonths, euro, fmtDateShortYear, fmtMonth, integer, iso, percent, signed, ym } from "@/lib/format";
import { href, navigate, useRoute } from "@/lib/router";
import { useAsync } from "@/lib/useAsync";
import { cn } from "@/lib/utils";
import { ChangeBadges } from "./Overview";

type TagPeriod = "month" | "30" | "year" | "all";
const TAG_PERIODS: { id: TagPeriod; label: string }[] = [
  { id: "month", label: "Mese" },
  { id: "30", label: "30 gg" },
  { id: "year", label: "Anno" },
  { id: "all", label: "Sempre" },
];
const RANGES: { days: number; label: string }[] = [
  { days: 30, label: "30 gg" },
  { days: 90, label: "3 mesi" },
  { days: 365, label: "1 anno" },
  { days: 3650, label: "Tutto" },
];

function tagRange(p: TagPeriod): { from?: string } {
  const now = new Date();
  if (p === "month") return { from: iso(new Date(now.getFullYear(), now.getMonth(), 1)) };
  if (p === "30") return { from: iso(new Date(now.getFullYear(), now.getMonth(), now.getDate() - 30)) };
  if (p === "year") return { from: iso(new Date(now.getFullYear(), 0, 1)) };
  return {};
}

function Stat({ label, value, note, negative }: { label: string; value: string; note?: ReactNode; negative?: boolean }) {
  return (
    <div className="flex flex-col gap-0.5 rounded-xl border bg-muted/30 p-3">
      <span className="text-xs font-medium text-muted-foreground">{label}</span>
      <span className={cn("tabular text-xl font-semibold tracking-tight", negative && "text-destructive")}>{value}</span>
      {note && <span className="text-xs text-muted-foreground">{note}</span>}
    </div>
  );
}

function TableToggle({ pressed, onChange }: { pressed: boolean; onChange: (v: boolean) => void }) {
  return (
    <Toggle size="sm" pressed={pressed} onPressedChange={onChange} aria-label="Mostra come tabella">
      <Table2 /> Tabella
    </Toggle>
  );
}

export function Analisi() {
  const { version, categoryByName } = useApp();
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
      <PageHeader title="Analisi" description="Categorie, andamento, saldo, budget, tag e spese ricorrenti" />

      <Card>
        <CardHeader>
          <CardTitle>Spese per categoria</CardTitle>
          <CardAction className="flex items-center gap-1">
            <Button variant="ghost" size="icon-sm" onClick={() => setMonth(addMonths(month, -1))} aria-label="Mese precedente">
              <ChevronLeft />
            </Button>
            <span className="min-w-28 text-center text-sm font-medium capitalize" aria-live="polite">
              {fmtMonth(month)}
            </span>
            <Button variant="ghost" size="icon-sm" onClick={() => setMonth(addMonths(month, 1))} disabled={month >= thisMonth} aria-label="Mese successivo">
              <ChevronRight />
            </Button>
          </CardAction>
        </CardHeader>
        <CardContent>
          <Loaded {...summary} rows={5}>
            {(s) => (
              <div className="flex flex-col gap-4">
                <div className="grid grid-cols-2 gap-3 sm:grid-cols-3">
                  <Stat label="Entrate" value={euro(s.income)} />
                  <Stat label="Uscite" value={euro(s.expenses)} />
                  <Stat
                    label="Risparmio"
                    value={signed(s.saved)}
                    negative={s.saved < 0}
                    note={
                      <>
                        {s.savings_rate !== null && s.income > 0 && Math.abs(s.savings_rate) <= 1 ? `${percent(s.savings_rate)} delle entrate` : "entrate − uscite"}
                        {s.moved_to_savings > 0 && ` · ${euro(s.moved_to_savings)} messi da parte`}
                      </>
                    }
                  />
                </div>
                {s.by_category.length ? (
                  <BarList
                    items={s.by_category.map((c) => {
                      const cat = categoryByName.get(c.category.toLowerCase());
                      const none = c.category === "Da categorizzare";
                      return {
                        key: c.category,
                        name: c.category,
                        icon: <CategoryIcon category={none ? undefined : (cat ?? { name: c.category, color: null, icon: null })} />,
                        barClass: none ? "bg-warning" : SWATCH[colorOf(cat ?? { name: c.category, color: null })],
                        value: c.amount,
                        valueLabel: euro(c.amount),
                        share: percent(c.share),
                        title: "Mostra i movimenti",
                        onClick: () =>
                          navigate(
                            none
                              ? href("/movimenti", { uncategorized: "1", period: "custom", from: monthFrom, to: monthTo })
                              : href("/movimenti", { category: c.category, period: "custom", from: monthFrom, to: monthTo }),
                          ),
                      };
                    })}
                  />
                ) : (
                  <Empty title="Nessuna spesa in questo mese." />
                )}
                {s.uncategorized > 0 && <p className="text-sm text-muted-foreground">{integer(s.uncategorized)} movimenti ancora da categorizzare in questo mese.</p>}
              </div>
            )}
          </Loaded>
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>Ultimi 12 mesi</CardTitle>
          <CardDescription>Entrate e uscite per mese</CardDescription>
          <CardAction>
            <TableToggle pressed={trendTable} onChange={setTrendTable} />
          </CardAction>
        </CardHeader>
        <CardContent>
          <Loaded {...trend} rows={5}>
            {(d) => (d.length ? <TrendChart data={d} showTable={trendTable} /> : <Empty title="Ancora nessun dato." />)}
          </Loaded>
        </CardContent>
      </Card>

      <Card id="saldo" className="scroll-mt-20">
        <CardHeader>
          <CardTitle>Saldo nel tempo</CardTitle>
          <CardDescription className="tabular text-2xl font-semibold text-foreground">{balance.data?.current != null ? euro(balance.data.current) : "—"}</CardDescription>
          <CardAction>
            <TableToggle pressed={balTable} onChange={setBalTable} />
          </CardAction>
        </CardHeader>
        <CardContent className="flex flex-col gap-4">
          <ToggleGroup type="single" variant="outline" size="sm" value={String(range)} onValueChange={(v) => v && setRange(Number(v))} aria-label="Intervallo">
            {RANGES.map((r) => (
              <ToggleGroupItem key={r.days} value={String(r.days)} className="px-3">
                {r.label}
              </ToggleGroupItem>
            ))}
          </ToggleGroup>
          <Loaded {...balance} rows={5}>
            {(b) => (
              <div className="flex flex-col gap-3">
                {b.points.length >= 2 ? (
                  <BalanceChart points={b.points} showTable={balTable} />
                ) : (
                  <Empty title={b.current === null ? "Nessun saldo ancora." : "Servono movimenti di più giorni per vedere l'andamento."}>
                    {b.current === null ? "Arriva con la prima sincronizzazione." : undefined}
                  </Empty>
                )}
                {b.changes.length > 0 && <ChangeBadges changes={b.changes} />}
                <p className="text-xs text-muted-foreground">Ricostruito dai movimenti salvati a partire dal saldo comunicato dalla banca (solo conti in euro).</p>
              </div>
            )}
          </Loaded>
        </CardContent>
      </Card>

      <div className="grid gap-6 lg:grid-cols-2">
        <Card id="budget" className="scroll-mt-20">
          <CardHeader>
            <CardTitle>Budget</CardTitle>
            <CardDescription>La tacca indica dove dovresti essere oggi</CardDescription>
          </CardHeader>
          <CardContent>
            <Loaded {...status} rows={4}>
              {(st) =>
                st.budgets.length ? (
                  <ul className="flex flex-col gap-5">
                    {st.budgets.map((b) => (
                      <li key={`${b.category}-${b.period}`} className="flex items-start gap-3">
                        <CategoryIcon category={categoryByName.get(b.category.toLowerCase())} />
                        <div className="flex min-w-0 flex-1 flex-col gap-1.5">
                          <div className="flex justify-between gap-2 text-sm">
                            <span className="truncate font-medium">
                              {b.category} <span className="font-normal text-muted-foreground">· {PERIOD_LABEL[b.period]}</span>
                            </span>
                            <span className="tabular shrink-0">
                              {euro(b.spent)} <span className="text-muted-foreground">/ {euro(b.limit)}</span>
                            </span>
                          </div>
                          <Meter value={b.used} marker={b.elapsed} state={b.state} label={`${b.category}: ${percent(b.used)} del budget ${PERIOD_LABEL[b.period]} usato`} />
                          <span className={cn("text-xs", b.remaining < 0 ? "text-destructive" : "text-muted-foreground")}>
                            {b.remaining >= 0 ? `Restano ${euro(b.remaining)}` : `Sforato di ${euro(-b.remaining)}`} · periodo al {percent(b.elapsed)}
                          </span>
                        </div>
                      </li>
                    ))}
                  </ul>
                ) : (
                  <Empty title="Nessun budget impostato.">Chiedi alla tua AI: «imposta un budget mensile di 300 € per Ristoranti e bar».</Empty>
                )
              }
            </Loaded>
          </CardContent>
        </Card>

        <Card>
          <CardHeader>
            <CardTitle>Accantonamenti</CardTitle>
            <CardDescription>Per le spese annuali previste</CardDescription>
          </CardHeader>
          <CardContent>
            <Loaded {...sinking} rows={3}>
              {(list) =>
                list.length ? (
                  <ul className="flex flex-col gap-5">
                    {list.map((f) => (
                      <li key={f.name} className="flex flex-col gap-1.5">
                        <div className="flex justify-between gap-2 text-sm">
                          <span className="truncate font-medium">
                            {f.name} <span className="font-normal text-muted-foreground">· {fmtDateShortYear(f.due)}</span>
                          </span>
                          <span className="tabular shrink-0">{euro(f.monthly_quota)}/mese</span>
                        </div>
                        <Meter value={f.amount ? f.should_have_saved / f.amount : 0} label={`${f.name}: da avere da parte ${euro(f.should_have_saved)} di ${euro(f.amount)}`} />
                        <span className="text-xs text-muted-foreground">
                          Da avere da parte oggi: {euro(f.should_have_saved)} di {euro(f.amount)}
                          {f.months_to_due > 0 ? ` · tra ${integer(f.months_to_due)} ${f.months_to_due === 1 ? "mese" : "mesi"}` : " · in scadenza"}
                        </span>
                      </li>
                    ))}
                  </ul>
                ) : (
                  <Empty title="Nessuna spesa annuale prevista.">Assicurazioni, bollo, tasse… Chiedi alla tua AI: «aggiungi la RC auto da 540 € a dicembre».</Empty>
                )
              }
            </Loaded>
          </CardContent>
        </Card>
      </div>

      <Card>
        <CardHeader>
          <CardTitle>Tag</CardTitle>
          <CardDescription>Spese raggruppate per tag</CardDescription>
        </CardHeader>
        <CardContent className="flex flex-col gap-4">
          <ToggleGroup type="single" variant="outline" size="sm" value={tagPeriod} onValueChange={(v) => v && setTagPeriod(v as TagPeriod)} aria-label="Periodo dei tag">
            {TAG_PERIODS.map((p) => (
              <ToggleGroupItem key={p.id} value={p.id} className="px-3">
                {p.label}
              </ToggleGroupItem>
            ))}
          </ToggleGroup>
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
                    barClass: "bg-chart-3",
                    sub: `${integer(t.transactions)} movimenti${t.rules.length ? ` · automatico per: ${t.rules.join(", ")}` : ""}`,
                    title: "Mostra i movimenti con questo tag",
                    onClick: () => navigate(href("/movimenti", { tag: t.name, period: tagPeriod === "all" ? undefined : tagPeriod })),
                  }))}
                />
              ) : (
                <Empty title={list.length ? "Nessun movimento con tag in questo periodo." : "Nessun tag ancora."}>
                  {list.length ? undefined : "Aggiungili aprendo un movimento, oppure chiedi all'AI: «tagga come vacanza-londra le spese dal 1 al 7 agosto»."}
                </Empty>
              );
            }}
          </Loaded>
        </CardContent>
      </Card>

      <Card id="ricorrenti" className="scroll-mt-20">
        <CardHeader>
          <CardTitle>Spese ricorrenti</CardTitle>
          <CardDescription>Abbonamenti e bollette rilevati dallo storico</CardDescription>
        </CardHeader>
        <CardContent>
          <Loaded {...recurring} rows={4}>
            {(list) => {
              const active = list.filter((r) => r.active);
              const total = active.reduce((a, r) => a + r.monthly_equivalent, 0);
              return active.length ? (
                <div className="flex flex-col gap-3">
                  <ul className="flex flex-col divide-y">
                    {active.map((r) => (
                      <li key={`${r.merchant}-${r.cadence}`} className="flex items-center gap-3 py-2.5">
                        <CategoryIcon category={r.category ? categoryByName.get(r.category.toLowerCase()) : undefined} />
                        <div className="flex min-w-0 flex-1 flex-col">
                          <span className="flex items-center gap-2 text-sm font-medium">
                            <span className="truncate">{r.merchant}</span>
                            {!r.planned && (
                              <Badge variant="outline" className="border-warning/40 text-warning">
                                non pianificata
                              </Badge>
                            )}
                          </span>
                          <span className="text-xs text-muted-foreground">
                            {r.category ?? "senza categoria"} · prossima {fmtDateShortYear(r.next_expected)} · {integer(r.occurrences)} volte
                          </span>
                        </div>
                        <span className="tabular shrink-0 text-sm">
                          {euro(r.typical_amount)}
                          <span className="text-muted-foreground">/{CADENCE_SHORT[r.cadence]}</span>
                        </span>
                      </li>
                    ))}
                  </ul>
                  <p className="text-xs text-muted-foreground">
                    Totale ricorrente ≈ {euro(total)} al mese. Le voci «non pianificate» non sono ancora tra le spese previste: chiedi all'AI di aggiungerle o valuta se disdirle.
                  </p>
                </div>
              ) : (
                <Empty title="Nessuna spesa ricorrente rilevata.">Servono almeno 3 mesi di movimenti.</Empty>
              );
            }}
          </Loaded>
        </CardContent>
      </Card>
    </>
  );
}
