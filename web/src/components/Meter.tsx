// Budget/pace meter: the fill carries the state (ok / attenzione / sforato), the track is a
// lighter step of the same hue; an optional marker shows where "today" sits in the period.
import type { BudgetState } from "../lib/types";

export function Meter({
  value,
  marker,
  markerLabel,
  state,
  label,
  thin = false,
}: {
  /** 0..1 (clamped for drawing; the real value stays in the accessible name). */
  value: number;
  marker?: number;
  markerLabel?: string;
  state?: BudgetState | "";
  label: string;
  thin?: boolean;
}) {
  const w = Math.max(0, Math.min(1, value)) * 100;
  const m = marker === undefined ? undefined : Math.max(0, Math.min(1, marker)) * 100;
  return (
    <div
      className={`meter ${state ?? ""}${thin ? " thin" : ""}${markerLabel ? " labeled" : ""}`}
      role="progressbar"
      aria-valuemin={0}
      aria-valuemax={100}
      aria-valuenow={Math.round(value * 100)}
      aria-label={label}
    >
      <span className="fill" style={{ width: `${w}%` }} />
      {m !== undefined && <span className="marker" style={{ left: `${m}%` }} data-label={markerLabel ?? ""} />}
    </div>
  );
}
