// Horizontal bars for one magnitude: categories of a month, tags of a period.
import type { ReactNode } from "react";
import { cn } from "@/lib/utils";

export interface BarItem {
  key: string;
  name: ReactNode;
  value: number;
  valueLabel: string;
  share?: string;
  sub?: ReactNode;
  icon?: ReactNode;
  /** Tailwind bg class for the bar. */
  barClass?: string;
  onClick?: () => void;
  title?: string;
}

export function BarList({ items }: { items: BarItem[] }) {
  const top = Math.max(1, ...items.map((i) => i.value));
  return (
    <ul className="flex flex-col">
      {items.map((it) => {
        const inner = (
          <>
            {it.icon}
            <span className="flex min-w-0 flex-1 flex-col gap-1.5">
              <span className="flex items-baseline justify-between gap-3">
                <span className="truncate text-sm font-medium">{it.name}</span>
                <span className="tabular shrink-0 text-sm">
                  {it.valueLabel}
                  {it.share && <span className="ml-1.5 text-xs text-muted-foreground">{it.share}</span>}
                </span>
              </span>
              <span className="h-1.5 w-full overflow-hidden rounded-full bg-muted" aria-hidden="true">
                <span className={cn("block h-full rounded-full", it.barClass ?? "bg-primary")} style={{ width: `${Math.max(1.5, Math.min(100, (it.value / top) * 100))}%` }} />
              </span>
              {it.sub && <span className="text-xs text-muted-foreground">{it.sub}</span>}
            </span>
          </>
        );
        return (
          <li key={it.key}>
            {it.onClick ? (
              <button type="button" onClick={it.onClick} title={it.title} className="flex w-full items-center gap-3 rounded-lg px-2 py-2 text-left transition-colors hover:bg-muted/60 focus-visible:ring-2 focus-visible:ring-ring/50 focus-visible:outline-none">
                {inner}
              </button>
            ) : (
              <div className="flex items-center gap-3 px-2 py-2">{inner}</div>
            )}
          </li>
        );
      })}
    </ul>
  );
}
