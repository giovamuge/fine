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
import { t } from "@/lib/i18n";

const BY: Record<string, string> = {
  manual: t("assegnata a mano"),
  rule: t("assegnata da una regola"),
  ai: t("assegnata dall'AI"),
  learned: t("assegnata da una regola"),
  paypal: t("assegnata riconciliando il pagamento PayPal"),
};

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

  const item = tx ?? shown;
  if (!item) return null;
  const title = item.merchant || item.counterparty || t("Movimento");
  const canRule = (item.merchant || "").replace(/[^\p{L}\p{N}]/gu, "").length >= 3;
  const catChanged = cat !== (item.category ?? "");

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
      const r = await api.categorize([item.id], cat, learn && canRule);
      if (r.rule_pattern)
        return r.auto_categorized
          ? t("«{cat}» assegnata. Regola «{rule}» creata: {n} altri movimenti sistemati.", { cat, rule: r.rule_pattern, n: integer(r.auto_categorized) })
          : t("«{cat}» assegnata. Regola «{rule}» creata.", { cat, rule: r.rule_pattern });
      return t("«{cat}» assegnata.", { cat });
    });

  const addTag = (name: string) =>
    run("tag", async () => {
      const r = await api.tag([item.id], [name], tagRule && canRule);
      if (!r.rule_pattern) return t("#{tag} aggiunto.", { tag: name });
      return r.auto_tagged
        ? t("#{tag} aggiunto, anche ai futuri movimenti di «{rule}» ({n} già taggati).", { tag: name, rule: r.rule_pattern, n: integer(r.auto_tagged) })
        : t("#{tag} aggiunto, anche ai futuri movimenti di «{rule}».", { tag: name, rule: r.rule_pattern });
    });

  const removeTag = (name: string) =>
    run("tag", async () => {
      await api.untag([item.id], [name]);
      return t("#{tag} rimosso.", { tag: name });
    });

  const applyAmortize = () =>
    run("amort", async () => {
      await api.amortize(item.id, months);
      return months > 1 ? t("Spesa spalmata su {n} mesi.", { n: months }) : t("Ammortamento rimosso.");
    });

  const category = item.category ? categoryByName.get(item.category.toLowerCase()) : undefined;
  const facts: [string, ReactNode][] = [
    [t("Data"), fmtDate(item.date)],
    ...(item.counterparty && item.counterparty !== item.merchant ? ([[t("Controparte"), item.counterparty]] as [string, ReactNode][]) : []),
    ...(item.description ? ([[t("Descrizione"), item.description]] as [string, ReactNode][]) : []),
    [t("Conto"), item.account],
    ...(item.reconciled ? ([["PayPal", item.reconciled]] as [string, ReactNode][]) : []),
    ...(item.category && item.categorized_by ? ([[t("Categoria"), BY[item.categorized_by] ?? item.categorized_by]] as [string, ReactNode][]) : []),
  ];

  return (
    <ResponsiveDialog open={!!tx} onClose={onClose} title={title} description={fmtDate(item.date)}>
      <div className="flex flex-col gap-5 pb-2">
        <div className="flex items-center gap-3">
          <CategoryIcon category={category} className="size-11 rounded-xl text-xl" />
          <div className={cn("tabular text-3xl font-semibold tracking-tight", item.amount > 0 && "text-positive")}>{item.amount > 0 ? `+${euro(item.amount)}` : euro(item.amount)}</div>
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
        <Section title={t("Categoria")}>
          <CategoryPicker value={cat} onChange={setCat} allowNone ariaLabel={t("Categoria")} />
          {canRule && (
            <CheckRow
              id="learn"
              checked={learn}
              onChange={setLearn}
              title={t("Usa questa categoria anche per i prossimi movimenti di «{merchant}»", { merchant: item.merchant })}
              hint={t("Crea una regola: vale per i movimenti futuri e per quelli passati ancora da categorizzare.")}
            />
          )}
          <div>
            <Button disabled={!cat || (!catChanged && !learn) || busy !== null} onClick={applyCategory}>
              {busy === "cat" && <Loader2 className="animate-spin" />}
              {learn ? t("Applica e crea regola") : t("Applica")}
            </Button>
          </div>
        </Section>

        <Separator />
        <Section title={t("Tag")}>
          {item.tags.length ? (
            <TagChips tags={item.tags} onRemove={removeTag} />
          ) : (
            <p className="text-sm text-muted-foreground">{t("Nessun tag. I tag sono liberi (#vacanza-londra, #lavoro, #rimborsabile…) e indipendenti dalla categoria.")}</p>
          )}
          <TagAdd suggestions={tagSuggestions.filter((s) => !item.tags.includes(s))} onAdd={addTag} disabled={busy !== null} />
          {canRule && (
            <CheckRow id="tagrule" checked={tagRule} onChange={setTagRule} title={t("Tagga automaticamente anche i prossimi movimenti di «{merchant}»", { merchant: item.merchant })} hint={t("Vale per i tag che aggiungi da ora.")} />
          )}
        </Section>

        {item.amount < 0 && (
          <>
            <Separator />
            <Section title={t("Ammortamento")}>
              <p className="text-sm text-muted-foreground">{t("Una spesa grande può pesare 1/N al mese sul budget invece che tutta nel mese in cui l'hai fatta.")}</p>
              <div className="flex gap-2">
                <Select value={String(months)} onValueChange={(v) => setMonths(Number(v))}>
                  <SelectTrigger className="flex-1" aria-label={t("Mesi di ammortamento")}>
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem value="0">{t("Nessuno")}</SelectItem>
                    {[3, 6, 12, 24, 36].map((m) => (
                      <SelectItem key={m} value={String(m)}>
                        {t("{n} mesi ({amount} al mese)", { n: m, amount: euro(-item.amount / m) })}
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
                <Button variant="outline" disabled={months === (item.amortized_months ?? 0) || busy !== null} onClick={applyAmortize}>
                  {busy === "amort" && <Loader2 className="animate-spin" />}
                  {t("Applica")}
                </Button>
              </div>
            </Section>
          </>
        )}
      </div>
    </ResponsiveDialog>
  );
}
