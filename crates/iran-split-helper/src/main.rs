use clap::Parser;
use std::path::PathBuf;
#[cfg(unix)]
use tracing_subscriber::EnvFilter;

#[derive(Debug, Parser)]
#[command(version, about = "Privileged service for Iran Split Desktop")]
struct Arguments {
    /// Root-owned helper configuration written by the installer.
    #[arg(long, default_value = "/etc/iran-split/helper.toml")]
    config: PathBuf,
}

#[cfg(unix)]
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .json()
        .init();
    let arguments = Arguments::parse();
    iran_split_helper::run_linux(&arguments.config).await?;
    Ok(())
}

#[cfg(windows)]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _arguments = Arguments::parse();
    iran_split_helper::windows::run_service()?;
    Ok(())
}

#[cfg(not(any(unix, windows)))]
fn main() {
    eprintln!("iran-split-helper is unsupported on this target");
    std::process::exit(1);
}
