import { Bar, BarChart, CartesianGrid, XAxis, YAxis } from "recharts";
import { ChartContainer, ChartLegend, ChartLegendContent, ChartTooltip, ChartTooltipContent, type ChartConfig } from "@/components/ui/chart";
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table";
import { compact, euro, fmtMonth, fmtMonthShort, signed } from "@/lib/format";
import type { TrendPoint } from "@/lib/types";
import { cn } from "@/lib/utils";

const config = {
  income: { label: "Entrate", color: "var(--chart-1)" },
  expenses: { label: "Uscite", color: "var(--chart-2)" },
} satisfies ChartConfig;

export function TrendChart({ data, showTable }: { data: TrendPoint[]; showTable?: boolean }) {
  if (showTable) {
    return (
      <div className="rounded-lg border">
        <Table>
          <TableHeader>
            <TableRow>
              <TableHead>Mese</TableHead>
              <TableHead className="text-right">Entrate</TableHead>
              <TableHead className="text-right">Uscite</TableHead>
              <TableHead className="text-right">Risparmio</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {[...data].reverse().map((p) => (
              <TableRow key={p.month}>
                <TableCell className="capitalize">{fmtMonth(p.month)}</TableCell>
                <TableCell className="tabular text-right">{euro(p.income)}</TableCell>
                <TableCell className="tabular text-right">{euro(p.expenses)}</TableCell>
                <TableCell className={cn("tabular text-right", p.saved < 0 && "text-destructive")}>{signed(p.saved)}</TableCell>
              </TableRow>
            ))}
          </TableBody>
        </Table>
      </div>
    );
  }
  return (
    <ChartContainer config={config} className="aspect-auto h-64 w-full" role="img" aria-label="Entrate e uscite degli ultimi 12 mesi">
      <BarChart data={data} margin={{ left: 4, right: 8, top: 8 }}>
        <CartesianGrid vertical={false} />
        <XAxis dataKey="month" tickLine={false} axisLine={false} tickMargin={8} tickFormatter={(m: string) => fmtMonthShort(m)} />
        <YAxis tickLine={false} axisLine={false} width={56} tickFormatter={(v: number) => compact(v)} />
        <ChartTooltip
          cursor={false}
          content={<ChartTooltipContent labelFormatter={(_, p) => fmtMonth(String(p[0]?.payload?.month ?? ""))} formatter={(v, name) => (
            <span className="flex w-full justify-between gap-4"><span className="text-muted-foreground">{config[name as keyof typeof config]?.label}</span><span className="tabular font-medium">{euro(Number(v))}</span></span>
          )} />}
        />
        <ChartLegend content={<ChartLegendContent />} />
        <Bar dataKey="income" fill="var(--color-income)" radius={[4, 4, 0, 0]} />
        <Bar dataKey="expenses" fill="var(--color-expenses)" radius={[4, 4, 0, 0]} />
      </BarChart>
    </ChartContainer>
  );
}
