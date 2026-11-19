//! The AI clients fine can drive headless: Claude Code (`claude -p`) and Codex (`codex exec`).
//! Both run with no tools, outside any project, and must answer with JSON matching a schema.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use serde::de::DeserializeOwned;
use serde_json::Value;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

use crate::error::{Error, Result};

const TIMEOUT: Duration = Duration::from_secs(300);

#[derive(Debug, Clone)]
pub enum Backend {
    Claude(PathBuf),
    Codex(PathBuf),
}

impl Backend {
    /// Claude Code first, then Codex. `None` when neither CLI is installed.
    pub fn detect() -> Option<Self> {
        find_in_path("claude")
            .map(Self::Claude)
            .or_else(|| find_in_path("codex").map(Self::Codex))
    }

    pub fn name(&self) -> &'static str {
        match self {
            Self::Claude(_) => "Claude Code",
            Self::Codex(_) => "Codex",
        }
    }

    /// Ask for an answer matching `schema`, with `input` as the prompt.
    pub(super) async fn ask<T: DeserializeOwned>(&self, input: &str, schema: &Value) -> Result<T> {
        // Run outside any project so no CLAUDE.md / AGENTS.md / project config is picked up.
        let dir = ScratchDir::new()?;
        let schema = schema.to_string();
        match self {
            Self::Claude(bin) => {
                let out = run(
                    Command::new(bin).current_dir(dir.path()).args([
                        "-p",
                        "--model",
                        "sonnet",
                        "--tools",
                        "",
                        "--strict-mcp-config",
                        "--setting-sources",
                        "",
                        "--no-session-persistence",
                        "--output-format",
                        "json",
                        "--json-schema",
                        &schema,
                    ]),
                    input,
                )
                .await?;
                let v: Value = serde_json::from_str(&out)
                    .map_err(|e| Error::Ai(format!("risposta di Claude non valida: {e}")))?;
                if v["is_error"].as_bool() == Some(true) {
                    return Err(Error::Ai(format!(
                        "Claude: {}",
                        v["result"].as_str().unwrap_or("errore")
                    )));
                }
                serde_json::from_value(v["structured_output"].clone()).map_err(|e| {
                    Error::Ai(format!(
                        "Claude non ha restituito il formato richiesto: {e}"
                    ))
                })
            }
            Self::Codex(bin) => {
                let schema_file = dir.path().join("schema.json");
                let answer_file = dir.path().join("answer.json");
                std::fs::write(&schema_file, &schema)?;
                run(
                    Command::new(bin)
                        .current_dir(dir.path())
                        .args([
                            "exec",
                            "--skip-git-repo-check",
                            "--ephemeral",
                            "--sandbox",
                            "read-only",
                            "--output-schema",
                        ])
                        .arg(&schema_file)
                        .arg("-o")
                        .arg(&answer_file)
                        .arg("-"),
                    input,
                )
                .await?;
                let text = std::fs::read_to_string(&answer_file)
                    .map_err(|e| Error::Ai(format!("Codex non ha prodotto una risposta: {e}")))?;
                serde_json::from_str(&text).map_err(|e| {
                    Error::Ai(format!("Codex non ha restituito il formato richiesto: {e}"))
                })
            }
        }
    }
}

fn find_in_path(bin: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|d| d.join(bin))
        .find(|p| p.is_file())
}

/// Run a command with `input` on stdin; its stdout, or an error with what it printed.
async fn run(cmd: &mut Command, input: &str) -> Result<String> {
    let mut child = cmd
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| Error::Ai(format!("avvio del client AI: {e}")))?;
    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| Error::Ai("stdin del client AI".into()))?;
    stdin.write_all(input.as_bytes()).await?;
    drop(stdin);
    let out = tokio::time::timeout(TIMEOUT, child.wait_with_output())
        .await
        .map_err(|_| Error::Ai("il client AI non ha risposto entro 5 minuti".into()))??;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        let msg = if err.trim().is_empty() {
            String::from_utf8_lossy(&out.stdout)
        } else {
            err
        };
        return Err(Error::Ai(format!(
            "il client AI ha restituito un errore: {}",
            msg.trim()
        )));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// A private temporary directory, removed when dropped (also on errors and cancellation).
struct ScratchDir(PathBuf);

impl ScratchDir {
    fn new() -> Result<Self> {
        let dir = std::env::temp_dir().join(format!("fine-ai-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir)?;
        Ok(Self(dir))
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for ScratchDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
