// Transaction detail: category (with the explicit "also for future movements of this
// merchant" choice), tags with autocomplete and optional rule, amortization.
import { useEffect, useState } from "react";
import type { TxView } from "../lib/types";
import { api, errorMessage } from "../lib/api";
import { useApp } from "../lib/app-state";
import { useToast } from "../lib/toast";
import { euro, fmtDate, integer } from "../lib/format";
import { Modal } from "./Modal";
import { TagAdd, TagChips } from "./TagInput";
import { Icon } from "./Icon";

const BY: Record<string, string> = { manual: "a mano", rule: "da una regola", ai: "dall'AI", learned: "da una regola" };

export function TxDetail({ tx, onClose, tagSuggestions }: { tx: TxView | null; onClose: () => void; tagSuggestions: string[] }) {
  const { categories, refresh } = useApp();
  const toast = useToast();
  const [cat, setCat] = useState("");
  const [learn, setLearn] = useState(false);
  const [tagRule, setTagRule] = useState(false);
  const [months, setMonths] = useState(0);
  const [busy, setBusy] = useState<string | null>(null);

  useEffect(() => {
    if (!tx) return;
    setCat(tx.category ?? "");
    setLearn(false);
    setTagRule(false);
    setMonths(tx.amortized_months ?? 0);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [tx?.id]);

  if (!tx) return <Modal open={false} onClose={onClose} title="">{null}</Modal>;
  const t = tx;
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
      if (r.rule_pattern) {
        return `«${cat}» assegnata. Regola «${r.rule_pattern}» creata${r.auto_categorized ? `: ${integer(r.auto_categorized)} altri movimenti sistemati` : ""}.`;
      }
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

  return (
    <Modal open={!!tx} onClose={onClose} title={title} initialFocus=".close">
      <div className={`detail-amount${t.amount > 0 ? " pos" : ""}`}>{t.amount > 0 ? `+${euro(t.amount)}` : euro(t.amount)}</div>
      <dl className="detail-grid">
        <dt>Data</dt>
        <dd>{fmtDate(t.date)}</dd>
        {t.counterparty && t.counterparty !== t.merchant && (
          <>
            <dt>Controparte</dt>
            <dd>{t.counterparty}</dd>
          </>
        )}
        {t.description && (
          <>
            <dt>Descrizione</dt>
            <dd>{t.description}</dd>
          </>
        )}
        <dt>Conto</dt>
        <dd>{t.account}</dd>
        {t.category && (
          <>
            <dt>Categoria</dt>
            <dd>
              {t.category}
              {t.categorized_by && <span className="muted"> · assegnata {BY[t.categorized_by] ?? t.categorized_by}</span>}
            </dd>
          </>
        )}
      </dl>

      <div className="section">
        <h3>Categoria</h3>
        <select className="select" value={cat} onChange={(e) => setCat(e.target.value)} aria-label="Categoria">
          <option value="">Da categorizzare</option>
          {categories.map((c) => (
            <option key={c.name} value={c.name}>{c.name}</option>
          ))}
        </select>
        {canRule && (
          <label className="check">
            <input type="checkbox" checked={learn} onChange={(e) => setLearn(e.target.checked)} />
            <span>
              <span className="t">Usa questa categoria anche per i prossimi movimenti di «{t.merchant}»</span>
              <br />
              <span className="d">Crea una regola: vale per i movimenti futuri e per quelli passati ancora da categorizzare.</span>
            </span>
          </label>
        )}
        <div className="actions" style={{ justifyContent: "flex-start" }}>
          <button type="button" className="btn primary" disabled={!cat || (!catChanged && !learn) || busy !== null} onClick={applyCategory}>
            {busy === "cat" && <Icon name="sync" size={16} className="spin" />}
            {learn ? "Applica e crea regola" : "Applica"}
          </button>
        </div>
      </div>

      <div className="section">
        <h3>Tag</h3>
        {t.tags.length ? <TagChips tags={t.tags} onRemove={removeTag} /> : <p className="small muted">Nessun tag. I tag sono liberi (#vacanza-londra, #lavoro, #rimborsabile…) e indipendenti dalla categoria.</p>}
        <TagAdd suggestions={tagSuggestions.filter((s) => !t.tags.includes(s))} onAdd={addTag} disabled={busy !== null} />
        {canRule && (
          <label className="check">
            <input type="checkbox" checked={tagRule} onChange={(e) => setTagRule(e.target.checked)} />
            <span>
              <span className="t">Tagga automaticamente anche i prossimi movimenti di «{t.merchant}»</span>
              <br />
              <span className="d">Vale per i tag che aggiungi da ora.</span>
            </span>
          </label>
        )}
      </div>

      {t.amount < 0 && (
        <div className="section">
          <h3>Ammortamento</h3>
          <p className="small muted">Una spesa grande può pesare 1/N al mese sul budget invece che tutta nel mese in cui l'hai fatta.</p>
          <div className="inline">
            <select className="select" value={months} onChange={(e) => setMonths(Number(e.target.value))} aria-label="Mesi di ammortamento">
              <option value={0}>Nessuno</option>
              {[3, 6, 12, 24, 36].map((m) => (
                <option key={m} value={m}>{m} mesi ({euro(-t.amount / m)} al mese)</option>
              ))}
            </select>
            <button type="button" className="btn" disabled={months === (t.amortized_months ?? 0) || busy !== null} onClick={applyAmortize}>
              {busy === "amort" && <Icon name="sync" size={16} className="spin" />}
              Applica
            </button>
          </div>
        </div>
      )}
    </Modal>
  );
}
