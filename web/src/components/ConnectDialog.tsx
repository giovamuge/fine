// Guided bank connection: 1) choose the bank, 2) authorize at the bank (new tab),
// 3) paste the final address, 4) done. Also used to renew a consent (preset bank).
import { useEffect, useState, type ReactNode } from "react";
import { AlertTriangle, Check, ChevronsUpDown, ExternalLink, Loader2, RefreshCw } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Command, CommandEmpty, CommandGroup, CommandInput, CommandItem, CommandList } from "@/components/ui/command";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { ResponsiveDialog } from "@/components/ResponsiveDialog";
import { api, errorMessage } from "@/lib/api";
import { useApp } from "@/lib/app-state";
import { useToast } from "@/lib/toast";
import type { ConnectedAccount } from "@/lib/types";
import { cn } from "@/lib/utils";

export interface ConnectPreset {
  bank?: string;
  country?: string;
  /** Label of the account being renewed (only for the title). */
  renew?: string;
}

const COUNTRIES = ["IT", "DE", "FR", "ES", "NL", "BE", "AT", "PT", "IE", "LT", "FI", "SE", "DK", "NO", "PL", "GB"];
const NAMES = new Intl.DisplayNames(["it"], { type: "region" });

function Step({ n, step, title, children }: { n: number; step: number; title: string; children: ReactNode }) {
  const active = step === n;
  const done = step > n;
  return (
    <li className="relative flex gap-3 pb-5 last:pb-0">
      <span
        className={cn(
          "z-10 flex size-7 shrink-0 items-center justify-center rounded-full border text-xs font-semibold",
          active && "border-primary bg-primary text-primary-foreground",
          done && "border-positive bg-positive text-white",
          !active && !done && "bg-background text-muted-foreground",
        )}
      >
        {done ? <Check className="size-4" /> : n}
      </span>
      <span className="absolute top-7 bottom-0 left-3.5 w-px bg-border" aria-hidden="true" />
      <div className="flex min-w-0 flex-1 flex-col gap-2 pt-1">
        <span className={cn("text-sm font-medium", !active && "text-muted-foreground")}>{title}</span>
        {active && <div className="flex flex-col gap-3 text-sm">{children}</div>}
      </div>
    </li>
  );
}

