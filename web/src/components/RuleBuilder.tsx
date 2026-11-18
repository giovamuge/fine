// Visual editor for budget rules: "count transactions matching all/any of …, except …".
import { useState, type KeyboardEvent } from "react";
import { Plus, Trash2, X } from "lucide-react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuTrigger } from "@/components/ui/dropdown-menu";
import { Input } from "@/components/ui/input";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";
import { CategoryPicker } from "@/components/CategoryPicker";
import { COND_LABEL, COND_TYPES, condType, emptyLeaf, type CondType, type Leaf, type RuleForm } from "@/lib/budget-rules";
import { KINDS, KIND_LABEL } from "@/lib/categories";
import type { AccountView, CategoryKind } from "@/lib/types";
import { LOCALE, t, tc } from "@/lib/i18n";

// Monday first; 1 Jan 2024 was a Monday.
const MONDAYS = Array.from({ length: 7 }, (_, i) => new Date(2024, 0, 1 + i));
const WEEKDAYS = MONDAYS.map((d) => new Intl.DateTimeFormat(LOCALE, { weekday: "narrow" }).format(d));
const WEEKDAY_NAMES = MONDAYS.map((d) => new Intl.DateTimeFormat(LOCALE, { weekday: "long" }).format(d));

function Chips({ values, onRemove, prefix = "" }: { values: string[]; onRemove: (v: string) => void; prefix?: string }) {
  if (!values.length) return null;
  return (
    <span className="flex flex-wrap gap-1">
      {values.map((v) => (
        <Badge key={v} variant="secondary" className="gap-0.5 font-normal">
          {prefix}
          {v}
          <button type="button" className="-mr-1 ml-0.5 rounded-full p-0.5 hover:bg-foreground/10" onClick={() => onRemove(v)} aria-label={`Rimuovi ${v}`}>
            <X className="size-3" />
          </button>
        </Badge>
      ))}
    </span>
  );
}

