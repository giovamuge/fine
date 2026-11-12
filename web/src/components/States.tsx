import type { ReactNode } from "react";
import { AlertTriangle, RotateCw } from "lucide-react";
import { Alert, AlertAction, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { Empty as EmptyRoot, EmptyContent, EmptyDescription, EmptyHeader, EmptyMedia, EmptyTitle } from "@/components/ui/empty";
import { Skeleton } from "@/components/ui/skeleton";
import { cn } from "@/lib/utils";

export function Rows({ rows = 3 }: { rows?: number }) {
  return (
    <div className="flex flex-col gap-3" aria-busy="true" aria-label="Caricamento">
      {Array.from({ length: rows }, (_, i) => (
        <Skeleton key={i} className="h-4" style={{ width: `${92 - (i % 3) * 17}%` }} />
      ))}
    </div>
  );
}

export function ErrorState({ message, onRetry }: { message: string; onRetry?: () => void }) {
  return (
    <Alert variant="destructive">
      <AlertTriangle />
      <AlertTitle>Non riesco a caricare questi dati</AlertTitle>
      <AlertDescription>{message}</AlertDescription>
      {onRetry && (
        <AlertAction>
          <Button size="sm" variant="outline" onClick={onRetry}>
            <RotateCw /> Riprova
          </Button>
        </AlertAction>
      )}
    </Alert>
  );
}

export function Empty({ title, children, icon, action }: { title: string; children?: ReactNode; icon?: ReactNode; action?: ReactNode }) {
  return (
    <EmptyRoot className="border border-dashed py-8">
      <EmptyHeader>
        {icon && <EmptyMedia variant="icon">{icon}</EmptyMedia>}
        <EmptyTitle>{title}</EmptyTitle>
        {children && <EmptyDescription>{children}</EmptyDescription>}
      </EmptyHeader>
      {action && <EmptyContent>{action}</EmptyContent>}
    </EmptyRoot>
  );
}

/** Wraps the three states of a loaded section; a failed refresh never hides silently. */
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
    return <Rows rows={rows ?? 3} />;
  }
  return (
    <>
      {error && (
        <Alert variant="destructive" className="mb-3">
          <AlertTriangle />
          <AlertTitle>Dati non aggiornati</AlertTitle>
          <AlertDescription>{error}</AlertDescription>
          <AlertAction>
            <Button size="sm" variant="outline" onClick={reload}>
              Riprova
            </Button>
          </AlertAction>
        </Alert>
      )}
      <div className={cn("transition-opacity", loading && "opacity-60")}>{children(data)}</div>
    </>
  );
}
