//! Plugin API error types

use std::path::PathBuf;
use thiserror::Error;

/// Error type for language and config format plugins
#[derive(Error, Debug)]
pub enum Error {
    /// Error during file parsing
    #[error("Parse error in {file}:{line}: {message}")]
    ParseError {
        /// File path where error occurred
        file: PathBuf,
        /// Line number
        line: usize,
        /// Error message
        message: String,
    },

    /// Language plugin error
    #[error("Plugin error: {0}")]
    PluginError(String),

    /// Unsupported language
    #[error("Unsupported language: {0}")]
    UnsupportedLanguage(String),
}

/// Result type alias for plugin operations
pub type Result<T> = std::result::Result<T, Error>;

impl From<serde_json::Error> for Error {
    fn from(err: serde_json::Error) -> Self {
        Error::PluginError(err.to_string())
    }
}

impl From<std::str::Utf8Error> for Error {
    fn from(err: std::str::Utf8Error) -> Self {
        Error::ParseError {
            file: "unknown".into(),
            line: 0,
            message: format!("UTF-8 decoding error: {err}"),
        }
    }
}

impl From<serde_yaml::Error> for Error {
    fn from(err: serde_yaml::Error) -> Self {
        Error::PluginError(err.to_string())
    }
}

impl From<toml::de::Error> for Error {
    fn from(err: toml::de::Error) -> Self {
        Error::PluginError(err.to_string())
    }
}
