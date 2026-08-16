use std::path::PathBuf;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "haichi",
    about = "Apply a declarative screen layout from TOML via gdctl."
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
pub enum Command {
    /// Write the live layout as TOML
    Export {
        /// File to write (default: stdout)
        #[arg(short, long, default_value = "-")]
        output: String,
        /// Overwrite an existing output file
        #[arg(long)]
        force: bool,
    },
    /// Apply a layout from TOML
    Apply {
        /// Path to the layout TOML (default: $XDG_CONFIG_HOME/haichi/config.toml,
        /// or ~/.config/haichi/config.toml if XDG_CONFIG_HOME is unset)
        config: Option<PathBuf>,
        /// Print the gdctl command instead of running it
        #[arg(short = 'n', long)]
        dry_run: bool,
        /// Ask gdctl to validate the layout without applying
        #[arg(short = 'V', long)]
        verify: bool,
        /// Do not write the layout to monitors.xml (it will be lost on hotplug, wake or login)
        #[arg(long)]
        no_persistent: bool,
    },
}
