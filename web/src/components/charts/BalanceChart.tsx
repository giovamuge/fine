import { Area, AreaChart, CartesianGrid, XAxis, YAxis } from "recharts";
import { ChartContainer, ChartTooltip, ChartTooltipContent, type ChartConfig } from "@/components/ui/chart";
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table";
import { compact, euro, fmtDateShort, fmtDateShortYear } from "@/lib/format";
import type { BalancePoint } from "@/lib/types";

const config = { balance: { label: "Saldo", color: "var(--chart-3)" } } satisfies ChartConfig;

export function BalanceChart({ points, showTable }: { points: BalancePoint[]; showTable?: boolean }) {
  if (showTable) {
    // One row per week keeps the table readable.
    const rows = points.filter((_, i) => i % 7 === 0 || i === points.length - 1).reverse();
    return (
      <div className="max-h-80 overflow-y-auto rounded-lg border">
        <Table>
          <TableHeader>
            <TableRow>
              <TableHead>Data</TableHead>
              <TableHead className="text-right">Saldo</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {rows.map((p) => (
              <TableRow key={p.date}>
                <TableCell>{fmtDateShortYear(p.date)}</TableCell>
                <TableCell className="tabular text-right">{euro(p.balance)}</TableCell>
              </TableRow>
            ))}
          </TableBody>
        </Table>
      </div>
    );
  }
  return (
    <ChartContainer config={config} className="aspect-auto h-64 w-full" role="img" aria-label="Andamento del saldo">
      <AreaChart data={points} margin={{ left: 4, right: 8, top: 8 }}>
        <defs>
          <linearGradient id="fillBalance" x1="0" y1="0" x2="0" y2="1">
            <stop offset="5%" stopColor="var(--color-balance)" stopOpacity={0.35} />
            <stop offset="95%" stopColor="var(--color-balance)" stopOpacity={0.02} />
          </linearGradient>
        </defs>
        <CartesianGrid vertical={false} />
        <XAxis dataKey="date" tickLine={false} axisLine={false} tickMargin={8} minTickGap={40} tickFormatter={(d: string) => fmtDateShort(d)} />
        <YAxis tickLine={false} axisLine={false} width={56} tickFormatter={(v: number) => compact(v)} domain={["auto", "auto"]} />
        <ChartTooltip
          cursor={false}
          content={<ChartTooltipContent labelFormatter={(_, p) => fmtDateShortYear(String(p[0]?.payload?.date ?? ""))} formatter={(v) => <span className="tabular font-medium">{euro(Number(v))}</span>} indicator="line" />}
        />
        <Area dataKey="balance" type="monotone" stroke="var(--color-balance)" strokeWidth={2} fill="url(#fillBalance)" />
      </AreaChart>
    </ChartContainer>
  );
}
