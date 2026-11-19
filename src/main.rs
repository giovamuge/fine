mod cli;

use std::process::ExitCode;

#[tokio::main]
async fn main() -> anyhow::Result<ExitCode> {
    cli::run().await
}
