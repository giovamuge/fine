import { useState } from "react";
import { api, errorMessage } from "../lib/api";
import { useApp } from "../lib/app-state";
import { useAsync } from "../lib/useAsync";
import { useToast } from "../lib/toast";
import { daysUntil, fmtDate, fmtDateTime, integer, money, plural, relativeTime } from "../lib/format";
import { Empty, Loaded } from "../components/States";
import { Icon } from "../components/Icon";
import { ConfirmDialog, Modal } from "../components/Modal";
import { ConnectDialog, type ConnectPreset } from "../components/ConnectDialog";
import { SyncResultDialog } from "../components/SyncResultDialog";
import type { AccountView } from "../lib/types";

export function Conti() {
  const { version, refresh, sync, runSync, clearSyncResult } = useApp();
  const toast = useToast();
  const accounts = useAsync(() => api.accounts(), [version]);
  const [connect, setConnect] = useState<ConnectPreset | null>(null);
  const [rename, setRename] = useState<AccountView | null>(null);
  const [remove, setRemove] = useState<AccountView | null>(null);
  const [busy, setBusy] = useState(false);
  const [label, setLabel] = useState("");

  const doRename = async () => {
    if (!rename) return;
    setBusy(true);
    try {
      await api.renameAccount(rename.uid, label.trim() || null);
      toast.success(label.trim() ? `Conto rinominato in «${label.trim()}».` : "Nome automatico ripristinato.");
      setRename(null);
      refresh();
    } catch (e) {
      toast.error(errorMessage(e));
    } finally {
      setBusy(false);
    }
  };

  const doRemove = async () => {
    if (!remove) return;
    setBusy(true);
    try {
      const r = await api.removeAccount(remove.uid);
      toast.success(`Conto rimosso: ${integer(r.transactions_deleted)} movimenti cancellati${r.consent_revoked ? ", consenso revocato presso la banca" : ""}.`);
      setRemove(null);
      refresh();
    } catch (e) {
      toast.error(errorMessage(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <>
      <header className="page-head">
        <div>
          <h1>Conti</h1>
          <p className="sub">Banche collegate tramite Enable Banking (PSD2, sola lettura)</p>
        </div>
        <div className="inline">
          <button type="button" className="btn" onClick={runSync} disabled={sync.running || !accounts.data?.length} title="Scarica i nuovi movimenti (max 4 volte al giorno per conto)">
            <Icon name="sync" size={16} className={sync.running ? "spin" : undefined} />
            {sync.running ? "Sincronizzo…" : "Sincronizza ora"}
          </button>
          <button type="button" className="btn primary" onClick={() => setConnect({})}>
            <Icon name="plus" size={16} /> Collega una banca
          </button>
        </div>
      </header>

      <Loaded {...accounts} rows={6}>
        {(list) =>
          list.length ? (
            <div className="acct-grid">
              {list.map((a) => (
                <AccountCard
                  key={a.uid}
                  a={a}
                  onRenew={() => setConnect({ bank: a.bank ?? undefined, country: a.country ?? "IT", renew: a.label })}
                  onRename={() => {
                    setLabel(a.custom_label ?? "");
                    setRename(a);
                  }}
                  onRemove={() => setRemove(a)}
                />
              ))}
            </div>
          ) : (
            <section className="card">
              <Empty title="Nessun conto collegato.">
                Collega la tua banca: scegli dall'elenco, autorizzi sulla pagina della banca e incolli l'indirizzo finale. Serve un'applicazione Enable Banking configurata (vedi README).
              </Empty>
              <div style={{ marginTop: 12 }}>
                <button type="button" className="btn primary" onClick={() => setConnect({})}>
                  <Icon name="plus" size={16} /> Collega una banca
                </button>
              </div>
            </section>
          )
        }
      </Loaded>

      <ConnectDialog preset={connect} onClose={() => setConnect(null)} />

      <Modal open={rename !== null} onClose={() => setRename(null)} title="Rinomina conto">
        <div className="field">
          <label htmlFor="rename-label">Nome</label>
          <input id="rename-label" className="input" value={label} onChange={(e) => setLabel(e.target.value)} placeholder={rename?.label ?? ""} maxLength={60} onKeyDown={(e) => e.key === "Enter" && doRename()} />
          <span className="hint">Lascia vuoto per tornare al nome automatico ({rename?.label}).</span>
        </div>
        <div className="actions">
          <button type="button" className="btn" onClick={() => setRename(null)} disabled={busy}>
            Annulla
          </button>
          <button type="button" className="btn primary" onClick={doRename} disabled={busy}>
            Salva
          </button>
        </div>
      </Modal>

      <ConfirmDialog
        open={remove !== null}
        onClose={() => setRemove(null)}
        onConfirm={doRemove}
        title={`Rimuovere «${remove?.label ?? ""}»?`}
        confirmLabel="Rimuovi definitivamente"
        danger
        busy={busy}
      >
        <p>
          Verranno cancellati da questo computer il conto e i suoi <strong>{plural(remove?.transactions ?? 0, "movimento", "movimenti")}</strong>, con tag e ammortamenti. Il consenso viene revocato presso la banca se non serve ad altri conti.
        </p>
        <p className="neg-text">
          <strong>Non si può annullare.</strong> Lo storico si recupera solo per quanto la banca concede ricollegando il conto.
        </p>
      </ConfirmDialog>

      <SyncResultDialog state={sync} onClose={clearSyncResult} />
    </>
  );
}

function AccountCard({ a, onRenew, onRename, onRemove }: { a: AccountView; onRenew: () => void; onRename: () => void; onRemove: () => void }) {
  const days = a.consent_valid_until ? daysUntil(a.consent_valid_until) : null;
  const consentBad = days !== null && days < 0;
  const consentWarn = days !== null && days >= 0 && days <= 14;
  const failed = a.last_sync_ok === false;
  return (
    <section className={`card acct${failed || consentBad ? "" : ""}`} aria-label={a.label}>
      <div className="top">
        <div style={{ minWidth: 0 }}>
          <div className="name">{a.custom_label ?? a.label}</div>
          <div className="bank">
            {[a.bank, a.country].filter(Boolean).join(" · ") || "Banca"}
            {a.custom_label && a.custom_label !== a.label ? ` · ${a.label}` : ""}
          </div>
        </div>
        <div style={{ textAlign: "right" }}>
          <div className="balance">{a.balance === null ? "—" : money(a.balance, a.balance_currency ?? a.currency)}</div>
          {a.balance_at && <div className="small muted">al {fmtDateTime(a.balance_at)}</div>}
        </div>
      </div>
      {a.iban && <div className="iban">{a.iban}</div>}
      <div className="status">
        {a.last_sync_at === null ? (
          <span className="badge warn"><Icon name="clock" size={13} /> Mai sincronizzato</span>
        ) : failed ? (
          <span className="badge bad"><Icon name="alert" size={13} /> Sincronizzazione fallita {relativeTime(a.last_sync_at)}</span>
        ) : (
          <span className="badge ok"><Icon name="check" size={13} /> Sincronizzato {relativeTime(a.last_sync_at)}</span>
        )}
        {days === null ? (
          <span className="badge neutral">Consenso: —</span>
        ) : consentBad ? (
          <span className="badge bad"><Icon name="alert" size={13} /> Consenso scaduto il {fmtDate(a.consent_valid_until ?? "")}</span>
        ) : consentWarn ? (
          <span className="badge warn"><Icon name="alert" size={13} /> Consenso scade tra {days === 0 ? "oggi" : `${integer(days)} ${days === 1 ? "giorno" : "giorni"}`}</span>
        ) : (
          <span className="badge neutral">Consenso fino al {fmtDate(a.consent_valid_until ?? "")}</span>
        )}
        <span className="badge neutral">{plural(a.transactions, "movimento", "movimenti")}</span>
      </div>
      {failed && a.last_sync_error && (
        <div className="err" role="alert">
          <strong>Errore:</strong> {a.last_sync_error}
        </div>
      )}
      {(consentBad || consentWarn) && !failed && (
        <p className="small" style={{ color: "var(--warn-text)" }}>Rinnova il consenso per continuare a ricevere i movimenti: lo storico resta, senza duplicati.</p>
      )}
      <div className="actions">
        <button type="button" className={`btn sm${failed || consentBad || consentWarn ? " primary" : ""}`} onClick={onRenew} title="Rinnova o ripristina l'autorizzazione della banca: lo storico resta">
          <Icon name="sync" size={14} /> Rinnova consenso
        </button>
        <button type="button" className="btn sm" onClick={onRename}>
          <Icon name="edit" size={14} /> Rinomina
        </button>
        <button type="button" className="btn sm danger" onClick={onRemove}>
          <Icon name="trash" size={14} /> Rimuovi
        </button>
      </div>
    </section>
  );
}
