//! Code body hashing and lookup index (Phase 12.0).

use blake3;
use rbuilder_error::{Error, Result};
use rbuilder_plugin_api::SourceLocation;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Location of a hashed code fragment.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CodeLocation {
    /// Source file path
    pub file_path: String,
    /// Start line (1-based)
    pub start_line: usize,
    /// End line (1-based)
    pub end_line: usize,
    /// Code text that was hashed
    pub code: String,
}

/// BLAKE3-backed index for fast change detection.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CodeIndex {
    hash_to_code: HashMap<String, CodeLocation>,
    #[serde(skip)]
    cache_file: Option<PathBuf>,
}

impl CodeIndex {
    /// Create an empty in-memory index.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create an index backed by a cache file path.
    pub fn with_cache_file(cache_file: PathBuf) -> Self {
        Self {
            hash_to_code: HashMap::new(),
            cache_file: Some(cache_file),
        }
    }

    /// Hash code and record its location. Returns the hex digest.
    pub fn add_code(&mut self, code: &str, location: &SourceLocation) -> String {
        let hash = hash_code(code);
        self.hash_to_code.insert(
            hash.clone(),
            CodeLocation {
                file_path: location.file.clone(),
                start_line: location.start_line,
                end_line: location.end_line,
                code: code.to_string(),
            },
        );
        hash
    }

    /// Returns true when `stored_hash` differs from the hash of `current_code`.
    pub fn has_changed(stored_hash: &str, current_code: &str) -> bool {
        hash_code(current_code) != stored_hash
    }

    /// Look up code by hash.
    pub fn get_code(&self, hash: &str) -> Option<&str> {
        self.hash_to_code.get(hash).map(|loc| loc.code.as_str())
    }

    /// Number of indexed fragments.
    pub fn len(&self) -> usize {
        self.hash_to_code.len()
    }

    /// Returns true when the index has no entries.
    pub fn is_empty(&self) -> bool {
        self.hash_to_code.is_empty()
    }

    /// Persist the index to the configured cache file.
    pub fn save(&self) -> Result<()> {
        let Some(path) = &self.cache_file else {
            return Ok(());
        };
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(&self.hash_to_code)?;
        std::fs::write(path, json)?;
        Ok(())
    }

    /// Load an index from disk, or return empty if missing.
    pub fn load(cache_file: PathBuf) -> Result<Self> {
        if cache_file.exists() {
            let json = std::fs::read_to_string(&cache_file)?;
            let hash_to_code: HashMap<String, CodeLocation> =
                serde_json::from_str(&json).map_err(|e| Error::SerdeError(e.to_string()))?;
            Ok(Self {
                hash_to_code,
                cache_file: Some(cache_file),
            })
        } else {
            Ok(Self::with_cache_file(cache_file))
        }
    }

    /// Default cache path under a repository root.
    pub fn default_cache_path(repo_root: &Path) -> PathBuf {
        repo_root.join(".rbuilder").join("code_index.json")
    }
}

/// Compute a BLAKE3 hex digest of code text.
pub fn hash_code(code: &str) -> String {
    blake3::hash(code.as_bytes()).to_hex().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_code_hash_stable() {
        let h1 = hash_code("fn main() {}");
        let h2 = hash_code("fn main() {}");
        assert_eq!(h1, h2);
        assert_ne!(h1, hash_code("fn main() { }"));
    }

    #[test]
    fn test_code_index_change_detection() {
        let mut index = CodeIndex::new();
        let loc = SourceLocation {
            file: "main.rs".to_string(),
            start_line: 1,
            end_line: 1,
            start_column: 0,
            end_column: 0,
        };
        let hash = index.add_code("fn old() {}", &loc);
        assert!(!CodeIndex::has_changed(&hash, "fn old() {}"));
        assert!(CodeIndex::has_changed(&hash, "fn new() {}"));
    }
}
