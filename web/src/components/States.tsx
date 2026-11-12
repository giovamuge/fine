import type { ReactNode } from "react";
import { Icon } from "./Icon";

export function Skeleton({ rows = 3 }: { rows?: number }) {
  return (
    <div className="skeleton" aria-busy="true" aria-label="Caricamento">
      {Array.from({ length: rows }, (_, i) => (
        <i key={i} style={{ width: `${90 - (i % 3) * 18}%` }} />
      ))}
    </div>
  );
}

export function ErrorState({ message, onRetry }: { message: string; onRetry?: () => void }) {
  return (
    <div className="error-state" role="alert">
      <span className="neg-text">
        <Icon name="alert" size={16} /> {message}
      </span>
      {onRetry && (
        <button type="button" className="btn sm" onClick={onRetry}>
          Riprova
        </button>
      )}
    </div>
  );
}

/** Shown above stale data when a refresh failed: never hide that silently. */
export function StaleBanner({ message, onRetry }: { message: string; onRetry: () => void }) {
  return (
    <div className="banner" role="alert">
      <Icon name="alert" size={18} />
      <span>
        <strong>Dati non aggiornati.</strong> {message}
      </span>
      <button type="button" className="btn sm" onClick={onRetry}>
        Riprova
      </button>
    </div>
  );
}

export function Empty({ title, children }: { title: string; children?: ReactNode }) {
  return (
    <div className="empty">
      <strong>{title}</strong>
      {children && <span className="hint">{children}</span>}
    </div>
  );
}

/** Wraps the three states of a loaded section. */
export function Loaded<T>({
  data,
  error,
  loading,
  reload,
  rows,
  children,
}: {
  data: T | null;
  error: string | null;
  loading: boolean;
  reload: () => void;
  rows?: number;
  children: (data: T) => ReactNode;
}) {
  if (data === null) {
    if (error) return <ErrorState message={error} onRetry={reload} />;
    return <Skeleton rows={rows ?? 3} />;
  }
  return (
    <>
      {error && <StaleBanner message={error} onRetry={reload} />}
      <div className={loading ? "loading" : undefined}>{children(data)}</div>
    </>
  );
}
