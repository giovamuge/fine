// JSON shapes of the Rust API (src/*.rs structs that derive Serialize).
// Amounts are euro floats, negative = money out; dates are YYYY-MM-DD.

export type AlertLevel = "info" | "warning" | "critical";

export interface Alert {
  level: AlertLevel;
  message: string;
}

export type BudgetState = "ok" | "attenzione" | "sforato";

export interface BudgetStatus {
  category: string;
  period: "week" | "month" | "year";
  limit: number;
  spent: number;
  remaining: number;
  used: number;
  elapsed: number;
  state: BudgetState;
}

export interface Status {
  today: string;
  month: string;
  days_left_in_month: number;
  balance_total: number | null;
  income_received: number;
  income_expected_remaining: number;
  income_source: "planned" | "average_3_months";
  fixed_paid: number;
  fixed_still_due: number;
  sinking_fund_quota: number;
  savings_goal: number;
  variable_budget: number;
  variable_spent: number;
  available_month: number;
  available_per_day: number;
  available_week: number;
  spent_this_week: number;
  month_elapsed: number;
  budget_used: number;
  budgets: BudgetStatus[];
  uncategorized: number;
  alerts: Alert[];
}

export interface AccountBalance {
  account: string;
  balance: number;
  currency: string;
  as_of: string;
  in_total: boolean;
}

export interface BalancePoint {
  date: string;
  balance: number;
}

export interface BalanceChange {
  days: number;
  change: number;
}

export interface BalanceHistory {
  current: number | null;
  accounts: AccountBalance[];
  points: BalancePoint[];
  changes: BalanceChange[];
}

export interface AccountView {
  uid: string;
  label: string;
  custom_label: string | null;
  bank: string | null;
  country: string | null;
  iban: string | null;
  currency: string;
  balance: number | null;
  balance_currency: string | null;
  balance_at: string | null;
  consent_valid_until: string | null;
  transactions: number;
  last_sync_at: string | null;
  last_sync_ok: boolean | null;
  last_sync_error: string | null;
}

export interface RemoveOutcome {
  transactions_deleted: number;
  consent_revoked: boolean;
}

export interface PendingConnect {
  url: string;
  redirect: string;
}

export interface ConnectedAccount {
  name: string | null;
  iban: string | null;
  currency: string;
  renewed: boolean;
}

export interface AccountSync {
  account: string;
  inserted: number;
  skipped: string | null;
  error: string | null;
}

export interface SyncReport {
  accounts: AccountSync[];
  auto_categorized: number;
  auto_tagged: number;
}

export interface AiOutcome {
  backend: string;
  examined: number;
  categorized: number;
  rules_created: string[];
}

export interface SyncOutcome {
  sync: SyncReport;
  ai: AiOutcome | null;
  ai_error: string | null;
}

export interface CategoryAmount {
  category: string;
  amount: number;
  share: number;
}

export interface MonthSummary {
  month: string;
  income: number;
  expenses: number;
  saved: number;
  savings_rate: number | null;
  moved_to_savings: number;
  by_category: CategoryAmount[];
  uncategorized: number;
}

export interface TrendPoint {
  month: string;
  income: number;
  expenses: number;
  saved: number;
}

export type Cadence = "week" | "month" | "year";

export interface Recurring {
  merchant: string;
  category: string | null;
  cadence: Cadence;
  typical_amount: number;
  monthly_equivalent: number;
  occurrences: number;
  last_date: string;
  next_expected: string;
  active: boolean;
  planned: boolean;
}

export interface SinkingFund {
  name: string;
  amount: number;
  due: string;
  months_to_due: number;
  monthly_quota: number;
  should_have_saved: number;
}

export type CategoryKind = "expense" | "income" | "transfer" | "saving";

export interface CategoryView {
  name: string;
  kind: CategoryKind;
  icon: string | null;
  color: string | null;
  transactions: number;
  rules: number;
}

export interface CategoryPatch {
  rename?: string;
  kind?: CategoryKind;
  /** "" clears it */
  icon?: string;
  /** "" clears it */
  color?: string;
}

export interface DeleteCategoryOutcome {
  transactions: number;
  moved_to: string | null;
}

export interface Rule {
  id: number;
  pattern: string;
  category: string;
  source: string;
  hits: number;
}

export interface TxView {
  id: number;
  date: string;
  amount: number;
  merchant: string;
  counterparty: string | null;
  description: string | null;
  category: string | null;
  categorized_by: string | null;
  amortized_months: number | null;
  account: string;
  tags: string[];
}

export interface TagView {
  name: string;
  transactions: number;
  spent: number;
  received: number;
  rules: string[];
}

export interface CategorizeOutcome {
  updated: number;
  rule_pattern: string | null;
  auto_categorized: number;
}

export interface TagOutcome {
  tagged: number;
  rule_pattern: string | null;
  auto_tagged: number;
}

export interface SearchParams {
  text?: string;
  category?: string;
  tag?: string;
  uncategorized_only?: boolean;
  from?: string;
  to?: string;
  limit?: number;
}
