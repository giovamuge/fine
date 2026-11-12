// Transaction detail: category (with the explicit "also for future movements of this
// merchant" choice), tags with autocomplete and optional rule, amortization.
import { useEffect, useState, type ReactNode } from "react";
import { Loader2 } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { Label } from "@/components/ui/label";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { Separator } from "@/components/ui/separator";
import { CategoryPicker } from "@/components/CategoryPicker";
import { ResponsiveDialog } from "@/components/ResponsiveDialog";
import { TagAdd, TagChips } from "@/components/TagInput";
import { api, errorMessage } from "@/lib/api";
import { useApp } from "@/lib/app-state";
import { CategoryIcon } from "@/lib/categories";
import { euro, fmtDate, integer } from "@/lib/format";
import { useToast } from "@/lib/toast";
import type { TxView } from "@/lib/types";
import { cn } from "@/lib/utils";

const BY: Record<string, string> = { manual: "a mano", rule: "da una regola", ai: "dall'AI", learned: "da una regola" };

function Section({ title, children }: { title: string; children: ReactNode }) {
  return (
    <section className="flex flex-col gap-3">
      <h3 className="text-sm font-semibold">{title}</h3>
      {children}
    </section>
  );
}

function CheckRow({ id, checked, onChange, title, hint }: { id: string; checked: boolean; onChange: (v: boolean) => void; title: string; hint: string }) {
  return (
    <div className="flex items-start gap-3 rounded-lg border p-3">
      <Checkbox id={id} checked={checked} onCheckedChange={(v) => onChange(v === true)} className="mt-0.5" />
      <Label htmlFor={id} className="flex flex-col items-start gap-1 font-normal">
        <span className="font-medium">{title}</span>
        <span className="text-xs text-muted-foreground">{hint}</span>
      </Label>
    </div>
  );
}

