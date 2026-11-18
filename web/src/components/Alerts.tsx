import { AlertTriangle, ChevronRight, Info, OctagonAlert } from "lucide-react";
import { t } from "@/lib/i18n";
import { Link } from "@/lib/router";
import type { Alert as AlertT } from "@/lib/types";
import { cn } from "@/lib/utils";

const ORDER = { critical: 0, warning: 1, info: 2 } as const;

/** Where to act on an alert, by what it is about. */
function actionFor(a: AlertT): { to: string; label: string } | null {
  switch (a.kind) {
    case "consent":
    case "stale":
    case "sync":
      return { to: "/conti", label: t("Conti") };
    case "uncategorized":
      return { to: "/movimenti?uncategorized=1", label: t("Categorizza") };
    case "budget":
      return { to: "/budget", label: t("Budget") };
    case "recurring":
    case "upcoming":
      return { to: "/analisi#ricorrenti", label: t("Dettagli") };
    default:
      return null;
  }
}

export function Alerts({ alerts }: { alerts: AlertT[] }) {
  const sorted = [...alerts].sort((a, b) => ORDER[a.level] - ORDER[b.level]);
  if (sorted.length === 0) return null;
  // One grouped list (iOS "inset grouped" style): a row per alert, the action as a trailing link.
  return (
    <ul className="divide-y divide-border overflow-hidden rounded-2xl bg-card shadow-[0_1px_3px_rgb(0_0_0/0.04)] ring-1 ring-black/[0.04] dark:ring-white/[0.06]">
      {sorted.map((a, i) => {
        const act = actionFor(a);
        const Icon = a.level === "critical" ? OctagonAlert : a.level === "warning" ? AlertTriangle : Info;
        const row = (
          <>
            <Icon className={cn("size-[18px] shrink-0", a.level === "critical" ? "text-destructive" : a.level === "warning" ? "text-warning" : "text-primary")} aria-hidden="true" />
            <span className="min-w-0 flex-1 text-sm">
              <span className="sr-only">{a.level === "critical" ? t("Critico: ") : a.level === "warning" ? t("Attenzione: ") : t("Info: ")}</span>
              {a.message}
            </span>
            {act && (
              <span className="flex shrink-0 items-center gap-0.5 text-sm text-muted-foreground">
                {act.label} <ChevronRight className="size-4" aria-hidden="true" />
              </span>
            )}
          </>
        );
        return (
          <li key={i} role={a.level === "critical" ? "alert" : undefined}>
            {act ? (
              <Link to={act.to} className="flex items-center gap-3 px-4 py-3 transition-colors hover:bg-muted/60">
                {row}
              </Link>
            ) : (
              <div className="flex items-center gap-3 px-4 py-3">{row}</div>
            )}
          </li>
        );
      })}
    </ul>
  );
}
