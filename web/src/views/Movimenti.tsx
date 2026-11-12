import { useCallback, useEffect, useMemo, useState } from "react";
import { CalendarRange, CheckSquare, Hash, Search, Shapes, Tag, X } from "lucide-react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent } from "@/components/ui/card";
import { Checkbox } from "@/components/ui/checkbox";
import { Input } from "@/components/ui/input";
import { InputGroup, InputGroupAddon, InputGroupInput } from "@/components/ui/input-group";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { Toggle } from "@/components/ui/toggle";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";
import { BulkDialog } from "@/components/BulkDialog";
import { CategoryPicker } from "@/components/CategoryPicker";
import { PageHeader } from "@/components/PageHeader";
import { Empty, Loaded } from "@/components/States";
import { TagChips } from "@/components/TagInput";
import { TxDetail } from "@/components/TxDetail";
import { api, errorMessage } from "@/lib/api";
import { useApp } from "@/lib/app-state";
import { CategoryIcon } from "@/lib/categories";
import { euro, fmtDay, integer, iso, plural, signed } from "@/lib/format";
import { href, navigate, useRoute } from "@/lib/router";
import { useToast } from "@/lib/toast";
import type { TxView } from "@/lib/types";
import { useAsync } from "@/lib/useAsync";
import { cn } from "@/lib/utils";

