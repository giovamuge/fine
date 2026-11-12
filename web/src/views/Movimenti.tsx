import { useCallback, useEffect, useMemo, useState, type ChangeEvent } from "react";
import { api, errorMessage } from "../lib/api";
import { useApp } from "../lib/app-state";
import { useAsync } from "../lib/useAsync";
import { useToast } from "../lib/toast";
import { href, navigate, useRoute } from "../lib/router";
import { euro, fmtDay, integer, iso, money, plural, signed } from "../lib/format";
import { Empty, Loaded } from "../components/States";
import { Icon } from "../components/Icon";
import { TagChips } from "../components/TagInput";
import { TxDetail } from "../components/TxDetail";
import { BulkDialog } from "../components/BulkDialog";
import type { TxView } from "../lib/types";

type Period = "month" | "30" | "90" | "year" | "all" | "custom";
const PERIODS: { id: Period; label: string }[] = [
  { id: "month", label: "Questo mese" },
  { id: "30", label: "30 giorni" },
  { id: "90", label: "3 mesi" },
  { id: "year", label: "Anno" },
  { id: "all", label: "Tutto" },
];
const LIMITS = [100, 250, 500];

function rangeFor(p: Period, from: string, to: string): { from?: string; to?: string } {
  const now = new Date();
  switch (p) {
    case "month":
      return { from: iso(new Date(now.getFullYear(), now.getMonth(), 1)) };
    case "30":
      return { from: iso(new Date(now.getFullYear(), now.getMonth(), now.getDate() - 30)) };
    case "90":
      return { from: iso(new Date(now.getFullYear(), now.getMonth() - 3, now.getDate())) };
    case "year":
      return { from: iso(new Date(now.getFullYear(), 0, 1)) };
    case "custom":
      return { from: from || undefined, to: to || undefined };
    default:
      return {};
  }
}