export function ConnectDialog({ preset, onClose }: { preset: ConnectPreset | null; onClose: () => void }) {
  const { refresh, runSync } = useApp();
  const toast = useToast();
  const [country, setCountry] = useState("IT");
  const [bank, setBank] = useState("");
  const [banks, setBanks] = useState<string[]>([]);
  const [banksError, setBanksError] = useState<string | null>(null);
  const [bankOpen, setBankOpen] = useState(false);
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
    if (!b) return setError("Scegli una banca dall'elenco.");
    setBusy(true);
    setError(null);
    try {
      setPending(await api.connect(b, country));
      setStep(2);
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  };

  const finish = async () => {
    const u = finalUrl.trim();
    if (!u) return setError("Incolla l'indirizzo completo della pagina finale.");
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

  const footer =
    step === 1 ? (
      <Button onClick={start} disabled={busy || !bank}>
        {busy && <Loader2 className="animate-spin" />} Avvia il collegamento
      </Button>
    ) : step === 2 ? (
      <Button variant="outline" onClick={() => setStep(3)}>
        Ho già autorizzato
      </Button>
    ) : step === 3 ? (
      <>
        <Button variant="outline" onClick={() => setStep(2)} disabled={busy}>
          Indietro
        </Button>
        <Button onClick={finish} disabled={busy}>
          {busy && <Loader2 className="animate-spin" />} Completa
        </Button>
      </>
    ) : (
      <>
        <Button variant="outline" onClick={onClose}>
          Chiudi
        </Button>
        <Button
          onClick={() => {
            onClose();
            void runSync();
          }}
        >
          <RefreshCw /> Sincronizza ora
        </Button>
      </>
    );

  return (
    <ResponsiveDialog
      open={open}
      onClose={onClose}
      title={preset?.renew ? `Rinnova il consenso di «${preset.renew}»` : "Collega una banca"}
      description={preset?.renew ? "Rinnovare rifà solo l'autorizzazione presso la banca: lo storico resta, senza duplicati." : "Accesso in sola lettura (PSD2), per al massimo 180 giorni."}
      footer={footer}
    >
      <ol className="flex flex-col">
        <Step n={1} step={step} title="Scegli la banca">
          <div className="grid gap-3 sm:grid-cols-[150px_1fr]">
            <div className="flex flex-col gap-1.5">
              <Label>Paese</Label>
              <Select
                value={country}
                onValueChange={(v) => {
                  setCountry(v);
                  setBank("");
                }}
              >
                <SelectTrigger className="w-full">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  {COUNTRIES.map((c) => (
                    <SelectItem key={c} value={c}>
                      {NAMES.of(c) ?? c}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </div>
            <div className="flex flex-col gap-1.5">
              <Label>Banca</Label>
              <Popover open={bankOpen} onOpenChange={setBankOpen}>
                <PopoverTrigger asChild>
                  <Button variant="outline" role="combobox" className="w-full justify-between font-normal">
                    <span className={cn("truncate", !bank && "text-muted-foreground")}>{bank || (banks.length ? "Cerca la tua banca" : banksError ? "Elenco non disponibile" : "Caricamento…")}</span>
                    <ChevronsUpDown className="opacity-50" />
                  </Button>
                </PopoverTrigger>
                <PopoverContent className="w-[min(320px,calc(100vw-2rem))] p-0" align="start">
                  <Command>
                    <CommandInput placeholder="Es. Fineco, Revolut…" />
                    <CommandList className="max-h-64">
                      <CommandEmpty>Nessuna banca trovata.</CommandEmpty>
                      <CommandGroup>
                        {banks.map((b) => (
                          <CommandItem
                            key={b}
                            value={b}
                            onSelect={() => {
                              setBank(b);
                              setBankOpen(false);
                            }}
                          >
                            {b}
                            {bank === b && <Check className="ml-auto" />}
                          </CommandItem>
                        ))}
                      </CommandGroup>
                    </CommandList>
                  </Command>
                </PopoverContent>
              </Popover>
            </div>
          </div>
          {banksError && <p className="text-destructive">Elenco banche non disponibile: {banksError}</p>}
        </Step>
        <Step n={2} step={step} title="Autorizza sulla pagina della banca">
          <p className="text-muted-foreground">Si apre in una nuova scheda. Accedi alla banca e consenti la lettura dei conti.</p>
          {pending && (
            <Button asChild className="self-start">
              <a href={pending.url} target="_blank" rel="noopener noreferrer" onClick={() => setStep(3)}>
                <ExternalLink /> Apri la pagina di {bank}
              </a>
            </Button>
          )}
        </Step>
        <Step n={3} step={step} title="Incolla l'indirizzo finale">
          {pending && (
            <>
              <p className="text-muted-foreground">
                Alla fine la banca ti manda su <code className="rounded bg-muted px-1 py-0.5 text-xs">{pending.redirect}</code>: la pagina può dare errore, è normale. Copia l'indirizzo completo dalla barra del browser e incollalo qui.
              </p>
              <Input type="url" autoFocus value={finalUrl} onChange={(e) => setFinalUrl(e.target.value)} placeholder="https://…?code=…&state=…" aria-label="Indirizzo finale" onKeyDown={(e) => e.key === "Enter" && finish()} />
              <Button asChild variant="link" size="sm" className="self-start px-0">
                <a href={pending.url} target="_blank" rel="noopener noreferrer">
                  <ExternalLink /> Riapri la pagina della banca
                </a>
              </Button>
            </>
          )}
        </Step>
        <Step n={4} step={step} title="Fatto">
          {result.length ? (
            <ul className="flex flex-col gap-1">
              {result.map((a, i) => (
                <li key={i}>
                  <strong>{a.renewed ? "Rinnovato" : "Collegato"}</strong>: {[a.name, a.iban].filter(Boolean).join(" · ")} ({a.currency})
                </li>
              ))}
            </ul>
          ) : (
            <p>Nessun conto restituito dalla banca.</p>
          )}
          <p className="text-muted-foreground">Ora scarica i movimenti con una sincronizzazione.</p>
        </Step>
      </ol>
      {error && (
        <p className="mt-3 flex items-center gap-2 text-sm text-destructive" role="alert">
          <AlertTriangle className="size-4" /> {error}
        </p>
      )}
    </ResponsiveDialog>
  );
}
