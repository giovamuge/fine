//! Command-line arguments. The doc comments are the `--help` text (in Italian).

use std::net::SocketAddr;
use std::path::PathBuf;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "fine",
    version = fine::VERSION,
    about = "Finanza personale sui tuoi dati bancari, con la tua AI"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
pub enum Command {
    /// Configurazione guidata: banca, client AI, sync automatico, notifiche
    Init {
        /// Rispondi sì a tutte le domande
        #[arg(long)]
        yes: bool,
    },
    /// Configurazione manuale di Enable Banking, ntfy e dashboard
    Setup {
        /// Chiave privata .pem di Enable Banking (l'app id viene dal nome del file)
        #[arg(long)]
        key: Option<PathBuf>,
        /// Application id (se il file .pem non si chiama <app_id>.pem)
        #[arg(long)]
        app_id: Option<String>,
        /// Redirect URL (default: il primo registrato nell'applicazione)
        #[arg(long)]
        redirect_url: Option<String>,
        #[arg(long)]
        api_base: Option<String>,
        /// URL del topic ntfy (default con --ntfy: un topic segreto generato)
        #[arg(long)]
        ntfy_url: Option<String>,
        #[arg(long)]
        ntfy_token: Option<String>,
        /// Password della dashboard (obbligatoria se esposta oltre localhost)
        #[arg(long)]
        dashboard_password: Option<String>,
    },
    /// Elenca le banche disponibili in un paese
    Banks {
        #[arg(long, default_value = "IT")]
        country: String,
        #[arg(long)]
        search: Option<String>,
    },
    /// Collega (o ricollega) una banca; senza --bank la scegli da un elenco
    Connect {
        #[arg(long)]
        bank: Option<String>,
        #[arg(long, default_value = "IT")]
        country: String,
        /// Durata del consenso in giorni (il massimo dipende dalla banca)
        #[arg(long, default_value_t = 180)]
        days: u64,
    },
    /// Conti collegati
    Accounts,
    /// Scarica le nuove transazioni (max 4 volte al giorno per conto) e le categorizza
    Sync {
        /// Non usare l'AI (Claude Code / Codex) per i movimenti che le regole non riconoscono
        #[arg(long)]
        no_ai: bool,
    },
    /// Categorizza con l'AI (il tuo abbonamento Claude Code o Codex) i movimenti senza categoria
    Categorize {
        /// Ripropone anche i movimenti che l'AI aveva già lasciato senza categoria
        #[arg(long)]
        retry: bool,
    },
    /// Quanto puoi spendere e avvisi (sincronizza prima se i dati hanno più di 6 ore)
    Status {
        #[arg(long)]
        json: bool,
    },
    /// Invia lo stato come notifica push (ntfy)
    Notify {
        #[arg(long, default_value = "fine - situazione")]
        title: String,
    },
    /// Installa (o rimuove) sync ogni 6 ore, notifica giornaliera e dashboard sempre attiva
    Schedule {
        #[arg(long)]
        remove: bool,
        /// Non installare la notifica giornaliera
        #[arg(long)]
        no_notify: bool,
        /// Non tenere la dashboard sempre attiva in background
        #[arg(long)]
        no_dashboard: bool,
    },
    /// Avvia il server MCP (stdio) per Claude, ChatGPT e altri client
    Mcp,
    /// Avvia la dashboard web
    Serve {
        #[arg(long, default_value = "127.0.0.1:7878")]
        bind: SocketAddr,
    },
}
