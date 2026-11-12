// Categorize or tag many transactions at once.
import { useEffect, useState } from "react";
import { Loader2 } from "lucide-react";
import { Button } from "@/components/ui/button";
import { CategoryPicker } from "@/components/CategoryPicker";
import { ResponsiveDialog } from "@/components/ResponsiveDialog";
import { TagAdd, TagChips } from "@/components/TagInput";
import { api, errorMessage } from "@/lib/api";
import { useApp } from "@/lib/app-state";
import { integer } from "@/lib/format";
import { useToast } from "@/lib/toast";

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
  const { refresh } = useApp();
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
    <ResponsiveDialog
      open={mode !== null}
      onClose={onClose}
      title={mode === "tag" ? `Aggiungi tag a ${n} movimenti` : `Categoria per ${n} movimenti`}
      description={mode === "categorize" ? "Non crea regole: per quelle apri il singolo movimento." : "I tag si aggiungono a quelli già presenti."}
      footer={
        <>
          <Button variant="outline" onClick={onClose} disabled={busy}>
            Annulla
          </Button>
          <Button onClick={apply} disabled={busy || (mode === "categorize" ? !cat : tags.length === 0)}>
            {busy && <Loader2 className="animate-spin" />}
            Applica a {n}
          </Button>
        </>
      }
    >
      {mode === "categorize" ? (
        <CategoryPicker value={cat} onChange={setCat} ariaLabel="Categoria" />
      ) : (
        <div className="flex flex-col gap-3">
          <TagChips tags={tags} onRemove={(t) => setTags((x) => x.filter((y) => y !== t))} />
          <TagAdd suggestions={tagSuggestions.filter((s) => !tags.includes(s))} onAdd={(t) => setTags((x) => (x.includes(t) ? x : [...x, t]))} autoFocus />
        </div>
      )}
    </ResponsiveDialog>
  );
}
