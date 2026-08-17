use std::io::Write as _;
use std::path::PathBuf;

use clap::Args;

use crate::commands::{Status, warn};
use crate::core::error::AppError;
use crate::core::state::read_state;

mod render;

use render::export_toml;

#[derive(Args)]
pub struct ExportArgs {
    /// File to write (default: stdout)
    #[arg(short, long, default_value = "-")]
    output: String,
    /// Overwrite an existing output file
    #[arg(long)]
    force: bool,
}

pub fn run(args: ExportArgs) -> Result<Status, AppError> {
    let state = read_state()?;
    let (document, notes) = export_toml(&state);
    for note in &notes {
        warn(note);
    }

    if args.output == "-" {
        std::io::stdout().write_all(document.as_bytes())?;
        return Ok(Status::Ok);
    }

    let path = PathBuf::from(&args.output);
    if path.exists() && !args.force {
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
