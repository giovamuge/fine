// Tag chips with an add field (native datalist autocomplete: keyboard-accessible for free).
import { useId, useState, type KeyboardEvent } from "react";
import { Hash, Plus, X } from "lucide-react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { t } from "@/lib/i18n";

export function TagChips({ tags, onRemove }: { tags: string[]; onRemove?: (tag: string) => void }) {
  if (!tags.length) return null;
  return (
    <span className="inline-flex flex-wrap gap-1">
      {tags.map((tag) => (
        <Badge key={tag} variant="secondary" className="gap-0.5 font-normal">
          <Hash className="opacity-60" />
          {tag}
          {onRemove && (
            <button type="button" className="-mr-1 ml-0.5 rounded-full p-0.5 hover:bg-foreground/10" onClick={() => onRemove(tag)} aria-label={t("Rimuovi tag {tag}", { tag })}>
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
        placeholder={t("Aggiungi tag… (es. vacanza-londra)")}
        aria-label={t("Nuovo tag")}
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
        <Plus /> {t("Aggiungi")}
      </Button>
    </div>
  );
}
