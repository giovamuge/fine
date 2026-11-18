# fine

**English** · [Italiano](README.it.md)

**Your AI knows how much you can still spend.** Bank transactions synced to your disk, categorized automatically, analyzed by the Claude or ChatGPT subscription you already have. Read-only, local, one Rust executable.

`fine` downloads your transactions through [Enable Banking](https://enablebanking.com) (PSD2, 2,700+ European banks), stores them in a local SQLite database and exposes them to any [MCP](https://modelcontextprotocol.io) client. No paid API, no payments, no third party holding your data.

> "How much can I spend this week?" · "Which subscriptions could I cancel?" ·
> "Amortize the laptop over 24 months." · "Budget 300 € a month for restaurants,
> excluding #work lunches."

## How it works

```
Your assistant ──MCP──▶ fine ──▶ SQLite on your disk
                         └──────▶ Enable Banking ──PSD2──▶ your bank   (4 times a day)
```

- **Sync** runs in the background every 6 hours, within the PSD2 limit of 4 reads a day. Questions are answered from the local database.
- **Categorization** uses rules; what no rule recognizes goes to `claude -p` or `codex exec`, which turns confident answers into new rules.
- **Left to spend** for the month, week and day accounts for income, fixed expenses, annual expenses, budgets and your savings goal.

## Install

```sh
curl -fsSL https://raw.githubusercontent.com/giovamuge/fine/main/install.sh | sh
```

macOS and Linux, Intel and ARM; installs to `~/.local/bin`. From source (needs Rust and Node.js):

```sh
cargo install --git https://github.com/giovamuge/fine
```

## Setup

You need a free Enable Banking account and about ten minutes.

### 1. Register an Enable Banking application

At <https://enablebanking.com/cp/applications> create an application:

- environment **Production** (your real accounts);
- redirect URL `https://localhost/fine`;
- keep *generate private key* selected, and save.

A `.pem` file downloads once: that is the private key, named after the application id (`<app-id>.pem`). Keep the name and put the file where it will stay, for example `~/.fine/`: `fine` reads it from there on every sync and does not copy it.

### 2. Activate it with your own accounts

Production normally requires a contract, but Enable Banking has a **restricted mode** for your own accounts, allowed for individual non-commercial use:

1. On the (Inactive) application click **Activate by linking accounts**.
2. Log in at your bank and approve. Repeat for each bank you want.
3. The application becomes active and the API returns only the accounts you linked.

Read the *Restriction of Use* section of Enable Banking's [Terms of Service](https://enablebanking.com/terms-of-service/): restricted mode is for your own accounts, not for offering a service to others.

### 3. Run the guided setup

```sh
fine init
```

It asks for the `.pem` file, checks it against Enable Banking, lets you pick your bank and opens the bank's page in the browser. After you approve, the bank sends you to `https://localhost/fine?code=...`: the page fails to load, which is expected. Copy the full address from the address bar and paste it in the terminal.

No certificate to install: the redirect URL must be `https`, but nothing has to answer on it, so `fine` runs no local https server and never touches your system's trust store.

`fine init` then downloads your transactions, registers `fine` in Claude Code, Codex and Claude Desktop, and schedules sync and dashboard.

### Renewing consent

Consent lasts up to 180 days, depending on the bank. `fine` warns you two weeks before it expires; renew it with `fine connect` or from the dashboard. History is kept, without duplicates.

## Connect your AI

`fine init` already does this. By hand:

Claude Code, as a plugin (install `fine` first):

```
/plugin install fine --marketplace giovamuge/fine
```

Claude Code, as an MCP server:

```sh
claude mcp add fine -- fine mcp
```

Claude Desktop, in `claude_desktop_config.json`:

```json
{ "mcpServers": { "fine": { "command": "/path/to/fine", "args": ["mcp"] } } }
```

Codex CLI, in `~/.codex/config.toml`:

```toml
[mcp_servers.fine]
command = "fine"
args = ["mcp"]
```

## What you get

| Tool | What it does |
|---|---|
| `status` | left to spend this month, week and day, with alerts |
| `list_accounts`, `sync`, `balance_history` | accounts, manual sync, balance over time |
| `search_transactions`, `monthly_summary`, `trend`, `recurring` | transactions, monthly totals, 12-month trend, subscriptions |
| `list_uncategorized`, `categorize`, `list_rules`, `delete_rule` | categorization with rules that learn |
| `list_categories`, `upsert_category`, `update_category`, `delete_category` | customizable categories |
| `tag_transactions`, `untag_transactions`, `list_tags`, `delete_tag` | free-form tags, many per transaction |
| `upsert_budget`, `preview_budget`, `delete_budget`, `budget_status` | rule-based budgets and goals |
| `upsert_planned`, `delete_planned`, `list_planned`, `sinking_funds`, `set_savings_goal` | salary, fixed and annual expenses, savings goal |
| `amortize` | spread a large expense over N months |

**Prompts**: `weekly-review`, `categorize-pending`, `monthly-plan`.

**Alerts**: budgets exceeded or spent too fast, upcoming annual expenses, new subscriptions, unusual expenses, expiring consent, failed sync.

Default categories and dashboard are in Italian; your AI answers in your language.

## Dashboard

```sh
fine serve   # http://127.0.0.1:7878
```

A mobile-first web app embedded in the executable, with no external requests: overview, transactions, budgets, analysis, categories and accounts. `fine init` installs it as an always-on service.

To open it from your phone, set a password and expose it on a private network such as [Tailscale](https://tailscale.com):

```sh
fine setup --dashboard-password '<at least 12 characters>'
fine serve --bind 0.0.0.0:7878
```

`fine` refuses to listen beyond localhost without a password. Don't expose it directly to the Internet.

## Scheduling

```sh
fine schedule                  # sync every 6 hours, always-on dashboard
fine schedule --no-dashboard   # same, without the dashboard
fine schedule --remove         # removes everything
```

launchd agents on macOS, crontab lines on Linux; logs in `~/.fine/schedule.log`.

## Self-hosting on a server

A small Linux box (a VPS, a Raspberry Pi) keeps syncing while your computer is off. Everything is done over SSH.

**1. Install and set up.** Install with the script above, copy the key and run the setup:

```sh
ssh you@server mkdir -p .fine
scp <app-id>.pem you@server:.fine/
ssh you@server
fine init
```

The server has no browser: `fine init` prints the bank link. Open it on your computer, approve, and paste the final `https://localhost/fine?code=...` address back into the SSH session.

**2. Scheduling.** `fine init` adds crontab lines for the sync every 6 hours and the dashboard at boot (`crontab -l` shows them, marked `# fine`). Times follow the server's time zone.

**3. AI categorization.** Background categorization needs Claude Code or Codex installed and signed in on the server. Without them only rules apply, and your assistant categorizes the rest when you ask.

**4. Dashboard.** It speaks plain HTTP with a password (HTTP Basic), so keep it on `127.0.0.1` and put TLS in front:

```sh
fine setup --dashboard-password '<at least 12 characters>'
```

- [Caddy](https://caddyserver.com), two lines: `YOUR-HOST { reverse_proxy localhost:7878 }`;
- or [Tailscale](https://tailscale.com): `tailscale serve 7878`, reachable only from your devices.

Once set, the password is required on localhost too, because a proxy or tunnel connects from there. Never publish port 7878 itself.

**5. Your assistant.** The MCP server speaks stdio, so run it over SSH from your computer (key-based login, full path to the executable):

```sh
claude mcp add fine -- ssh you@server .local/bin/fine mcp
```

```json
{ "mcpServers": { "fine": { "command": "ssh", "args": ["you@server", ".local/bin/fine", "mcp"] } } }
```

There is no remote (HTTP) MCP endpoint yet, so claude.ai on the web and phone apps cannot connect to it.

**6. Backup.** Everything is in `~/.fine/`: database, configuration and key. Back it up to avoid re-consenting; delete it to forget everything.

## How "left to spend" is computed

```
income for the month (received + expected)
− fixed expenses for the month
− 1/12 of each planned annual expense
− savings goal
= variable budget
− variable expenses so far
− unspent part of budgets marked "reserve"
= left to spend  →  ÷ days remaining = left to spend per day
```

Without a planned salary, income is the average of the last 3 months. An amortized purchase weighs 1/N per month.

## Budgets

A budget is a goal (`max_spend`, `min_save`, `min_income`), an amount, a period (`week`, `month`, `quarter`, `year`, `rolling`, `custom`) and a rule combining categories, tags, text, accounts, amounts and dates with `all`, `any`, `not`. Optional rollover, alert thresholds and `reserve`.

```json
{ "name": "Leisure", "amount": 300, "period": { "type": "month", "start_day": 27 }, "rollover": "surplus",
  "rule": { "all": [ { "category": ["Ristoranti e bar", "Svago"] }, { "not": { "tag": ["lavoro"] } } ] } }
```

Create them by asking your AI or in the dashboard's visual editor.

## Privacy and security

- `fine` never moves money: it only has read access to your accounts.
- Data stays in `~/.fine/` (or `FINE_DATA_DIR`) with 0600 permissions. Delete the folder to forget everything.
- A cloud AI sends the transactions it reads to its provider. To keep everything local, use an MCP client with an Ollama model.
- Bank descriptions are untrusted text: returned as data, never interpreted as HTML, and the AI is told not to follow instructions in them.
- Analyses only use **EUR** transactions; accounts in other currencies are synced and browsable.

## Development

```sh
cargo test && cargo clippy --all-targets -- -D warnings
```

The dashboard is a React + TypeScript app in `web/`; `build.rs` builds it with npm and embeds it. Without Node, build with `FINE_SKIP_WEB_BUILD=1`. For hot reload:

```sh
fine serve                             # API on http://127.0.0.1:7878
cd web && npm install && npm run dev   # Vite on http://localhost:5173
```

To publish a release run Actions → **Release** → *Run workflow* and type the version (e.g. `0.3.0`): it builds the binaries and creates the `vX.Y.Z` tag and release. Pushing a `vX.Y.Z` tag works too. Keep the version in `Cargo.toml` and `.claude-plugin/plugin.json` aligned.

Inspired by [BankMCP](https://github.com/noskillish/bankmcp) (MIT). License: [MIT](LICENSE).
