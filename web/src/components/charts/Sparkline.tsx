// Small single-series line with a 10% wash, for stat tiles. No axes, no hover: the
// values live in the chart it links to.
export function Sparkline({ values, width = 120, height = 36 }: { values: number[]; width?: number; height?: number }) {
  if (values.length < 2) return null;
  let lo = Math.min(...values);
  let hi = Math.max(...values);
  if (hi - lo < 1) {
    hi += 1;
    lo -= 1;
  }
  const x = (i: number) => (i / (values.length - 1)) * (width - 4) + 2;
  const y = (v: number) => 3 + (height - 6) * (1 - (v - lo) / (hi - lo));
  const d = values.map((v, i) => `${i ? "L" : "M"}${x(i).toFixed(1)},${y(v).toFixed(1)}`).join("");
  const last = values[values.length - 1] ?? 0;
  return (
    <svg width={width} height={height} viewBox={`0 0 ${width} ${height}`} aria-hidden="true" focusable="false">
      <path d={`${d}L${x(values.length - 1)},${height}L${x(0)},${height}Z`} fill="var(--accent)" fillOpacity={0.1} />
      <path d={d} fill="none" stroke="var(--accent)" strokeWidth={2} strokeLinejoin="round" strokeLinecap="round" />
      <circle cx={x(values.length - 1)} cy={y(last)} r={4} fill="var(--accent)" stroke="var(--card)" strokeWidth={2} />
    </svg>
  );
}
