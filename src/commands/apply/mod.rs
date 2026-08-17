use std::io::Write as _;
use std::path::PathBuf;
use std::process::Command;

use clap::Args;

use crate::commands::{Status, warn};
use crate::core::config::{default_path, load_layout};
use crate::core::error::{AppError, ConfigError};
use crate::core::resolve::resolve;
use crate::core::state::read_state;

mod gdctl;

use gdctl::build_command;

#[derive(Args)]
pub struct ApplyArgs {
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
}

pub fn run(args: ApplyArgs) -> Result<Status, AppError> {
    let config = args.config.unwrap_or_else(default_path);
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
    let persistent = !args.no_persistent && !args.verify;
    let cmd = build_command(&resolved, &layout, persistent, args.verify);

    if args.dry_run {
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
    if args.verify {
        warn("verified only; nothing was applied");
    }
    Ok(Status::Ok)
}
