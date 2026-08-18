use thiserror::Error;

/// The TOML does not describe a layout that can be applied.
///
/// Carries every problem found, not just the first, so a single run reports
/// everything wrong with a config instead of making the user fix-and-rerun
/// one error at a time.
#[derive(Debug, Error)]
pub enum ConfigError {
    /// The config file is empty.
    #[error("The config file is empty: {0}")]
    EmptyConfig(String),
    /// Validation failed for one or more fields.
    #[error("Validation failed for one or more fields: {0:?}")]
    InvalidFieldValues(Vec<String>),
    /// The TOML is not valid.
    #[error(r#"The TOML at "{path}" is not valid: {source}"#)]
    InvalidFormat {
        path: String,
        #[source]
        source: toml::de::Error,
    },
    /// The file could not be read, e.g. because it does not exist or the user does not have permission.
    #[error(r#"The file "{path}" could not be read: {source}"#)]
    FileReadError {
        path: String,
        #[source]
        source: std::io::Error,
    },
}

#[derive(Debug, Error)]
pub enum AppError {
    #[error("The application is in an incoherent state: {0}")]
    IncoherentState(String),
    #[error(transparent)]
    Config(#[from] ConfigError),
    #[error("D-Bus call failed: {0}")]
    DBus(#[from] zbus::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}