type Period = "month" | "30" | "90" | "year" | "all" | "custom";
const PERIODS: { id: Period; label: string }[] = [
  { id: "month", label: "Mese" },
  { id: "30", label: "30 gg" },
  { id: "90", label: "3 mesi" },
  { id: "year", label: "Anno" },
  { id: "all", label: "Tutto" },
  { id: "custom", label: "Date" },
];
const LIMITS = [100, 250, 500];
const MAX = LIMITS[LIMITS.length - 1] ?? 500;

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
  const { version, refresh } = useApp();
  const toast = useToast();
  const q = useRoute().query;

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

  const multiAccount = useMemo(() => new Set((txs.data ?? []).map((t) => t.account)).size > 1, [txs.data]);
  const anyFilter = !!(text || category || tag || uncategorized || period !== "all");
  const tagNames = (tags.data ?? []).map((t) => t.name);

  return (
    <>
      <PageHeader
        title="Movimenti"
        description={txs.data ? `${plural(txs.data.length, "movimento", "movimenti")}${txs.data.length >= limit ? " (i più recenti)" : ""}` : " "}
        actions={
          <Button
            variant={selecting ? "secondary" : "outline"}
            size="sm"
            onClick={() => {
              setSelecting((v) => !v);
              setSelected(new Set());
            }}
          >
            <CheckSquare /> {selecting ? "Fine selezione" : "Seleziona"}
          </Button>
        }
      />

      <Card className="gap-3 py-4">
        <CardContent className="flex flex-col gap-3 px-4">
          <InputGroup>
            <InputGroupAddon>
              <Search />
            </InputGroupAddon>
            <InputGroupInput type="search" placeholder="Cerca esercente, descrizione o controparte" value={search} onChange={(e) => setSearch(e.target.value)} aria-label="Cerca nei movimenti" />
          </InputGroup>
          <div className="-mx-4 overflow-x-auto px-4">
            <ToggleGroup
              type="single"
              variant="outline"
              size="sm"
              value={period}
              onValueChange={(v) => {
                if (!v) return;
                if (v === "custom") setParams({ period: "custom", from: from || iso(new Date(new Date().getFullYear(), new Date().getMonth() - 1, 1)), to: to || iso(new Date()) });
                else setParams({ period: v === "all" ? undefined : v });
              }}
              aria-label="Periodo"
            >
              {PERIODS.map((p) => (
                <ToggleGroupItem key={p.id} value={p.id} className="px-3">
                  {p.id === "custom" && <CalendarRange />}
                  {p.label}
                </ToggleGroupItem>
              ))}
            </ToggleGroup>
          </div>
          {period === "custom" && (
            <div className="flex flex-wrap items-center gap-2 text-sm">
              <span className="text-muted-foreground">Dal</span>
              <Input type="date" className="w-auto" value={from} max={to || undefined} onChange={(e) => setParams({ period: "custom", from: e.target.value })} aria-label="Dal" />
              <span className="text-muted-foreground">al</span>
              <Input type="date" className="w-auto" value={to} min={from || undefined} onChange={(e) => setParams({ period: "custom", to: e.target.value })} aria-label="Al" />
            </div>
          )}
          <div className="flex flex-wrap items-center gap-2">
            <CategoryPicker value={category} onChange={(c) => setParams({ category: c || undefined })} allowNone noneLabel="Tutte le categorie" className="h-8 w-auto min-w-44" ariaLabel="Filtra per categoria" />
            <Select value={tag || "__all"} onValueChange={(v) => setParams({ tag: v === "__all" ? undefined : v })}>
              <SelectTrigger size="sm" className="min-w-36" aria-label="Filtra per tag">
                <Hash className="opacity-60" />
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="__all">Tutti i tag</SelectItem>
                {tagNames.map((t) => (
                  <SelectItem key={t} value={t}>
                    {t}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
            <Toggle variant="outline" size="sm" pressed={uncategorized} onPressedChange={(p) => setParams({ uncategorized: p ? "1" : undefined })}>
              <Shapes /> Da categorizzare
            </Toggle>
            {anyFilter && (
              <Button
                variant="ghost"
                size="sm"
                onClick={() => {
                  setSearch("");
                  navigate("/movimenti", { replace: true });
                }}
              >
                <X /> Azzera filtri
              </Button>
            )}
          </div>
        </CardContent>
      </Card>

      <Loaded {...txs} rows={8}>
        {(list) =>
          list.length === 0 ? (
            <Empty title={anyFilter ? "Nessun movimento con questi filtri." : "Nessun movimento ancora."}>
              {anyFilter ? "Prova ad allargare il periodo o ad azzerare i filtri." : "Collega un conto dalla sezione Conti e sincronizza."}
            </Empty>
          ) : (
            <div className="flex flex-col gap-4">
              {groups.map((g) => (
                <section key={g.date} aria-label={fmtDay(g.date)}>
                  <div className="flex items-center justify-between px-1 pb-1.5 text-xs font-medium text-muted-foreground">
                    <span className="first-letter:uppercase">{fmtDay(g.date)}</span>
                    <span className="tabular">{signed(g.total)}</span>
                  </div>
                  <Card className="gap-0 py-1">
                    <ul className="divide-y">
                      {g.items.map((t) => (
                        <TxRow
                          key={t.id}
                          t={t}
                          selecting={selecting}
                          selected={selected.has(t.id)}
                          onToggle={() => toggle(t.id)}
                          onOpen={() => setDetail(t)}
                          showAccount={multiAccount}
                          onQuickCategory={(c) => quickCategorize(t, c)}
                        />
                      ))}
                    </ul>
                  </Card>
                </section>
              ))}
              {list.length >= limit && limit < MAX && (
                <Button variant="outline" className="self-center" onClick={() => setLimit(LIMITS.find((l) => l > limit) ?? limit)}>
                  Mostra altri movimenti
                </Button>
              )}
              {list.length >= MAX && <p className="text-center text-sm text-muted-foreground">Mostrati i primi {MAX}: restringi il periodo o cerca per vedere il resto.</p>}
            </div>
          )
        }
      </Loaded>

      {selecting && selected.size > 0 && (
        <div role="region" aria-label="Azioni sui movimenti selezionati" className="fixed inset-x-4 bottom-20 z-40 mx-auto flex max-w-md items-center gap-2 rounded-2xl border bg-popover p-2 pl-4 shadow-lg md:bottom-6">
          <span className="flex-1 text-sm font-medium">{integer(selected.size)} selezionati</span>
          <Button size="sm" variant="secondary" onClick={() => setBulk("categorize")}>
            <Shapes /> Categoria
          </Button>
          <Button size="sm" variant="secondary" onClick={() => setBulk("tag")}>
            <Tag /> Tag
          </Button>
          <Button size="icon-sm" variant="ghost" onClick={() => setSelected(new Set())} aria-label="Annulla selezione">
            <X />
          </Button>
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
  onQuickCategory,
}: {
  t: TxView;
  selecting: boolean;
  selected: boolean;
  onToggle: () => void;
  onOpen: () => void;
  showAccount: boolean;
  onQuickCategory: (c: string) => void;
}) {
  const { categoryByName } = useApp();
  const title = t.merchant || t.counterparty || "Movimento";
  const cat = t.category ? categoryByName.get(t.category.toLowerCase()) : undefined;
  return (
    <li className={cn("flex items-center gap-3 px-3 py-2.5 transition-colors", selected && "bg-muted")}>
      {selecting && <Checkbox checked={selected} onCheckedChange={onToggle} aria-label={`Seleziona ${title}`} />}
      {selecting ? (
        <CategoryIcon category={cat} />
      ) : (
        <CategoryPicker
          value={t.category ?? ""}
          onChange={onQuickCategory}
          trigger={
            <button type="button" className="rounded-lg focus-visible:ring-2 focus-visible:ring-ring/50 focus-visible:outline-none" aria-label={`Categoria di ${title}: ${t.category ?? "da categorizzare"}. Cambia`}>
              <CategoryIcon category={cat} className={cn("transition-transform hover:scale-105", !t.category && "ring-2 ring-warning/60")} />
            </button>
          }
        />
      )}
      <button type="button" className="flex min-w-0 flex-1 items-center gap-3 text-left focus-visible:outline-none" onClick={selecting ? onToggle : onOpen}>
        <span className="flex min-w-0 flex-1 flex-col gap-0.5">
          <span className="truncate text-sm font-medium">{title}</span>
          <span className="flex min-w-0 flex-wrap items-center gap-x-2 gap-y-1 text-xs text-muted-foreground">
            <span className={cn(!t.category && "font-medium text-warning")}>{t.category ?? "Da categorizzare"}</span>
            {t.amortized_months && <Badge variant="outline">in {t.amortized_months} mesi</Badge>}
            {showAccount && <span className="truncate">· {t.account}</span>}
            {t.tags.length > 0 && <TagChips tags={t.tags} />}
          </span>
          {t.description && t.description !== t.merchant && <span className="line-clamp-1 text-xs text-muted-foreground/80">{t.description}</span>}
        </span>
        <span className={cn("tabular shrink-0 text-sm font-medium", t.amount > 0 && "text-positive")}>{t.amount > 0 ? `+${euro(t.amount)}` : euro(t.amount)}</span>
      </button>
    </li>
  );
}
