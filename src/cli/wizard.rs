//! `fine init`: one guided command that does everything for the user.
//! - connects a bank (only the Enable Banking .pem is needed);
//! - downloads transactions;
//! - registers fine in Claude Code, Codex and Claude Desktop;
//! - schedules sync every 6 hours (4/day = PSD2 limit) and a daily push notification.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use fine::bank::Client;
use fine::config::{Config, DEFAULT_API_BASE, EnableBankingConfig, NtfyConfig, Paths};
use fine::domain::assistant::explain_sync_error;
use fine::services::{ai, connect, sync};
use fine::storage::{Database, Store};

use super::clients::register_clients;
use super::prompt::{ask, ask_no, open_browser, prompt};
use super::schedule::{NOTIFY_TIME, install_schedule};

/// Consent duration asked for when the bank is picked from the list.
const DEFAULT_CONSENT_DAYS: u64 = 180;

/// App id from a key file named "<app_id>.pem" (how Enable Banking names the download).
pub fn app_id_from_key(path: &Path) -> Option<String> {
    let stem = path.file_stem()?.to_str()?;
    uuid::Uuid::parse_str(stem).ok().map(|u| u.to_string())
}

/// Connect a bank: open its page, then read back the address it redirects to.
pub async fn connect_bank(
    store: &mut Store,
    eb: &EnableBankingConfig,
    bank: &str,
    country: &str,
    days: u64,
) -> Result<()> {
    let pending = connect::start(store, eb, bank, country, days).await?;
    println!(
        "\nSi apre il browser: autorizza l'accesso in sola lettura.\n{}\n",
        pending.url
    );
    open_browser(&pending.url);
    let pasted = prompt(&format!(
        "Alla fine verrai portato su {}... (la pagina può dare errore, è normale).\nIncolla qui l'indirizzo completo dalla barra del browser: ",
        pending.redirect
    ));
    for a in connect::finish(store, eb, &pasted).await? {
        println!(
            "{}: {} {} ({})",
            if a.renewed { "Rinnovato" } else { "Collegato" },
            a.name.as_deref().unwrap_or(""),
            a.iban.as_deref().unwrap_or(""),
            a.currency
        );
    }
    Ok(())
}

/// Interactive bank choice from a numbered list.
pub async fn choose_and_connect(store: &mut Store, eb: &EnableBankingConfig) -> Result<()> {
    let client = Client::new(eb)?;
    let country = prompt("Paese della banca [IT]: ").to_uppercase();
    let country = if country.is_empty() {
        "IT".to_owned()
    } else {
        country
    };
    let banks = client.aspsps(&country).await?;
    loop {
        let q = prompt("Cerca la tua banca (es. intesa, fineco, revolut): ").to_lowercase();
        let found: Vec<_> = banks
            .iter()
            .filter(|b| b.name.to_lowercase().contains(&q))
            .take(20)
            .collect();
        if found.is_empty() {
            println!("Nessun risultato, riprova.");
            continue;
        }
        for (i, b) in found.iter().enumerate() {
            println!("  {}. {}", i + 1, b.name);
        }
        let pick = prompt("Numero (invio per cercare di nuovo): ");
        if let Some(b) = pick
            .parse::<usize>()
            .ok()
            .and_then(|n| found.get(n.wrapping_sub(1)))
        {
            return connect_bank(store, eb, &b.name, &country, DEFAULT_CONSENT_DAYS).await;
        }
    }
}

/// A new secret ntfy topic: its name is the key, whoever knows it can read the notifications.
pub fn new_ntfy_topic() -> NtfyConfig {
    let id = uuid::Uuid::new_v4().simple().to_string();
    NtfyConfig {
        url: format!("https://ntfy.sh/fine-{}", &id[..20]),
        token: None,
    }
}

