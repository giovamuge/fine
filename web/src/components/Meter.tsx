// Budget/pace meter: the fill carries the state, an optional marker shows where "today" is.
import type { BudgetState } from "@/lib/types";
import { cn } from "@/lib/utils";

const FILL: Record<BudgetState | "neutral", string> = {
  ok: "bg-positive",
  attenzione: "bg-warning",
  sforato: "bg-destructive",
  neutral: "bg-primary",
};

export function Meter({
  value,
  marker,
  markerLabel,
  state,
  label,
  className,
}: {
  /** 0..1, clamped for drawing (the real value stays in the accessible name). */
  value: number;
  marker?: number;
  markerLabel?: string;
  state?: BudgetState;
  label: string;
  className?: string;
}) {
  const w = Math.max(0, Math.min(1, value)) * 100;
  const m = marker === undefined ? undefined : Math.max(0, Math.min(1, marker)) * 100;
  return (
    <div className={cn("relative", markerLabel && "pb-5", className)}>
      <div
        className="relative h-2 w-full overflow-hidden rounded-full bg-muted"
        role="progressbar"
        aria-valuemin={0}
        aria-valuemax={100}
        aria-valuenow={Math.round(value * 100)}
        aria-label={label}
      >
        <div className={cn("h-full rounded-full transition-[width] duration-500", FILL[state ?? "neutral"])} style={{ width: `${w}%` }} />
      </div>
      {m !== undefined && (
        <div className="absolute top-[-3px] h-[14px] w-0.5 -translate-x-1/2 rounded bg-foreground/70" style={{ left: `${m}%` }} aria-hidden="true">
          {markerLabel && <span className="absolute top-4 left-1/2 -translate-x-1/2 text-[10px] font-medium text-muted-foreground">{markerLabel}</span>}
        </div>
      )}
    </div>
  );
}
