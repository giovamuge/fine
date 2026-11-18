import { useEffect } from "react";
import { ArrowLeftRight, ChartPie, Landmark, LayoutDashboard, RefreshCw, Settings, Shapes, Target } from "lucide-react";
import { Button } from "@/components/ui/button";
import { DropdownMenu, DropdownMenuContent, DropdownMenuLabel, DropdownMenuRadioGroup, DropdownMenuRadioItem, DropdownMenuSeparator, DropdownMenuTrigger } from "@/components/ui/dropdown-menu";
import { Toaster } from "@/components/ui/sonner";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { useApp, useTheme, type Theme } from "@/lib/app-state";
import { LANG, setLang, t, type Lang } from "@/lib/i18n";
import { Link, useRoute } from "@/lib/router";
import { cn } from "@/lib/utils";
import { Overview } from "@/views/Overview";
import { Movimenti } from "@/views/Movimenti";
import { Analisi } from "@/views/Analisi";
import { BudgetView } from "@/views/Budget";
import { Categorie } from "@/views/Categorie";
import { Conti } from "@/views/Conti";

// `mobile`: shown in the phone's bottom bar (five fit); Categorie is reachable from Movimenti and Analisi there.
const NAV = [
  { to: "/", label: t("Panoramica"), icon: LayoutDashboard, mobile: true },
  { to: "/movimenti", label: t("Movimenti"), icon: ArrowLeftRight, mobile: true },
  { to: "/budget", label: t("Budget"), icon: Target, mobile: true },
  { to: "/analisi", label: t("Analisi"), icon: ChartPie, mobile: true },
  { to: "/categorie", label: t("Categorie"), icon: Shapes, mobile: false },
  { to: "/conti", label: t("Conti"), icon: Landmark, mobile: true },
] as const;

function Logo() {
  return (
    <span className="flex items-center gap-2 text-[15px] font-semibold tracking-tight">
      <span className="flex size-7 items-center justify-center rounded-[9px] bg-gradient-to-b from-[#3d9bff] to-[#0071e3] text-sm font-bold text-white shadow-sm" aria-hidden="true">
        f
      </span>
      fine
    </span>
  );
}

/** Theme and language, in one place. */
function SettingsMenu({ theme, setTheme }: { theme: Theme; setTheme: (theme: Theme) => void }) {
  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button variant="ghost" size="icon-sm" className="text-muted-foreground" aria-label={t("Impostazioni")}>
          <Settings />
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end" className="w-48">
        <DropdownMenuLabel>{t("Aspetto")}</DropdownMenuLabel>
        <DropdownMenuRadioGroup value={theme} onValueChange={(v) => setTheme(v as Theme)}>
          <DropdownMenuRadioItem value="system">{t("Automatico")}</DropdownMenuRadioItem>
          <DropdownMenuRadioItem value="light">{t("Chiaro")}</DropdownMenuRadioItem>
          <DropdownMenuRadioItem value="dark">{t("Scuro")}</DropdownMenuRadioItem>
        </DropdownMenuRadioGroup>
        <DropdownMenuSeparator />
        <DropdownMenuLabel>{t("Lingua")}</DropdownMenuLabel>
        <DropdownMenuRadioGroup value={LANG} onValueChange={(v) => setLang(v as Lang)}>
          <DropdownMenuRadioItem value="it">Italiano</DropdownMenuRadioItem>
          <DropdownMenuRadioItem value="en">English</DropdownMenuRadioItem>
        </DropdownMenuRadioGroup>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}

function SyncButton({ compact }: { compact?: boolean }) {
  const { sync, runSync } = useApp();
  const label = sync.running ? t("Sincronizzo…") : t("Sincronizza");
  const icon = <RefreshCw className={cn(sync.running && "animate-spin")} />;
  if (compact) {
    return (
      <Button variant="ghost" size="icon" onClick={runSync} disabled={sync.running} aria-label={sync.running ? t("Sincronizzazione in corso") : t("Sincronizza ora")}>
        {icon}
      </Button>
    );
  }
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <Button variant="ghost" size="sm" className="min-w-0 flex-1 justify-start text-muted-foreground" onClick={runSync} disabled={sync.running}>
          {icon}
          <span className="truncate">{label}</span>
        </Button>
      </TooltipTrigger>
      <TooltipContent side="right">{t("Scarica i nuovi movimenti (max 4 volte al giorno per conto)")}</TooltipContent>
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
    <div className="min-h-svh bg-background md:grid md:grid-cols-[220px_1fr]">
      <aside className="sticky top-0 hidden h-svh min-w-0 flex-col gap-5 border-r border-sidebar-border bg-sidebar px-3 py-4 backdrop-blur-xl md:flex">
        <div className="px-2">
          <Logo />
        </div>
        <nav aria-label={t("Sezioni")} className="flex flex-col gap-0.5">
          {NAV.map((n) => {
            const on = n.to === active.to;
            return (
              <Link
                key={n.to}
                to={n.to}
                aria-current={on ? "page" : undefined}
                className={cn(
                  "flex items-center gap-2.5 rounded-lg px-2.5 py-1.5 text-[13px] text-foreground/85 transition-colors hover:bg-sidebar-accent",
                  on && "bg-sidebar-accent font-medium text-foreground",
                )}
              >
                <n.icon className="size-4 text-primary" /> {n.label}
              </Link>
            );
          })}
        </nav>
        <div className="mt-auto flex min-w-0 items-center gap-1 border-t border-sidebar-border pt-3">
          <SyncButton />
          <SettingsMenu theme={theme} setTheme={setTheme} />
        </div>
      </aside>

      <div className="flex min-w-0 flex-col">
        <header className="sticky top-0 z-30 flex h-12 items-center justify-between border-b bg-background/75 px-4 backdrop-blur-xl backdrop-saturate-150 md:hidden">
          <Logo />
          <div className="flex items-center gap-1">
            <SettingsMenu theme={theme} setTheme={setTheme} />
            <SyncButton compact />
          </div>
        </header>

        <main id="main" className="mx-auto flex w-full max-w-5xl flex-col gap-6 px-4 pt-6 pb-28 md:px-10 md:pt-10 md:pb-16">
          {route.path === "/movimenti" ? <Movimenti /> : route.path === "/budget" ? <BudgetView /> : route.path === "/analisi" ? <Analisi /> : route.path === "/categorie" ? <Categorie /> : route.path === "/conti" ? <Conti /> : <Overview />}
        </main>
      </div>

      <nav aria-label={t("Sezioni")} className="pb-safe fixed inset-x-0 bottom-0 z-30 grid grid-cols-5 border-t bg-background/75 backdrop-blur-xl backdrop-saturate-150 md:hidden">
        {NAV.filter((n) => n.mobile).map((n) => {
          const on = n.to === active.to;
          return (
            <Link key={n.to} to={n.to} aria-current={on ? "page" : undefined} className={cn("flex flex-col items-center gap-0.5 pt-2 pb-1.5 text-[10px] font-medium text-muted-foreground transition-colors", on && "text-primary")}>
              <n.icon className="size-6" strokeWidth={on ? 2.2 : 1.8} />
              {n.label}
            </Link>
          );
        })}
      </nav>

      <Toaster theme={theme} position="top-center" richColors closeButton />
    </div>
  );
}
