import { useEffect } from "react";
import { Link, useRoute } from "./lib/router";
import { useApp, useTheme, type Theme } from "./lib/app-state";
import { Icon } from "./components/Icon";
import { Overview } from "./views/Overview";
import { Movimenti } from "./views/Movimenti";
import { Analisi } from "./views/Analisi";
import { Conti } from "./views/Conti";

const NAV = [
  { to: "/", label: "Panoramica", icon: "home" },
  { to: "/movimenti", label: "Movimenti", icon: "list" },
  { to: "/analisi", label: "Analisi", icon: "chart" },
  { to: "/conti", label: "Conti", icon: "bank" },
] as const;

const THEME_NEXT: Record<Theme, Theme> = { auto: "light", light: "dark", dark: "auto" };
const THEME_LABEL: Record<Theme, string> = { auto: "Tema: automatico", light: "Tema: chiaro", dark: "Tema: scuro" };
const THEME_ICON: Record<Theme, string> = { auto: "auto", light: "sun", dark: "moon" };

export function App() {
  const route = useRoute();
  const { sync, runSync } = useApp();
  const [theme, setTheme] = useTheme();
  const active = NAV.find((n) => n.to === route.path) ?? NAV[0];

  useEffect(() => {
    document.title = active.to === "/" ? "fine" : `${active.label} · fine`;
    if (!window.location.hash) window.scrollTo({ top: 0 });
  }, [active]);

  const themeButton = (
    <button type="button" className="btn ghost sm" onClick={() => setTheme(THEME_NEXT[theme])} aria-label={`${THEME_LABEL[theme]}. Cambia tema`} title={THEME_LABEL[theme]}>
      <Icon name={THEME_ICON[theme]} size={18} />
      <span className="theme-text">{THEME_LABEL[theme].replace("Tema: ", "")}</span>
    </button>
  );

  return (
    <div className="shell">
      <aside className="sidebar">
        <div className="brand">
          <span className="logo" aria-hidden="true">f</span>
          fine
        </div>
        <nav aria-label="Sezioni">
          {NAV.map((n) => (
            <Link key={n.to} to={n.to} aria-current={n.to === active.to ? "page" : undefined}>
              <Icon name={n.icon} size={20} /> {n.label}
            </Link>
          ))}
        </nav>
        <div className="spacer" />
        <div className="foot">
          <button type="button" className="btn" onClick={runSync} disabled={sync.running} title="Scarica i nuovi movimenti (max 4 volte al giorno per conto)">
            <Icon name="sync" size={16} className={sync.running ? "spin" : undefined} />
            {sync.running ? "Sincronizzo…" : "Sincronizza ora"}
          </button>
          {themeButton}
        </div>
      </aside>

      <header className="topbar">
        <div className="brand">
          <span className="logo" aria-hidden="true">f</span>
          fine
          <small>{active.to === "/" ? "" : active.label}</small>
        </div>
        <div className="inline" style={{ gap: 4 }}>
          <button type="button" className="btn ghost icon" onClick={() => setTheme(THEME_NEXT[theme])} aria-label={`${THEME_LABEL[theme]}. Cambia tema`} title={THEME_LABEL[theme]}>
            <Icon name={THEME_ICON[theme]} size={20} />
          </button>
          <button type="button" className="btn ghost icon" onClick={runSync} disabled={sync.running} aria-label={sync.running ? "Sincronizzazione in corso" : "Sincronizza ora"} title="Sincronizza ora">
            <Icon name="sync" size={20} className={sync.running ? "spin" : undefined} />
          </button>
        </div>
      </header>

      <main className="content" id="main">
        {route.path === "/movimenti" ? <Movimenti /> : route.path === "/analisi" ? <Analisi /> : route.path === "/conti" ? <Conti /> : <Overview />}
      </main>

      <nav className="tabbar" aria-label="Sezioni">
        {NAV.map((n) => (
          <Link key={n.to} to={n.to} aria-current={n.to === active.to ? "page" : undefined}>
            <Icon name={n.icon} size={22} />
            {n.label}
          </Link>
        ))}
      </nav>
    </div>
  );
}
