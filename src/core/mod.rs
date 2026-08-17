//! Domain logic shared across commands: the config schema, D-Bus state
//! reading, and matching declared screens to connected monitors. None of
//! this knows about the CLI — see `commands/` for that.

pub mod config;
pub mod error;
pub mod resolve;
pub mod state;
