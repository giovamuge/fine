// Tag chips with an add field (native datalist autocomplete: keyboard-accessible for free).
import { useId, useState, type KeyboardEvent } from "react";
import { Icon } from "./Icon";

export function TagChips({ tags, onRemove, small }: { tags: string[]; onRemove?: (tag: string) => void; small?: boolean }) {
  if (!tags.length) return null;
  return (
    <span className="tags" style={{ display: "inline-flex", gap: 4, flexWrap: "wrap" }}>
      {tags.map((t) => (
        <span key={t} className="chip tag" style={small ? { padding: "3px 7px", fontSize: 11 } : undefined}>
          {t}
          {onRemove && (
            <button type="button" onClick={() => onRemove(t)} aria-label={`Rimuovi tag ${t}`} title="Rimuovi">
              <Icon name="x" size={12} />
            </button>
          )}
        </span>
      ))}
    </span>
  );
}

export function TagAdd({
  suggestions,
  onAdd,
  disabled,
  placeholder = "Aggiungi tag…",
  autoFocus,
}: {
  suggestions: string[];
  onAdd: (tag: string) => void;
  disabled?: boolean;
  placeholder?: string;
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
    <div className="inline">
      <input
        className="input"
        list={listId}
        value={value}
        onChange={(e) => setValue(e.target.value)}
        onKeyDown={onKey}
        placeholder={placeholder}
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
      <button type="button" className="btn" onClick={submit} disabled={disabled || !value.trim()}>
        <Icon name="plus" size={16} /> Aggiungi
      </button>
    </div>
  );
}
