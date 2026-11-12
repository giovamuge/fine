// Searchable category picker, grouped by kind, with "create «text»" when nothing matches.
import { useState, type ReactNode } from "react";
import { Check, ChevronsUpDown, Plus } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Command, CommandEmpty, CommandGroup, CommandInput, CommandItem, CommandList, CommandSeparator } from "@/components/ui/command";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
import { useApp } from "@/lib/app-state";
import { api, errorMessage } from "@/lib/api";
import { CategoryIcon, KINDS, KIND_LABEL } from "@/lib/categories";
import { useToast } from "@/lib/toast";
import { cn } from "@/lib/utils";

export function CategoryPicker({
  value,
  onChange,
  allowNone,
  noneLabel = "Da categorizzare",
  placeholder = "Scegli una categoria",
  trigger,
  className,
  disabled,
  ariaLabel,
}: {
  value: string;
  onChange: (name: string) => void;
  allowNone?: boolean;
  noneLabel?: string;
  placeholder?: string;
  /** Custom trigger (e.g. a compact chip); defaults to a full-width button. */
  trigger?: ReactNode;
  className?: string;
  disabled?: boolean;
  ariaLabel?: string;
}) {
  const { categories, categoryByName, refresh } = useApp();
  const toast = useToast();
  const [open, setOpen] = useState(false);
  const [search, setSearch] = useState("");
  const current = value ? categoryByName.get(value.toLowerCase()) : undefined;
  const typed = search.trim();
  const canCreate = typed.length > 1 && !categoryByName.has(typed.toLowerCase());

  const pick = (name: string) => {
    onChange(name);
    setOpen(false);
    setSearch("");
  };

  const create = async () => {
    try {
      await api.createCategory(typed, "expense", "", "");
      refresh();
      toast.success(`Categoria «${typed}» creata: personalizzala dalla sezione Categorie.`);
      pick(typed);
    } catch (e) {
      toast.error(errorMessage(e));
    }
  };

  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger asChild disabled={disabled}>
        {trigger ?? (
          <Button variant="outline" role="combobox" aria-expanded={open} aria-label={ariaLabel} className={cn("h-9 w-full justify-between font-normal", className)}>
            <span className="flex min-w-0 items-center gap-2">
              {current && <CategoryIcon category={current} className="size-5 rounded text-xs" />}
              <span className={cn("truncate", !value && "text-muted-foreground")}>{value || (allowNone ? noneLabel : placeholder)}</span>
            </span>
            <ChevronsUpDown className="opacity-50" />
          </Button>
        )}
      </PopoverTrigger>
      <PopoverContent className="w-[min(320px,calc(100vw-2rem))] p-0" align="start" onClick={(e) => e.stopPropagation()}>
        <Command>
          <CommandInput placeholder="Cerca o crea…" value={search} onValueChange={setSearch} />
          <CommandList className="max-h-72">
            <CommandEmpty>Nessuna categoria.</CommandEmpty>
            {allowNone && (
              <CommandGroup>
                <CommandItem value={`__none ${noneLabel}`} onSelect={() => pick("")}>
                  <span className="inline-flex size-5 items-center justify-center rounded bg-muted text-xs">–</span>
                  {noneLabel}
                  {!value && <Check className="ml-auto" />}
                </CommandItem>
              </CommandGroup>
            )}
            {KINDS.map((k) => {
              const list = categories.filter((c) => c.kind === k);
              if (!list.length) return null;
              return (
                <CommandGroup key={k} heading={KIND_LABEL[k]}>
                  {list.map((c) => (
                    <CommandItem key={c.name} value={c.name} onSelect={() => pick(c.name)}>
                      <CategoryIcon category={c} className="size-5 rounded text-xs" />
                      <span className="truncate">{c.name}</span>
                      {current?.name === c.name && <Check className="ml-auto" />}
                    </CommandItem>
                  ))}
                </CommandGroup>
              );
            })}
            {canCreate && (
              <>
                <CommandSeparator />
                <CommandGroup>
                  <CommandItem value={`__create ${typed}`} onSelect={create}>
                    <Plus /> Crea «{typed}»
                  </CommandItem>
                </CommandGroup>
              </>
            )}
          </CommandList>
        </Command>
      </PopoverContent>
    </Popover>
  );
}

/** A compact, read-only category label. */
export function CategoryLabel({ name, className }: { name: string | null; className?: string }) {
  const { categoryByName } = useApp();
  const c = name ? categoryByName.get(name.toLowerCase()) : undefined;
  return (
    <span className={cn("inline-flex min-w-0 items-center gap-1.5", className)}>
      {c?.icon && <span aria-hidden="true">{c.icon}</span>}
      <span className={cn("truncate", !name && "font-medium text-warning")}>{name ?? "Da categorizzare"}</span>
    </span>
  );
}
