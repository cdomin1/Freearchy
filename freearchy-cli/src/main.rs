use anyhow::Result;
use clap::{Parser, Subcommand};

mod doctor;
mod image;
mod install;
mod packages;
mod theme;
mod util;

#[derive(Parser)]
#[command(
    name = "freearchy",
    version,
    about = "Opinionated Hyprland + Caelestia desktop for FreeBSD"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Install freearchy onto a FreeBSD system
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
enum ImageCommand {
    /// Build a bootable FreeBSD installer image with freearchy baked in
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
        Command::Doctor => doctor::run(),
        Command::Image { command } => match command {
            ImageCommand::Build { release } => image::build(&release),
        },
    }
}
