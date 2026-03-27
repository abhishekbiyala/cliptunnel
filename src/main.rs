use clap::Parser;
use cliptunnel::cli::{Cli, Command};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(&cli.log_level)),
        )
        .init();

    match cli.command {
        Command::Setup {
            host,
            x11,
            binary,
            arch,
        } => {
            cliptunnel::setup::run(&host, x11, binary.as_deref(), &arch).await?;
        }
        Command::Daemon {
            foreground: _,
            install,
            uninstall,
            port,
        } => {
            #[cfg(feature = "daemon")]
            {
                if install {
                    cliptunnel::daemon::launchd::install(port)?;
                } else if uninstall {
                    cliptunnel::daemon::launchd::uninstall()?;
                } else {
                    cliptunnel::daemon::run_foreground(port).await?;
                }
            }
            #[cfg(not(feature = "daemon"))]
            {
                let _ = (install, uninstall, port);
                anyhow::bail!("daemon command requires the 'daemon' feature (macOS only)");
            }
        }
        Command::Connect {
            host,
            x11,
            binary,
            arch,
        } => {
            cliptunnel::connect::run(&host, x11, binary.as_deref(), &arch).await?;
        }
        Command::Disconnect { host, clean } => {
            cliptunnel::disconnect::run(&host, clean).await?;
        }
        Command::Doctor { host } => {
            cliptunnel::doctor::run(host.as_deref()).await?;
        }
        Command::InstallRemote { x11 } => {
            cliptunnel::remote::install::run(x11).await?;
        }
        Command::Gc { max_age } => {
            cliptunnel::remote::gc::run(max_age)?;
        }
        Command::Tunnel {
            host,
            install,
            uninstall,
            port,
        } => {
            cliptunnel::tunnel::run(&host, install, uninstall, port).await?;
        }
        Command::Paste { path, tmux, url } => {
            cliptunnel::paste::run(path.as_deref(), tmux, &url).await?;
        }
    }

    Ok(())
}
