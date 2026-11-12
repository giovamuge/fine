import { Area, AreaChart } from "recharts";
import { ChartContainer, type ChartConfig } from "@/components/ui/chart";

const config = { v: { label: "Saldo", color: "var(--chart-3)" } } satisfies ChartConfig;

export function Sparkline({ values }: { values: number[] }) {
  if (values.length < 2) return null;
  const data = values.map((v, i) => ({ i, v }));
  return (
    <ChartContainer config={config} className="aspect-auto h-12 w-32" aria-hidden="true">
      <AreaChart data={data} margin={{ top: 2, bottom: 2, left: 0, right: 0 }}>
        <defs>
          <linearGradient id="fillSpark" x1="0" y1="0" x2="0" y2="1">
            <stop offset="5%" stopColor="var(--color-v)" stopOpacity={0.3} />
            <stop offset="95%" stopColor="var(--color-v)" stopOpacity={0} />
          </linearGradient>
        </defs>
        <Area dataKey="v" type="monotone" stroke="var(--color-v)" strokeWidth={1.5} fill="url(#fillSpark)" isAnimationActive={false} />
      </AreaChart>
    </ChartContainer>
  );
}
