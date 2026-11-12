// Guided bank connection: 1) choose the bank, 2) authorize at the bank (new tab),
// 3) paste the final address, 4) done. Also used to renew a consent (preset bank).
import { useEffect, useId, useState } from "react";
import { api, errorMessage } from "../lib/api";
import { useApp } from "../lib/app-state";
import { useToast } from "../lib/toast";
import type { ConnectedAccount } from "../lib/types";
import { Modal } from "./Modal";
import { Icon } from "./Icon";

export interface ConnectPreset {
  bank?: string;
  country?: string;
  /** Label of the account being renewed (only for the title). */
  renew?: string;
}

const COUNTRIES = ["IT", "DE", "FR", "ES", "NL", "BE", "AT", "PT", "IE", "LT", "FI", "SE", "DK", "NO", "PL", "GB"];
const NAMES = new Intl.DisplayNames(["it"], { type: "region" });

export function ConnectDialog({ preset, onClose }: { preset: ConnectPreset | null; onClose: () => void }) {
  const { refresh, runSync } = useApp();
  const toast = useToast();
  const listId = useId();
  const [country, setCountry] = useState("IT");
  const [bank, setBank] = useState("");
  const [banks, setBanks] = useState<string[]>([]);
  const [banksError, setBanksError] = useState<string | null>(null);
  const [step, setStep] = useState<1 | 2 | 3 | 4>(1);
  const [pending, setPending] = useState<{ url: string; redirect: string } | null>(null);
  const [finalUrl, setFinalUrl] = useState("");
  const [result, setResult] = useState<ConnectedAccount[]>([]);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const open = preset !== null;
  useEffect(() => {
    if (!preset) return;
    setCountry(preset.country?.toUpperCase() || "IT");
    setBank(preset.bank ?? "");
    setStep(1);
    setPending(null);
    setFinalUrl("");
    setResult([]);
    setError(null);
  }, [preset]);

  useEffect(() => {
    if (!open) return;
    let cancelled = false;
    setBanks([]);
    setBanksError(null);
    api.banks(country).then(
      (b) => !cancelled && setBanks(b),
      (e: unknown) => !cancelled && setBanksError(errorMessage(e)),
    );
    return () => {
      cancelled = true;
    };
  }, [open, country]);

  const start = async () => {
    const b = bank.trim();
    if (!b) {
      setError("Scegli una banca dall'elenco.");
      return;
    }
    setBusy(true);
    setError(null);
    try {
      const p = await api.connect(b, country);
      setPending(p);
      setStep(2);
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  };

  const finish = async () => {
    const u = finalUrl.trim();
    if (!u) {
      setError("Incolla l'indirizzo completo della pagina finale.");
      return;
    }
    setBusy(true);
    setError(null);
    try {
      const list = await api.connectFinish(u);
      setResult(list);
      setStep(4);
      refresh();
      toast.success(list.some((a) => a.renewed) ? "Consenso rinnovato." : "Conto collegato.");
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  };

  const title = preset?.renew ? `Rinnova il consenso di «${preset.renew}»` : "Collega una banca";
  const cls = (n: number) => `step${step === n ? " active" : step > n ? " done" : ""}`;

  return (
    <Modal open={open} onClose={onClose} title={title} initialFocus={step === 1 ? "#cd-bank" : step === 3 ? "#cd-url" : ".close"}>
      {preset?.renew && step === 1 && (
        <p className="small muted">Rinnovare rifà solo l'autorizzazione presso la banca: lo storico dei movimenti resta, senza duplicati.</p>
      )}
      <ol className="steps" style={{ margin: 0, padding: 0, listStyle: "none" }}>
        <li className={cls(1)}>
          <span className="t">Scegli la banca</span>
          <div className="b">
            <div className="inline">
              <div className="field" style={{ flex: "0 0 150px" }}>
                <label htmlFor="cd-country">Paese</label>
                <select id="cd-country" className="select" value={country} onChange={(e) => { setCountry(e.target.value); setBank(""); }}>
                  {COUNTRIES.map((c) => (
                    <option key={c} value={c}>{NAMES.of(c) ?? c}</option>
                  ))}
                </select>
              </div>
              <div className="field" style={{ flex: "1 1 200px" }}>
                <label htmlFor="cd-bank">Banca</label>
                <input id="cd-bank" className="input" list={listId} value={bank} onChange={(e) => setBank(e.target.value)} placeholder={banks.length ? "Cerca (es. Revolut)" : banksError ? "Nome della banca" : "Caricamento elenco…"} autoComplete="off" onKeyDown={(e) => e.key === "Enter" && start()} />
                <datalist id={listId}>
                  {banks.map((b) => (
                    <option key={b} value={b} />
                  ))}
                </datalist>
              </div>
            </div>
            {banksError && (
              <p className="small neg-text" role="alert">
                Elenco banche non disponibile: {banksError}
              </p>
            )}
            <p className="small muted">L'accesso è in sola lettura (AIS) e dura al massimo 180 giorni, a seconda della banca.</p>
          </div>
        </li>
        <li className={cls(2)}>
          <span className="t">Autorizza sulla pagina della banca</span>
          <div className="b">
            {pending && (
              <>
                <p>Si apre in una nuova scheda. Accedi alla banca e consenti la lettura dei conti.</p>
                <a className="btn primary" href={pending.url} target="_blank" rel="noopener noreferrer" onClick={() => setStep(3)}>
                  <Icon name="external" size={16} /> Apri la pagina di {bank}
                </a>
              </>
            )}
          </div>
        </li>
        <li className={cls(3)}>
          <span className="t">Incolla l'indirizzo finale</span>
          <div className="b">
            {pending && (
              <>
                <p>
                  Alla fine la banca ti manda su <code>{pending.redirect}</code>: la pagina può dare errore, è normale. Copia l'indirizzo completo dalla barra del browser e incollalo qui.
                </p>
                <input id="cd-url" type="url" className="input" value={finalUrl} onChange={(e) => setFinalUrl(e.target.value)} placeholder="https://…?code=…&state=…" aria-label="Indirizzo finale" onKeyDown={(e) => e.key === "Enter" && finish()} />
                <div className="inline">
                  <a className="btn sm ghost" href={pending.url} target="_blank" rel="noopener noreferrer">
                    <Icon name="external" size={14} /> Riapri la pagina della banca
                  </a>
                </div>
              </>
            )}
          </div>
        </li>
        <li className={cls(4)}>
          <span className="t">Fatto</span>
          <div className="b">
            {result.length ? (
              <ul style={{ margin: 0, paddingLeft: 18 }}>
                {result.map((a, i) => (
                  <li key={i}>
                    <strong>{a.renewed ? "Rinnovato" : "Collegato"}</strong>: {[a.name, a.iban].filter(Boolean).join(" · ")} ({a.currency})
                  </li>
                ))}
              </ul>
            ) : (
              <p>Nessun conto restituito dalla banca.</p>
            )}
            <p>Ora scarica i movimenti con una sincronizzazione.</p>
          </div>
        </li>
      </ol>
      {error && (
        <p className="small neg-text" role="alert">
          <Icon name="alert" size={14} /> {error}
        </p>
      )}
      <div className="actions">
        {step === 1 && (
          <button type="button" className="btn primary" onClick={start} disabled={busy}>
            {busy && <Icon name="sync" size={16} className="spin" />} Avvia il collegamento
          </button>
        )}
        {step === 2 && (
          <button type="button" className="btn" onClick={() => setStep(3)}>
            Ho già autorizzato
          </button>
        )}
        {step === 3 && (
          <>
            <button type="button" className="btn" onClick={() => setStep(2)} disabled={busy}>
              Indietro
            </button>
            <button type="button" className="btn primary" onClick={finish} disabled={busy}>
              {busy && <Icon name="sync" size={16} className="spin" />} Completa
            </button>
          </>
        )}
        {step === 4 && (
          <>
            <button type="button" className="btn" onClick={onClose}>
              Chiudi
            </button>
            <button type="button" className="btn primary" onClick={() => { onClose(); void runSync(); }}>
              <Icon name="sync" size={16} /> Sincronizza ora
            </button>
          </>
        )}
      </div>
    </Modal>
  );
}
