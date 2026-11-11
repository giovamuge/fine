# fine

**Finanza personale sui tuoi dati bancari, con la tua AI.**
`fine` scarica i movimenti dei tuoi conti tramite [Enable Banking](https://enablebanking.com) (PSD2, sola lettura) e li salva **in locale** (SQLite). Li categorizza con regole che imparano da sole e ti dice ogni giorno **quanto puoi ancora spendere**.

Come "cervello" usa il tuo abbonamento **Claude** o **ChatGPT** tramite [MCP](https://modelcontextprotocol.io): non serve nessuna API a pagamento.

- 🏦 **Sincronizzazione incrementale**: lo storico resta sul tuo disco e le analisi non consumano le 4 letture giornaliere concesse dalla PSD2.
- 🏷️ **Categorizzazione che impara**: regole predefinite per i principali esercenti italiani, più le regole che crei tu o la tua AI con `categorize`.
- 💶 **Assistente**: disponibile del mese, della settimana e del giorno. Tiene conto di entrate, spese fisse, accantonamenti per le spese annuali e obiettivo di risparmio.
- 🚨 **Avvisi**:
  - budget sforati o spesi troppo in fretta;
  - spese annuali in arrivo;
  - nuovi abbonamenti ricorrenti;
  - spese insolite;
  - consenso bancario in scadenza.
- 📆 **Ammortamento**: una spesa grande può essere spalmata su N mesi. Le spese annuali diventano quote mensili da accantonare.
- 📊 **Dashboard web** leggera, senza dipendenze esterne, e **notifiche push** con [ntfy](https://ntfy.sh).
- 🦀 **Un solo eseguibile in Rust**, pochi MB di RAM: gira anche su un Raspberry Pi.

> `fine` non muove denaro: usa solo l'accesso in lettura ai conti (AIS).

## Installazione

```sh
cargo install --git https://github.com/giovamuge/fine
```

Serve Rust: installalo da [rustup.rs](https://rustup.rs).

## Avvio rapido: un solo comando

```sh
fine init
```

La configurazione guidata fa tutto da sola:

1. **Banca**: ti chiede solo il file `.pem` di Enable Banking (l'app id si ricava dal nome del file, il redirect URL dall'applicazione), poi scegli la banca da un elenco e il browser si apre da solo.
2. **Primo download** dei movimenti, con categorizzazione automatica.
3. **Registrazione** di `fine` in Claude Code, Codex e Claude Desktop (della config di Claude Desktop viene fatto un backup).
4. **Sincronizzazione automatica ogni 6 ore** (01:07, 07:07, 13:07, 19:07) per coprire tutta la giornata entro il limite PSD2 di 4 letture al giorno. Se lo vuoi, anche una **notifica sul telefono** alle 08:12.

Prima di tutto serve un'applicazione Enable Banking. È gratis per uso personale:
- creane una **Production** su [enablebanking.com](https://enablebanking.com) e registra un redirect URL, per esempio `https://localhost/fine` (non serve che risponda);
- collega i tuoi conti dal pannello: l'app si attiva in *restricted mode*, solo per i **tuoi** conti;
- scarica la chiave `.pem`.

Il consenso PSD2 dura al massimo 180 giorni, a seconda della banca. `fine` ti avvisa due settimane prima della scadenza; per rinnovarlo basta `fine connect`.

Se una banca concede meno storico di quello richiesto, `fine` parte da 90 giorni e, se serve, riprova da solo con 30. Una sincronizzazione fallita non passa mai in silenzio: diventa un avviso critico in `fine status`, nella dashboard, nelle notifiche e nelle risposte dell'AI.

## 2. Usarlo con la tua AI (MCP)

**Claude Code**
```sh
claude mcp add fine -- fine mcp
```

**Claude Desktop**: aggiungi a `claude_desktop_config.json`:
```json
{ "mcpServers": { "fine": { "command": "/percorso/di/fine", "args": ["mcp"] } } }
```

**Codex CLI (ChatGPT)**: aggiungi a `~/.codex/config.toml`:
```toml
[mcp_servers.fine]
command = "fine"
args = ["mcp"]
```

Poi chiedi per esempio:
- *"Come sto questo mese? Quanto posso spendere questa settimana?"*
- *"Categorizza le transazioni in sospeso"*: c'è anche il prompt `categorize-pending`.
- *"Aiutami a impostare il piano mensile"*, con il prompt `monthly-plan`: stipendio, affitto, bollette, spese annuali, budget, obiettivo di risparmio.
- *"Ammortizza l'acquisto del portatile su 24 mesi"*
- *"Quali abbonamenti potrei disdire?"*

**Strumenti disponibili:** `status`, `list_accounts`, `sync`, `search_transactions`, `monthly_summary`, `trend`, `list_uncategorized`, `categorize`, `list_categories`, `upsert_category`, `list_rules`, `delete_rule`, `set_budget`, `budget_status`, `recurring`, `upsert_planned`, `delete_planned`, `list_planned`, `sinking_funds`, `amortize`, `set_savings_goal`.

## 3. Dashboard

```sh
fine serve                       # http://127.0.0.1:7878
```

Per aprirla dal telefono, il modo consigliato è [Tailscale](https://tailscale.com) (gratis): imposta una password ed esponila sulla rete privata.

```sh
fine setup --dashboard-password '<almeno 12 caratteri>'
fine serve --bind 0.0.0.0:7878   # raggiungibile come http://<nome-mac>:7878 nella tailnet
```

Su un indirizzo diverso da localhost `fine` si rifiuta di partire senza password. Non esporlo direttamente su Internet.

## 4. Aggiornamento automatico e notifiche

`fine init` installa già tutto. Per gestirlo a mano:

```sh
fine schedule            # sync ogni 6 ore (+ notifica giornaliera se ntfy è configurato)
fine schedule --remove   # rimuove la pianificazione
```

- **macOS**: usa due agenti launchd (`com.fine.sync`, `com.fine.notify`). Se il Mac è in stop all'orario previsto, l'avvio viene recuperato al risveglio.
- **Linux**: righe in crontab marcate `# fine`.
- **Rete di sicurezza**: `fine status`, lo strumento MCP `status` e la dashboard sincronizzano da soli se i dati hanno più di 6 ore.
- **Log**: in `~/.fine/schedule.log`.
- **Notifiche**: con ntfy le ricevi sull'app gratuita, iscrivendoti al topic segreto generato da `fine init`.

Per un briefing in linguaggio naturale, che consuma il tuo abbonamento e non un'API:
```sh
claude -p "Usa fine: fammi il briefing finanziario di oggi in 5 righe"
```

## Come calcola il "disponibile"

```
entrate del mese (ricevute + attese)
− spese fisse del mese (pianificate settimanali/mensili, pagate o ancora da pagare)
− 1/12 di ogni spesa annuale pianificata (accantonamento)
− obiettivo di risparmio
= budget variabile
− spese variabili già fatte
= disponibile  →  ÷ giorni rimasti = disponibile al giorno
```

**Entrate:** se non hai pianificato lo stipendio, `fine` usa la media degli ultimi 3 mesi.

**Spese annuali:** il pagamento di una spesa annuale pianificata (per esempio l'assicurazione) non fa crollare il mese in cui arriva. È già coperto dalle quote mensili.

**Ammortamento:** un acquisto ammortizzato pesa 1/N al mese.

## Privacy e sicurezza

- I dati restano in `~/.fine/`, oppure in `FINE_DATA_DIR` se lo imposti, con permessi 0600.
- Le analisi (budget, disponibile, andamento) usano solo i movimenti in **EUR**. I conti in altre valute vengono comunque sincronizzati e sono consultabili.
- Quando usi un'AI in cloud, le transazioni che legge vengono inviate al suo fornitore. Per restare tutto in locale usa un client MCP con un modello Ollama.
- Le descrizioni delle transazioni sono testo non fidato: `fine` le restituisce come dati e chiede all'AI di non eseguirne mai istruzioni.

## Sviluppo

```sh
cargo test && cargo clippy --all-targets -- -D warnings
```

Ispirato a [BankMCP](https://github.com/noskillish/bankmcp) (MIT). Licenza: [MIT](LICENSE).
