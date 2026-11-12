import { createContext, useCallback, useContext, useMemo, useRef, useState, type ReactNode } from "react";
import { Icon } from "../components/Icon";

export interface ToastAction {
  label: string;
  onClick: () => void;
}

export interface Toast {
  id: number;
  text: string;
  kind: "info" | "success" | "error";
  action?: ToastAction;
}

interface ToastApi {
  show: (text: string, kind?: Toast["kind"], action?: ToastAction, ms?: number) => void;
  error: (text: string) => void;
  success: (text: string, action?: ToastAction) => void;
}

const Ctx = createContext<ToastApi>({ show: () => {}, error: () => {}, success: () => {} });

export function ToastProvider({ children }: { children: ReactNode }) {
  const [toasts, setToasts] = useState<Toast[]>([]);
  const seq = useRef(0);
  const timers = useRef(new Map<number, number>());
  const box = useRef<HTMLDivElement>(null);

  // The container is a manual popover: it lives in the top layer, so toasts stay visible
  // above open <dialog>s. Re-showing it on each toast puts it back on top of the stack.
  const raise = () => {
    const el = box.current;
    if (!el || typeof el.showPopover !== "function") return;
    try {
      if (el.matches(":popover-open")) el.hidePopover();
      el.showPopover();
    } catch {
      /* unsupported: the container still renders as a normal fixed element */
    }
  };

  const dismiss = useCallback((id: number) => {
    setToasts((t) => t.filter((x) => x.id !== id));
    const h = timers.current.get(id);
    if (h) window.clearTimeout(h);
    timers.current.delete(id);
  }, []);

  const show = useCallback(
    (text: string, kind: Toast["kind"] = "info", action?: ToastAction, ms?: number) => {
      const id = ++seq.current;
      const toast: Toast = action ? { id, text, kind, action } : { id, text, kind };
      setToasts((t) => [...t.slice(-3), toast]);
      raise();
      const timeout = ms ?? (kind === "error" ? 9000 : action ? 8000 : 4500);
      timers.current.set(id, window.setTimeout(() => dismiss(id), timeout));
    },
    [dismiss],
  );

  const value = useMemo<ToastApi>(
    () => ({
      show,
      error: (text) => show(text, "error"),
      success: (text, action) => show(text, "success", action),
    }),
    [show],
  );

  return (
    <Ctx.Provider value={value}>
      {children}
      <div className="toasts" ref={box} popover="manual" aria-live="polite" aria-relevant="additions">
        {toasts.map((t) => (
          <div key={t.id} className={`toast ${t.kind}`} role={t.kind === "error" ? "alert" : "status"}>
            <Icon name={t.kind === "error" ? "alert" : t.kind === "success" ? "check" : "info"} size={18} />
            <span>{t.text}</span>
            {t.action && (
              <button
                type="button"
                className="btn"
                onClick={() => {
                  t.action?.onClick();
                  dismiss(t.id);
                }}
              >
                {t.action.label}
              </button>
            )}
            <button type="button" className="x" aria-label="Chiudi avviso" onClick={() => dismiss(t.id)}>
              <Icon name="x" size={16} />
            </button>
          </div>
        ))}
      </div>
    </Ctx.Provider>
  );
}

export function useToast(): ToastApi {
  return useContext(Ctx);
}
