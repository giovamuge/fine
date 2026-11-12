// Income vs expenses per month: grouped columns (<= 24px, 2px surface gap, rounded caps),
// hairline grid, per-month hover/focus tooltip with both values and the saving, legend,
// and a table view.
import { useId, useMemo, useState } from "react";
import type { TrendPoint } from "../../lib/types";
import { compact, euro, fmtMonth, fmtMonthShort, signed } from "../../lib/format";
import { ticks, useWidth } from "./useWidth";

const H = 220;
const PAD = { top: 10, right: 8, bottom: 26, left: 44 };

function column(x: number, y: number, w: number, h: number, r: number): string {
  // Rounded top corners, square at the baseline.
  if (h <= 0) return "";
  const rr = Math.min(r, w / 2, h);
  return `M${x},${y + h}V${y + rr}Q${x},${y} ${x + rr},${y}H${x + w - rr}Q${x + w},${y} ${x + w},${y + rr}V${y + h}Z`;
}

export function TrendChart({ data, showTable }: { data: TrendPoint[]; showTable: boolean }) {
  const [ref, width] = useWidth<HTMLDivElement>();
  const [hover, setHover] = useState<number | null>(null);
  const id = useId();

  const geo = useMemo(() => {
    const max = Math.max(1, ...data.flatMap((d) => [d.income, d.expenses]));
    const yTicks = ticks(0, max, 4);
    const top = Math.max(max, yTicks[yTicks.length - 1] ?? max);
    const innerW = width - PAD.left - PAD.right;
    const innerH = H - PAD.top - PAD.bottom;
    const slot = innerW / Math.max(1, data.length);
    const bw = Math.max(3, Math.min(24, (slot - 8) / 2 - 1));
    const y = (v: number) => PAD.top + innerH * (1 - v / top);
    const x0 = (i: number) => PAD.left + i * slot + slot / 2;
    return { yTicks, top, slot, bw, y, x0, innerH };
  }, [data, width]);

  if (!data.length) return null;
  const baseY = PAD.top + geo.innerH;
  const hp = hover === null ? null : data[hover];

  return (
    <div className="chart" ref={ref}>
      <svg
        viewBox={`0 0 ${width} ${H}`}
        width={width}
        height={H}
        role="img"
        aria-labelledby={`${id}-t`}
        onPointerLeave={() => setHover(null)}
      >
        <title id={`${id}-t`}>Entrate e uscite negli ultimi {data.length} mesi</title>
        {geo.yTicks.map((v) => (
          <g key={v}>
            <line className={v === 0 ? "baseline" : "grid-line"} x1={PAD.left} x2={width - PAD.right} y1={geo.y(v)} y2={geo.y(v)} />
            <text className="axis" x={PAD.left - 8} y={geo.y(v) + 4} textAnchor="end">
              {compact(v)}
            </text>
          </g>
        ))}
        {data.map((d, i) => {
          const cx = geo.x0(i);
          const ih = baseY - geo.y(d.income);
          const eh = baseY - geo.y(d.expenses);
          const active = hover === i;
          return (
            <g
              key={d.month}
              tabIndex={0}
              role="img"
              aria-label={`${fmtMonth(d.month)}: entrate ${euro(d.income)}, uscite ${euro(d.expenses)}, risparmio ${signed(d.saved)}`}
              onPointerEnter={() => setHover(i)}
              onPointerMove={() => setHover(i)}
              onFocus={() => setHover(i)}
              onBlur={() => setHover(null)}
              style={{ outline: "none" }}
            >
              {/* hit target: the whole slot */}
              <rect x={cx - geo.slot / 2} y={PAD.top} width={geo.slot} height={geo.innerH} fill="transparent" />
              {active && <rect x={cx - geo.slot / 2} y={PAD.top} width={geo.slot} height={geo.innerH} fill="var(--card-2)" />}
              <path d={column(cx - geo.bw - 1, geo.y(d.income), geo.bw, ih, 4)} fill="var(--income)" opacity={hover === null || active ? 1 : 0.55} />
              <path d={column(cx + 1, geo.y(d.expenses), geo.bw, eh, 4)} fill="var(--expense)" opacity={hover === null || active ? 1 : 0.55} />
              <text className="axis" x={cx} y={H - 6} textAnchor="middle">
                {fmtMonthShort(d.month)}
              </text>
            </g>
          );
        })}
        <line className="baseline" x1={PAD.left} x2={width - PAD.right} y1={baseY} y2={baseY} />
      </svg>
      {hp && hover !== null && (
        <div className="tooltip" style={{ left: Math.max(90, Math.min(width - 90, geo.x0(hover))), top: 0 }}>
          <div className="d">{fmtMonth(hp.month)}</div>
          <div className="r">
            <span className="k" style={{ background: "var(--income)" }} />
            <span>Entrate</span>
            <b>{euro(hp.income)}</b>
          </div>
          <div className="r">
            <span className="k" style={{ background: "var(--expense)" }} />
            <span>Uscite</span>
            <b>{euro(hp.expenses)}</b>
          </div>
          <div className="r">
            <span className="k" style={{ background: "transparent" }} />
            <span>Risparmio</span>
            <b>{signed(hp.saved)}</b>
          </div>
        </div>
      )}
      {showTable && (
        <table className="chart-table">
          <caption className="sr-only">Entrate, uscite e risparmio per mese</caption>
          <thead>
            <tr>
              <th scope="col">Mese</th>
              <th scope="col" className="n">
                Entrate
              </th>
              <th scope="col" className="n">
                Uscite
              </th>
              <th scope="col" className="n">
                Risparmio
              </th>
            </tr>
          </thead>
          <tbody>
            {[...data].reverse().map((d) => (
              <tr key={d.month}>
                <td>{fmtMonth(d.month)}</td>
                <td className="n">{euro(d.income)}</td>
                <td className="n">{euro(d.expenses)}</td>
                <td className="n">{signed(d.saved)}</td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </div>
  );
}
