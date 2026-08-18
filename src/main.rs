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

use crate::core::error::{AppError, ConfigError};
use cli::Cli;
use commands::{Status, dispatch, warn};

const EXIT_OK: u8 = 0;
const EXIT_FAILED: u8 = 1;
const EXIT_CONFIG: u8 = 2;

fn main() -> ExitCode {
    let cli = Cli::parse();

    let result = dispatch(cli.command);

    match result {
        Ok(Status::Ok) => ExitCode::from(EXIT_OK),
        Ok(Status::Failed) => ExitCode::from(EXIT_FAILED),
        // code-review follow-up (Copilot, PR #8, suppressed but still
        // valid): `InvalidFieldValues`'s Display impl uses `{0:?}` (its
        // only sane option — thiserror's format strings can't call
        // `.join()` on a field), so `e.to_string()` alone would collapse
        // every problem into one Rust-debug-formatted line
        // (`["...", "...", ...]`) instead of the one-`haichi:`-line-per-
        // problem output this error type's own doc comment promises.
        Err(AppError::Config(ConfigError::InvalidFieldValues(problems))) => {
            for problem in &problems {
                warn(problem);
            }
            ExitCode::from(EXIT_CONFIG)
        }
        Err(AppError::Config(e)) => {
            warn(&e.to_string());
            ExitCode::from(EXIT_CONFIG)
        }
        Err(err) => {
            warn(&err.to_string());
            ExitCode::from(EXIT_FAILED)
        }
    }
}