export function Movimenti() {
  const { version, categories, refresh } = useApp();
  const toast = useToast();
  const route = useRoute();
  const q = route.query;

  // Filters live in the URL: links from Analisi and the browser back button just work.
  const text = q.get("q") ?? "";
  const category = q.get("category") ?? "";
  const tag = q.get("tag") ?? "";
  const uncategorized = q.get("uncategorized") === "1";
  const from = q.get("from") ?? "";
  const to = q.get("to") ?? "";
  const period: Period = (q.get("period") as Period | null) ?? (from || to ? "custom" : "all");
  const [limit, setLimit] = useState(LIMITS[0] ?? 100);
  const [search, setSearch] = useState(text);

  const setParams = useCallback(
    (patch: Record<string, string | undefined>) => {
      const next: Record<string, string | undefined> = {
        q: text,
        category,
        tag,
        uncategorized: uncategorized ? "1" : undefined,
        period: period === "all" ? undefined : period,
        from,
        to,
        ...patch,
      };
      if (next.period !== "custom") {
        next.from = undefined;
        next.to = undefined;
      }
      navigate(href("/movimenti", next), { replace: true });
    },
    [text, category, tag, uncategorized, period, from, to],
  );

  // Debounced search box → URL.
  useEffect(() => setSearch(text), [text]);
  useEffect(() => {
    if (search === text) return;
    const h = window.setTimeout(() => setParams({ q: search || undefined }), 250);
    return () => window.clearTimeout(h);
  }, [search, text, setParams]);

  const range = rangeFor(period, from, to);
  const txs = useAsync(
    () => api.transactions({ text: text || undefined, category: category || undefined, tag: tag || undefined, uncategorized_only: uncategorized, from: range.from, to: range.to, limit }),
    [version, text, category, tag, uncategorized, range.from, range.to, limit],
  );
  const tags = useAsync(() => api.tags(), [version]);

  const [selecting, setSelecting] = useState(false);
  const [selected, setSelected] = useState<Set<number>>(new Set());
  const [detail, setDetail] = useState<TxView | null>(null);
  const [bulk, setBulk] = useState<"categorize" | "tag" | null>(null);

  useEffect(() => {
    // Keep the open detail in sync with fresh data, drop selections that vanished.
    if (!txs.data) return;
    if (detail) setDetail(txs.data.find((t) => t.id === detail.id) ?? null);
    setSelected((s) => new Set([...s].filter((id) => txs.data?.some((t) => t.id === id))));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [txs.data]);

  const toggle = (id: number) =>
    setSelected((s) => {
      const n = new Set(s);
      if (n.has(id)) n.delete(id);
      else n.add(id);
      return n;
    });

  const quickCategorize = async (t: TxView, cat: string) => {
    if (!cat || cat === t.category) return;
    try {
      await api.categorize([t.id], cat, false);
      refresh();
      toast.success(`«${cat}» assegnata a ${t.merchant || "movimento"}.`, {
        label: "Anche in futuro",
        onClick: async () => {
          try {
            const r = await api.categorize([t.id], cat, true);
            refresh();
            toast.success(`Regola creata: «${r.rule_pattern ?? t.merchant}» → ${cat}${r.auto_categorized ? ` (${integer(r.auto_categorized)} movimenti sistemati)` : ""}.`);
          } catch (e) {
            toast.error(errorMessage(e));
          }
        },
      });
    } catch (e) {
      toast.error(errorMessage(e));
    }
  };

  const groups = useMemo(() => {
    const out: { date: string; items: TxView[]; total: number }[] = [];
    for (const t of txs.data ?? []) {
      const last = out[out.length - 1];
      if (last && last.date === t.date) {
        last.items.push(t);
        last.total += t.amount;
      } else out.push({ date: t.date, items: [t], total: t.amount });
    }
    return out;
  }, [txs.data]);

  const accountsInList = useMemo(() => new Set((txs.data ?? []).map((t) => t.account)).size, [txs.data]);
  const anyFilter = !!(text || category || tag || uncategorized || period !== "all");
  const tagNames = (tags.data ?? []).map((t) => t.name);

  return (
    <>
      <header className="page-head">
        <div>
          <h1>Movimenti</h1>
          <p className="sub">{txs.data ? `${plural(txs.data.length, "movimento", "movimenti")}${txs.data.length >= limit ? " (i più recenti)" : ""}` : " "}</p>
        </div>
        <div className="inline">
          <button type="button" className="btn sm" aria-pressed={selecting} onClick={() => { setSelecting((v) => !v); setSelected(new Set()); }}>
            <Icon name="check" size={15} /> {selecting ? "Fine selezione" : "Seleziona"}
          </button>
        </div>
      </header>

      <section className="card" aria-label="Filtri">
        <div className="tx-toolbar">
          <div className="search">
            <Icon name="search" size={18} />
            <input type="search" className="input" placeholder="Cerca esercente, descrizione o controparte" value={search} onChange={(e) => setSearch(e.target.value)} aria-label="Cerca nei movimenti" />
          </div>
          <div className="pills" role="group" aria-label="Periodo">
            {PERIODS.map((p) => (
              <button key={p.id} type="button" className="pill" aria-pressed={period === p.id} onClick={() => setParams({ period: p.id === "all" ? undefined : p.id })}>
                {p.label}
              </button>
            ))}
            <button type="button" className="pill" aria-pressed={period === "custom"} onClick={() => setParams({ period: "custom", from: from || iso(new Date(new Date().getFullYear(), new Date().getMonth() - 1, 1)), to: to || iso(new Date()) })}>
              <Icon name="calendar" size={13} /> Date…
            </button>
          </div>
          {period === "custom" && (
            <div className="inline">
              <label className="small muted" htmlFor="f-from">Dal</label>
              <input id="f-from" type="date" className="input sm" value={from} max={to || undefined} onChange={(e) => setParams({ period: "custom", from: e.target.value })} />
              <label className="small muted" htmlFor="f-to">al</label>
              <input id="f-to" type="date" className="input sm" value={to} min={from || undefined} onChange={(e) => setParams({ period: "custom", to: e.target.value })} />
            </div>
          )}
          <div className="tx-filters">
            <select className="select sm" value={category} onChange={(e) => setParams({ category: e.target.value || undefined })} aria-label="Categoria">
              <option value="">Tutte le categorie</option>
              {categories.map((c) => (
                <option key={c.name} value={c.name}>{c.name}</option>
              ))}
            </select>
            <select className="select sm" value={tag} onChange={(e) => setParams({ tag: e.target.value || undefined })} aria-label="Tag">
              <option value="">Tutti i tag</option>
              {tagNames.map((t) => (
                <option key={t} value={t}>#{t}</option>
              ))}
            </select>
            <button type="button" className="pill" aria-pressed={uncategorized} onClick={() => setParams({ uncategorized: uncategorized ? undefined : "1" })}>
              Da categorizzare
            </button>
            {anyFilter && (
              <button type="button" className="btn sm ghost" onClick={() => { setSearch(""); navigate("/movimenti", { replace: true }); }}>
                <Icon name="x" size={14} /> Azzera filtri
              </button>
            )}
          </div>
        </div>
      </section>

      <section className="card" aria-label="Elenco movimenti">
        <Loaded {...txs} rows={8}>
          {(list) =>
            list.length === 0 ? (
              <Empty title={anyFilter ? "Nessun movimento con questi filtri." : "Nessun movimento ancora."}>
                {anyFilter ? "Prova ad allargare il periodo o ad azzerare i filtri." : "Collega un conto dalla sezione Conti e sincronizza."}
              </Empty>
            ) : (
              <div className="tx-list">
                {groups.map((g) => (
                  <div key={g.date}>
                    <div className="tx-day">
                      <span>{fmtDay(g.date)}</span>
                      <span className="num" style={{ textTransform: "none", letterSpacing: 0 }}>{signed(g.total)}</span>
                    </div>
                    {g.items.map((t) => (
                      <TxRow
                        key={t.id}
                        t={t}
                        selecting={selecting}
                        selected={selected.has(t.id)}
                        onToggle={() => toggle(t.id)}
                        onOpen={() => setDetail(t)}
                        showAccount={accountsInList > 1}
                        categories={categories.map((c) => c.name)}
                        onQuickCategory={(c) => quickCategorize(t, c)}
                      />
                    ))}
                  </div>
                ))}
                {list.length >= limit && limit < (LIMITS[LIMITS.length - 1] ?? 500) && (
                  <div style={{ paddingTop: 14, display: "flex", justifyContent: "center" }}>
                    <button type="button" className="btn" onClick={() => setLimit(LIMITS.find((l) => l > limit) ?? limit)}>
                      Mostra altri movimenti
                    </button>
                  </div>
                )}
                {list.length >= (LIMITS[LIMITS.length - 1] ?? 500) && (
                  <p className="small muted" style={{ paddingTop: 12, textAlign: "center" }}>Mostrati i primi 500: restringi il periodo o cerca per vedere il resto.</p>
                )}
              </div>
            )
          }
        </Loaded>
      </section>

      {selecting && selected.size > 0 && (
        <div className="bulkbar" role="region" aria-label="Azioni sui movimenti selezionati">
          <strong>{integer(selected.size)} selezionati</strong>
          <button type="button" className="btn sm" onClick={() => setBulk("categorize")}>
            Categoria…
          </button>
          <button type="button" className="btn sm" onClick={() => setBulk("tag")}>
            <Icon name="tag" size={14} /> Tag…
          </button>
          <button type="button" className="btn sm" onClick={() => setSelected(new Set())} aria-label="Annulla selezione">
            <Icon name="x" size={14} />
          </button>
        </div>
      )}

      <TxDetail tx={detail} onClose={() => setDetail(null)} tagSuggestions={tagNames} />
      <BulkDialog
        mode={bulk}
        ids={[...selected]}
        onClose={() => setBulk(null)}
        onDone={() => {
          setBulk(null);
          setSelected(new Set());
        }}
        tagSuggestions={tagNames}
      />
    </>
  );
}

function TxRow({
  t,
  selecting,
  selected,
  onToggle,
  onOpen,
  showAccount,
  categories,
  onQuickCategory,
}: {
  t: TxView;
  selecting: boolean;
  selected: boolean;
  onToggle: () => void;
  onOpen: () => void;
  showAccount: boolean;
  categories: string[];
  onQuickCategory: (c: string) => void;
}) {
  const title = t.merchant || t.counterparty || "Movimento";
  const onSelect = (e: ChangeEvent<HTMLSelectElement>) => onQuickCategory(e.target.value);
  return (
    <div
      className={`tx${selected ? " selected" : ""}`}
      role="button"
      tabIndex={0}
      onClick={() => (selecting ? onToggle() : onOpen())}
      onKeyDown={(e) => {
        if (e.key === "Enter" || e.key === " ") {
          e.preventDefault();
          if (selecting) onToggle();
          else onOpen();
        }
      }}
      aria-label={`${title}, ${money(t.amount, "EUR")}, ${t.category ?? "da categorizzare"}`}
    >
      {selecting ? (
        <input type="checkbox" className="sel" checked={selected} onChange={onToggle} onClick={(e) => e.stopPropagation()} aria-label={`Seleziona ${title}`} tabIndex={-1} />
      ) : (
        <span className="sel" aria-hidden="true" style={{ width: 4 }} />
      )}
      <div className="main">
        <div className="merchant">{title}</div>
        <div className="meta">
          {selecting || categories.length === 0 ? (
            <span className={`cat${t.category ? "" : " none"}`}>{t.category ?? "Da categorizzare"}</span>
          ) : (
            <select
              className={`select sm cat-inline${t.category ? "" : " none"}`}
              value={t.category ?? ""}
              onChange={onSelect}
              onClick={(e) => e.stopPropagation()}
              onKeyDown={(e) => e.stopPropagation()}
              aria-label={`Categoria di ${title}`}
              style={{ minHeight: 26, padding: "2px 24px 2px 6px", fontSize: 12, width: "auto", color: t.category ? "var(--text-2)" : "var(--warn-text)", fontWeight: 600 }}
            >
              <option value="">Da categorizzare</option>
              {categories.map((c) => (
                <option key={c} value={c}>{c}</option>
              ))}
            </select>
          )}
          {t.tags.length > 0 && <TagChips tags={t.tags} small />}
          {t.amortized_months && <span className="badge neutral">in {t.amortized_months} mesi</span>}
          {showAccount && <span className="truncate">{t.account}</span>}
          {t.description && t.description !== t.merchant && <span className="desc">{t.description}</span>}
        </div>
      </div>
      <div className={`amount${t.amount > 0 ? " in" : ""}`}>{t.amount > 0 ? `+${euro(t.amount)}` : euro(t.amount)}</div>
    </div>
  );
}
