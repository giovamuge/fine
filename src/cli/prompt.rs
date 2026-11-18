//! Interactive questions on the terminal.

use std::io::{BufRead, Write};
use std::process::Command;

pub fn ask(question: &str, yes: bool) -> bool {
    if yes {
        println!("{question} [S/n] s");
        return true;
    }
    let answer = prompt(&format!("{question} [S/n] "));
    !matches!(answer.trim().to_lowercase().as_str(), "n" | "no")
}

/// Question whose default answer is "no".
pub fn ask_no(question: &str) -> bool {
    matches!(
        prompt(&format!("{question} [s/N] "))
            .trim()
            .to_lowercase()
            .as_str(),
        "s" | "si" | "sì" | "y" | "yes"
    )
}

pub fn prompt(text: &str) -> String {
    print!("{text}");
    let _ = std::io::stdout().flush();
    let mut line = String::new();
    let _ = std::io::stdin().lock().read_line(&mut line);
    line.trim().to_string()
}

/// Open a link in the default browser (best effort).
pub fn open_browser(url: &str) {
    let cmd = if cfg!(target_os = "macos") {
        Command::new("open").arg(url).status()
    } else if cfg!(target_os = "windows") {
        Command::new("cmd").args(["/C", "start", "", url]).status()
    } else {
        Command::new("xdg-open").arg(url).status()
    };
    if cmd.is_err() {
        eprintln!("(apri il link manualmente)");
    }
}
