// Manage categories: create, rename, kind, icon, color, the rules that feed them,
// delete (moving their transactions elsewhere = merge).
import { useEffect, useMemo, useState } from "react";
import { ArrowRight, Loader2, Plus, Search, Sparkles, Trash2, Wand2 } from "lucide-react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { InputGroup, InputGroupAddon, InputGroupInput } from "@/components/ui/input-group";
import { Label } from "@/components/ui/label";
import { RadioGroup, RadioGroupItem } from "@/components/ui/radio-group";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { Separator } from "@/components/ui/separator";
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { CategoryPicker } from "@/components/CategoryPicker";
import { PageHeader } from "@/components/PageHeader";
import { ResponsiveDialog } from "@/components/ResponsiveDialog";
import { Empty, Rows } from "@/components/States";
import { api, errorMessage } from "@/lib/api";
import { useApp } from "@/lib/app-state";
import { COLORS, COLOR_LABEL, CategoryIcon, EMOJI, KINDS, KIND_HINT, KIND_LABEL, SWATCH, colorOf, type CategoryColor } from "@/lib/categories";
import { euro, integer, plural, ym } from "@/lib/format";
import { href, navigate } from "@/lib/router";
import { useToast } from "@/lib/toast";
import type { CategoryKind, CategoryView, Rule } from "@/lib/types";
import { useAsync } from "@/lib/useAsync";
import { cn } from "@/lib/utils";

const SOURCE: Record<string, string> = { seed: "predefinita", manual: "tua", ai: "AI", learned: "tua" };

export function Categorie() {
  const { version, categories } = useApp();
  const rules = useAsync(() => api.rules(), [version]);
  const summary = useAsync(() => api.summary(ym(new Date())), [version]);
  const [kind, setKind] = useState<CategoryKind>("expense");
  const [filter, setFilter] = useState("");
  const [editing, setEditing] = useState<CategoryView | "new" | null>(null);
  const [deleting, setDeleting] = useState<CategoryView | null>(null);

  const spent = useMemo(() => new Map((summary.data?.by_category ?? []).map((c) => [c.category.toLowerCase(), c.amount])), [summary.data]);
  const counts = useMemo(() => Object.fromEntries(KINDS.map((k) => [k, categories.filter((c) => c.kind === k).length])) as Record<CategoryKind, number>, [categories]);
  const list = categories.filter((c) => c.kind === kind && c.name.toLowerCase().includes(filter.trim().toLowerCase()));
  const editingCat = editing === "new" ? null : editing;

  return (
    <>
      <PageHeader
        title="Categorie"
        description="Personalizza nomi, icone e colori. Le regole assegnano la categoria ai nuovi movimenti in automatico."
        actions={
          <Button onClick={() => setEditing("new")}>
            <Plus /> Nuova categoria
          </Button>
        }
      />

      <div className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
        <Tabs value={kind} onValueChange={(v) => setKind(v as CategoryKind)}>
          <TabsList>
            {KINDS.map((k) => (
              <TabsTrigger key={k} value={k}>
                {KIND_LABEL[k]} <span className="text-muted-foreground tabular">{counts[k]}</span>
              </TabsTrigger>
            ))}
          </TabsList>
        </Tabs>
        <InputGroup className="sm:max-w-64">
          <InputGroupAddon>
            <Search />
          </InputGroupAddon>
          <InputGroupInput placeholder="Cerca una categoria" value={filter} onChange={(e) => setFilter(e.target.value)} aria-label="Cerca una categoria" />
        </InputGroup>
      </div>
      <p className="-mt-3 text-sm text-muted-foreground">{KIND_HINT[kind]}</p>

      {categories.length === 0 ? (
        <Rows rows={6} />
      ) : list.length === 0 ? (
        <Empty title={filter ? "Nessuna categoria trovata." : "Nessuna categoria di questo tipo."} action={<Button variant="outline" onClick={() => setEditing("new")}><Plus /> Crea categoria</Button>} />
      ) : (
        <div className="grid gap-3 sm:grid-cols-2 xl:grid-cols-3">
          {list.map((c) => {
            const amount = spent.get(c.name.toLowerCase());
            return (
              <Card key={c.name} className="p-0 transition-shadow hover:shadow-md">
                <button type="button" onClick={() => setEditing(c)} className="flex items-center gap-3 rounded-xl p-4 text-left focus-visible:ring-2 focus-visible:ring-ring/50 focus-visible:outline-none">
                  <CategoryIcon category={c} className="size-11 rounded-xl text-xl" />
                  <span className="flex min-w-0 flex-1 flex-col gap-0.5">
                    <span className="truncate font-medium">{c.name}</span>
                    <span className="text-xs text-muted-foreground">
                      {plural(c.transactions, "movimento", "movimenti")} · {plural(c.rules, "regola", "regole")}
                    </span>
                  </span>
                  {amount !== undefined && (
                    <span className="flex shrink-0 flex-col items-end">
                      <span className="tabular text-sm font-medium">{euro(amount)}</span>
                      <span className="text-[11px] text-muted-foreground">questo mese</span>
                    </span>
                  )}
                </button>
              </Card>
            );
          })}
        </div>
      )}

      <CategoryEditor
        open={editing !== null}
        category={editingCat}
        defaultKind={kind}
        rules={(rules.data ?? []).filter((r) => editingCat && r.category.toLowerCase() === editingCat.name.toLowerCase())}
        onClose={() => setEditing(null)}
        onDelete={(c) => {
          setEditing(null);
          setDeleting(c);
        }}
      />
      <DeleteCategoryDialog category={deleting} onClose={() => setDeleting(null)} />
    </>
  );
}

