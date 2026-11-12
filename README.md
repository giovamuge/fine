# fine

**English** · [Italiano](README.it.md)

**Personal finance on your own bank data, with your own AI.**
`fine` downloads your account transactions through [Enable Banking](https://enablebanking.com) (PSD2, read-only) and stores them **locally** (SQLite). It categorizes them with rules that learn on their own and tells you every day **how much you can still spend**.

Its "brain" is your **Claude** or **ChatGPT** subscription, through [MCP](https://modelcontextprotocol.io): no paid API needed.

- 🏦 **Incremental sync**: your history stays on your disk, and analyses don't use up the 4 daily reads PSD2 allows.
- 🏷️ **Categorization that learns**: built-in rules for the main Italian merchants, plus the rules you or your AI create with `categorize`.
- 🤖 **Background AI categorization**: after each sync, transactions no rule recognizes are passed to `claude -p` (or `codex exec`) using your subscription. The AI has no tools and only returns categories, which `fine` validates; when it is confident it creates rules, so next time it isn't needed. Doubtful cases (e.g. transfers to people) are left to you. Manually: `fine categorize`.
- 💶 **Assistant**: what's left to spend this month, this week and today. It accounts for income, fixed expenses, savings for annual expenses and your savings goal.
- 🚨 **Alerts**:
  - budgets exceeded or spent too fast;
  - upcoming annual expenses;
  - new recurring subscriptions;
  - unusual expenses;
  - bank consent about to expire.
- 🏷️ **Free-form tags**: as many as you want per transaction (`#london-trip`, `#work`, `#reimbursable`, a person...), independent of the category. Totals per tag, filters, and rules that tag future transactions automatically.
- 📈 **Balance over time**: current balance per account and daily history, rebuilt from stored transactions, with the change over 7/30/90/365 days.
- 📆 **Amortization**: a large expense can be spread over N months. Annual expenses become monthly amounts to set aside.
- 📊 **Mobile-first web dashboard**, embedded in the executable with no external requests, and **push notifications** via [ntfy](https://ntfy.sh).
- 🦀 **A single Rust executable**, a few MB of RAM: it runs on a Raspberry Pi too.

> `fine` never moves money: it only uses read access to your accounts (AIS).

The dashboard, notifications and default categories are in Italian; your AI answers in whatever language you use.

## Installation

```sh
curl -fsSL https://raw.githubusercontent.com/giovamuge/fine/main/install.sh | sh
```

It downloads the latest release for macOS or Linux (Intel/ARM), verifies its checksum and puts it in `~/.local/bin`. Alternatively, build from source (you need Rust from [rustup.rs](https://rustup.rs) and Node.js for the dashboard):

```sh
cargo install --git https://github.com/giovamuge/fine
```

## Quick start: one command

```sh
fine init
```

The guided setup does everything:

1. **Bank**: it only asks for your Enable Banking `.pem` file (the app id comes from the file name, the redirect URL from the application), then you pick your bank from a list and the browser opens by itself.
2. **First download** of your transactions, with automatic categorization.
3. **Registration** of `fine` in Claude Code, Codex and Claude Desktop (the Claude Desktop config is backed up first).
4. **Automatic sync every 6 hours** (01:07, 07:07, 13:07, 19:07) to cover the whole day within the PSD2 limit of 4 reads per day. Optionally, a **phone notification** at 08:12.

First you need an Enable Banking application. It's free for personal use:
- create a **Production** one on [enablebanking.com](https://enablebanking.com) and register a redirect URL, e.g. `https://localhost/fine` (it doesn't need to respond);
- link your accounts from the control panel: the app is activated in *restricted mode*, for **your** accounts only;
- download the `.pem` key.

PSD2 consent lasts at most 180 days, depending on the bank. `fine` warns you two weeks before it expires; to renew it, just run `fine connect`.

If a bank grants less history than requested, `fine` starts from 90 days and, if needed, retries with 30 by itself. A failed sync never goes unnoticed: it becomes a critical alert in `fine status`, in the dashboard, in notifications and in the AI's answers.

## Using it with your AI (MCP)

**Claude Code, as a plugin** (recommended): it registers the MCP server by itself and updates with `claude plugin update`.
```
/plugin install fine --marketplace giovamuge/fine
```
Answer `y` to add the marketplace and choose the scope. The plugin doesn't contain the executable: install `fine` first (above). In the desktop app: **+** next to the prompt › Plugins › Add marketplace `giovamuge/fine` › Add plugin.

**Claude Code, manually**
```sh
claude mcp add fine -- fine mcp
```

**Claude Desktop**: add to `claude_desktop_config.json`:
```json
{ "mcpServers": { "fine": { "command": "/path/to/fine", "args": ["mcp"] } } }
```

**Codex CLI (ChatGPT)**: add to `~/.codex/config.toml`:
```toml
[mcp_servers.fine]
command = "fine"
args = ["mcp"]
```

Then ask, for example:
- *"How am I doing this month? How much can I spend this week?"*
- *"Categorize the pending transactions"*: there's also the `categorize-pending` prompt.
- *"Help me set up my monthly plan"*, with the `monthly-plan` prompt: salary, rent, bills, annual expenses, budgets, savings goal.
- *"Amortize the laptop purchase over 24 months"*
- *"Which subscriptions could I cancel?"*
- *"Tag everything from August 1 to 7 as london-trip and tell me how much it cost"*
- *"How has my balance changed over the last 3 months?"*

**Available tools:** `status`, `list_accounts`, `sync`, `search_transactions`, `monthly_summary`, `trend`, `list_uncategorized`, `categorize`, `list_categories`, `upsert_category`, `update_category`, `delete_category`, `list_rules`, `delete_rule`, `set_budget`, `budget_status`, `recurring`, `upsert_planned`, `delete_planned`, `list_planned`, `sinking_funds`, `amortize`, `set_savings_goal`, `tag_transactions`, `untag_transactions`, `list_tags`, `delete_tag`, `balance_history`.

## Dashboard

```sh
fine serve                       # http://127.0.0.1:7878
```

`fine init` and `fine schedule` also install it as an always-on service (it starts at login and restarts if it stops; it uses about ten MB of memory and no CPU when idle): just open http://127.0.0.1:7878. To opt out: `fine schedule --no-dashboard`.

To open it from your phone, the recommended way is [Tailscale](https://tailscale.com) (free): set a password and expose it on your private network.

```sh
fine setup --dashboard-password '<at least 12 characters>'
fine serve --bind 0.0.0.0:7878   # reachable as http://<mac-name>:7878 in the tailnet
```

On any address other than localhost, `fine` refuses to start without a password. Don't expose it directly to the Internet.

The dashboard is built with React and [shadcn/ui](https://ui.shadcn.com). It's designed for the phone (bottom navigation bar, bottom sheets) but works well on desktop too; it follows the system light/dark theme (with a toggle to force it), is keyboard-navigable and makes no external requests: everything is embedded in the executable. Five sections:

- **Overview** (*Panoramica*): what's left to spend this month front and center, with week and day; the spending-pace bar (the marker shows where you should be today); alerts (a failed sync is impossible to miss); balance with history and 7/30/90/365-day changes; the budgets closest to their limit.
- **Transactions** (*Movimenti*): instant search; filters by period, category, tag and "uncategorized" (all in the URL, so they're shareable and work with "back"); category editable right in the row; per-transaction detail with the explicit choice *"also use this category for future transactions from this merchant"*, tags with suggestions (and an optional automatic rule), amortization over N months; multi-select to categorize or tag many transactions at once.
- **Analysis** (*Analisi*): spending by category for the month (each row links to its transactions), income/expenses over the last 12 months, balance over time with tooltips and a table view, budgets, savings for annual expenses, totals per tag by period, recurring expenses.
- **Categories** (*Categorie*): 35 built-in categories (expenses, income, transfers, savings), all customizable: name, kind, icon (any emoji) and color; for each, the automatic rules that assign it, deletable one by one. A category is deleted either by moving its transactions, rules and budgets into another one (a merge) or by leaving them uncategorized, for the AI to try again. A new category can also be created on the fly from the transaction picker.
- **Accounts** (*Conti*): one card per account with balance, IBAN, consent expiry (warning from 14 days before), result of the last sync with error details; linking a new bank in four guided steps (choose, authorize on the bank's page, paste the final URL, done); **consent renewal** (history is kept, no duplicates); rename; removal with explicit confirmation (it deletes the account's transactions from this computer; consent is revoked at the bank if no other account needs it); and "Sync now" with a summary of the result.

Descriptions coming from the bank are always shown as text, never interpreted as HTML; scripts are served only by the executable (`Content-Security-Policy: script-src 'self'`).

## Automatic updates and notifications

`fine init` already installs everything. To manage it manually:

```sh
fine schedule                  # sync every 6 hours, always-on dashboard (+ notification if ntfy is configured)
fine schedule --no-dashboard   # same, without the background dashboard
fine schedule --remove         # removes everything
```

- **macOS**: launchd agents `com.fine.sync`, `com.fine.notify` and `com.fine.dashboard`. If the Mac is asleep at the scheduled time, the sync runs when it wakes up.
- **Linux**: crontab lines marked `# fine` (the dashboard with `@reboot`).
- **Safety net**: `fine status`, the MCP `status` tool and the dashboard sync by themselves if the data is more than 6 hours old.
- **Logs**: in `~/.fine/schedule.log`.
- **Notifications**: with ntfy you get them in the free app, by subscribing to the secret topic generated by `fine init`.

For a natural-language briefing, using your subscription rather than an API:
```sh
claude -p "Use fine: give me today's financial briefing in 5 lines"
```

## How "left to spend" is computed

```
income for the month (received + expected)
− fixed expenses for the month (planned weekly/monthly, paid or still due)
− 1/12 of each planned annual expense (set aside)
− savings goal
= variable budget
− variable expenses so far
= left to spend  →  ÷ days remaining = left to spend per day
```

**Income:** if you haven't planned your salary, `fine` uses the average of the last 3 months.

**Annual expenses:** paying a planned annual expense (e.g. insurance) doesn't sink the month it lands in. It's already covered by the monthly amounts.

**Amortization:** an amortized purchase weighs 1/N per month.

## Privacy and security

- Data stays in `~/.fine/`, or in `FINE_DATA_DIR` if you set it, with 0600 permissions.
- Analyses (budgets, left to spend, trends) only use **EUR** transactions. Accounts in other currencies are still synced and can be browsed.
- When you use a cloud AI, the transactions it reads are sent to its provider. To keep everything local, use an MCP client with an Ollama model.
- Transaction descriptions are untrusted text: `fine` returns them as data and tells the AI never to follow instructions found in them.

## Development

```sh
cargo test && cargo clippy --all-targets -- -D warnings
```

**Frontend.** The dashboard is a React + TypeScript app (Vite, Tailwind CSS 4, [shadcn/ui](https://ui.shadcn.com) components in `web/src/components/ui`, added with `npx shadcn@latest add <name>`) in `web/`. Building `fine` also needs **Node.js (LTS) with npm**: `build.rs` runs `npm ci` and `npm run build` by itself when the sources in `web/` change, and the output (`web/dist`, ignored by git) is embedded in the executable. Without Node you can still build with `FINE_SKIP_WEB_BUILD=1` (the dashboard shows a placeholder page; CLI and MCP are unaffected).

To work on the frontend with hot reload:

```sh
fine serve                     # the Rust API on http://127.0.0.1:7878
cd web && npm install && npm run dev   # Vite on http://localhost:5173, proxying /api to the Rust server
```

If the Rust server listens elsewhere: `FINE_API=http://127.0.0.1:7899 npm run dev`. Type checking: `npm run check` (`tsc --noEmit`, also in CI).

**Release.** A `vX.Y.Z` tag triggers `.github/workflows/release.yml`: it builds the binaries (macOS and Linux, Intel and ARM, dashboard included) and publishes them as a GitHub release, which `install.sh` downloads from. Before tagging, align the version in `Cargo.toml` and `.claude-plugin/plugin.json`.

**Claude Code plugin.** The repository is also a marketplace: `.claude-plugin/marketplace.json` lists the `fine` plugin (the repo root), `.claude-plugin/plugin.json` registers the MCP server through `scripts/fine-mcp.sh`, which finds the executable even outside the PATH. Validate with `claude plugin validate .`.

Inspired by [BankMCP](https://github.com/noskillish/bankmcp) (MIT). License: [MIT](LICENSE).
