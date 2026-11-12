// Daily total balance: one line, area wash, hairline grid, crosshair + tooltip on hover
// and keyboard (arrow keys), plus a table view.
import { useId, useMemo, useState, type KeyboardEvent, type PointerEvent } from "react";
import type { BalancePoint } from "../../lib/types";
import { compact, euro, fmtDate, fmtDateShort, fmtDateShortYear } from "../../lib/format";
import { ticks, useWidth } from "./useWidth";

const H = 220;
const PAD = { top: 12, right: 12, bottom: 26, left: 48 };

export function BalanceChart({ points, showTable }: { points: BalancePoint[]; showTable: boolean }) {
  const [ref, width] = useWidth<HTMLDivElement>();
  const [hover, setHover] = useState<number | null>(null);
  const id = useId();

  const geo = useMemo(() => {
    const vals = points.map((p) => p.balance);
    let lo = Math.min(0, ...vals);
    let hi = Math.max(...vals);
    if (hi - lo < 1) {
      hi += 50;
      lo -= 50;
    }
    const pad = (hi - lo) * 0.08;
    lo = Math.min(lo, Math.min(...vals) - pad);
    hi += pad;
    const innerW = width - PAD.left - PAD.right;
    const innerH = H - PAD.top - PAD.bottom;
    const x = (i: number) => PAD.left + (points.length > 1 ? (innerW * i) / (points.length - 1) : innerW / 2);
    const y = (v: number) => PAD.top + innerH * (1 - (v - lo) / (hi - lo));
    const line = points.map((p, i) => `${i ? "L" : "M"}${x(i).toFixed(1)},${y(p.balance).toFixed(1)}`).join("");
    const base = y(Math.max(lo, 0));
    const area = `${line}L${x(points.length - 1).toFixed(1)},${base.toFixed(1)}L${x(0).toFixed(1)},${base.toFixed(1)}Z`;
    const yTicks = ticks(lo, hi, 4);
    const step = Math.max(1, Math.ceil(points.length / Math.max(2, Math.floor(innerW / 90))));
    const xTicks: number[] = [];
    for (let i = 0; i < points.length; i += step) xTicks.push(i);
    return { x, y, line, area, yTicks, xTicks, lo, hi, innerW };
  }, [points, width]);

  if (points.length < 2) return null;

  const pick = (e: PointerEvent<SVGSVGElement>) => {
    const r = e.currentTarget.getBoundingClientRect();
    const px = e.clientX - r.left;
    const i = Math.round(((px - PAD.left) / geo.innerW) * (points.length - 1));
    setHover(Math.max(0, Math.min(points.length - 1, i)));
  };
  const onKey = (e: KeyboardEvent<SVGSVGElement>) => {
    if (e.key === "ArrowLeft" || e.key === "ArrowRight") {
      e.preventDefault();
      setHover((h) => {
        const cur = h ?? points.length - 1;
        return Math.max(0, Math.min(points.length - 1, cur + (e.key === "ArrowLeft" ? -1 : 1)));
      });
    } else if (e.key === "Home") setHover(0);
    else if (e.key === "End") setHover(points.length - 1);
    else if (e.key === "Escape") setHover(null);
  };

  const hp = hover === null ? null : points[hover];
  const first = points[0];
  const last = points[points.length - 1];
  const desc = `Saldo dal ${first ? fmtDate(first.date) : ""} al ${last ? fmtDate(last.date) : ""}: da ${euro(first?.balance ?? 0)} a ${euro(last?.balance ?? 0)}.`;

  return (
    <div className="chart" ref={ref}>
      <svg
        viewBox={`0 0 ${width} ${H}`}
        width={width}
        height={H}
        role="img"
        aria-labelledby={`${id}-t`}
        aria-describedby={`${id}-d`}
        tabIndex={0}
        onPointerMove={pick}
        onPointerDown={pick}
        onPointerLeave={() => setHover(null)}
        onKeyDown={onKey}
        onFocus={() => setHover((h) => h ?? points.length - 1)}
        onBlur={() => setHover(null)}
      >
        <title id={`${id}-t`}>Andamento del saldo</title>
        <desc id={`${id}-d`}>{desc} Usa le frecce per scorrere i giorni.</desc>
        {geo.yTicks.map((v) => (
          <g key={v}>
            <line className={v === 0 ? "baseline" : "grid-line"} x1={PAD.left} x2={width - PAD.right} y1={geo.y(v)} y2={geo.y(v)} />
            <text className="axis" x={PAD.left - 8} y={geo.y(v) + 4} textAnchor="end">
              {compact(v)}
            </text>
          </g>
        ))}
        <path d={geo.area} fill="var(--accent)" fillOpacity={0.1} />
        <path d={geo.line} fill="none" stroke="var(--accent)" strokeWidth={2} strokeLinejoin="round" strokeLinecap="round" />
        {geo.xTicks.map((i) => (
          <text key={i} className="axis" x={geo.x(i)} y={H - 6} textAnchor={i === 0 ? "start" : "middle"}>
            {fmtDateShort(points[i]?.date ?? "")}
          </text>
        ))}
        {hp && hover !== null && (
          <g>
            <line x1={geo.x(hover)} x2={geo.x(hover)} y1={PAD.top} y2={H - PAD.bottom} stroke="var(--line-2)" strokeWidth={1} />
            <circle cx={geo.x(hover)} cy={geo.y(hp.balance)} r={5} fill="var(--accent)" stroke="var(--card)" strokeWidth={2} />
          </g>
        )}
      </svg>
      {hp && hover !== null && (
        <div
          className="tooltip"
          style={{
            left: Math.max(80, Math.min(width - 80, geo.x(hover))),
            top: Math.max(0, geo.y(hp.balance) - 64),
          }}
        >
          <div className="d">{fmtDateShortYear(hp.date)}</div>
          <div className="r">
            <span className="k" />
            <span>Saldo</span>
            <b>{euro(hp.balance)}</b>
          </div>
        </div>
      )}
      {showTable && <BalanceTable points={points} />}
    </div>
  );
}

function BalanceTable({ points }: { points: BalancePoint[] }) {
  // One row per week (last day of each week) plus the latest day: compact and complete enough.
  const rows: BalancePoint[] = [];
  for (let i = points.length - 1; i >= 0; i -= 7) {
    const p = points[i];
    if (p) rows.push(p);
  }
  return (
    <table className="chart-table">
      <caption className="sr-only">Saldo totale per settimana</caption>
      <thead>
        <tr>
          <th scope="col">Giorno</th>
          <th scope="col" className="n">
            Saldo
          </th>
        </tr>
      </thead>
      <tbody>
        {rows.map((p) => (
          <tr key={p.date}>
            <td>{fmtDateShortYear(p.date)}</td>
            <td className="n">{euro(p.balance)}</td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}
