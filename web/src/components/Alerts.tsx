import type { Alert } from "../lib/types";
import { Icon } from "./Icon";
import { Link } from "../lib/router";

const ORDER = { critical: 0, warning: 1, info: 2 } as const;

/** Suggests where to act for the well-known alert messages (the text itself is data). */
function actionFor(a: Alert): { to: string; label: string } | null {
  const m = a.message;
  if (/sincronizzazione fallita|consenso bancario|mai sincronizzat|ultimo aggiornamento/i.test(m)) {
    return { to: "/conti", label: "Vai ai conti" };
  }
  if (/da categorizzare/i.test(m)) return { to: "/movimenti?uncategorized=1", label: "Categorizza" };
  if (/budget/i.test(m)) return { to: "/analisi#budget", label: "Vedi budget" };
  if (/ricorrente|in arrivo/i.test(m)) return { to: "/analisi#ricorrenti", label: "Dettagli" };
  return null;
}

export function Alerts({ alerts, max }: { alerts: Alert[]; max?: number }) {
  const sorted = [...alerts].sort((a, b) => ORDER[a.level] - ORDER[b.level]);
  const shown = max ? sorted.slice(0, max) : sorted;
  if (!shown.length) return null;
  return (
    <ul className="alerts">
      {shown.map((a, i) => {
        const act = actionFor(a);
        return (
          <li key={i} className={`alert ${a.level}`} role={a.level === "critical" ? "alert" : undefined}>
            <Icon name={a.level === "info" ? "info" : "alert"} size={18} />
            <span>
              <span className="sr-only">
                {a.level === "critical" ? "Critico: " : a.level === "warning" ? "Attenzione: " : "Info: "}
              </span>
              {a.message}
            </span>
            {act && (
              <span className="actions">
                <Link to={act.to} className="btn sm">
                  {act.label}
                </Link>
              </span>
            )}
          </li>
        );
      })}
    </ul>
  );
}
