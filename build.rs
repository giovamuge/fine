//! Builds the web dashboard (web/ → web/dist) so that `src/web.rs` can embed it.
//!
//! Runs `npm ci` (when node_modules is missing) and `npm run build` whenever the frontend
//! sources change. Node.js and npm are required to build `fine`: see README, "Sviluppo".
//! Set `FINE_SKIP_WEB_BUILD=1` to compile without Node (the dashboard then shows a
//! placeholder page; the CLI and the MCP server are unaffected).

use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    let root = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let web = root.join("web");
    let dist = web.join("dist");

    for p in [
        "web/src",
        "web/public",
        "web/index.html",
        "web/package.json",
        "web/package-lock.json",
        "web/vite.config.ts",
        "web/tsconfig.json",
    ] {
        println!("cargo:rerun-if-changed={p}");
    }
    println!("cargo:rerun-if-env-changed=FINE_SKIP_WEB_BUILD");

    if std::env::var_os("FINE_SKIP_WEB_BUILD").is_some_and(|v| !v.is_empty() && v != "0") {
        placeholder(&dist);
        println!("cargo:warning=FINE_SKIP_WEB_BUILD: dashboard non compilata (pagina segnaposto)");
        return;
    }

    let npm = if cfg!(windows) { "npm.cmd" } else { "npm" };
    if Command::new(npm).arg("--version").output().is_err() {
        panic!(
            "\n\nnpm non trovato: per compilare la dashboard di fine serve Node.js (LTS) con npm.\n\
             Installa Node da https://nodejs.org oppure compila senza dashboard con FINE_SKIP_WEB_BUILD=1.\n\
             (npm not found: Node.js is required to build the web dashboard; install it or set FINE_SKIP_WEB_BUILD=1.)\n"
        );
    }

    if !web.join("node_modules").is_dir() {
        let lock = web.join("package-lock.json").is_file();
        run(
            npm,
            if lock {
                &["ci", "--no-audit", "--no-fund"]
            } else {
                &["install", "--no-audit", "--no-fund"]
            },
            &web,
        );
    }
    run(npm, &["run", "build", "--silent"], &web);

    if !dist.join("index.html").is_file() {
        panic!("la build del frontend non ha prodotto web/dist/index.html");
    }
}

fn run(npm: &str, args: &[&str], dir: &Path) {
    let status = Command::new(npm)
        .args(args)
        .current_dir(dir)
        // Keep npm quiet and non-interactive inside cargo.
        .env("CI", "1")
        .env("NO_COLOR", "1")
        .status()
        .unwrap_or_else(|e| panic!("impossibile eseguire `npm {}`: {e}", args.join(" ")));
    if !status.success() {
        panic!(
            "\n\n`npm {}` in {} è fallito ({status}): controlla l'output qui sopra.\n",
            args.join(" "),
            dir.display()
        );
    }
}

/// A minimal dist so that the embed macro has something to include.
fn placeholder(dist: &Path) {
    if dist.join("index.html").is_file() {
        return;
    }
    std::fs::create_dir_all(dist).expect("mkdir web/dist");
    std::fs::write(
        dist.join("index.html"),
        "<!doctype html><html lang=\"it\"><meta charset=\"utf-8\"><title>fine</title>\
         <p>Dashboard non compilata: ricompila fine con Node.js installato (vedi README, Sviluppo).</p>",
    )
    .expect("write placeholder");
}
