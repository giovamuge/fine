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
import { t, tn } from "@/lib/i18n";

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
      toast.success(label.trim() ? t("Conto rinominato in «{name}».", { name: label.trim() }) : t("Nome automatico ripristinato."));
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
      toast.success(
        r.consent_revoked
          ? t("Conto rimosso: {n} movimenti cancellati, consenso revocato presso la banca.", { n: integer(r.transactions_deleted) })
          : t("Conto rimosso: {n} movimenti cancellati.", { n: integer(r.transactions_deleted) }),
      );
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
        title={t("Conti")}
        description={t("Banche collegate tramite Enable Banking (PSD2, sola lettura)")}
        actions={
          <>
            <Button variant="outline" onClick={runSync} disabled={sync.running || !accounts.data?.length}>
              <RefreshCw className={cn(sync.running && "animate-spin")} />
              {sync.running ? t("Sincronizzo…") : t("Sincronizza ora")}
            </Button>
            <Button onClick={() => setConnect({})}>
              <Plus /> {t("Collega una banca")}
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
              title={t("Nessun conto collegato.")}
              action={
                <Button onClick={() => setConnect({})}>
                  <Plus /> {t("Collega una banca")}
                </Button>
              }
            >
              {t("Scegli la banca, autorizzi sulla sua pagina e incolli l'indirizzo finale. Serve un'applicazione Enable Banking configurata (vedi README).")}
            </Empty>
          )
        }
      </Loaded>

      <ConnectDialog preset={connect} onClose={() => setConnect(null)} />

      <ResponsiveDialog
        open={rename !== null}
        onClose={() => setRename(null)}
        title={t("Rinomina conto")}
        description={t("Lascia vuoto per tornare al nome automatico ({name}).", { name: rename?.label ?? "" })}
        footer={
          <>
            <Button variant="outline" onClick={() => setRename(null)} disabled={busy}>
              {t("Annulla")}
            </Button>
            <Button onClick={doRename} disabled={busy}>
              {t("Salva")}
            </Button>
          </>
        }
      >
        <div className="flex flex-col gap-1.5 pb-2">
          <Label htmlFor="rename-label">{t("Nome")}</Label>
          <Input id="rename-label" value={label} onChange={(e) => setLabel(e.target.value)} placeholder={rename?.label ?? ""} maxLength={60} onKeyDown={(e) => e.key === "Enter" && doRename()} />
        </div>
      </ResponsiveDialog>

      <ConfirmDialog open={remove !== null} onClose={() => setRemove(null)} onConfirm={doRemove} title={t("Rimuovere «{name}»?", { name: remove?.label ?? "" })} confirmLabel={t("Rimuovi definitivamente")} danger busy={busy}>
        <p>
          {t("Verranno cancellati da questo computer il conto e i suoi")} <strong>{plural(remove?.transactions ?? 0, "movimento", "movimenti")}</strong>{t(", con tag e ammortamenti. Il consenso viene revocato presso la banca se non serve ad altri conti.")}
        </p>
        <p className="font-medium text-destructive">{t("Non si può annullare. Lo storico si recupera solo per quanto la banca concede ricollegando il conto.")}</p>
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
          {[a.bank, a.country].filter(Boolean).join(" · ") || t("Banca")}
          {a.custom_label && a.custom_label !== a.label ? ` · ${a.label}` : ""}
        </CardDescription>
        <CardAction>
          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <Button variant="ghost" size="icon-sm" aria-label={t("Azioni per {name}", { name: a.label })}>
                <MoreHorizontal />
              </Button>
            </DropdownMenuTrigger>
            <DropdownMenuContent align="end">
              <DropdownMenuItem onSelect={onRenew}>
                <RefreshCw /> {t("Rinnova consenso")}
              </DropdownMenuItem>
              <DropdownMenuItem onSelect={onRename}>
                <Pencil /> {t("Rinomina")}
              </DropdownMenuItem>
              <DropdownMenuSeparator />
              <DropdownMenuItem variant="destructive" onSelect={onRemove}>
                <Trash2 /> {t("Rimuovi")}
              </DropdownMenuItem>
            </DropdownMenuContent>
          </DropdownMenu>
        </CardAction>
      </CardHeader>
      <CardContent className="flex flex-col gap-4">
        <div>
          <div className="tabular text-3xl font-semibold tracking-tight">{a.balance === null ? "—" : money(a.balance, a.balance_currency ?? a.currency)}</div>
          {a.balance_at && <div className="text-xs text-muted-foreground">{t("al {date}", { date: fmtDateTime(a.balance_at) })}</div>}
          {a.iban && <div className="mt-1 font-mono text-xs tracking-wide text-muted-foreground">{a.iban}</div>}
        </div>
        <div className="flex flex-wrap gap-1.5">
          {a.last_sync_at === null ? (
            <Badge variant="outline" className="border-warning/40 text-warning">
              <Clock /> {t("Mai sincronizzato")}
            </Badge>
          ) : failed ? (
            <Badge variant="destructive">
              <AlertTriangle /> {t("Sincronizzazione fallita {when}", { when: relativeTime(a.last_sync_at) })}
            </Badge>
          ) : (
            <Badge variant="outline" className="border-positive/30 text-positive">
              <CheckCircle2 /> {t("Sincronizzato {when}", { when: relativeTime(a.last_sync_at) })}
            </Badge>
          )}
          {days === null ? (
            <Badge variant="outline">{t("Consenso: —")}</Badge>
          ) : consentBad ? (
            <Badge variant="destructive">
              <AlertTriangle /> {t("Consenso scaduto il {date}", { date: fmtDate(a.consent_valid_until ?? "") })}
            </Badge>
          ) : consentWarn ? (
            <Badge variant="outline" className="border-warning/40 text-warning">
              <AlertTriangle /> {days === 0 ? t("Consenso scade oggi") : tn(days, "Consenso scade tra {n} giorno", "Consenso scade tra {n} giorni", { n: integer(days) })}
            </Badge>
          ) : (
            <Badge variant="outline">{t("Consenso fino al {date}", { date: fmtDate(a.consent_valid_until ?? "") })}</Badge>
          )}
          <Badge variant="secondary">{plural(a.transactions, "movimento", "movimenti")}</Badge>
        </div>
        {failed && a.last_sync_error && (
          <p className="rounded-lg bg-destructive/10 p-3 text-sm text-destructive" role="alert">
            <strong>{t("Errore:")}</strong> {a.last_sync_error}
          </p>
        )}
        {(consentBad || consentWarn) && !failed && <p className="text-sm text-warning">{t("Rinnova il consenso per continuare a ricevere i movimenti: lo storico resta, senza duplicati.")}</p>}
      </CardContent>
      {urgent && (
        <CardFooter>
          <Button onClick={onRenew}>
            <RefreshCw /> {t("Rinnova consenso")}
          </Button>
        </CardFooter>
      )}
    </Card>
  );
}
