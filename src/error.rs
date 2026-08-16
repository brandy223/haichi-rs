use std::fmt;

/// The TOML does not describe a layout that can be applied.
///
/// Carries every problem found, not just the first, so a single run reports
/// everything wrong with a config instead of making the user fix-and-rerun
/// one error at a time.
#[derive(Debug)]
pub struct ConfigError {
    pub problems: Vec<String>,
}

impl ConfigError {
    pub fn new(problems: Vec<String>) -> Self {
        Self { problems }
    }

    pub fn single(problem: impl Into<String>) -> Self {
        Self {
            problems: vec![problem.into()],
        }
    }
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.problems.join("\n"))
    }
}

impl std::error::Error for ConfigError {}

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error(transparent)]
    Config(#[from] ConfigError),
    #[error("D-Bus call failed: {0}")]
    DBus(#[from] zbus::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}