/** Free text list: Enter or "Aggiungi" adds the typed value. */
function TextList({ values, onChange, placeholder, suggestions, prefix }: { values: string[]; onChange: (v: string[]) => void; placeholder: string; suggestions?: string[]; prefix?: string }) {
  const [text, setText] = useState("");
  const listId = `dl-${placeholder.length}-${values.length}`;
  const add = () => {
    const v = text.trim().replace(/^#/, "");
    if (v && !values.includes(v)) onChange([...values, v]);
    setText("");
  };
  const onKey = (e: KeyboardEvent<HTMLInputElement>) => {
    if (e.key === "Enter" || e.key === ",") {
      e.preventDefault();
      add();
    }
  };
  return (
    <div className="flex flex-col gap-2">
      <Chips values={values} prefix={prefix} onRemove={(v) => onChange(values.filter((x) => x !== v))} />
      <div className="flex gap-2">
        <Input value={text} onChange={(e) => setText(e.target.value)} onKeyDown={onKey} placeholder={placeholder} list={suggestions ? listId : undefined} autoCapitalize="none" autoCorrect="off" className="h-8" />
        {suggestions && (
          <datalist id={listId}>
            {suggestions.filter((s) => !values.includes(s)).map((s) => (
              <option key={s} value={s} />
            ))}
          </datalist>
        )}
        <Button type="button" size="sm" variant="outline" onClick={add} disabled={!text.trim()}>
          <Plus /> {t("Aggiungi")}
        </Button>
      </div>
    </div>
  );
}

function num(v: string): number | undefined {
  const n = Number(v.replace(",", "."));
  return v.trim() === "" || !Number.isFinite(n) ? undefined : n;
}

function LeafEditor({ leaf, onChange, tags, accounts }: { leaf: Leaf; onChange: (l: Leaf) => void; tags: string[]; accounts: AccountView[] }) {
  if ("category" in leaf) {
    const list = leaf.category;
    return (
      <div className="flex flex-col gap-2">
        <Chips values={list} onRemove={(v) => onChange({ category: list.filter((x) => x !== v) })} />
        <CategoryPicker
          value=""
          placeholder={t("Aggiungi una categoria")}
          ariaLabel={t("Aggiungi una categoria")}
          className="h-8"
          onChange={(name) => name && !list.includes(name) && onChange({ category: [...list, name] })}
        />
      </div>
    );
  }
  if ("tag" in leaf) return <TextList values={leaf.tag} prefix="#" onChange={(v) => onChange({ tag: v })} placeholder={t("es. vacanza-londra")} suggestions={tags} />;
  if ("text" in leaf) return <TextList values={leaf.text} onChange={(v) => onChange({ text: v })} placeholder={t("es. amazon")} />;
  if ("account" in leaf) {
    const list = leaf.account;
    return (
      <div className="flex flex-col gap-2">
        <Chips values={list} onRemove={(v) => onChange({ account: list.filter((x) => x !== v) })} />
        <Select value="" onValueChange={(v) => v && !list.includes(v) && onChange({ account: [...list, v] })}>
          <SelectTrigger className="h-8 w-full" aria-label={t("Aggiungi un conto")}>
            <SelectValue placeholder={t("Aggiungi un conto")} />
          </SelectTrigger>
          <SelectContent>
            {accounts.map((a) => {
              const key = a.iban || a.custom_label || a.label;
              return (
                <SelectItem key={a.uid} value={key}>
                  {a.label}
                </SelectItem>
              );
            })}
          </SelectContent>
        </Select>
      </div>
    );
  }
  if ("amount" in leaf) {
    const a = leaf.amount;
    return (
      <div className="flex items-center gap-2 text-sm">
        <span className="text-muted-foreground">{t("da")}</span>
        <Input inputMode="decimal" className="h-8" placeholder="min €" defaultValue={a.min ?? ""} onChange={(e) => onChange({ amount: { ...a, min: num(e.target.value) } })} aria-label={t("Importo minimo")} />
        <span className="text-muted-foreground">{t("a")}</span>
        <Input inputMode="decimal" className="h-8" placeholder="max €" defaultValue={a.max ?? ""} onChange={(e) => onChange({ amount: { ...a, max: num(e.target.value) } })} aria-label={t("Importo massimo")} />
      </div>
    );
  }
  if ("direction" in leaf) {
    return (
      <Select value={leaf.direction} onValueChange={(v) => onChange({ direction: v as "out" | "in" })}>
        <SelectTrigger className="h-8 w-full" aria-label={t("Direzione")}>
          <SelectValue />
        </SelectTrigger>
        <SelectContent>
          <SelectItem value="out">{t("Solo uscite")}</SelectItem>
          <SelectItem value="in">{t("Solo entrate e rimborsi")}</SelectItem>
        </SelectContent>
      </Select>
    );
  }
  if ("weekday" in leaf) {
    return (
      <ToggleGroup type="multiple" variant="outline" size="sm" value={leaf.weekday.map(String)} onValueChange={(v) => onChange({ weekday: v.map(Number).sort() })} aria-label={t("Giorni della settimana")}>
        {WEEKDAYS.map((d, i) => (
          <ToggleGroupItem key={i} value={String(i + 1)} aria-label={WEEKDAY_NAMES[i]} className="w-8">
            {d}
          </ToggleGroupItem>
        ))}
      </ToggleGroup>
    );
  }
  if ("day_of_month" in leaf) {
    const d = leaf.day_of_month;
    const set = (k: "from" | "to", v: string) => onChange({ day_of_month: { ...d, [k]: Math.min(31, Math.max(1, num(v) ?? 1)) } });
    return (
      <div className="flex items-center gap-2 text-sm">
        <span className="text-muted-foreground">{t("dal giorno")}</span>
        <Input inputMode="numeric" className="h-8 w-16" defaultValue={d.from} onChange={(e) => set("from", e.target.value)} aria-label={t("Dal giorno")} />
        <span className="text-muted-foreground">{t("al")}</span>
        <Input inputMode="numeric" className="h-8 w-16" defaultValue={d.to} onChange={(e) => set("to", e.target.value)} aria-label={tc("rule", "Al giorno")} />
      </div>
    );
  }
  if ("category_kind" in leaf) {
    return (
      <ToggleGroup type="multiple" variant="outline" size="sm" value={leaf.category_kind} onValueChange={(v) => onChange({ category_kind: v as CategoryKind[] })} className="flex-wrap">
        {KINDS.map((k) => (
          <ToggleGroupItem key={k} value={k}>
            {KIND_LABEL[k]}
          </ToggleGroupItem>
        ))}
      </ToggleGroup>
    );
  }
  if ("uncategorized" in leaf) {
    return (
      <Select value={String(leaf.uncategorized)} onValueChange={(v) => onChange({ uncategorized: v === "true" })}>
        <SelectTrigger className="h-8 w-full" aria-label={t("Categorizzazione")}>
          <SelectValue />
        </SelectTrigger>
        <SelectContent>
          <SelectItem value="true">{t("Solo da categorizzare")}</SelectItem>
          <SelectItem value="false">{t("Solo già categorizzati")}</SelectItem>
        </SelectContent>
      </Select>
    );
  }
  return (
    <Select value={String(leaf.fixed)} onValueChange={(v) => onChange({ fixed: v === "true" })}>
      <SelectTrigger className="h-8 w-full" aria-label={t("Spese fisse")}>
        <SelectValue />
      </SelectTrigger>
      <SelectContent>
        <SelectItem value="false">{t("Escludi affitto, bollette e altre spese fisse pianificate")}</SelectItem>
        <SelectItem value="true">{t("Solo le spese fisse pianificate")}</SelectItem>
      </SelectContent>
    </Select>
  );
}

function AddCondition({ onAdd, label }: { onAdd: (type: CondType) => void; label: string }) {
  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button type="button" variant="outline" size="sm" className="self-start">
          <Plus /> {label}
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="start" className="w-60">
        {COND_TYPES.map((type) => (
          <DropdownMenuItem key={type} onSelect={() => onAdd(type)}>
            {COND_LABEL[type]}
          </DropdownMenuItem>
        ))}
      </DropdownMenuContent>
    </DropdownMenu>
  );
}

