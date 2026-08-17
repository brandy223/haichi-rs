use clap::Parser;

use crate::commands::Command;

#[derive(Parser)]
#[command(
    name = "haichi",
    about = "Apply a declarative screen layout from TOML via gdctl."
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}
