import { useState } from "react";
import { AlertTriangle, CheckCircle2, Clock, Landmark, MoreHorizontal, Pencil, Plus, RefreshCw, Trash2 } from "lucide-react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardAction, CardContent, CardDescription, CardFooter, CardHeader, CardTitle } from "@/components/ui/card";
import { DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuSeparator, DropdownMenuTrigger } from "@/components/ui/dropdown-menu";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { ConfirmDialog } from "@/components/ConfirmDialog";
import { ConnectDialog, type ConnectPreset } from "@/components/ConnectDialog";
import { PageHeader } from "@/components/PageHeader";
import { ResponsiveDialog } from "@/components/ResponsiveDialog";
import { Empty, Loaded } from "@/components/States";
import { SyncResultDialog } from "@/components/SyncResultDialog";
import { api, errorMessage } from "@/lib/api";
import { useApp } from "@/lib/app-state";
import { daysUntil, fmtDate, fmtDateTime, integer, money, plural, relativeTime } from "@/lib/format";
import { useToast } from "@/lib/toast";
import type { AccountView } from "@/lib/types";
import { useAsync } from "@/lib/useAsync";
import { cn } from "@/lib/utils";

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
      <PageHeader
        title="Conti"
        description="Banche collegate tramite Enable Banking (PSD2, sola lettura)"
        actions={
          <>
            <Button variant="outline" onClick={runSync} disabled={sync.running || !accounts.data?.length}>
              <RefreshCw className={cn(sync.running && "animate-spin")} />
              {sync.running ? "Sincronizzo…" : "Sincronizza ora"}
            </Button>
            <Button onClick={() => setConnect({})}>
              <Plus /> Collega una banca
            </Button>
          </>
        }
      />

      <Loaded {...accounts} rows={6}>
        {(list) =>
          list.length ? (
            <div className="grid gap-4 lg:grid-cols-2">
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
            <Empty
              icon={<Landmark />}
              title="Nessun conto collegato."
              action={
                <Button onClick={() => setConnect({})}>
                  <Plus /> Collega una banca
                </Button>
              }
            >
              Scegli la banca, autorizzi sulla sua pagina e incolli l'indirizzo finale. Serve un'applicazione Enable Banking configurata (vedi README).
            </Empty>
          )
        }
      </Loaded>

      <ConnectDialog preset={connect} onClose={() => setConnect(null)} />

      <ResponsiveDialog
        open={rename !== null}
        onClose={() => setRename(null)}
        title="Rinomina conto"
        description={`Lascia vuoto per tornare al nome automatico (${rename?.label ?? ""}).`}
        footer={
          <>
            <Button variant="outline" onClick={() => setRename(null)} disabled={busy}>
              Annulla
            </Button>
            <Button onClick={doRename} disabled={busy}>
              Salva
            </Button>
          </>
        }
      >
        <div className="flex flex-col gap-1.5 pb-2">
          <Label htmlFor="rename-label">Nome</Label>
          <Input id="rename-label" value={label} onChange={(e) => setLabel(e.target.value)} placeholder={rename?.label ?? ""} maxLength={60} onKeyDown={(e) => e.key === "Enter" && doRename()} />
        </div>
      </ResponsiveDialog>

      <ConfirmDialog open={remove !== null} onClose={() => setRemove(null)} onConfirm={doRemove} title={`Rimuovere «${remove?.label ?? ""}»?`} confirmLabel="Rimuovi definitivamente" danger busy={busy}>
        <p>
          Verranno cancellati da questo computer il conto e i suoi <strong>{plural(remove?.transactions ?? 0, "movimento", "movimenti")}</strong>, con tag e ammortamenti. Il consenso viene revocato presso la banca se non serve ad altri conti.
        </p>
        <p className="font-medium text-destructive">Non si può annullare. Lo storico si recupera solo per quanto la banca concede ricollegando il conto.</p>
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
  const urgent = failed || consentBad || consentWarn;
  return (
    <Card className={cn(failed && "border-destructive/40")}>
      <CardHeader>
        <CardTitle className="truncate">{a.custom_label ?? a.label}</CardTitle>
        <CardDescription className="truncate">
          {[a.bank, a.country].filter(Boolean).join(" · ") || "Banca"}
          {a.custom_label && a.custom_label !== a.label ? ` · ${a.label}` : ""}
        </CardDescription>
        <CardAction>
          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <Button variant="ghost" size="icon-sm" aria-label={`Azioni per ${a.label}`}>
                <MoreHorizontal />
              </Button>
            </DropdownMenuTrigger>
            <DropdownMenuContent align="end">
              <DropdownMenuItem onSelect={onRenew}>
                <RefreshCw /> Rinnova consenso
              </DropdownMenuItem>
              <DropdownMenuItem onSelect={onRename}>
                <Pencil /> Rinomina
              </DropdownMenuItem>
              <DropdownMenuSeparator />
              <DropdownMenuItem variant="destructive" onSelect={onRemove}>
                <Trash2 /> Rimuovi
              </DropdownMenuItem>
            </DropdownMenuContent>
          </DropdownMenu>
        </CardAction>
      </CardHeader>
      <CardContent className="flex flex-col gap-4">
        <div>
          <div className="tabular text-3xl font-semibold tracking-tight">{a.balance === null ? "—" : money(a.balance, a.balance_currency ?? a.currency)}</div>
          {a.balance_at && <div className="text-xs text-muted-foreground">al {fmtDateTime(a.balance_at)}</div>}
          {a.iban && <div className="mt-1 font-mono text-xs tracking-wide text-muted-foreground">{a.iban}</div>}
        </div>
        <div className="flex flex-wrap gap-1.5">
          {a.last_sync_at === null ? (
            <Badge variant="outline" className="border-warning/40 text-warning">
              <Clock /> Mai sincronizzato
            </Badge>
          ) : failed ? (
            <Badge variant="destructive">
              <AlertTriangle /> Sincronizzazione fallita {relativeTime(a.last_sync_at)}
            </Badge>
          ) : (
            <Badge variant="outline" className="border-positive/30 text-positive">
              <CheckCircle2 /> Sincronizzato {relativeTime(a.last_sync_at)}
            </Badge>
          )}
          {days === null ? (
            <Badge variant="outline">Consenso: —</Badge>
          ) : consentBad ? (
            <Badge variant="destructive">
              <AlertTriangle /> Consenso scaduto il {fmtDate(a.consent_valid_until ?? "")}
            </Badge>
          ) : consentWarn ? (
            <Badge variant="outline" className="border-warning/40 text-warning">
              <AlertTriangle /> Consenso scade {days === 0 ? "oggi" : `tra ${integer(days)} ${days === 1 ? "giorno" : "giorni"}`}
            </Badge>
          ) : (
            <Badge variant="outline">Consenso fino al {fmtDate(a.consent_valid_until ?? "")}</Badge>
          )}
          <Badge variant="secondary">{plural(a.transactions, "movimento", "movimenti")}</Badge>
        </div>
        {failed && a.last_sync_error && (
          <p className="rounded-lg bg-destructive/10 p-3 text-sm text-destructive" role="alert">
            <strong>Errore:</strong> {a.last_sync_error}
          </p>
        )}
        {(consentBad || consentWarn) && !failed && <p className="text-sm text-warning">Rinnova il consenso per continuare a ricevere i movimenti: lo storico resta, senza duplicati.</p>}
      </CardContent>
      {urgent && (
        <CardFooter>
          <Button onClick={onRenew}>
            <RefreshCw /> Rinnova consenso
          </Button>
        </CardFooter>
      )}
    </Card>
  );
}
