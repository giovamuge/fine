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
import { Link, href, navigate, useRoute } from "@/lib/router";
import { useToast } from "@/lib/toast";
import type { TxView } from "@/lib/types";
import { useAsync } from "@/lib/useAsync";
import { cn } from "@/lib/utils";
import { t } from "@/lib/i18n";

type Period = "month" | "30" | "90" | "year" | "all" | "custom";
const PERIODS: { id: Period; label: string }[] = [
  { id: "month", label: t("Mese") },
  { id: "30", label: t("30 gg") },
  { id: "90", label: t("3 mesi") },
  { id: "year", label: t("Anno") },
  { id: "all", label: t("Tutto") },
  { id: "custom", label: t("Date") },
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
    if (detail) setDetail(txs.data.find((tx) => tx.id === detail.id) ?? null);
    setSelected((s) => new Set([...s].filter((id) => txs.data?.some((tx) => tx.id === id))));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [txs.data]);

  const toggle = (id: number) =>
    setSelected((s) => {
      const n = new Set(s);
      if (n.has(id)) n.delete(id);
      else n.add(id);
      return n;
    });

  const quickCategorize = async (tx: TxView, cat: string) => {
    if (!cat || cat === tx.category) return;
    try {
      await api.categorize([tx.id], cat, false);
      refresh();
      toast.success(t("«{cat}» assegnata a {merchant}.", { cat, merchant: tx.merchant || t("movimento") }), {
        label: t("Anche in futuro"),
        onClick: async () => {
          try {
            const r = await api.categorize([tx.id], cat, true);
            refresh();
            toast.success(
              r.auto_categorized
                ? t("Regola creata: «{rule}» → {cat} ({n} movimenti sistemati).", { rule: r.rule_pattern ?? tx.merchant, cat, n: integer(r.auto_categorized) })
                : t("Regola creata: «{rule}» → {cat}.", { rule: r.rule_pattern ?? tx.merchant, cat }),
            );
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
    for (const tx of txs.data ?? []) {
      const last = out[out.length - 1];
      if (last && last.date === tx.date) {
        last.items.push(tx);
        last.total += tx.amount;
      } else out.push({ date: tx.date, items: [tx], total: tx.amount });
    }
    return out;
  }, [txs.data]);

  const multiAccount = useMemo(() => new Set((txs.data ?? []).map((tx) => tx.account)).size > 1, [txs.data]);
  const anyFilter = !!(text || category || tag || uncategorized || period !== "all");
  const tagNames = (tags.data ?? []).map((tx) => tx.name);

  return (
    <>
      <PageHeader
        title={t("Movimenti")}
        description={txs.data ? `${plural(txs.data.length, "movimento", "movimenti")}${txs.data.length >= limit ? ` ${t("(i più recenti)")}` : ""}` : " "}
        actions={
          <>
            <Button asChild variant="outline" size="sm" className="md:hidden">
              <Link to="/categorie">
                <Shapes /> {t("Categorie")}
              </Link>
            </Button>
            <Button
              variant={selecting ? "secondary" : "outline"}
              size="sm"
              onClick={() => {
                setSelecting((v) => !v);
                setSelected(new Set());
              }}
            >
              <CheckSquare /> {selecting ? t("Fine selezione") : t("Seleziona")}
            </Button>
          </>
        }
      />

      <Card className="gap-3 py-4">
        <CardContent className="flex flex-col gap-3 px-4">
          <InputGroup>
            <InputGroupAddon>
              <Search />
            </InputGroupAddon>
            <InputGroupInput type="search" placeholder={t("Cerca esercente, descrizione o controparte")} value={search} onChange={(e) => setSearch(e.target.value)} aria-label={t("Cerca nei movimenti")} />
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
              aria-label={t("Periodo")}
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
              <span className="text-muted-foreground">{t("Dal")}</span>
              <Input type="date" className="w-auto" value={from} max={to || undefined} onChange={(e) => setParams({ period: "custom", from: e.target.value })} aria-label={t("Dal")} />
              <span className="text-muted-foreground">{t("al")}</span>
              <Input type="date" className="w-auto" value={to} min={from || undefined} onChange={(e) => setParams({ period: "custom", to: e.target.value })} aria-label={t("Al")} />
            </div>
          )}
          <div className="flex flex-wrap items-center gap-2">
            <CategoryPicker value={category} onChange={(c) => setParams({ category: c || undefined })} allowNone noneLabel={t("Tutte le categorie")} className="h-8 w-auto min-w-44" ariaLabel={t("Filtra per categoria")} />
            <Select value={tag || "__all"} onValueChange={(v) => setParams({ tag: v === "__all" ? undefined : v })}>
              <SelectTrigger size="sm" className="min-w-36" aria-label={t("Filtra per tag")}>
                <Hash className="opacity-60" />
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="__all">{t("Tutti i tag")}</SelectItem>
                {tagNames.map((tx) => (
                  <SelectItem key={tx} value={tx}>
                    {tx}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
            <Toggle variant="outline" size="sm" pressed={uncategorized} onPressedChange={(p) => setParams({ uncategorized: p ? "1" : undefined })}>
              <Shapes /> {t("Da categorizzare")}
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
                <X /> {t("Azzera filtri")}
              </Button>
            )}
          </div>
        </CardContent>
      </Card>

      <Loaded {...txs} rows={8}>
        {(list) =>
          list.length === 0 ? (
            <Empty title={anyFilter ? t("Nessun movimento con questi filtri.") : t("Nessun movimento ancora.")}>
              {anyFilter ? t("Prova ad allargare il periodo o ad azzerare i filtri.") : t("Collega un conto dalla sezione Conti e sincronizza.")}
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
                      {g.items.map((tx) => (
                        <TxRow
                          key={tx.id}
                          tx={tx}
                          selecting={selecting}
                          selected={selected.has(tx.id)}
                          onToggle={() => toggle(tx.id)}
                          onOpen={() => setDetail(tx)}
                          showAccount={multiAccount}
                          onQuickCategory={(c) => quickCategorize(tx, c)}
                        />
                      ))}
                    </ul>
                  </Card>
                </section>
              ))}
              {list.length >= limit && limit < MAX && (
                <Button variant="outline" className="self-center" onClick={() => setLimit(LIMITS.find((l) => l > limit) ?? limit)}>
                  {t("Mostra altri movimenti")}
                </Button>
              )}
              {list.length >= MAX && <p className="text-center text-sm text-muted-foreground">{t("Mostrati i primi {n}: restringi il periodo o cerca per vedere il resto.", { n: MAX })}</p>}
            </div>
          )
        }
      </Loaded>

      {selecting && selected.size > 0 && (
        <div role="region" aria-label={t("Azioni sui movimenti selezionati")} className="fixed inset-x-4 bottom-20 z-40 mx-auto flex max-w-md items-center gap-2 rounded-2xl border bg-popover p-2 pl-4 shadow-lg md:bottom-6">
          <span className="flex-1 text-sm font-medium">{t("{n} selezionati", { n: integer(selected.size) })}</span>
          <Button size="sm" variant="secondary" onClick={() => setBulk("categorize")}>
            <Shapes /> {t("Categoria")}
          </Button>
          <Button size="sm" variant="secondary" onClick={() => setBulk("tag")}>
            <Tag /> {t("Tag")}
          </Button>
          <Button size="icon-sm" variant="ghost" onClick={() => setSelected(new Set())} aria-label={t("Annulla selezione")}>
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
  tx,
  selecting,
  selected,
  onToggle,
  onOpen,
  showAccount,
  onQuickCategory,
}: {
  tx: TxView;
  selecting: boolean;
  selected: boolean;
  onToggle: () => void;
  onOpen: () => void;
  showAccount: boolean;
  onQuickCategory: (c: string) => void;
}) {
  const { categoryByName } = useApp();
  const title = tx.merchant || tx.counterparty || t("Movimento");
  const cat = tx.category ? categoryByName.get(tx.category.toLowerCase()) : undefined;
  return (
    <li className={cn("flex items-center gap-3 px-3 py-2.5 transition-colors", selected && "bg-muted")}>
      {selecting && <Checkbox checked={selected} onCheckedChange={onToggle} aria-label={t("Seleziona {name}", { name: title })} />}
      {selecting ? (
        <CategoryIcon category={cat} />
      ) : (
        <CategoryPicker
          value={tx.category ?? ""}
          onChange={onQuickCategory}
          trigger={
            <button type="button" className="rounded-lg focus-visible:ring-2 focus-visible:ring-ring/50 focus-visible:outline-none" aria-label={t("Categoria di {name}: {category}. Cambia", { name: title, category: tx.category ?? t("da categorizzare") })}>
              <CategoryIcon category={cat} className={cn("transition-transform hover:scale-105", !tx.category && "ring-2 ring-warning/60")} />
            </button>
          }
        />
      )}
      <button type="button" className="flex min-w-0 flex-1 items-center gap-3 text-left focus-visible:outline-none" onClick={selecting ? onToggle : onOpen}>
        <span className="flex min-w-0 flex-1 flex-col gap-0.5">
          <span className="truncate text-sm font-medium">{title}</span>
          <span className="flex min-w-0 flex-wrap items-center gap-x-2 gap-y-1 text-xs text-muted-foreground">
            <span className={cn(!tx.category && "font-medium text-warning")}>{tx.category ?? t("Da categorizzare")}</span>
            {tx.amortized_months && <Badge variant="outline">{t("in {n} mesi", { n: tx.amortized_months })}</Badge>}
            {showAccount && <span className="truncate">· {tx.account}</span>}
            {tx.tags.length > 0 && <TagChips tags={tx.tags} />}
          </span>
          {tx.description && tx.description !== tx.merchant && <span className="line-clamp-1 text-xs text-muted-foreground/80">{tx.description}</span>}
        </span>
        <span className={cn("tabular shrink-0 text-sm font-medium", tx.amount > 0 && "text-positive")}>{tx.amount > 0 ? `+${euro(tx.amount)}` : euro(tx.amount)}</span>
      </button>
    </li>
  );
}