export function TxDetail({ tx, onClose, tagSuggestions }: { tx: TxView | null; onClose: () => void; tagSuggestions: string[] }) {
  const { refresh, categoryByName } = useApp();
  const toast = useToast();
  const [cat, setCat] = useState("");
  const [learn, setLearn] = useState(false);
  const [tagRule, setTagRule] = useState(false);
  const [months, setMonths] = useState(0);
  const [busy, setBusy] = useState<string | null>(null);
  // Keep the last transaction while the dialog animates closed.
  const [shown, setShown] = useState<TxView | null>(tx);

  useEffect(() => {
    if (tx) setShown(tx);
  }, [tx]);
  useEffect(() => {
    if (!tx) return;
    setCat(tx.category ?? "");
    setLearn(false);
    setTagRule(false);
    setMonths(tx.amortized_months ?? 0);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [tx?.id]);

  const t = tx ?? shown;
  if (!t) return null;
  const title = t.merchant || t.counterparty || "Movimento";
  const canRule = (t.merchant || "").replace(/[^\p{L}\p{N}]/gu, "").length >= 3;
  const catChanged = cat !== (t.category ?? "");

  const run = async (what: string, fn: () => Promise<string>) => {
    setBusy(what);
    try {
      const msg = await fn();
      refresh();
      if (msg) toast.success(msg);
    } catch (e) {
      toast.error(errorMessage(e));
    } finally {
      setBusy(null);
    }
  };

  const applyCategory = () =>
    run("cat", async () => {
      const r = await api.categorize([t.id], cat, learn && canRule);
      if (r.rule_pattern) return `«${cat}» assegnata. Regola «${r.rule_pattern}» creata${r.auto_categorized ? `: ${integer(r.auto_categorized)} altri movimenti sistemati` : ""}.`;
      return `«${cat}» assegnata.`;
    });

  const addTag = (name: string) =>
    run("tag", async () => {
      const r = await api.tag([t.id], [name], tagRule && canRule);
      return r.rule_pattern ? `#${name} aggiunto, anche ai futuri movimenti di «${r.rule_pattern}»${r.auto_tagged ? ` (${integer(r.auto_tagged)} già taggati)` : ""}.` : `#${name} aggiunto.`;
    });

  const removeTag = (name: string) =>
    run("tag", async () => {
      await api.untag([t.id], [name]);
      return `#${name} rimosso.`;
    });

  const applyAmortize = () =>
    run("amort", async () => {
      await api.amortize(t.id, months);
      return months > 1 ? `Spesa spalmata su ${months} mesi.` : "Ammortamento rimosso.";
    });

  const category = t.category ? categoryByName.get(t.category.toLowerCase()) : undefined;
  const facts: [string, ReactNode][] = [
    ["Data", fmtDate(t.date)],
    ...(t.counterparty && t.counterparty !== t.merchant ? ([["Controparte", t.counterparty]] as [string, ReactNode][]) : []),
    ...(t.description ? ([["Descrizione", t.description]] as [string, ReactNode][]) : []),
    ["Conto", t.account],
    ...(t.category && t.categorized_by ? ([["Categoria", `assegnata ${BY[t.categorized_by] ?? t.categorized_by}`]] as [string, ReactNode][]) : []),
  ];

  return (
    <ResponsiveDialog open={!!tx} onClose={onClose} title={title} description={fmtDate(t.date)}>
      <div className="flex flex-col gap-5 pb-2">
        <div className="flex items-center gap-3">
          <CategoryIcon category={category} className="size-11 rounded-xl text-xl" />
          <div className={cn("tabular text-3xl font-semibold tracking-tight", t.amount > 0 && "text-positive")}>{t.amount > 0 ? `+${euro(t.amount)}` : euro(t.amount)}</div>
        </div>
        <dl className="grid grid-cols-[auto_1fr] gap-x-4 gap-y-2 text-sm">
          {facts.map(([k, v]) => (
            <div key={k} className="contents">
              <dt className="text-muted-foreground">{k}</dt>
              <dd className="break-words">{v}</dd>
            </div>
          ))}
        </dl>

        <Separator />
        <Section title="Categoria">
          <CategoryPicker value={cat} onChange={setCat} allowNone ariaLabel="Categoria" />
          {canRule && (
            <CheckRow
              id="learn"
              checked={learn}
              onChange={setLearn}
              title={`Usa questa categoria anche per i prossimi movimenti di «${t.merchant}»`}
              hint="Crea una regola: vale per i movimenti futuri e per quelli passati ancora da categorizzare."
            />
          )}
          <div>
            <Button disabled={!cat || (!catChanged && !learn) || busy !== null} onClick={applyCategory}>
              {busy === "cat" && <Loader2 className="animate-spin" />}
              {learn ? "Applica e crea regola" : "Applica"}
            </Button>
          </div>
        </Section>

        <Separator />
        <Section title="Tag">
          {t.tags.length ? (
            <TagChips tags={t.tags} onRemove={removeTag} />
          ) : (
            <p className="text-sm text-muted-foreground">Nessun tag. I tag sono liberi (#vacanza-londra, #lavoro, #rimborsabile…) e indipendenti dalla categoria.</p>
          )}
          <TagAdd suggestions={tagSuggestions.filter((s) => !t.tags.includes(s))} onAdd={addTag} disabled={busy !== null} />
          {canRule && (
            <CheckRow id="tagrule" checked={tagRule} onChange={setTagRule} title={`Tagga automaticamente anche i prossimi movimenti di «${t.merchant}»`} hint="Vale per i tag che aggiungi da ora." />
          )}
        </Section>

        {t.amount < 0 && (
          <>
            <Separator />
            <Section title="Ammortamento">
              <p className="text-sm text-muted-foreground">Una spesa grande può pesare 1/N al mese sul budget invece che tutta nel mese in cui l'hai fatta.</p>
              <div className="flex gap-2">
                <Select value={String(months)} onValueChange={(v) => setMonths(Number(v))}>
                  <SelectTrigger className="flex-1" aria-label="Mesi di ammortamento">
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem value="0">Nessuno</SelectItem>
                    {[3, 6, 12, 24, 36].map((m) => (
                      <SelectItem key={m} value={String(m)}>
                        {m} mesi ({euro(-t.amount / m)} al mese)
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
                <Button variant="outline" disabled={months === (t.amortized_months ?? 0) || busy !== null} onClick={applyAmortize}>
                  {busy === "amort" && <Loader2 className="animate-spin" />}
                  Applica
                </Button>
              </div>
            </Section>
          </>
        )}
      </div>
    </ResponsiveDialog>
  );
}
