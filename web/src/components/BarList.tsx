// Horizontal bars for one magnitude (one hue): categories of a month, tags of a period.
import type { ReactNode } from "react";

export interface BarItem {
  key: string;
  name: ReactNode;
  value: number;
  valueLabel: string;
  share?: string;
  sub?: ReactNode;
  onClick?: () => void;
  title?: string;
}

export function BarList({ items, max }: { items: BarItem[]; max?: number }) {
  const top = max ?? Math.max(1, ...items.map((i) => i.value));
  return (
    <div className="bars">
      {items.map((it) => {
        const inner = (
          <>
            <span className="name">{it.name}</span>
            <span className="amt">
              {it.valueLabel}
              {it.share && <span className="share">{it.share}</span>}
            </span>
            <span className="track" aria-hidden="true">
              <i style={{ width: `${Math.max(0, Math.min(100, (it.value / top) * 100))}%` }} />
            </span>
            {it.sub && <span className="sub">{it.sub}</span>}
          </>
        );
        return it.onClick ? (
          <button key={it.key} type="button" className="bar-row clickable" onClick={it.onClick} title={it.title}>
            {inner}
          </button>
        ) : (
          <div key={it.key} className="bar-row">
            {inner}
          </div>
        );
      })}
    </div>
  );
}
