// A tiny history-API router: four routes, query strings for filters. The Rust server
// answers index.html for every non-/api path, so deep links work.
import { createContext, useContext, useEffect, useState, type ComponentProps, type MouseEvent, type ReactNode } from "react";

export interface Route {
  path: string;
  query: URLSearchParams;
}

const RouteCtx = createContext<Route>({ path: "/", query: new URLSearchParams() });

function current(): Route {
  return { path: normalize(window.location.pathname), query: new URLSearchParams(window.location.search) };
}

function normalize(p: string): string {
  const s = p.replace(/\/+$/, "");
  return s === "" ? "/" : s;
}

export function navigate(to: string, opts: { replace?: boolean } = {}) {
  if (opts.replace) history.replaceState(null, "", to);
  else history.pushState(null, "", to);
  window.dispatchEvent(new PopStateEvent("popstate"));
}

export function RouterProvider({ children }: { children: ReactNode }) {
  const [route, setRoute] = useState<Route>(current);
  useEffect(() => {
    const on = () => setRoute(current());
    window.addEventListener("popstate", on);
    return () => window.removeEventListener("popstate", on);
  }, []);
  return <RouteCtx.Provider value={route}>{children}</RouteCtx.Provider>;
}

export function useRoute(): Route {
  return useContext(RouteCtx);
}

export function Link({ to, onClick, ...rest }: Omit<ComponentProps<"a">, "href"> & { to: string }) {
  const handle = (e: MouseEvent<HTMLAnchorElement>) => {
    onClick?.(e);
    if (e.defaultPrevented || e.button !== 0 || e.metaKey || e.ctrlKey || e.shiftKey || e.altKey) return;
    e.preventDefault();
    navigate(to);
  };
  return <a href={to} onClick={handle} {...rest} />;
}

export function href(path: string, params: Record<string, string | undefined>): string {
  const q = new URLSearchParams();
  for (const [k, v] of Object.entries(params)) if (v) q.set(k, v);
  const s = q.toString();
  return s ? `${path}?${s}` : path;
}
