use std::io::Write as _;
use std::os::unix::process::ExitStatusExt as _;
use std::path::PathBuf;
use std::process::Command;

use clap::Args;

use crate::commands::{Status, warn};
use crate::core::config::{default_path, load_layout};
use crate::core::error::AppError;
use crate::core::resolve::resolve;
use crate::core::state::read_state;

mod gdctl;

use gdctl::{build_command, build_pref_commands};

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

/// Returns a shell-escaped string describing the command, suitable for logging.
fn describe_cmd(cmd: &[String]) -> String {
    shlex::try_join(cmd.iter().map(String::as_str))
        .expect("gdctl arguments are plain strings, never containing a NUL byte")
}

/// Runs the `gdctl` command and reports any failure.
fn run_gdctl(cmd: &[String]) -> Result<bool, AppError> {
    let status = Command::new(&cmd[0]).args(&cmd[1..]).status()?;
    if status.success() {
        return Ok(true);
    }
    // code-review follow-up (Copilot, PR #8): `status.code()` is `None` when
    // the process was killed by a signal, not just "exited with an unusual
    // code" — `unwrap_or(-1)` printed a fake "-1" exit code for that case,
    // which isn't a real exit code gdctl could ever produce (POSIX exit
    // codes are 0-255) and reads as if gdctl chose to exit that way.
    let reason = match status.code() {
        Some(code) => format!("exited {code}"),
        None => match status.signal() {
            Some(signal) => format!("was killed by signal {signal}"),
            None => "exited for an unknown reason".to_string(),
        },
    };
    warn(&format!("gdctl {reason}: {}", describe_cmd(cmd)));
    Ok(false)
}

pub fn run(args: ApplyArgs) -> Result<Status, AppError> {
    let config = args.config.unwrap_or_else(default_path);
    let layout = load_layout(&config)?;
    let state = read_state()?;

    if let Some(declared) = &layout.layout_mode {
        if Some(declared) != state.layout_mode.as_ref() && !state.supports_changing_layout_mode {
            return Err(AppError::IncoherentState(format!(
                "layout-mode is {declared:?} but this session cannot change layout mode \
                 (currently {:?})",
                state.layout_mode
            )));
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
    // `gdctl pref` has no --verify *or* --persistent of its own — per
    // gdctl(1) it always writes for real, unconditionally, the moment it
    // runs. So it is skipped both under --verify (nothing should be applied)
    // and under --no-persistent. A screen with a declared luminance gets
    // a warning instead, so the skip isn't silent either.
    let pref_cmds = if args.verify {
        Vec::new()
    } else if args.no_persistent {
        if resolved.iter().any(|item| item.screen.luminance.is_some()) {
            warn("--no-persistent: not setting luminance — gdctl pref has no non-persistent mode");
        }
        Vec::new()
    } else {
        build_pref_commands(&resolved)
    };

    if args.dry_run {
        for cmd in std::iter::once(&cmd).chain(&pref_cmds) {
            writeln!(std::io::stdout(), "{}", describe_cmd(cmd))?;
        }
        return Ok(Status::Ok);
    }

    if !run_gdctl(&cmd)? {
        return Ok(Status::Failed);
    }
    if args.verify {
        warn("verified only; nothing was applied");
    }

    let mut pref_failed = false;
    for cmd in &pref_cmds {
        if !run_gdctl(cmd)? {
            pref_failed = true;
        }
    }

    Ok(if pref_failed {
        Status::Failed
    } else {
        Status::Ok
    })
}
