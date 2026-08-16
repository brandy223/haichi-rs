use std::io::Write as _;
use std::path::PathBuf;
use std::process::Command;

use crate::config::{default_path, load_layout};
use crate::error::{AppError, ConfigError};
use crate::export::export_toml;
use crate::gdctl::build_command;
use crate::resolve::resolve;
use crate::state::read_state;

/// Outcome of a command that isn't itself an error: `Ok` maps to exit code 0,
/// `Failed` to exit code 1. `AppError::Config` carries exit code 2 on its own.
pub enum Status {
    Ok,
    Failed,
}

pub fn warn(message: &str) {
    eprintln!("haichi: {message}");
}

pub fn cmd_export(output: &str, force: bool) -> Result<Status, AppError> {
    let state = read_state()?;
    let (document, notes) = export_toml(&state);
    for note in &notes {
        warn(note);
    }

    if output == "-" {
        std::io::stdout().write_all(document.as_bytes())?;
        return Ok(Status::Ok);
    }

    let path = PathBuf::from(output);
    if path.exists() && !force {
        warn(&format!(
            "{} exists; pass --force to overwrite",
            path.display()
        ));
        return Ok(Status::Failed);
    }
    std::fs::write(&path, &document)?;
    warn(&format!("wrote {}", path.display()));
    Ok(Status::Ok)
}

pub fn cmd_apply(
    config: Option<PathBuf>,
    dry_run: bool,
    verify: bool,
    no_persistent: bool,
) -> Result<Status, AppError> {
    let config = config.unwrap_or_else(default_path);
    let layout = load_layout(&config)?;
    let state = read_state()?;

    if let Some(declared) = &layout.layout_mode {
        if Some(declared) != state.layout_mode.as_ref() && !state.supports_changing_layout_mode {
            return Err(ConfigError::single(format!(
                "layout-mode is {declared:?} but this session cannot change layout mode \
                 (currently {:?})",
                state.layout_mode
            ))
            .into());
        }
    }

    let (resolved, absent) = resolve(&layout, &state)?;

    if !absent.is_empty() {
        warn(&format!(
            "not applying: declared screen(s) not connected: {}",
            absent.join(", ")
        ));
        return Ok(Status::Ok);
    }

    let keep: Vec<&str> = resolved
        .iter()
        .map(|item| item.monitor.connector.as_str())
        .collect();
    for monitor in &state.monitors {
        if !keep.contains(&monitor.connector.as_str()) {
            warn(&format!(
                "{} is connected but not in the layout; applying will switch it off",
                monitor.describe()
            ));
        }
    }

    // Mutter rejects a config that is both persistent and verify-only, and a
    // verify never writes anything anyway.
    let persistent = !no_persistent && !verify;
    let cmd = build_command(&resolved, &layout, persistent, verify);

    if dry_run {
        let joined = shlex::try_join(cmd.iter().map(String::as_str))
            .expect("gdctl arguments are plain strings, never containing a NUL byte");
        writeln!(std::io::stdout(), "{joined}")?;
        return Ok(Status::Ok);
    }

    let status = Command::new(&cmd[0]).args(&cmd[1..]).status()?;
    if !status.success() {
        let joined = shlex::try_join(cmd.iter().map(String::as_str))
            .expect("gdctl arguments are plain strings, never containing a NUL byte");
        warn(&format!(
            "gdctl exited {}: {joined}",
            status.code().unwrap_or(-1)
        ));
        return Ok(Status::Failed);
    }
    if verify {
        warn("verified only; nothing was applied");
    }
    Ok(Status::Ok)
}