function CategoryEditor({
  open,
  category,
  defaultKind,
  rules,
  onClose,
  onDelete,
}: {
  open: boolean;
  category: CategoryView | null;
  defaultKind: CategoryKind;
  rules: Rule[];
  onClose: () => void;
  onDelete: (c: CategoryView) => void;
}) {
  const { refresh } = useApp();
  const toast = useToast();
  const [name, setName] = useState("");
  const [kind, setKind] = useState<CategoryKind>("expense");
  const [icon, setIcon] = useState("");
  const [color, setColor] = useState<CategoryColor | "">("");
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    if (!open) return;
    setName(category?.name ?? "");
    setKind(category?.kind ?? defaultKind);
    setIcon(category?.icon ?? "");
    setColor((category?.color as CategoryColor | null) ?? "");
  }, [open, category, defaultKind]);

  const preview = { name: name || "Nuova", icon: icon || null, color: color || null };
  const save = async () => {
    const n = name.trim();
    if (!n) return;
    setBusy(true);
    try {
      if (category) {
        await api.updateCategory(category.name, {
          rename: n !== category.name ? n : undefined,
          kind: kind !== category.kind ? kind : undefined,
          icon: icon !== (category.icon ?? "") ? icon : undefined,
          color: color !== (category.color ?? "") ? color : undefined,
        });
        toast.success(`Categoria «${n}» aggiornata.`);
      } else {
        await api.createCategory(n, kind, icon, color);
        toast.success(`Categoria «${n}» creata. L'AI la userà per i movimenti ancora da categorizzare.`);
      }
      refresh();
      onClose();
    } catch (e) {
      toast.error(errorMessage(e));
    } finally {
      setBusy(false);
    }
  };

  const removeRule = async (r: Rule) => {
    try {
      await api.deleteRule(r.id);
      refresh();
      toast.success(`Regola «${r.pattern}» eliminata: i movimenti già categorizzati restano come sono.`);
    } catch (e) {
      toast.error(errorMessage(e));
    }
  };

  return (
    <ResponsiveDialog
      open={open}
      onClose={onClose}
      title={category ? "Modifica categoria" : "Nuova categoria"}
      footer={
        <>
          {category && (
            <Button variant="destructive" className="sm:mr-auto" onClick={() => onDelete(category)} disabled={busy}>
              <Trash2 /> Elimina
            </Button>
          )}
          <Button variant="outline" onClick={onClose} disabled={busy}>
            Annulla
          </Button>
          <Button onClick={save} disabled={busy || !name.trim()}>
            {busy && <Loader2 className="animate-spin" />}
            {category ? "Salva" : "Crea"}
          </Button>
        </>
      }
    >
      <div className="flex flex-col gap-5 pb-2">
        <div className="flex items-center gap-3">
          <CategoryIcon category={preview} className="size-14 rounded-2xl text-2xl" />
          <div className="flex flex-1 flex-col gap-1.5">
            <Label htmlFor="cat-name">Nome</Label>
            <Input id="cat-name" value={name} onChange={(e) => setName(e.target.value)} maxLength={40} placeholder="Es. Bicicletta" autoFocus={!category} onKeyDown={(e) => e.key === "Enter" && save()} />
          </div>
        </div>

        <div className="flex flex-col gap-1.5">
          <Label>Tipo</Label>
          <Select value={kind} onValueChange={(v) => setKind(v as CategoryKind)}>
            <SelectTrigger className="w-full">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {KINDS.map((k) => (
                <SelectItem key={k} value={k}>
                  {KIND_LABEL[k]}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
          <span className="text-xs text-muted-foreground">{KIND_HINT[kind]}</span>
        </div>

        <div className="flex flex-col gap-2">
          <Label htmlFor="cat-icon">Icona</Label>
          <div className="grid grid-cols-10 gap-1">
            {EMOJI.map((e) => (
              <button
                key={e}
                type="button"
                onClick={() => setIcon(e)}
                aria-label={`Icona ${e}`}
                aria-pressed={icon === e}
                className={cn("flex aspect-square items-center justify-center rounded-md text-lg transition-colors hover:bg-muted", icon === e && "bg-muted ring-2 ring-primary")}
              >
                {e}
              </button>
            ))}
          </div>
          <div className="flex items-center gap-2">
            <Input id="cat-icon" value={icon} onChange={(e) => setIcon(e.target.value)} maxLength={8} placeholder="…o scrivi qualsiasi emoji" className="max-w-56" />
            {icon && (
              <Button variant="ghost" size="sm" onClick={() => setIcon("")}>
                Nessuna
              </Button>
            )}
          </div>
        </div>

        <div className="flex flex-col gap-2">
          <Label>Colore</Label>
          <div className="flex flex-wrap gap-2" role="radiogroup" aria-label="Colore">
            <button
              type="button"
              role="radio"
              aria-checked={!color}
              onClick={() => setColor("")}
              title="Automatico"
              className={cn("flex size-8 items-center justify-center rounded-full border text-muted-foreground", !color && "ring-2 ring-primary ring-offset-2 ring-offset-background")}
            >
              <Wand2 className="size-4" />
            </button>
            {COLORS.map((c) => (
              <button
                key={c}
                type="button"
                role="radio"
                aria-checked={color === c}
                aria-label={COLOR_LABEL[c]}
                title={COLOR_LABEL[c]}
                onClick={() => setColor(c)}
                className={cn("size-8 rounded-full", SWATCH[c], color === c && "ring-2 ring-primary ring-offset-2 ring-offset-background")}
              />
            ))}
          </div>
          {!color && <span className="text-xs text-muted-foreground">Automatico: {COLOR_LABEL[colorOf(preview)].toLowerCase()}, ricavato dal nome.</span>}
        </div>

        {category && (
          <>
            <Separator />
            <section className="flex flex-col gap-3">
              <div className="flex items-center justify-between">
                <h3 className="text-sm font-semibold">Regole automatiche</h3>
                <Button variant="ghost" size="sm" onClick={() => navigate(href("/movimenti", { category: category.name }))}>
                  {integer(category.transactions)} movimenti <ArrowRight />
                </Button>
              </div>
              {rules.length ? (
                <ul className="flex flex-col divide-y rounded-lg border">
                  {rules.map((r) => (
                    <li key={r.id} className="flex items-center gap-2 px-3 py-2 text-sm">
                      <code className="min-w-0 flex-1 truncate font-mono text-xs">{r.pattern}</code>
                      <Badge variant="outline" className="font-normal">
                        {r.source === "ai" && <Sparkles />}
                        {SOURCE[r.source] ?? r.source}
                      </Badge>
                      <span className="tabular w-14 text-right text-xs text-muted-foreground">{plural(r.hits, "uso", "usi")}</span>
                      <Button variant="ghost" size="icon-sm" onClick={() => removeRule(r)} aria-label={`Elimina la regola ${r.pattern}`}>
                        <Trash2 />
                      </Button>
                    </li>
                  ))}
                </ul>
              ) : (
                <p className="text-sm text-muted-foreground">Nessuna regola. Si creano quando categorizzi un movimento scegliendo «usa anche per i prossimi», o dall'AI.</p>
              )}
            </section>
          </>
        )}
      </div>
    </ResponsiveDialog>
  );
}

function DeleteCategoryDialog({ category, onClose }: { category: CategoryView | null; onClose: () => void }) {
  const { refresh } = useApp();
  const toast = useToast();
  const [mode, setMode] = useState<"move" | "none">("move");
  const [target, setTarget] = useState("");
  const [busy, setBusy] = useState(false);
  const [shown, setShown] = useState<CategoryView | null>(category);

  useEffect(() => {
    if (!category) return;
    setShown(category);
    setMode(category.transactions > 0 ? "move" : "none");
    setTarget("");
  }, [category]);

  const c = category ?? shown;
  if (!c) return null;
  const blocked = mode === "move" && (!target || target.toLowerCase() === c.name.toLowerCase());

  const run = async () => {
    setBusy(true);
    try {
      const r = await api.deleteCategory(c.name, mode === "move" ? target : undefined);
      refresh();
      toast.success(
        r.moved_to
          ? `«${c.name}» unita a «${r.moved_to}»: ${plural(r.transactions, "movimento spostato", "movimenti spostati")}, con le sue regole.`
          : `«${c.name}» eliminata${r.transactions ? `: ${plural(r.transactions, "movimento torna", "movimenti tornano")} da categorizzare` : ""}.`,
      );
      onClose();
    } catch (e) {
      toast.error(errorMessage(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <ResponsiveDialog
      open={!!category}
      onClose={onClose}
      title={`Eliminare «${c.name}»?`}
      description={`Ha ${plural(c.transactions, "movimento", "movimenti")} e ${plural(c.rules, "regola", "regole")}.`}
      footer={
        <>
          <Button variant="outline" onClick={onClose} disabled={busy}>
            Annulla
          </Button>
          <Button variant="destructive" onClick={run} disabled={busy || blocked}>
            {busy && <Loader2 className="animate-spin" />}
            {mode === "move" ? "Unisci ed elimina" : "Elimina"}
          </Button>
        </>
      }
    >
      <RadioGroup value={mode} onValueChange={(v) => setMode(v as "move" | "none")} className="gap-3 pb-2">
        <Label htmlFor="del-move" className="flex items-start gap-3 rounded-lg border p-3 font-normal has-[[data-state=checked]]:border-primary">
          <RadioGroupItem id="del-move" value="move" className="mt-0.5" />
          <span className="flex flex-1 flex-col gap-2">
            <span className="font-medium">Sposta tutto in un'altra categoria</span>
            <span className="text-xs text-muted-foreground">Movimenti, regole, budget e spese previste passano lì: è un'unione.</span>
            {mode === "move" && <CategoryPicker value={target} onChange={setTarget} placeholder="Scegli dove spostarli" ariaLabel="Categoria di destinazione" />}
          </span>
        </Label>
        <Label htmlFor="del-none" className="flex items-start gap-3 rounded-lg border p-3 font-normal has-[[data-state=checked]]:border-primary">
          <RadioGroupItem id="del-none" value="none" className="mt-0.5" />
          <span className="flex flex-col gap-1">
            <span className="font-medium">Lasciali da categorizzare</span>
            <span className="text-xs text-muted-foreground">Regole e budget della categoria vengono eliminati; l'AI riproverà a categorizzare i movimenti.</span>
          </span>
        </Label>
      </RadioGroup>
    </ResponsiveDialog>
  );
}
