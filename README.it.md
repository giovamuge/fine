# fine

[English](README.md) · **Italiano**

**La tua AI sa quanto puoi ancora spendere.** Movimenti bancari sincronizzati sul tuo disco, categorizzati in automatico, analizzati con l'abbonamento Claude o ChatGPT che hai già. Sola lettura, in locale, un solo eseguibile in Rust.

`fine` scarica i movimenti tramite [Enable Banking](https://enablebanking.com) (PSD2, oltre 2.700 banche europee), li salva in un database SQLite locale e li espone a qualsiasi client [MCP](https://modelcontextprotocol.io). Nessuna API a pagamento, nessun pagamento, nessun terzo che conserva i tuoi dati.

> "Quanto posso spendere questa settimana?" · "Quali abbonamenti potrei disdire?" ·
> "Ammortizza il portatile su 24 mesi." · "Budget di 300 € al mese per i ristoranti,
> esclusi i pranzi #lavoro."

## Come funziona

```
Il tuo assistente ──MCP──▶ fine ──▶ SQLite sul tuo disco
                            └──────▶ Enable Banking ──PSD2──▶ la tua banca   (4 volte al giorno)
```

- **La sincronizzazione** gira in background ogni 6 ore, entro il limite PSD2 di 4 letture al giorno. Le domande trovano risposta nel database locale.
- **La categorizzazione** usa regole; ciò che nessuna regola riconosce passa a `claude -p` o `codex exec`, che trasforma le risposte sicure in nuove regole.
- **Il disponibile** del mese, della settimana e del giorno tiene conto di entrate, spese fisse, spese annuali, budget e obiettivo di risparmio.

## Installazione

```sh
curl -fsSL https://raw.githubusercontent.com/giovamuge/fine/main/install.sh | sh
```

macOS e Linux, Intel e ARM; installa in `~/.local/bin`. Dai sorgenti (servono Rust e Node.js):

```sh
cargo install --git https://github.com/giovamuge/fine
```

## Configurazione

Servono un account Enable Banking gratuito e una decina di minuti.

### 1. Registra un'applicazione Enable Banking

Su <https://enablebanking.com/cp/applications> crea un'applicazione:

- ambiente **Production** (i tuoi conti reali);
- redirect URL `https://localhost/fine`;
- lascia selezionato *generate private key* e salva.

Viene scaricato una sola volta un file `.pem`: è la chiave privata, e prende il nome dall'application id (`<app-id>.pem`). Non rinominarlo e mettilo dove resterà, per esempio in `~/.fine/`: `fine` lo legge da lì a ogni sincronizzazione e non lo copia.

### 2. Attivala con i tuoi conti

L'ambiente Production richiede di norma un contratto, ma Enable Banking prevede un **restricted mode** per i propri conti, consentito per uso personale non commerciale:

1. Sull'applicazione (Inactive) clicca **Activate by linking accounts**.
2. Accedi alla tua banca e approva. Ripeti per ogni banca che vuoi.
3. L'applicazione diventa attiva e l'API restituisce solo i conti collegati.

Leggi la sezione *Restriction of Use* dei [Terms of Service](https://enablebanking.com/terms-of-service/) di Enable Banking: il restricted mode vale per i tuoi conti, non per offrire un servizio ad altri.

### 3. Avvia la configurazione guidata

```sh
fine init
```

Chiede il file `.pem`, lo verifica con Enable Banking, ti fa scegliere la banca e ne apre la pagina nel browser. Dopo l'approvazione la banca ti porta su `https://localhost/fine?code=...`: la pagina non si carica, ed è normale. Copia l'indirizzo completo dalla barra del browser e incollalo nel terminale.

Nessun certificato da installare: il redirect URL deve essere `https`, ma non serve che risponda nulla, quindi `fine` non avvia un server https locale e non tocca i certificati attendibili del sistema.

Poi `fine init` scarica i movimenti, registra `fine` in Claude Code, Codex e Claude Desktop e pianifica sincronizzazione e dashboard.

### Rinnovo del consenso

Il consenso dura al massimo 180 giorni, a seconda della banca. `fine` ti avvisa due settimane prima della scadenza; si rinnova con `fine connect` o dalla dashboard. Lo storico resta, senza duplicati.

## Collega la tua AI

`fine init` lo fa già. A mano:

Claude Code, come plugin (installa prima `fine`):

```
/plugin install fine --marketplace giovamuge/fine
```

Claude Code, come server MCP:

```sh
claude mcp add fine -- fine mcp
```

Claude Desktop, in `claude_desktop_config.json`:

```json
{ "mcpServers": { "fine": { "command": "/percorso/di/fine", "args": ["mcp"] } } }
```

Codex CLI, in `~/.codex/config.toml`:

```toml
[mcp_servers.fine]
command = "fine"
args = ["mcp"]
```

## Cosa ottieni

| Strumento | Cosa fa |
|---|---|
| `status` | disponibile del mese, della settimana e del giorno, con gli avvisi |
| `list_accounts`, `sync`, `balance_history` | conti, sincronizzazione manuale, saldo nel tempo |
| `search_transactions`, `monthly_summary`, `trend`, `recurring` | movimenti, totali mensili, andamento a 12 mesi, abbonamenti |
| `list_uncategorized`, `categorize`, `list_rules`, `delete_rule` | categorizzazione con regole che imparano |
| `list_categories`, `upsert_category`, `update_category`, `delete_category` | categorie personalizzabili |
| `tag_transactions`, `untag_transactions`, `list_tags`, `delete_tag` | tag liberi, più di uno per movimento |
| `upsert_budget`, `preview_budget`, `delete_budget`, `budget_status` | budget e obiettivi con regole |
| `upsert_planned`, `delete_planned`, `list_planned`, `sinking_funds`, `set_savings_goal` | stipendio, spese fisse e annuali, obiettivo di risparmio |
| `amortize` | spalma una spesa grande su N mesi |

**Prompt**: `weekly-review`, `categorize-pending`, `monthly-plan`.

**Avvisi**: budget sforati o spesi troppo in fretta, spese annuali in arrivo, nuovi abbonamenti, spese insolite, consenso in scadenza, sincronizzazione fallita.

## Dashboard

```sh
fine serve   # http://127.0.0.1:7878
```

Un'app web pensata per il telefono, incorporata nell'eseguibile e senza richieste esterne: panoramica, movimenti, budget, analisi, categorie e conti. `fine init` la installa come servizio sempre attivo.

Per aprirla dal telefono, imposta una password ed esponila su una rete privata come [Tailscale](https://tailscale.com):

```sh
fine setup --dashboard-password '<almeno 12 caratteri>'
fine serve --bind 0.0.0.0:7878
```

`fine` si rifiuta di ascoltare oltre localhost senza password. Non esporla direttamente su Internet.

## Pianificazione

```sh
fine schedule                  # sync ogni 6 ore, dashboard sempre attiva
fine schedule --no-dashboard   # come sopra, senza dashboard
fine schedule --remove         # rimuove tutto
```

Agenti launchd su macOS, righe in crontab su Linux; log in `~/.fine/schedule.log`.

## Self-hosting su un server

Una piccola macchina Linux (un VPS, un Raspberry Pi) continua a sincronizzare anche a computer spento. Si fa tutto via SSH.

**1. Installa e configura.** Installa con lo script qui sopra, copia la chiave e avvia la configurazione:

```sh
ssh tu@server mkdir -p .fine
scp <app-id>.pem tu@server:.fine/
ssh tu@server
fine init
```

Il server non ha un browser: `fine init` stampa il link della banca. Aprilo sul tuo computer, approva e incolla nella sessione SSH l'indirizzo finale `https://localhost/fine?code=...`.

**2. Pianificazione.** `fine init` aggiunge al crontab la sincronizzazione ogni 6 ore e la dashboard all'avvio (le vedi con `crontab -l`, marcate `# fine`). Gli orari seguono il fuso orario del server.

**3. Categorizzazione AI.** Quella in background richiede Claude Code o Codex installati e con l'accesso fatto sul server. Senza, valgono solo le regole e il resto lo categorizza il tuo assistente quando glielo chiedi.

**4. Dashboard.** Parla HTTP in chiaro con una password (HTTP Basic): lasciala su `127.0.0.1` e mettile davanti TLS:

```sh
fine setup --dashboard-password '<almeno 12 caratteri>'
```

- [Caddy](https://caddyserver.com), due righe: `TUO-HOST { reverse_proxy localhost:7878 }`;
- oppure [Tailscale](https://tailscale.com): `tailscale serve 7878`, raggiungibile solo dai tuoi dispositivi.

Una volta impostata, la password è richiesta anche su localhost, perché un proxy o un tunnel si collega da lì. Non pubblicare mai direttamente la porta 7878.

**5. Il tuo assistente.** Il server MCP parla stdio, quindi avvialo via SSH dal tuo computer (accesso con chiave, percorso completo dell'eseguibile):

```sh
claude mcp add fine -- ssh tu@server .local/bin/fine mcp
```

```json
{ "mcpServers": { "fine": { "command": "ssh", "args": ["tu@server", ".local/bin/fine", "mcp"] } } }
```

Non esiste ancora un endpoint MCP remoto (HTTP), quindi claude.ai sul web e le app per telefono non possono collegarsi.

**6. Backup.** È tutto in `~/.fine/`: database, configurazione e chiave. Fanne un backup per non rifare il consenso; elimina la cartella per dimenticare tutto.

## Come calcola il "disponibile"

```
entrate del mese (ricevute + attese)
− spese fisse del mese
− 1/12 di ogni spesa annuale pianificata
− obiettivo di risparmio
= budget variabile
− spese variabili già fatte
− parte non spesa dei budget con "reserve"
= disponibile  →  ÷ giorni rimasti = disponibile al giorno
```

Senza uno stipendio pianificato, le entrate sono la media degli ultimi 3 mesi. Un acquisto ammortizzato pesa 1/N al mese.

## Budget

Un budget è un obiettivo (`max_spend`, `min_save`, `min_income`), un importo, un periodo (`week`, `month`, `quarter`, `year`, `rolling`, `custom`) e una regola che combina categorie, tag, testo, conti, importi e date con `all`, `any`, `not`. Facoltativi: riporto, soglie di avviso e `reserve`.

```json
{ "name": "Svago", "amount": 300, "period": { "type": "month", "start_day": 27 }, "rollover": "surplus",
  "rule": { "all": [ { "category": ["Ristoranti e bar", "Svago"] }, { "not": { "tag": ["lavoro"] } } ] } }
```

Si creano chiedendolo alla tua AI o dall'editor visuale della dashboard.

## Privacy e sicurezza

- `fine` non muove denaro: ha solo accesso in lettura ai conti.
- I dati restano in `~/.fine/` (o `FINE_DATA_DIR`) con permessi 0600. Elimina la cartella per dimenticare tutto.
- Un'AI in cloud invia al suo fornitore le transazioni che legge. Per restare tutto in locale usa un client MCP con un modello Ollama.
- Le descrizioni della banca sono testo non fidato: restituite come dati, mai interpretate come HTML, e all'AI viene detto di non eseguirne le istruzioni.
- Le analisi usano solo i movimenti in **EUR**; i conti in altre valute vengono sincronizzati e sono consultabili.

## Sviluppo

```sh
cargo test && cargo clippy --all-targets -- -D warnings
```

La dashboard è un'app React + TypeScript in `web/`; `build.rs` la compila con npm e la incorpora. Senza Node, compila con `FINE_SKIP_WEB_BUILD=1`. Per la ricarica immediata:

```sh
fine serve                             # API su http://127.0.0.1:7878
cd web && npm install && npm run dev   # Vite su http://localhost:5173
```

Per fare una release basta alzare la versione in `Cargo.toml` (e in `.claude-plugin/plugin.json`): al push successivo su `main` il workflow Release compila i binari e li pubblica come `vX.Y.Z`, creando il tag.

Ispirato a [BankMCP](https://github.com/noskillish/bankmcp) (MIT). Licenza: [MIT](LICENSE).
