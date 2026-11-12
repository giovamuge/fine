import type {
  AccountView,
  BalanceHistory,
  CategorizeOutcome,
  CategoryView,
  ConnectedAccount,
  MonthSummary,
  PendingConnect,
  Recurring,
  RemoveOutcome,
  SearchParams,
  SinkingFund,
  Status,
  SyncOutcome,
  TagOutcome,
  TagView,
  TrendPoint,
  TxView,
} from "./types";

export class ApiError extends Error {
  status: number;
  constructor(message: string, status: number) {
    super(message);
    this.name = "ApiError";
    this.status = status;
  }
}

async function request<T>(path: string, body?: unknown): Promise<T> {
  let res: Response;
  try {
    res = await fetch(path, body === undefined
      ? { headers: { Accept: "application/json" } }
      : { method: "POST", headers: { "Content-Type": "application/json", Accept: "application/json" }, body: JSON.stringify(body) });
  } catch {
    throw new ApiError("Impossibile raggiungere fine: controlla che `fine serve` sia in esecuzione.", 0);
  }
  let json: unknown = null;
  const text = await res.text();
  if (text) {
    try {
      json = JSON.parse(text);
    } catch {
      json = null;
    }
  }
  if (!res.ok) {
    const msg =
      json && typeof json === "object" && typeof (json as { error?: unknown }).error === "string"
        ? (json as { error: string }).error
        : res.status === 401
          ? "Autenticazione richiesta: ricarica la pagina e inserisci la password."
          : `Errore ${res.status}`;
    throw new ApiError(msg, res.status);
  }
  return json as T;
}

function qs(params: Record<string, string | number | boolean | undefined>): string {
  const p = new URLSearchParams();
  for (const [k, v] of Object.entries(params)) {
    if (v === undefined || v === "" || v === false) continue;
    p.set(k, String(v));
  }
  const s = p.toString();
  return s ? `?${s}` : "";
}

export const api = {
  status: () => request<Status>("/api/status"),
  balance: (days = 365) => request<BalanceHistory>(`/api/balance${qs({ days })}`),
  accounts: () => request<AccountView[]>("/api/accounts"),
  renameAccount: (uid: string, label: string | null) => request<string>("/api/accounts/rename", { uid, label }),
  removeAccount: (uid: string) => request<RemoveOutcome>("/api/accounts/remove", { uid }),
  banks: (country: string) => request<string[]>(`/api/banks${qs({ country })}`),
  connect: (bank: string, country: string, days?: number) =>
    request<PendingConnect>("/api/connect", { bank, country, days }),
  connectFinish: (url: string) => request<ConnectedAccount[]>("/api/connect/finish", { url }),
  sync: () => request<SyncOutcome>("/api/sync", {}),
  summary: (month?: string) => request<MonthSummary>(`/api/summary${qs({ month })}`),
  trend: () => request<TrendPoint[]>("/api/trend"),
  recurring: () => request<Recurring[]>("/api/recurring"),
  sinking: () => request<SinkingFund[]>("/api/sinking"),
  categories: () => request<CategoryView[]>("/api/categories"),
  transactions: (p: SearchParams) =>
    request<TxView[]>(
      `/api/transactions${qs({
        text: p.text,
        category: p.category,
        tag: p.tag,
        uncategorized_only: p.uncategorized_only,
        from: p.from,
        to: p.to,
        limit: p.limit,
      })}`,
    ),
  categorize: (transaction_ids: number[], category: string, learn_rule: boolean, pattern?: string) =>
    request<CategorizeOutcome>("/api/categorize", { transaction_ids, category, learn_rule, pattern }),
  amortize: (transaction_id: number, months: number) => request<string>("/api/amortize", { transaction_id, months }),
  tags: (from?: string, to?: string) => request<TagView[]>(`/api/tags${qs({ from, to })}`),
  tag: (transaction_ids: number[], tags: string[], learn_rule: boolean, pattern?: string) =>
    request<TagOutcome>("/api/tag", { transaction_ids, tags, learn_rule, pattern }),
  untag: (transaction_ids: number[], tags: string[]) =>
    request<TagOutcome>("/api/untag", { transaction_ids, tags, learn_rule: false }),
};

export function errorMessage(e: unknown): string {
  if (e instanceof Error) return e.message;
  return String(e);
}
