use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "cliptunnel",
    version,
    about = "Forward Mac clipboard images to remote Linux devboxes over SSH"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,

    /// Log level (trace, debug, info, warn, error)
    #[arg(long, global = true, default_value = "info", env = "CLIPTUNNEL_LOG")]
    pub log_level: String,
}

#[derive(Subcommand)]
pub enum Command {
    /// One-command setup: install daemon, deploy to remote, verify
    Setup {
        /// SSH destination (user@host or Host alias)
        host: String,

        /// Enable X11 bridge (Xvfb) on remote for Codex CLI support
        #[arg(long)]
        x11: bool,

        /// Path to pre-built Linux binary (skip download)
        #[arg(long)]
        binary: Option<PathBuf>,

        /// Target architecture for remote (x86_64 or aarch64)
        #[arg(long, default_value = "x86_64")]
        arch: String,
    },

    /// Start the local clipboard daemon (Mac)
    Daemon {
        /// Run in foreground (don't daemonize)
        #[arg(long)]
        foreground: bool,

        /// Install launchd plist for auto-start
        #[arg(long)]
        install: bool,

        /// Uninstall launchd plist
        #[arg(long)]
        uninstall: bool,

        /// Port to listen on
        #[arg(long, default_value = "18442", env = "CLIPTUNNEL_PORT")]
        port: u16,
    },

    /// Configure SSH and deploy to a remote host
    Connect {
        /// SSH destination (user@host or Host alias)
        host: String,

        /// Enable X11 bridge (Xvfb) on remote for Codex CLI support
        #[arg(long)]
        x11: bool,

        /// Path to pre-built Linux binary (skip cross-compile lookup)
        #[arg(long)]
        binary: Option<PathBuf>,

        /// Target architecture for remote (x86_64 or aarch64)
        #[arg(long, default_value = "x86_64")]
        arch: String,
    },

    /// Remove SSH forwarding and optionally clean up remote
    Disconnect {
        /// SSH destination
        host: String,

        /// Also remove remote shims and binary
        #[arg(long)]
        clean: bool,
    },

    /// Run diagnostics
    Doctor {
        /// Also check remote host connectivity
        #[arg(long)]
        host: Option<String>,
    },

    /// Install shims on remote (called via SSH by connect, not directly)
    #[command(name = "install-remote")]
    InstallRemote {
        /// Enable X11 bridge mode
        #[arg(long)]
        x11: bool,
    },

    /// Garbage collect old temp images on remote
    Gc {
        /// Max age in minutes before deletion
        #[arg(long, default_value = "30")]
        max_age: u64,
    },

    /// Keep a persistent SSH tunnel to a remote host (for mosh users)
    Tunnel {
        /// SSH destination (user@host or Host alias)
        host: String,

        /// Install as launchd service for auto-start
        #[arg(long)]
        install: bool,

        /// Uninstall launchd tunnel service
        #[arg(long)]
        uninstall: bool,

        /// Port to forward
        #[arg(long, default_value = "18442", env = "CLIPTUNNEL_PORT")]
        port: u16,
    },

    /// Manually fetch and save clipboard image
    Paste {
        /// Output file path (default: auto-generated in /tmp)
        #[arg(long)]
        path: Option<PathBuf>,

        /// Send path to current tmux pane
        #[arg(long)]
        tmux: bool,

        /// Daemon URL
        #[arg(long, default_value = "http://127.0.0.1:18442", env = "CLIPTUNNEL_URL")]
        url: String,
    },
}
