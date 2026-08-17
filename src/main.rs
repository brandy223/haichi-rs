//! Apply a declarative screen layout from TOML via `gdctl`.
//!
//! Screens are keyed on EDID identity (vendor/product/serial) and resolved to
//! a connector at runtime, because connector names are not stable across
//! kernel, GPU and dock topology changes.

mod cli;
mod commands;
mod core;

use std::process::ExitCode;

use clap::Parser;

use cli::Cli;
use commands::{Status, dispatch, warn};
use core::error::AppError;

const EXIT_OK: u8 = 0;
const EXIT_FAILED: u8 = 1;
const EXIT_CONFIG: u8 = 2;

fn main() -> ExitCode {
    let cli = Cli::parse();

    let result = dispatch(cli.command);

    match result {
        Ok(Status::Ok) => ExitCode::from(EXIT_OK),
        Ok(Status::Failed) => ExitCode::from(EXIT_FAILED),
        Err(AppError::Config(e)) => {
            for problem in &e.problems {
                warn(problem);
            }
            ExitCode::from(EXIT_CONFIG)
        }
        Err(err) => {
            warn(&err.to_string());
            ExitCode::from(EXIT_FAILED)
        }
    }
}
