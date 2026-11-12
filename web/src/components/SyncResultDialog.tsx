import type { SyncState } from "../lib/app-state";
import { integer } from "../lib/format";
import { Modal } from "./Modal";
import { Icon } from "./Icon";

export function SyncResultDialog({ state, onClose }: { state: SyncState; onClose: () => void }) {
  const open = !state.running && (state.result !== null || state.error !== null);
  const r = state.result;
  return (
    <Modal open={open} onClose={onClose} title="Esito della sincronizzazione" initialFocus=".actions .btn">
      {state.error && (
        <div className="sync-result">
          <div className="r bad" role="alert">
            <Icon name="alert" size={18} />
            <span>{state.error}</span>
          </div>
        </div>
      )}
      {r && (
        <div className="sync-result">
          {r.sync.accounts.map((a) => (
            <div key={a.account} className={`r ${a.error ? "bad" : "ok"}`}>
              <Icon name={a.error ? "alert" : "check"} size={18} />
              <span>
                <strong>{a.account}</strong>
                <br />
                {a.error ? a.error : a.skipped ? `Saltato: ${a.skipped}` : a.inserted ? `${integer(a.inserted)} nuovi movimenti` : "Nessun nuovo movimento"}
              </span>
            </div>
          ))}
          {r.sync.accounts.length === 0 && <p className="muted">Nessun conto da sincronizzare.</p>}
          {(r.sync.auto_categorized > 0 || r.sync.auto_tagged > 0) && (
            <p className="small muted">
              Regole applicate: {integer(r.sync.auto_categorized)} categorizzati, {integer(r.sync.auto_tagged)} taggati.
            </p>
          )}
          {r.ai && (
            <p className="small muted">
              AI ({r.ai.backend}): {integer(r.ai.categorized)} su {integer(r.ai.examined)} categorizzati
              {r.ai.rules_created.length ? `, ${integer(r.ai.rules_created.length)} nuove regole` : ""}.
            </p>
          )}
          {state.result?.ai_error && (
            <p className="small neg-text">Categorizzazione AI non riuscita: {state.result.ai_error}</p>
          )}
        </div>
      )}
      <div className="actions">
        <button type="button" className="btn primary" onClick={onClose}>
          Ok
        </button>
      </div>
    </Modal>
  );
}
