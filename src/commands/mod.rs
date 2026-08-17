//! The CLI's subcommands, one folder per command.
//!
//! Each command owns a folder holding its `clap::Args` struct, its `run`
//! entry point, and any helper modules only it needs (e.g.
//! `apply::gdctl`). Adding a command means adding a folder here and a
//! variant below; adding a parameter means adding a field to that
//! command's `Args` struct.

pub mod apply;
pub mod export;

use clap::Subcommand;

use crate::core::error::AppError;

#[derive(Subcommand)]
pub enum Command {
    /// Write the live layout as TOML
    Export(export::ExportArgs),
    /// Apply a layout from TOML
    Apply(apply::ApplyArgs),
}

/// Outcome of a command that isn't itself an error: `Ok` maps to exit code 0,
/// `Failed` to exit code 1. `AppError::Config` carries exit code 2 on its own.
pub enum Status {
    Ok,
    Failed,
}

pub fn warn(message: &str) {
    eprintln!("haichi: {message}");
}

pub fn dispatch(command: Command) -> Result<Status, AppError> {
    match command {
        Command::Export(args) => export::run(args),
        Command::Apply(args) => apply::run(args),
    }
}
