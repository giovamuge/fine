// Categorize or tag many transactions at once.
import { useEffect, useState } from "react";
import { api, errorMessage } from "../lib/api";
import { useApp } from "../lib/app-state";
import { useToast } from "../lib/toast";
import { integer } from "../lib/format";
import { Modal } from "./Modal";
import { TagAdd, TagChips } from "./TagInput";
import { Icon } from "./Icon";

export function BulkDialog({
  mode,
  ids,
  onClose,
  onDone,
  tagSuggestions,
}: {
  mode: "categorize" | "tag" | null;
  ids: number[];
  onClose: () => void;
  onDone: () => void;
  tagSuggestions: string[];
}) {
  const { categories, refresh } = useApp();
  const toast = useToast();
  const [cat, setCat] = useState("");
  const [tags, setTags] = useState<string[]>([]);
  const [busy, setBusy] = useState(false);
  useEffect(() => {
    setCat("");
    setTags([]);
  }, [mode]);

  const n = integer(ids.length);
  const apply = async () => {
    setBusy(true);
    try {
      if (mode === "categorize") {
        const r = await api.categorize(ids, cat, false);
        toast.success(`«${cat}» assegnata a ${integer(r.updated)} movimenti.`);
      } else {
        const r = await api.tag(ids, tags, false);
        toast.success(`${tags.map((t) => `#${t}`).join(", ")} su ${n} movimenti (${integer(r.tagged)} nuovi).`);
      }
      refresh();
      onDone();
    } catch (e) {
      toast.error(errorMessage(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <Modal open={mode !== null} onClose={onClose} title={mode === "tag" ? `Aggiungi tag a ${n} movimenti` : `Categoria per ${n} movimenti`}>
      {mode === "categorize" ? (
        <div className="field">
          <label htmlFor="bulk-cat">Categoria</label>
          <select id="bulk-cat" className="select" value={cat} onChange={(e) => setCat(e.target.value)}>
            <option value="">Scegli…</option>
            {categories.map((c) => (
              <option key={c.name} value={c.name}>{c.name}</option>
            ))}
          </select>
          <span className="hint">Non crea regole: per quelle apri il singolo movimento.</span>
        </div>
      ) : (
        <div className="stack">
          <TagChips tags={tags} onRemove={(t) => setTags((x) => x.filter((y) => y !== t))} />
          <TagAdd suggestions={tagSuggestions.filter((s) => !tags.includes(s))} onAdd={(t) => setTags((x) => (x.includes(t) ? x : [...x, t]))} autoFocus />
        </div>
      )}
      <div className="actions">
        <button type="button" className="btn" onClick={onClose} disabled={busy}>
          Annulla
        </button>
        <button type="button" className="btn primary" onClick={apply} disabled={busy || (mode === "categorize" ? !cat : tags.length === 0)}>
          {busy && <Icon name="sync" size={16} className="spin" />}
          Applica a {n}
        </button>
      </div>
    </Modal>
  );
}
