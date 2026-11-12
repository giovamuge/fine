# fine

[English](README.md) · **Italiano**

**Finanza personale sui tuoi dati bancari, con la tua AI.**
`fine` scarica i movimenti dei tuoi conti tramite [Enable Banking](https://enablebanking.com) (PSD2, sola lettura) e li salva **in locale** (SQLite). Li categorizza con regole che imparano da sole e ti dice ogni giorno **quanto puoi ancora spendere**.

Come "cervello" usa il tuo abbonamento **Claude** o **ChatGPT** tramite [MCP](https://modelcontextprotocol.io): non serve nessuna API a pagamento.

- 🏦 **Sincronizzazione incrementale**: lo storico resta sul tuo disco e le analisi non consumano le 4 letture giornaliere concesse dalla PSD2.
- 🏷️ **Categorizzazione che impara**: regole predefinite per i principali esercenti italiani, più le regole che crei tu o la tua AI con `categorize`.
- 🤖 **Categorizzazione AI in background**: dopo ogni sync, i movimenti che le regole non riconoscono vengono passati a `claude -p` (o `codex exec`) con il tuo abbonamento. L'AI non ha strumenti e restituisce solo categorie, che `fine` valida; dove è sicura crea regole, così la volta dopo non serve più. I casi dubbi (es. bonifici a persone) restano a te. A mano: `fine categorize`.
- 💶 **Assistente**: disponibile del mese, della settimana e del giorno. Tiene conto di entrate, spese fisse, accantonamenti per le spese annuali e obiettivo di risparmio.
- 🚨 **Avvisi**:
  - budget sforati o spesi troppo in fretta;
  - spese annuali in arrivo;
  - nuovi abbonamenti ricorrenti;
  - spese insolite;
  - consenso bancario in scadenza.
- 🏷️ **Tag liberi**: quanti ne vuoi per ogni movimento (`#vacanza-londra`, `#lavoro`, `#rimborsabile`, una persona...), indipendenti dalla categoria. Totali per tag, filtri e regole che taggano da sole i movimenti futuri.
- 📈 **Saldo nel tempo**: saldo attuale per conto e andamento giornaliero, ricostruito dai movimenti salvati, con la variazione a 7/30/90/365 giorni.
- 📆 **Ammortamento**: una spesa grande può essere spalmata su N mesi. Le spese annuali diventano quote mensili da accantonare.
- 📊 **Dashboard web** pensata per il telefono, incorporata nell'eseguibile e senza richieste esterne, e **notifiche push** con [ntfy](https://ntfy.sh).
- 🦀 **Un solo eseguibile in Rust**, pochi MB di RAM: gira anche su un Raspberry Pi.

> `fine` non muove denaro: usa solo l'accesso in lettura ai conti (AIS).

## Installazione

```sh
curl -fsSL https://raw.githubusercontent.com/giovamuge/fine/main/install.sh | sh
```

Scarica l'ultima release per macOS o Linux (Intel/ARM), ne verifica il checksum e la mette in `~/.local/bin`. In alternativa, compilando dai sorgenti (servono Rust da [rustup.rs](https://rustup.rs) e Node.js per la dashboard):

```sh
cargo install --git https://github.com/giovamuge/fine
```

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

**Claude Code, come plugin** (consigliato): registra da solo il server MCP e si aggiorna con `claude plugin update`.
```
/plugin install fine --marketplace giovamuge/fine
```
Rispondi `y` per aggiungere il marketplace e scegli l'ambito. Il plugin non contiene l'eseguibile: installa prima `fine` (sopra). Nell'app desktop: **+** accanto al prompt › Plugins › Add marketplace `giovamuge/fine` › Add plugin.

**Claude Code, a mano**
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
- *"Tagga come vacanza-londra tutte le spese dal 1 al 7 agosto e dimmi quanto è costata"*
- *"Come è cambiato il mio saldo negli ultimi 3 mesi?"*

**Strumenti disponibili:** `status`, `list_accounts`, `sync`, `search_transactions`, `monthly_summary`, `trend`, `list_uncategorized`, `categorize`, `list_categories`, `upsert_category`, `update_category`, `delete_category`, `list_rules`, `delete_rule`, `set_budget`, `budget_status`, `recurring`, `upsert_planned`, `delete_planned`, `list_planned`, `sinking_funds`, `amortize`, `set_savings_goal`, `tag_transactions`, `untag_transactions`, `list_tags`, `delete_tag`, `balance_history`.

## 3. Dashboard

```sh
fine serve                       # http://127.0.0.1:7878
```

`fine init` e `fine schedule` la installano anche come servizio sempre attivo (parte al login e si riavvia se si chiude; occupa una decina di MB di memoria e niente CPU a riposo): basta aprire http://127.0.0.1:7878. Per non averla: `fine schedule --no-dashboard`.

Per aprirla dal telefono, il modo consigliato è [Tailscale](https://tailscale.com) (gratis): imposta una password ed esponila sulla rete privata.

```sh
fine setup --dashboard-password '<almeno 12 caratteri>'
fine serve --bind 0.0.0.0:7878   # raggiungibile come http://<nome-mac>:7878 nella tailnet
```

Su un indirizzo diverso da localhost `fine` si rifiuta di partire senza password. Non esporlo direttamente su Internet.

La dashboard è fatta con React e [shadcn/ui](https://ui.shadcn.com); è pensata per il telefono (barra di navigazione in basso, finestre a scorrimento dal basso) ma sta bene anche sul desktop; segue il tema chiaro/scuro del sistema (con un interruttore per forzarlo), è navigabile da tastiera e non fa nessuna richiesta esterna: tutto è incorporato nell'eseguibile. Cinque sezioni:

- **Panoramica**: il "disponibile" del mese in evidenza, con settimana e giorno, la barra del ritmo di spesa (la tacca indica dove dovresti essere oggi), gli avvisi (una sincronizzazione fallita è impossibile da non vedere), il saldo con andamento e variazioni a 7/30/90/365 giorni, i budget più vicini al limite.
- **Movimenti**: ricerca istantanea, filtri per periodo, categoria, tag e "da categorizzare" (tutti nell'indirizzo, quindi condivisibili e raggiungibili con "indietro"); categoria modificabile direttamente nella riga; dettaglio di ogni movimento con la scelta esplicita *"usa questa categoria anche per i prossimi movimenti di questo esercente"*, tag con suggerimenti (e regola automatica opzionale), ammortamento su N mesi; selezione multipla per categorizzare o taggare molti movimenti in un colpo.
- **Analisi**: spese per categoria del mese (ogni riga porta ai movimenti), entrate/uscite degli ultimi 12 mesi, saldo nel tempo con tooltip e vista tabella, budget, accantonamenti per le spese annuali, totali per tag per periodo, spese ricorrenti.
- **Categorie**: 35 categorie predefinite (spese, entrate, giroconti, risparmio) tutte personalizzabili: nome, tipo, icona (qualsiasi emoji) e colore; per ognuna le regole automatiche che la assegnano, eliminabili una per una. Una categoria si elimina spostando movimenti, regole e budget in un'altra (unione) oppure lasciandoli da categorizzare: l'AI ci riproverà. Una nuova categoria si crea anche al volo dal selettore dei movimenti.
- **Conti**: una scheda per conto con saldo, IBAN, scadenza del consenso (avviso da 14 giorni), esito dell'ultima sincronizzazione con il dettaglio dell'errore; collegamento di una nuova banca guidato in quattro passi (scegli, autorizza sulla pagina della banca, incolla l'indirizzo finale, fatto), **rinnovo del consenso** (lo storico resta, senza duplicati), rinomina, rimozione con conferma esplicita (cancella i movimenti del conto da questo computer; il consenso viene revocato presso la banca se non serve ad altri conti) e "Sincronizza ora" con il riepilogo del risultato.

Le descrizioni che arrivano dalla banca sono sempre mostrate come testo, mai interpretate come HTML; gli script sono serviti solo dall'eseguibile (`Content-Security-Policy: script-src 'self'`).

## 4. Aggiornamento automatico e notifiche

`fine init` installa già tutto. Per gestirlo a mano:

```sh
fine schedule                  # sync ogni 6 ore, dashboard sempre attiva (+ notifica se ntfy è configurato)
fine schedule --no-dashboard   # come sopra, senza dashboard in background
fine schedule --remove         # rimuove tutto
```

- **macOS**: agenti launchd `com.fine.sync`, `com.fine.notify` e `com.fine.dashboard`. Se il Mac è in stop all'orario previsto, la sync viene recuperata al risveglio.
- **Linux**: righe in crontab marcate `# fine` (la dashboard con `@reboot`).
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

**Frontend.** La dashboard è un'app React + TypeScript (Vite, Tailwind CSS 4, componenti [shadcn/ui](https://ui.shadcn.com) in `web/src/components/ui`, aggiungibili con `npx shadcn@latest add <nome>`) in `web/`. Per compilare `fine` serve anche **Node.js (LTS) con npm**: `build.rs` esegue da solo `npm ci` e `npm run build` quando i sorgenti in `web/` cambiano e il risultato (`web/dist`, ignorato da git) viene incorporato nell'eseguibile. Senza Node si può comunque compilare con `FINE_SKIP_WEB_BUILD=1` (la dashboard mostra una pagina segnaposto; CLI e MCP non cambiano).

Per lavorare sul frontend con ricarica immediata:

```sh
fine serve                     # l'API Rust su http://127.0.0.1:7878
cd web && npm install && npm run dev   # Vite su http://localhost:5173, con proxy di /api verso il server Rust
```

Se il server Rust ascolta altrove: `FINE_API=http://127.0.0.1:7899 npm run dev`. Controllo dei tipi: `npm run check` (`tsc --noEmit`, anche in CI).

**Release.** Un tag `vX.Y.Z` avvia `.github/workflows/release.yml`: compila i binari (macOS e Linux, Intel e ARM, dashboard inclusa) e li pubblica come release GitHub, da cui li scarica `install.sh`. Prima del tag, allinea la versione in `Cargo.toml` e in `.claude-plugin/plugin.json`.

**Plugin Claude Code.** Il repository è anche un marketplace: `.claude-plugin/marketplace.json` elenca il plugin `fine` (la radice del repo), `.claude-plugin/plugin.json` registra il server MCP tramite `scripts/fine-mcp.sh`, che cerca l'eseguibile anche fuori dal PATH. Verifica con `claude plugin validate .`.

Ispirato a [BankMCP](https://github.com/noskillish/bankmcp) (MIT). Licenza: [MIT](LICENSE).
