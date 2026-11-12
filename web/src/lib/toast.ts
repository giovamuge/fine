// Thin wrapper over sonner: same call sites as before (toast.success(msg, action)).
import { toast as sonner } from "sonner";

export interface ToastAction {
  label: string;
  onClick: () => void;
}

const api = {
  success: (message: string, action?: ToastAction) => sonner.success(message, action ? { action, duration: 8000 } : undefined),
  error: (message: string) => sonner.error(message, { duration: 10000 }),
  info: (message: string) => sonner(message),
};

export function useToast() {
  return api;
}
