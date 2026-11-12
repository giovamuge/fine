// App-wide state: a "data version" that views re-fetch on (after any mutation), the
// category list, the bank sync (one at a time, visible everywhere) and the theme.
import { createContext, useCallback, useContext, useEffect, useMemo, useState, type ReactNode } from "react";
import { api, errorMessage } from "./api";
import type { CategoryView, SyncOutcome } from "./types";
import { useToast } from "./toast";

export interface SyncState {
  running: boolean;
  result: SyncOutcome | null;
  error: string | null;
  /** When the last result was produced. */
  at: Date | null;
}

interface AppState {
  version: number;
  refresh: () => void;
  categories: CategoryView[];
  categoryByName: Map<string, CategoryView>;
  reloadCategories: () => void;
  sync: SyncState;
  runSync: () => Promise<void>;
  clearSyncResult: () => void;
}

const Ctx = createContext<AppState | null>(null);

export function AppStateProvider({ children }: { children: ReactNode }) {
  const [version, setVersion] = useState(0);
  const [categories, setCategories] = useState<CategoryView[]>([]);
  const [sync, setSync] = useState<SyncState>({ running: false, result: null, error: null, at: null });
  const toast = useToast();

  const reloadCategories = useCallback(() => {
    api.categories().then(setCategories, () => {});
  }, []);
  useEffect(reloadCategories, [reloadCategories]);

  // Every mutation goes through refresh(): categories may have changed too (counts, names).
  const refresh = useCallback(() => {
    setVersion((v) => v + 1);
    reloadCategories();
  }, [reloadCategories]);

  const runSync = useCallback(async () => {
    setSync((s) => (s.running ? s : { ...s, running: true, error: null }));
    try {
      const result = await api.sync();
      setSync({ running: false, result, error: null, at: new Date() });
      const failed = result.sync.accounts.filter((a) => a.error).length;
      const inserted = result.sync.accounts.reduce((n, a) => n + a.inserted, 0);
      if (failed) toast.error(`Sincronizzazione con errori: ${failed} ${failed === 1 ? "conto non aggiornato" : "conti non aggiornati"}.`);
      else toast.success(inserted ? `Sincronizzato: ${inserted} nuovi movimenti.` : "Sincronizzato: nessun nuovo movimento.");
    } catch (e) {
      const error = errorMessage(e);
      setSync({ running: false, result: null, error, at: new Date() });
      toast.error(`Sincronizzazione fallita: ${error}`);
    } finally {
      refresh();
    }
  }, [refresh, toast]);

  const clearSyncResult = useCallback(() => setSync((s) => ({ ...s, result: null, error: null })), []);

  const categoryByName = useMemo(() => new Map(categories.map((c) => [c.name.toLowerCase(), c])), [categories]);

  const value = useMemo<AppState>(
    () => ({ version, refresh, categories, categoryByName, reloadCategories, sync, runSync, clearSyncResult }),
    [version, refresh, categories, categoryByName, reloadCategories, sync, runSync, clearSyncResult],
  );
  return <Ctx.Provider value={value}>{children}</Ctx.Provider>;
}

export function useApp(): AppState {
  const v = useContext(Ctx);
  if (!v) throw new Error("AppStateProvider mancante");
  return v;
}

// ---------- theme ----------
export type Theme = "system" | "light" | "dark";
const THEME_KEY = "fine.theme";
const dark = () => window.matchMedia("(prefers-color-scheme: dark)");

export function readTheme(): Theme {
  try {
    const t = localStorage.getItem(THEME_KEY);
    return t === "light" || t === "dark" ? t : "system";
  } catch {
    return "system";
  }
}

export function applyTheme(t: Theme) {
  const isDark = t === "dark" || (t === "system" && dark().matches);
  document.documentElement.classList.toggle("dark", isDark);
  document.documentElement.style.colorScheme = isDark ? "dark" : "light";
  try {
    if (t === "system") localStorage.removeItem(THEME_KEY);
    else localStorage.setItem(THEME_KEY, t);
  } catch {
    /* private mode: ignore */
  }
}

export function useTheme(): [Theme, (t: Theme) => void] {
  const [theme, set] = useState<Theme>(readTheme);
  useEffect(() => {
    applyTheme(theme);
    if (theme !== "system") return;
    const mq = dark();
    const on = () => applyTheme("system");
    mq.addEventListener("change", on);
    return () => mq.removeEventListener("change", on);
  }, [theme]);
  return [theme, set];
}
