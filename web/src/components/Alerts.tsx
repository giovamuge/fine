import { AlertTriangle, ArrowRight, Info, OctagonAlert } from "lucide-react";
import { Alert, AlertAction, AlertDescription } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { Link } from "@/lib/router";
import type { Alert as AlertT } from "@/lib/types";
import { cn } from "@/lib/utils";

const ORDER = { critical: 0, warning: 1, info: 2 } as const;

/** Suggests where to act for the well-known alert messages (the text itself is data). */
function actionFor(a: AlertT): { to: string; label: string } | null {
  const m = a.message;
  if (/sincronizzazione fallita|consenso bancario|mai sincronizzat|ultimo aggiornamento/i.test(m)) return { to: "/conti", label: "Conti" };
  if (/da categorizzare/i.test(m)) return { to: "/movimenti?uncategorized=1", label: "Categorizza" };
  if (/budget/i.test(m)) return { to: "/analisi#budget", label: "Budget" };
  if (/ricorrente|in arrivo/i.test(m)) return { to: "/analisi#ricorrenti", label: "Dettagli" };
  return null;
}

export function Alerts({ alerts }: { alerts: AlertT[] }) {
  const sorted = [...alerts].sort((a, b) => ORDER[a.level] - ORDER[b.level]);
  return (
    <div className="flex flex-col gap-2">
      {sorted.map((a, i) => {
        const act = actionFor(a);
        const Icon = a.level === "critical" ? OctagonAlert : a.level === "warning" ? AlertTriangle : Info;
        return (
          <Alert
            key={i}
            variant={a.level === "critical" ? "destructive" : "default"}
            role={a.level === "critical" ? "alert" : undefined}
            className={cn(a.level === "warning" && "border-warning/40 bg-warning/5 [&>svg]:text-warning")}
          >
            <Icon />
            <AlertDescription className="text-foreground/90">
              <span className="sr-only">{a.level === "critical" ? "Critico: " : a.level === "warning" ? "Attenzione: " : "Info: "}</span>
              {a.message}
            </AlertDescription>
            {act && (
              <AlertAction>
                <Button asChild size="sm" variant="ghost">
                  <Link to={act.to}>
                    {act.label} <ArrowRight />
                  </Link>
                </Button>
              </AlertAction>
            )}
          </Alert>
        );
      })}
    </div>
  );
}