pub async fn init(paths: &Paths, config: &mut Config, yes: bool) -> Result<()> {
    let exe = std::env::current_exe()?.canonicalize()?;
    let mut store = Database::open(&paths.db)?.connect()?;
    println!("fine · configurazione guidata\n");

    // 1. Bank
    let accounts = store.db().account_count()?;
    if accounts > 0 && config.enable_banking.is_some() {
        println!("1. Banca: {accounts} conti già collegati.");
    }
    if config.enable_banking.is_none() {
        println!("1. Collega la banca con Enable Banking (gratis per uso personale):");
        println!(
            "   - crea un'applicazione Production su https://enablebanking.com/cp/applications"
        );
        println!("   - collega i tuoi conti nel pannello e scarica la chiave .pem");
        let key = PathBuf::from(
            prompt("   Trascina qui il file .pem e premi invio: ").trim_matches(['\'', '"', ' ']),
        );
        let key = key
            .canonicalize()
            .with_context(|| format!("file non trovato: {}", key.display()))?;
        let app_id = match app_id_from_key(&key) {
            Some(id) => id,
            None => prompt("   Application id: "),
        };
        let eb = EnableBankingConfig {
            app_id,
            key_path: key,
            redirect_url: String::new(),
            api_base: DEFAULT_API_BASE.to_owned(),
        };
        let app = Client::new(&eb)?
            .application()
            .await
            .context("verifica dell'applicazione")?;
        println!(
            "   Applicazione \"{}\" verificata.",
            app.name.as_deref().unwrap_or("?")
        );
        config.enable_banking = Some(eb);
        config.save(paths)?;
    }
    let eb = config.enable_banking()?.clone();
    if store.db().account_count()? == 0 || (!yes && ask_no("   Vuoi collegare un'altra banca?")) {
        choose_and_connect(&mut store, &eb).await?;
    }

    // 2. First sync
    println!("\n2. Scarico i movimenti...");
    let report = sync::sync_all(&mut store, &Client::new(&eb)?).await?;
    let mut sync_failures = 0;
    for a in &report.accounts {
        match (&a.skipped, &a.error) {
            (Some(s), _) => println!("   {}: saltato ({s})", a.account),
            (_, Some(e)) => {
                sync_failures += 1;
                println!("   ✗ {}: {}", a.account, explain_sync_error(e));
            }
            _ => println!("   ✓ {}: {} transazioni", a.account, a.inserted),
        }
    }
    println!(
        "   Categorizzate automaticamente: {}",
        report.auto_categorized
    );
    if let Some(backend) = ai::Backend::detect() {
        println!(
            "   Chiedo a {} di categorizzare il resto...",
            backend.name()
        );
        match ai::categorize_pending(&mut store, &backend).await {
            Ok(Some(o)) => println!(
                "   ✓ AI: {} esaminati, {} categorizzati, {} nuove regole",
                o.examined,
                o.categorized,
                o.rules_created.len()
            ),
            Ok(None) => {}
            Err(e) => println!("   ✗ AI: {} (riprovo al prossimo sync)", e.chain()),
        }
    }

    // 3. AI clients
    println!();
    if ask(
        "3. Registrare fine nei client AI trovati (Claude Code, Codex, Claude Desktop)?",
        yes,
    ) {
        for line in register_clients(&exe) {
            println!("   {line}");
        }
    }

    // 4. Schedule + notifications
    println!();
    let notify = ask(
        &format!(
            "4. Notifica giornaliera sul telefono alle {:02}:{:02} (app gratuita ntfy)?",
            NOTIFY_TIME.0, NOTIFY_TIME.1
        ),
        yes,
    );
    if notify && config.ntfy.is_none() {
        config.ntfy = Some(new_ntfy_topic());
        config.save(paths)?;
    }
    match install_schedule(paths, &exe, notify, true) {
        Ok(msg) => println!("   Pianificato: {msg}."),
        Err(e) => println!("   Pianificazione non installata: {e:#}"),
    }
    if let (true, Some(n)) = (notify, &config.ntfy) {
        println!("   Installa l'app ntfy e iscriviti a: {}", n.url);
        println!("   (il nome del topic è la tua chiave: non condividerlo)");
    }

    if sync_failures > 0 {
        let one = sync_failures == 1;
        println!(
            "\nConfigurazione completata, ma {sync_failures} {} NON sincronizzat{}: i dati sono incompleti.",
            if one { "conto" } else { "conti" },
            if one { "o" } else { "i" }
        );
        println!(
            "Dettagli con `fine status`; riprova con `fine sync` (o attendi la prossima sincronizzazione automatica)."
        );
        bail!("sincronizzazione non riuscita per {sync_failures} conti");
    }
    println!("\nFatto! Prova:");
    println!("   fine status        quanto puoi spendere");
    println!("   fine serve         dashboard su http://127.0.0.1:7878");
    println!("   e chiedi a Claude: \"aiutami a impostare il piano mensile\"");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_id_comes_from_key_filename() {
        let id = "3fa85f64-5717-4562-b3fc-2c963f66afa6";
        assert_eq!(
            app_id_from_key(Path::new(&format!("/x/{id}.pem"))).as_deref(),
            Some(id)
        );
        assert_eq!(app_id_from_key(Path::new("/x/private.pem")), None);
    }
}
