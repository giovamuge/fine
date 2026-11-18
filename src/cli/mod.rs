//! The `fine` command line.

mod args;
mod clients;
mod prompt;
mod schedule;
mod wizard;

use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{Context, Result, bail};
use clap::Parser;

use fine::App;
use fine::bank::Client;
use fine::config::{
    Config, DEFAULT_API_BASE, DashboardConfig, EnableBankingConfig, NtfyConfig, Paths,
};
use fine::domain::assistant;
use fine::services::{ai, notify, reports, sync};
use fine::storage::{Database, Store};

use args::{Cli, Command};

/// Exit code when the command ran but something failed (a sync, the AI).
const PARTIAL_FAILURE: u8 = 2;

pub async fn run() -> Result<ExitCode> {
    let cli = Cli::parse();
    let paths = Paths::resolve()?;
    paths.ensure_dir()?;
    let mut config = Config::load(&paths)?;
    let open = || -> Result<Store> { Ok(Database::open(&paths.db)?.connect()?) };
    match cli.command {
        Command::Init { yes } => wizard::init(&paths, &mut config, yes).await?,
        Command::Setup {
            key,
            app_id,
            redirect_url,
            api_base,
            ntfy_url,
            ntfy_token,
            dashboard_password,
        } => {
            let setup = BankSetup {
                key,
                app_id,
                redirect_url,
                api_base,
            };
            if setup.is_requested() {
                let eb = setup.resolve(config.enable_banking.as_ref())?;
                let app = Client::new(&eb)?
                    .application()
                    .await
                    .context("verifica dell'applicazione Enable Banking")?;
                println!(
                    "Applicazione: {} ({}), attiva: {}",
                    app.name.as_deref().unwrap_or("?"),
                    app.environment.as_deref().unwrap_or("?"),
                    app.active
                        .map_or_else(|| "null".to_owned(), |a| a.to_string())
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
            let client = Client::new(config.enable_banking()?)?;
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
            let mut store = open()?;
            match bank {
                Some(b) => {
                    wizard::connect_bank(&mut store, eb, &b, &country.to_uppercase(), days).await?
                }
                None => wizard::choose_and_connect(&mut store, eb).await?,
            }
            println!("\nFatto. Esegui `fine sync` per scaricare le transazioni.");
        }
        Command::Accounts => {
            for a in reports::accounts(&open()?)? {
                println!(
                    "{}  {}  saldo: {}  consenso fino al: {}  transazioni: {}",
                    a.label,
                    a.iban.unwrap_or_default(),
                    a.balance.map_or_else(|| "-".into(), |b| b.to_string()),
                    a.consent_valid_until
                        .as_deref()
                        .and_then(|v| v.get(..10))
                        .unwrap_or("-"),
                    a.transactions
                );
            }
        }
        Command::Sync { no_ai } => {
            let client = Client::new(config.enable_banking()?)?;
            let mut store = open()?;
            let report = sync::sync_all(&mut store, &client).await?;
            let mut failed = false;
            println!("[{}]", chrono::Local::now().format("%Y-%m-%d %H:%M"));
            for a in &report.accounts {
                match (&a.skipped, &a.error) {
                    (Some(s), _) => println!("{}: saltato ({s})", a.account),
                    (_, Some(e)) => {
                        failed = true;
                        eprintln!("{}: errore: {e}", a.account);
                    }
                    _ => println!("{}: {} nuove transazioni", a.account, a.inserted),
                }
            }
            println!("Categorizzate automaticamente: {}", report.auto_categorized);
            if let Some(e) = &report.rates_error {
                eprintln!("Cambi valuta non aggiornati: {e}");
            }
            if !no_ai && !run_ai(&mut store, false).await {
                failed = true;
            }
            if failed {
                return Ok(ExitCode::from(PARTIAL_FAILURE));
            }
        }
        Command::Categorize { retry } => {
            if !run_ai(&mut open()?, retry).await {
                return Ok(ExitCode::from(PARTIAL_FAILURE));
            }
        }
        Command::Status { json } => {
            let st = fresh_status(&mut open()?, &config).await?;
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
            let st = fresh_status(&mut open()?, &config).await?;
            notify::send(ntfy, &title, &assistant::render(&st), &st).await?;
            println!("Notifica inviata.");
        }
        Command::Schedule {
            remove,
            no_notify,
            no_dashboard,
        } => {
            if remove {
                schedule::remove_schedule()?;
                println!("Pianificazione rimossa.");
            } else {
                let exe = std::env::current_exe()?.canonicalize()?;
                let notify = !no_notify && config.ntfy.is_some();
                let done = schedule::install_schedule(&paths, &exe, notify, !no_dashboard)?;
                println!("Pianificato: {done}.");
            }
        }
        Command::Mcp => {
            fine::mcp::serve_stdio(App::new(Database::open(&paths.db)?, config)).await?
        }
        Command::Serve { bind } => {
            fine::web::serve(App::new(Database::open(&paths.db)?, config), bind).await?
        }
    }
    Ok(ExitCode::SUCCESS)
}

/// The status, after a sync if the data is more than a few hours old.
async fn fresh_status(store: &mut Store, config: &Config) -> Result<assistant::Status> {
    if let Some(msg) = sync::sync_if_stale(store, config.enable_banking.as_ref()).await {
        eprintln!("({msg})");
    }
    Ok(reports::status(store, App::today())?)
}

/// `fine setup` options for Enable Banking; missing ones keep the configured values.
struct BankSetup {
    key: Option<PathBuf>,
    app_id: Option<String>,
    redirect_url: Option<String>,
    api_base: Option<String>,
}

impl BankSetup {
    fn is_requested(&self) -> bool {
        self.key.is_some()
            || self.app_id.is_some()
            || self.redirect_url.is_some()
            || self.api_base.is_some()
    }

    fn resolve(self, prev: Option<&EnableBankingConfig>) -> Result<EnableBankingConfig> {
        let key_path = match self.key {
            Some(k) => k
                .canonicalize()
                .with_context(|| format!("file non trovato: {}", k.display()))?,
            None => prev.map(|p| p.key_path.clone()).context("--key mancante")?,
        };
        let app_id = self
            .app_id
            .or_else(|| wizard::app_id_from_key(&key_path))
            .or_else(|| prev.map(|p| p.app_id.clone()))
            .context("--app-id mancante (il file .pem non si chiama <app_id>.pem)")?;
        Ok(EnableBankingConfig {
            app_id,
            key_path,
            redirect_url: self
                .redirect_url
                .or_else(|| prev.map(|p| p.redirect_url.clone()))
                .unwrap_or_default(),
            api_base: self
                .api_base
                .or_else(|| prev.map(|p| p.api_base.clone()))
                .unwrap_or_else(|| DEFAULT_API_BASE.to_owned()),
        })
    }
}

/// AI categorization of what the rules did not recognize. Prints the outcome; returns false
/// only when the AI was available but failed (a missing CLI is not an error).
async fn run_ai(store: &mut Store, retry: bool) -> bool {
    let Some(backend) = ai::Backend::detect() else {
        println!("AI: né `claude` né `codex` trovati nel PATH, categorizzazione AI saltata.");
        return true;
    };
    if retry && let Err(e) = ai::reset_checked(store) {
        eprintln!("AI: {}", e.chain());
        return false;
    }
    match ai::categorize_pending(store, &backend).await {
        Ok(None) => {
            println!("AI: nessun movimento nuovo da categorizzare.");
            true
        }
        Ok(Some(o)) => {
            let rules = if o.rules_created.is_empty() {
                String::new()
            } else {
                format!(" ({})", o.rules_created.join(", "))
            };
            println!(
                "AI ({}): {} movimenti esaminati, {} categorizzati, {} nuove regole{rules}",
                o.backend,
                o.examined,
                o.categorized,
                o.rules_created.len(),
            );
            true
        }
        Err(e) => {
            eprintln!("AI ({}): errore: {}", backend.name(), e.chain());
            false
        }
    }
}
