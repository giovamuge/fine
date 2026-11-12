import { useCallback, useEffect, useRef, useState, type DependencyList } from "react";
import { errorMessage } from "./api";

export interface AsyncState<T> {
  data: T | null;
  /** The last error; when `data` is present too, the data is stale. */
  error: string | null;
  loading: boolean;
  reload: () => void;
}

/** Loads data, keeps the previous value while reloading and never hides a failed refresh. */
export function useAsync<T>(fn: () => Promise<T>, deps: DependencyList): AsyncState<T> {
  const [data, setData] = useState<T | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [tick, setTick] = useState(0);
  const seq = useRef(0);

  useEffect(() => {
    const id = ++seq.current;
    setLoading(true);
    fn().then(
      (d) => {
        if (id !== seq.current) return;
        setData(d);
        setError(null);
        setLoading(false);
      },
      (e: unknown) => {
        if (id !== seq.current) return;
        setError(errorMessage(e));
        setLoading(false);
      },
    );
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [...deps, tick]);

  const reload = useCallback(() => setTick((t) => t + 1), []);
  return { data, error, loading, reload };
}
