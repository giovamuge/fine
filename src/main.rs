use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};
use std::net::SocketAddr;
use std::path::PathBuf;

use fine::config::{Config, DashboardConfig, EnableBankingConfig, NtfyConfig, Paths};
use fine::{ai, assistant, db, enablebanking, mcp, notify, ops, setup, sync, web};

#[derive(Parser)]
#[command(
    name = "fine",
    version,
    about = "Finanza personale sui tuoi dati bancari, con la tua AI"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
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

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let paths = Paths::resolve()?;
    paths.ensure_dir()?;
    let mut config = Config::load(&paths)?;
    match cli.command {
        Command::Init { yes } => setup::init(&paths, &mut config, yes).await?,
        Command::Setup {
            key,
            app_id,
            redirect_url,
            api_base,
            ntfy_url,
            ntfy_token,
            dashboard_password,
        } => {
            if key.is_some() || app_id.is_some() || redirect_url.is_some() || api_base.is_some() {
                let prev = config.enable_banking.clone();
                let key_path = match key {
                    Some(k) => k
                        .canonicalize()
                        .with_context(|| format!("file non trovato: {}", k.display()))?,
                    None => prev
                        .as_ref()
                        .map(|p| p.key_path.clone())
                        .context("--key mancante")?,
                };
                let app_id = app_id
                    .or_else(|| setup::app_id_from_key(&key_path))
                    .or(prev.as_ref().map(|p| p.app_id.clone()))
                    .context("--app-id mancante (il file .pem non si chiama <app_id>.pem)")?;
                let eb = EnableBankingConfig {
                    app_id,
                    key_path,
                    redirect_url: redirect_url
                        .or(prev.as_ref().map(|p| p.redirect_url.clone()))
                        .unwrap_or_default(),
                    api_base: api_base
                        .or(prev.map(|p| p.api_base))
                        .unwrap_or_else(|| fine::config::DEFAULT_API_BASE.to_string()),
                };
                let app = enablebanking::Client::new(&eb)?
                    .application()
                    .await
                    .context("verifica dell'applicazione Enable Banking")?;
                println!(
                    "Applicazione: {} ({}), attiva: {}",
                    app["name"].as_str().unwrap_or("?"),
                    app["environment"].as_str().unwrap_or("?"),
                    app["active"]
                );
                config.enable_banking = Some(eb);
            }
            if let Some(url) = ntfy_url {
                config.ntfy = Some(NtfyConfig {
                    url,
                    token: ntfy_token,
                });
            }
            if let Some(pw) = dashboard_password {
                if pw.len() < 12 {
                    bail!("la password della dashboard deve avere almeno 12 caratteri");
                }
                config.dashboard = Some(DashboardConfig { password: Some(pw) });
            }
            config.save(&paths)?;
            println!("Configurazione salvata in {}", paths.config.display());
        }
        Command::Banks { country, search } => {
            let client = enablebanking::Client::new(config.enable_banking()?)?;
            let needle = search.map(|s| s.to_lowercase());
            for a in client.aspsps(&country.to_uppercase()).await? {
                if needle
                    .as_ref()
                    .is_none_or(|n| a.name.to_lowercase().contains(n))
                {
                    println!("{}", a.name);
                }
            }
        }
        Command::Connect {
            bank,
            country,
            days,
        } => {
            let eb = config.enable_banking()?;
            let mut conn = db::open(&paths.db)?;
            match bank {
                Some(b) => {
                    setup::connect_bank(&mut conn, eb, &b, &country.to_uppercase(), days).await?
                }
                None => setup::choose_and_connect(&mut conn, eb).await?,
            }
            println!("\nFatto. Esegui `fine sync` per scaricare le transazioni.");
        }
        Command::Accounts => {
            let conn = db::open(&paths.db)?;
            for a in ops::list_accounts(&conn)? {
                println!(
                    "{}  {}  saldo: {}  consenso fino al: {}  transazioni: {}",
                    a.label,
                    a.iban.unwrap_or_default(),
                    a.balance
                        .map(fine::money::fmt_eur)
                        .unwrap_or_else(|| "-".into()),
                    a.consent_valid_until
                        .as_deref()
                        .and_then(|v| v.get(..10))
                        .unwrap_or("-"),
                    a.transactions
                );
            }
        }
        Command::Sync { no_ai } => {
            let client = enablebanking::Client::new(config.enable_banking()?)?;
            let mut conn = db::open(&paths.db)?;
            let report = sync::sync_all(&mut conn, &client).await?;
            let mut failed = false;
            println!("[{}]", chrono::Local::now().format("%Y-%m-%d %H:%M"));
            for a in &report.accounts {
                match (&a.skipped, &a.error) {
                    (Some(s), _) => println!("{}: saltato ({s})", a.account),
                    (_, Some(e)) => {
                        failed = true;
                        eprintln!("{}: errore: {e}", a.account)
                    }
                    _ => println!("{}: {} nuove transazioni", a.account, a.inserted),
                }
            }
            println!("Categorizzate automaticamente: {}", report.auto_categorized);
            if !no_ai && !run_ai(&mut conn, false).await {
                failed = true;
            }
            if failed {
                std::process::exit(2);
            }
        }
        Command::Categorize { retry } => {
            let mut conn = db::open(&paths.db)?;
            if !run_ai(&mut conn, retry).await {
                std::process::exit(2);
            }
        }
        Command::Status { json } => {
            let mut conn = db::open(&paths.db)?;
            if let Some(msg) = sync::sync_if_stale(&mut conn, config.enable_banking.as_ref()).await
            {
                eprintln!("({msg})");
            }
            let st = assistant::status(&conn, chrono::Local::now().date_naive())?;
            if json {
                println!("{}", serde_json::to_string_pretty(&st)?);
            } else {
                print!("{}", assistant::render(&st));
            }
        }
        Command::Notify { title } => {
            let ntfy = config
                .ntfy
                .as_ref()
                .context("ntfy non configurato: esegui `fine init`")?;
            let mut conn = db::open(&paths.db)?;
            if let Some(msg) = sync::sync_if_stale(&mut conn, config.enable_banking.as_ref()).await
            {
                eprintln!("({msg})");
            }
            let st = assistant::status(&conn, chrono::Local::now().date_naive())?;
            notify::send(ntfy, &title, &assistant::render(&st), &st).await?;
            println!("Notifica inviata.");
        }
        Command::Schedule {
            remove,
            no_notify,
            no_dashboard,
        } => {
            if remove {
                setup::remove_schedule()?;
                println!("Pianificazione rimossa.");
            } else {
                let exe = std::env::current_exe()?.canonicalize()?;
                println!(
                    "Pianificato: {}.",
                    setup::install_schedule(
                        &paths,
                        &exe,
                        !no_notify && config.ntfy.is_some(),
                        !no_dashboard,
                    )?
                );
            }
        }
        Command::Mcp => mcp::serve_stdio(paths, config).await?,
        Command::Serve { bind } => {
            let password = config.dashboard.as_ref().and_then(|d| d.password.clone());
            web::serve(paths, config, bind, password).await?
        }
    }
    Ok(())
}

/// AI categorization of what the rules did not recognize. Prints the outcome; returns false
/// only when the AI was available but failed (a missing CLI is not an error).
async fn run_ai(conn: &mut rusqlite::Connection, retry: bool) -> bool {
    let Some(backend) = ai::detect() else {
        println!("AI: né `claude` né `codex` trovati nel PATH, categorizzazione AI saltata.");
        return true;
    };
    if retry && let Err(e) = ai::reset_checked(conn) {
        eprintln!("AI: {e:#}");
        return false;
    }
    match ai::categorize_pending(conn, &backend).await {
        Ok(None) => {
            println!("AI: nessun movimento nuovo da categorizzare.");
            true
        }
        Ok(Some(o)) => {
            println!(
                "AI ({}): {} movimenti esaminati, {} categorizzati, {} nuove regole{}",
                o.backend,
                o.examined,
                o.categorized,
                o.rules_created.len(),
                if o.rules_created.is_empty() {
                    String::new()
                } else {
                    format!(" ({})", o.rules_created.join(", "))
                }
            );
            true
        }
        Err(e) => {
            eprintln!("AI ({}): errore: {e:#}", backend.name());
            false
        }
    }
}
