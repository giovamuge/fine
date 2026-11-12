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

  const refresh = useCallback(() => setVersion((v) => v + 1), []);

  const reloadCategories = useCallback(() => {
    api.categories().then(setCategories, () => {});
  }, []);
  useEffect(reloadCategories, [reloadCategories]);

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
      reloadCategories();
    }
  }, [refresh, reloadCategories, toast]);

  const clearSyncResult = useCallback(() => setSync((s) => ({ ...s, result: null, error: null })), []);

  const value = useMemo<AppState>(
    () => ({ version, refresh, categories, reloadCategories, sync, runSync, clearSyncResult }),
    [version, refresh, categories, reloadCategories, sync, runSync, clearSyncResult],
  );
  return <Ctx.Provider value={value}>{children}</Ctx.Provider>;
}

export function useApp(): AppState {
  const v = useContext(Ctx);
  if (!v) throw new Error("AppStateProvider mancante");
  return v;
}

// ---------- theme ----------
export type Theme = "auto" | "light" | "dark";
const THEME_KEY = "fine.theme";

export function readTheme(): Theme {
  try {
    const t = localStorage.getItem(THEME_KEY);
    return t === "light" || t === "dark" ? t : "auto";
  } catch {
    return "auto";
  }
}

export function applyTheme(t: Theme) {
  const root = document.documentElement;
  if (t === "auto") root.removeAttribute("data-theme");
  else root.setAttribute("data-theme", t);
  try {
    if (t === "auto") localStorage.removeItem(THEME_KEY);
    else localStorage.setItem(THEME_KEY, t);
  } catch {
    /* private mode: ignore */
  }
}

export function useTheme(): [Theme, (t: Theme) => void] {
  const [theme, set] = useState<Theme>(readTheme);
  useEffect(() => applyTheme(theme), [theme]);
  return [theme, set];
}
