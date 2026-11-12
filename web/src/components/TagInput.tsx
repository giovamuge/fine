// Tag chips with an add field (native datalist autocomplete: keyboard-accessible for free).
import { useId, useState, type KeyboardEvent } from "react";
import { Hash, Plus, X } from "lucide-react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";

export function TagChips({ tags, onRemove }: { tags: string[]; onRemove?: (tag: string) => void }) {
  if (!tags.length) return null;
  return (
    <span className="inline-flex flex-wrap gap-1">
      {tags.map((t) => (
        <Badge key={t} variant="secondary" className="gap-0.5 font-normal">
          <Hash className="opacity-60" />
          {t}
          {onRemove && (
            <button type="button" className="-mr-1 ml-0.5 rounded-full p-0.5 hover:bg-foreground/10" onClick={() => onRemove(t)} aria-label={`Rimuovi tag ${t}`}>
              <X />
            </button>
          )}
        </Badge>
      ))}
    </span>
  );
}

export function TagAdd({
  suggestions,
  onAdd,
  disabled,
  autoFocus,
}: {
  suggestions: string[];
  onAdd: (tag: string) => void;
  disabled?: boolean;
  autoFocus?: boolean;
}) {
  const [value, setValue] = useState("");
  const listId = useId();
  const submit = () => {
    const v = value.trim().replace(/^#/, "");
    if (!v) return;
    onAdd(v);
    setValue("");
  };
  const onKey = (e: KeyboardEvent<HTMLInputElement>) => {
    if (e.key === "Enter" || e.key === ",") {
      e.preventDefault();
      submit();
    }
  };
  return (
    <div className="flex gap-2">
      <Input
        list={listId}
        value={value}
        onChange={(e) => setValue(e.target.value)}
        onKeyDown={onKey}
        placeholder="Aggiungi tag… (es. vacanza-londra)"
        aria-label="Nuovo tag"
        disabled={disabled}
        autoCapitalize="none"
        autoCorrect="off"
        autoFocus={autoFocus}
      />
      <datalist id={listId}>
        {suggestions.map((s) => (
          <option key={s} value={s} />
        ))}
      </datalist>
      <Button type="button" variant="outline" onClick={submit} disabled={disabled || !value.trim()}>
        <Plus /> Aggiungi
      </Button>
    </div>
  );
}
