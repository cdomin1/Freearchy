use anyhow::Result;
use clap::{Parser, Subcommand};

mod doctor;
mod gpu;
mod image;
mod install;
mod install_system;
mod install_tui;
mod packages;
mod power;
mod theme;
mod util;
mod web_install;

#[derive(Parser)]
#[command(
    name = "vellum",
    version,
    about = "Opinionated Omarchy-style X11 tiling desktop for FreeBSD"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Install vellum onto a FreeBSD system
    Install {
        /// Write configs only, skip pkg installation
        #[arg(long)]
        no_packages: bool,
        /// Install the optional dev tool manifest too
        #[arg(long)]
        dev: bool,
        /// Skip building/installing the Caelestia shell
        #[arg(long)]
        no_shell: bool,
    },
    /// Re-apply configs and theme from the repo (after a git pull)
    Update,
    /// Manage themes
    Theme {
        #[command(subcommand)]
        command: ThemeCommand,
    },
    /// Install FreeBSD + vellum onto a disk (runs on the live installer
    /// image). Interactive wizard by default; fully flag-driven with --yes.
    InstallSystem {
        /// Target disk, e.g. nda0 or da0. Prompted if omitted.
        #[arg(long)]
        disk: Option<String>,
        #[arg(long)]
        hostname: Option<String>,
        /// Login user to create (wheel+video). Prompted if omitted.
        #[arg(long)]
        user: Option<String>,
        /// Non-interactive: accept defaults, no prompts. USE WITH CARE.
        #[arg(long)]
        yes: bool,
        /// Serve a browser-based installer UI instead of the TUI
        #[arg(long)]
        web: bool,
        /// Port for --web
        #[arg(long, default_value = "8080")]
        port: u16,
    },
    /// Power profile: status, or set performance|balanced|power
    Power {
        #[command(subcommand)]
        command: PowerCommand,
    },
    /// Check the system for common problems
    Doctor,
    /// Build a bootable installer image
    Image {
        #[command(subcommand)]
        command: ImageCommand,
    },
}

#[derive(Subcommand)]
enum ThemeCommand {
    /// List available themes
    List,
    /// Apply a theme by name
    Set { name: String },
    /// Cycle to the next theme
    Next,
}

#[derive(Subcommand)]
enum PowerCommand {
    /// Show AC/battery, CPU freq, EPP hint, charge state
    Status,
    /// Apply a profile (devd calls this on AC events too)
    Set { mode: String },
    /// ThinkPad charge thresholds via acpi_call (e.g. --stop 80)
    Battery {
        /// Charge start threshold percent
        #[arg(long)]
        start: Option<u8>,
        /// Charge stop threshold percent
        #[arg(long)]
        stop: Option<u8>,
    },
}

#[derive(Subcommand)]
enum ImageCommand {
    /// Build a bootable FreeBSD installer image with vellum baked in
    Build {
        /// FreeBSD release to base on, e.g. 15.1
        #[arg(long, default_value = "15.1")]
        release: String,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Install { no_packages, dev, no_shell } => {
            install::run(no_packages, dev, no_shell)
        }
        Command::Update => install::update(),
        Command::Theme { command } => match command {
            ThemeCommand::List => theme::list(),
            ThemeCommand::Set { name } => theme::set(&name),
            ThemeCommand::Next => theme::next(),
        },
        Command::InstallSystem { disk, hostname, user, yes, web, port } => {
            if web {
                web_install::serve(port)
            } else {
                install_system::run(disk, hostname, user, yes)
            }
        }
        Command::Power { command } => match command {
            PowerCommand::Status => power::status(),
            PowerCommand::Set { mode } => power::set(&mode),
            PowerCommand::Battery { start, stop } => power::battery(start, stop),
        },
        Command::Doctor => doctor::run(),
        Command::Image { command } => match command {
            ImageCommand::Build { release } => image::build(&release),
        },
    }
}
