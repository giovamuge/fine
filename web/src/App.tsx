import { useEffect } from "react";
import { ArrowLeftRight, ChartPie, Landmark, LayoutDashboard, Monitor, Moon, RefreshCw, Shapes, Sun } from "lucide-react";
import { Button } from "@/components/ui/button";
import { DropdownMenu, DropdownMenuContent, DropdownMenuLabel, DropdownMenuRadioGroup, DropdownMenuRadioItem, DropdownMenuTrigger } from "@/components/ui/dropdown-menu";
import { Toaster } from "@/components/ui/sonner";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { useApp, useTheme, type Theme } from "@/lib/app-state";
import { Link, useRoute } from "@/lib/router";
import { cn } from "@/lib/utils";
import { Overview } from "@/views/Overview";
import { Movimenti } from "@/views/Movimenti";
import { Analisi } from "@/views/Analisi";
import { Categorie } from "@/views/Categorie";
import { Conti } from "@/views/Conti";

const NAV = [
  { to: "/", label: "Panoramica", icon: LayoutDashboard },
  { to: "/movimenti", label: "Movimenti", icon: ArrowLeftRight },
  { to: "/analisi", label: "Analisi", icon: ChartPie },
  { to: "/categorie", label: "Categorie", icon: Shapes },
  { to: "/conti", label: "Conti", icon: Landmark },
] as const;

const THEME_ICON: Record<Theme, typeof Sun> = { system: Monitor, light: Sun, dark: Moon };

function Logo() {
  return (
    <span className="flex items-center gap-2 text-base font-semibold tracking-tight">
      <span className="flex size-7 items-center justify-center rounded-lg bg-primary text-sm font-bold text-primary-foreground" aria-hidden="true">
        f
      </span>
      fine
    </span>
  );
}

function ThemeMenu({ theme, setTheme }: { theme: Theme; setTheme: (t: Theme) => void }) {
  const Icon = THEME_ICON[theme];
  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button variant="ghost" size="icon" aria-label="Tema">
          <Icon />
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end">
        <DropdownMenuLabel>Tema</DropdownMenuLabel>
        <DropdownMenuRadioGroup value={theme} onValueChange={(v) => setTheme(v as Theme)}>
          <DropdownMenuRadioItem value="system">Automatico</DropdownMenuRadioItem>
          <DropdownMenuRadioItem value="light">Chiaro</DropdownMenuRadioItem>
          <DropdownMenuRadioItem value="dark">Scuro</DropdownMenuRadioItem>
        </DropdownMenuRadioGroup>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}

function SyncButton({ compact }: { compact?: boolean }) {
  const { sync, runSync } = useApp();
  const icon = <RefreshCw className={cn(sync.running && "animate-spin")} />;
  if (compact) {
    return (
      <Button variant="ghost" size="icon" onClick={runSync} disabled={sync.running} aria-label={sync.running ? "Sincronizzazione in corso" : "Sincronizza ora"}>
        {icon}
      </Button>
    );
  }
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <Button variant="outline" className="w-full justify-start" onClick={runSync} disabled={sync.running}>
          {icon}
          {sync.running ? "Sincronizzo…" : "Sincronizza ora"}
        </Button>
      </TooltipTrigger>
      <TooltipContent side="right">Scarica i nuovi movimenti (max 4 volte al giorno per conto)</TooltipContent>
    </Tooltip>
  );
}

export function App() {
  const route = useRoute();
  const [theme, setTheme] = useTheme();
  const active = NAV.find((n) => n.to === route.path) ?? NAV[0];

  useEffect(() => {
    document.title = active.to === "/" ? "fine" : `${active.label} · fine`;
    if (!window.location.hash) window.scrollTo({ top: 0 });
  }, [active]);

  return (
    <div className="min-h-svh bg-muted/40 md:grid md:grid-cols-[232px_1fr]">
      <aside className="sticky top-0 hidden h-svh flex-col gap-6 border-r bg-background px-3 py-5 md:flex">
        <div className="px-2">
          <Logo />
        </div>
        <nav aria-label="Sezioni" className="flex flex-col gap-0.5">
          {NAV.map((n) => {
            const on = n.to === active.to;
            return (
              <Link
                key={n.to}
                to={n.to}
                aria-current={on ? "page" : undefined}
                className={cn(
                  "flex items-center gap-3 rounded-lg px-2.5 py-2 text-sm font-medium text-muted-foreground transition-colors hover:bg-muted hover:text-foreground",
                  on && "bg-muted text-foreground",
                )}
              >
                <n.icon className="size-4" /> {n.label}
              </Link>
            );
          })}
        </nav>
        <div className="mt-auto flex items-center gap-2">
          <SyncButton />
          <ThemeMenu theme={theme} setTheme={setTheme} />
        </div>
      </aside>

      <div className="flex min-w-0 flex-col">
        <header className="sticky top-0 z-30 flex h-14 items-center justify-between border-b bg-background/85 px-4 backdrop-blur md:hidden">
          <Logo />
          <div className="flex items-center gap-1">
            <ThemeMenu theme={theme} setTheme={setTheme} />
            <SyncButton compact />
          </div>
        </header>

        <main id="main" className="mx-auto flex w-full max-w-6xl flex-col gap-6 px-4 pt-6 pb-28 md:px-8 md:pt-8 md:pb-12">
          {route.path === "/movimenti" ? <Movimenti /> : route.path === "/analisi" ? <Analisi /> : route.path === "/categorie" ? <Categorie /> : route.path === "/conti" ? <Conti /> : <Overview />}
        </main>
      </div>

      <nav aria-label="Sezioni" className="pb-safe fixed inset-x-0 bottom-0 z-30 grid grid-cols-5 border-t bg-background/95 backdrop-blur md:hidden">
        {NAV.map((n) => {
          const on = n.to === active.to;
          return (
            <Link key={n.to} to={n.to} aria-current={on ? "page" : undefined} className={cn("flex flex-col items-center gap-1 py-2 text-[11px] font-medium text-muted-foreground", on && "text-foreground")}>
              <span className={cn("flex h-7 w-12 items-center justify-center rounded-full transition-colors", on && "bg-muted")}>
                <n.icon className="size-5" />
              </span>
              {n.label}
            </Link>
          );
        })}
      </nav>

      <Toaster theme={theme} position="top-center" richColors closeButton />
    </div>
  );
}
