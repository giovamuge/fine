import { CheckCircle2, OctagonAlert } from "lucide-react";
import { Button } from "@/components/ui/button";
import { ResponsiveDialog } from "@/components/ResponsiveDialog";
import type { SyncState } from "@/lib/app-state";
import { integer } from "@/lib/format";
import { t, tn } from "@/lib/i18n";

export function SyncResultDialog({ state, onClose }: { state: SyncState; onClose: () => void }) {
  const open = !state.running && (state.result !== null || state.error !== null);
  const r = state.result;
  return (
    <ResponsiveDialog open={open} onClose={onClose} title={t("Esito della sincronizzazione")} footer={<Button onClick={onClose}>Ok</Button>}>
      <div className="flex flex-col gap-3 text-sm">
        {state.error && (
          <div className="flex gap-2 text-destructive" role="alert">
            <OctagonAlert className="size-4 shrink-0" /> {state.error}
          </div>
        )}
        {r?.sync.accounts.map((a) => (
          <div key={a.account} className="flex gap-3 rounded-lg border p-3">
            {a.error ? <OctagonAlert className="size-4 shrink-0 text-destructive" /> : <CheckCircle2 className="size-4 shrink-0 text-positive" />}
            <div>
              <div className="font-medium">{a.account}</div>
              <div className={a.error ? "text-destructive" : "text-muted-foreground"}>
                {a.error ? a.error : a.skipped ? t("Saltato: {reason}", { reason: a.skipped }) : a.inserted ? tn(a.inserted, "{n} nuovo movimento", "{n} nuovi movimenti", { n: integer(a.inserted) }) : t("Nessun nuovo movimento")}
              </div>
            </div>
          </div>
        ))}
        {r && r.sync.accounts.length === 0 && <p className="text-muted-foreground">{t("Nessun conto da sincronizzare.")}</p>}
        {r && (r.sync.auto_categorized > 0 || r.sync.auto_tagged > 0) && (
          <p className="text-muted-foreground">
            {t("Regole applicate: {cat} categorizzati, {tag} taggati.", { cat: integer(r.sync.auto_categorized), tag: integer(r.sync.auto_tagged) })}
          </p>
        )}
        {r?.ai && (
          <p className="text-muted-foreground">
            {t("AI ({backend}): {done} su {seen} categorizzati", { backend: r.ai.backend, done: integer(r.ai.categorized), seen: integer(r.ai.examined) })}
            {r.ai.rules_created.length ? t(", {n} nuove regole", { n: integer(r.ai.rules_created.length) }) : ""}.
          </p>
        )}
        {r?.ai_error && <p className="text-destructive">{t("Categorizzazione AI non riuscita: {error}", { error: r.ai_error })}</p>}
      </div>
    </ResponsiveDialog>
  );
}
