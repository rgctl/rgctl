//! Error types for rBuilder
//!
//! This module defines all error types used throughout the rBuilder system.
//! We use `thiserror` for ergonomic error definitions with automatic trait implementations.

use std::path::PathBuf;
use thiserror::Error;

/// Main error type for rBuilder operations
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

    /// Error during graph operations
    #[error("Graph error: {0}")]
    GraphError(String),

    /// Error during query execution
    #[error("Query error: {0}")]
    QueryError(String),

    /// Error during NLP translation
    #[error("NLP translation error: {0}")]
    NlpError(String),

    /// Configuration error
    #[error("Configuration error: {0}")]
    ConfigError(String),

    /// File I/O error
    #[error("File I/O error: {0}")]
    IoError(#[from] std::io::Error),

    /// Serialization/deserialization error
    #[error("Serialization error: {0}")]
    SerdeError(String),

    /// Language plugin error
    #[error("Plugin error: {0}")]
    PluginError(String),

    /// Invalid syntax in source code
    #[error("Invalid syntax in {file} at line {line}")]
    InvalidSyntax {
        /// File path
        file: PathBuf,
        /// Line number
        line: usize,
    },

    /// Unsupported language
    #[error("Unsupported language: {0}")]
    UnsupportedLanguage(String),

    /// Node not found in graph
    #[error("Node not found: {0}")]
    NodeNotFound(String),

    /// Invalid query
    #[error("Invalid query: {0}")]
    InvalidQuery(String),

    /// Resource not found
    #[error("Resource not found: {0}")]
    NotFound(String),

    /// Symbol name matched multiple graph nodes
    #[error("Ambiguous symbol '{name}': {count} matches")]
    AmbiguousSymbol { name: String, count: usize },

    /// Generic error with context
    #[error("{0}")]
    Other(String),
}

/// Result type alias for rBuilder operations
pub type Result<T> = std::result::Result<T, Error>;

impl From<serde_json::Error> for Error {
    fn from(err: serde_json::Error) -> Self {
        Error::SerdeError(err.to_string())
    }
}

impl From<serde_yaml::Error> for Error {
    fn from(err: serde_yaml::Error) -> Self {
        Error::SerdeError(err.to_string())
    }
}

impl From<toml::de::Error> for Error {
    fn from(err: toml::de::Error) -> Self {
        Error::SerdeError(err.to_string())
    }
}

impl From<rbuilder_plugin_api::Error> for Error {
    fn from(err: rbuilder_plugin_api::Error) -> Self {
        match err {
            rbuilder_plugin_api::Error::ParseError {
                file,
                line,
                message,
            } => Error::ParseError {
                file,
                line,
                message,
            },
            rbuilder_plugin_api::Error::PluginError(message) => Error::PluginError(message),
            rbuilder_plugin_api::Error::UnsupportedLanguage(language) => {
                Error::UnsupportedLanguage(language)
            }
        }
    }
}

impl From<std::str::Utf8Error> for Error {
    fn from(err: std::str::Utf8Error) -> Self {
        Error::ParseError {
            file: "unknown".into(),
            line: 0,
            message: format!("UTF-8 decoding error: {}", err),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display() {
        let err = Error::ParseError {
            file: PathBuf::from("test.rs"),
            line: 42,
            message: "unexpected token".to_string(),
        };
        let display = format!("{}", err);
        assert!(display.contains("test.rs"));
        assert!(display.contains("42"));
        assert!(display.contains("unexpected token"));
    }

    #[test]
    fn test_error_source() {
        use std::error::Error as StdError;

        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let err = Error::from(io_err);
        assert!(err.source().is_some());
    }

    #[test]
    fn test_serde_json_conversion() {
        let json_err = serde_json::from_str::<serde_json::Value>("invalid json").unwrap_err();
        let err = Error::from(json_err);
        assert!(matches!(err, Error::SerdeError(_)));
    }

    #[test]
    fn test_graph_error() {
        let err = Error::GraphError("node not found".to_string());
        assert_eq!(err.to_string(), "Graph error: node not found");
    }

    #[test]
    fn test_unsupported_language() {
        let err = Error::UnsupportedLanguage("brainfuck".to_string());
        assert!(err.to_string().contains("Unsupported language"));
    }
}