function LeafList({ leaves, onChange, tags, accounts }: { leaves: Leaf[]; onChange: (l: Leaf[]) => void; tags: string[]; accounts: AccountView[] }) {
  return (
    <ul className="flex flex-col gap-2">
      {leaves.map((leaf, i) => (
        <li key={`${i}-${condType(leaf)}`} className="flex flex-col gap-2 rounded-lg border bg-background p-3">
          <div className="flex items-center justify-between gap-2">
            <span className="text-sm font-medium">{COND_LABEL[condType(leaf)]}</span>
            <Button type="button" variant="ghost" size="icon-sm" onClick={() => onChange(leaves.filter((_, j) => j !== i))} aria-label={t("Rimuovi la condizione {name}", { name: COND_LABEL[condType(leaf)] })}>
              <Trash2 />
            </Button>
          </div>
          <LeafEditor leaf={leaf} onChange={(l) => onChange(leaves.map((x, j) => (j === i ? l : x)))} tags={tags} accounts={accounts} />
        </li>
      ))}
    </ul>
  );
}

export function RuleBuilder({ value, onChange, emptyHint, tags, accounts }: { value: RuleForm; onChange: (f: RuleForm) => void; emptyHint: string; tags: string[]; accounts: AccountView[] }) {
  return (
    <div className="flex flex-col gap-4">
      <div className="flex flex-col gap-2">
        <div className="flex flex-wrap items-center gap-2 text-sm">
          <span>{value.include.length > 1 ? t("Conta i movimenti che soddisfano") : value.include.length === 1 ? t("Conta i movimenti che soddisfano questa condizione:") : t("Conta i movimenti che soddisfano le condizioni che aggiungi.")}</span>
          {value.include.length > 1 ? (
            <Select value={value.mode} onValueChange={(m) => onChange({ ...value, mode: m as "all" | "any" })}>
              <SelectTrigger size="sm" className="h-7 w-auto" aria-label={t("Come combinare le condizioni")}>
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="all">{t("tutte")}</SelectItem>
                <SelectItem value="any">{t("almeno una")}</SelectItem>
              </SelectContent>
            </Select>
          ) : null}
          {value.include.length > 1 && <span>{t("queste condizioni:")}</span>}
        </div>
        {value.include.length === 0 && <p className="text-xs text-muted-foreground">{emptyHint}</p>}
        <LeafList leaves={value.include} onChange={(include) => onChange({ ...value, include })} tags={tags} accounts={accounts} />
        <AddCondition label={t("Aggiungi condizione")} onAdd={(type) => onChange({ ...value, include: [...value.include, emptyLeaf(type)] })} />
      </div>
      <div className="flex flex-col gap-2">
        <span className="text-sm">{t("Escludi quelli con almeno una di queste:")}</span>
        <LeafList leaves={value.exclude} onChange={(exclude) => onChange({ ...value, exclude })} tags={tags} accounts={accounts} />
        <AddCondition label={t("Aggiungi esclusione")} onAdd={(type) => onChange({ ...value, exclude: [...value.exclude, emptyLeaf(type)] })} />
      </div>
    </div>
  );
}
