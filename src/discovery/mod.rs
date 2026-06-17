//! File discovery and filtering
//!
//! Task 1.6.1: Recursive directory traversal with .gitignore support

use crate::error::{Error, Result};
use crate::languages::registry::LanguageRegistry;
use ignore::overrides::OverrideBuilder;
use ignore::WalkBuilder;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Default maximum file size (1 MiB). Files larger than this are skipped.
pub const DEFAULT_MAX_FILE_SIZE: u64 = 1024 * 1024;

/// Number of bytes sampled when detecting binary files.
const BINARY_SAMPLE_SIZE: usize = 8192;

/// Configuration for file discovery.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveryConfig {
    /// Maximum file size in bytes. Larger files are skipped.
    pub max_file_size: u64,

    /// Additional glob patterns to exclude (e.g. `"vendor/**"`).
    pub exclude_patterns: Vec<String>,

    /// When set, only include files handled by these language or config format IDs.
    pub languages: Option<Vec<String>>,
}

impl Default for DiscoveryConfig {
    fn default() -> Self {
        Self {
            max_file_size: DEFAULT_MAX_FILE_SIZE,
            exclude_patterns: vec![".rbuilder/**".to_string()],
            languages: None,
        }
    }
}

/// Discovers source and config files in a repository.
pub struct FileDiscoverer {
    registry: Arc<LanguageRegistry>,
    config: DiscoveryConfig,
}

impl FileDiscoverer {
    /// Create a discoverer with default configuration.
    pub fn new(registry: Arc<LanguageRegistry>) -> Self {
        Self {
            registry,
            config: DiscoveryConfig::default(),
        }
    }

    /// Create a discoverer with custom configuration.
    pub fn with_config(registry: Arc<LanguageRegistry>, config: DiscoveryConfig) -> Self {
        Self { registry, config }
    }

    /// Return a reference to the language registry.
    pub fn registry(&self) -> &LanguageRegistry {
        &self.registry
    }

    /// Return the active discovery configuration.
    pub fn config(&self) -> &DiscoveryConfig {
        &self.config
    }

    /// Recursively discover processable files under `root`.
    ///
    /// Respects `.gitignore`, indexes dot-directories such as `.github/workflows/`, filters by
    /// supported extensions, and applies size/binary checks.
    pub fn discover(&self, root: &Path) -> Result<Vec<PathBuf>> {
        if !root.exists() {
            return Err(Error::NotFound(format!(
                "Repository path does not exist: {}",
                root.display()
            )));
        }
        if !root.is_dir() {
            return Err(Error::ConfigError(format!(
                "Repository path is not a directory: {}",
                root.display()
            )));
        }

        let overrides = build_overrides(root, &self.config.exclude_patterns)?;

        let walker = WalkBuilder::new(root)
            .follow_links(false)
            .require_git(false)
            .hidden(false)
            .git_ignore(true)
            .git_global(true)
            .git_exclude(true)
            .ignore(true)
            .overrides(overrides)
            .build();

        let mut files = Vec::new();

        for entry in walker.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }

            if !self.registry.can_process_file(path) {
                continue;
            }

            if let Some(ref languages) = self.config.languages {
                if !self.matches_language_filter(path, languages) {
                    continue;
                }
            }

            if self.is_too_large(path)? {
                continue;
            }

            if self.is_binary(path)? {
                continue;
            }

            files.push(path.to_path_buf());
        }

        files.sort();
        Ok(files)
    }

    fn matches_language_filter(&self, path: &Path, languages: &[String]) -> bool {
        if let Ok(plugin) = self.registry.get_plugin_for_file(path) {
            return languages.iter().any(|l| l == plugin.language_id());
        }

        if let Ok(plugin) = self.registry.get_config_plugin_for_file(path) {
            return languages.iter().any(|l| l == plugin.format_id());
        }

        false
    }

    fn is_too_large(&self, path: &Path) -> Result<bool> {
        let metadata = path.metadata()?;
        Ok(metadata.len() > self.config.max_file_size)
    }

    fn is_binary(&self, path: &Path) -> Result<bool> {
        let mut file = File::open(path)?;
        let mut buffer = vec![0u8; BINARY_SAMPLE_SIZE];
        let bytes_read = file.read(&mut buffer)?;
        buffer.truncate(bytes_read);
        Ok(buffer.contains(&0))
    }
}

fn build_overrides(root: &Path, exclude_patterns: &[String]) -> Result<ignore::overrides::Override> {
    let mut builder = OverrideBuilder::new(root);

    for pattern in exclude_patterns {
        let exclude = if pattern.starts_with('!') {
            pattern.clone()
        } else {
            format!("!{pattern}")
        };
        builder
            .add(&exclude)
            .map_err(|e| Error::ConfigError(format!("Invalid exclude pattern '{pattern}': {e}")))?;
    }

    builder
        .build()
        .map_err(|e| Error::ConfigError(format!("Failed to build exclude overrides: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn discoverer() -> FileDiscoverer {
        FileDiscoverer::new(Arc::new(LanguageRegistry::new()))
    }

    fn write_file(path: &Path, contents: &str) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, contents).unwrap();
    }

    #[test]
    fn test_file_discovery() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path();

        write_file(&root.join("src/main.rs"), "fn main() {}");
        write_file(&root.join("notes.txt"), "plain text");
        write_file(&root.join(".git/config"), "[core]");

        let files = discoverer().discover(root).unwrap();

        assert!(files.iter().any(|f| f.extension().is_some_and(|e| e == "rs")));
        assert!(!files.iter().any(|f| f.components().any(|c| c.as_os_str() == ".git")));
        assert!(!files.iter().any(|f| f.extension().is_some_and(|e| e == "txt")));
    }

    #[test]
    fn test_gitignore_respect() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path();

        write_file(&root.join("src/main.rs"), "fn main() {}");
        write_file(&root.join("target/debug/app.rs"), "fn app() {}");
        write_file(&root.join(".gitignore"), "target/\n");

        let files = discoverer().discover(root).unwrap();

        assert!(files.iter().any(|f| f.ends_with("src/main.rs")));
        assert!(!files.iter().any(|f| f.components().any(|c| c.as_os_str() == "target")));
    }

    #[test]
    fn test_custom_exclude_patterns() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path();

        write_file(&root.join("src/main.rs"), "fn main() {}");
        write_file(&root.join("vendor/lib.rs"), "pub fn vendor() {}");

        let config = DiscoveryConfig {
            exclude_patterns: vec!["vendor/**".to_string()],
            ..DiscoveryConfig::default()
        };
        let files = FileDiscoverer::with_config(Arc::new(LanguageRegistry::new()), config)
            .discover(root)
            .unwrap();

        assert!(files.iter().any(|f| f.ends_with("src/main.rs")));
        assert!(!files.iter().any(|f| f.components().any(|c| c.as_os_str() == "vendor")));
    }

    #[test]
    fn test_skip_large_files() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path();

        write_file(&root.join("small.rs"), "fn small() {}");
        let large_path = root.join("large.rs");
        write_file(&large_path, "fn large() {}");
        fs::write(&large_path, vec![b'x'; 2048]).unwrap();

        let config = DiscoveryConfig {
            max_file_size: 1024,
            ..DiscoveryConfig::default()
        };
        let files = FileDiscoverer::with_config(Arc::new(LanguageRegistry::new()), config)
            .discover(root)
            .unwrap();

        assert!(files.iter().any(|f| f.ends_with("small.rs")));
        assert!(!files.iter().any(|f| f.ends_with("large.rs")));
    }

    #[test]
    fn test_skip_binary_files() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path();

        write_file(&root.join("valid.rs"), "fn valid() {}");
        let binary_path = root.join("binary.rs");
        fs::write(&binary_path, b"fn fake()\0\x00\x00").unwrap();

        let files = discoverer().discover(root).unwrap();

        assert!(files.iter().any(|f| f.ends_with("valid.rs")));
        assert!(!files.iter().any(|f| f.ends_with("binary.rs")));
    }

    #[test]
    fn test_language_filter() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path();

        write_file(&root.join("main.rs"), "fn main() {}");
        write_file(&root.join("script.py"), "def main(): pass");

        let config = DiscoveryConfig {
            languages: Some(vec!["rust".to_string()]),
            ..DiscoveryConfig::default()
        };
        let files = FileDiscoverer::with_config(Arc::new(LanguageRegistry::new()), config)
            .discover(root)
            .unwrap();

        assert!(files.iter().any(|f| f.ends_with("main.rs")));
        assert!(!files.iter().any(|f| f.ends_with("script.py")));
    }

    #[test]
    fn test_discover_missing_path() {
        let result = discoverer().discover(Path::new("/nonexistent/rbuilder-test-path"));
        assert!(matches!(result, Err(Error::NotFound(_))));
    }

    #[test]
    fn test_discover_file_path_rejected() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("single.rs");
        write_file(&file_path, "fn main() {}");

        let result = discoverer().discover(&file_path);
        assert!(matches!(result, Err(Error::ConfigError(_))));
    }

    #[test]
    fn test_includes_config_files() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path();

        write_file(&root.join("config.yaml"), "key: value");
        write_file(&root.join("Cargo.toml"), "[package]\nname = \"demo\"");

        let files = discoverer().discover(root).unwrap();

        assert!(files.iter().any(|f| f.ends_with("config.yaml")));
        assert!(files.iter().any(|f| f.ends_with("Cargo.toml")));
    }

    #[test]
    fn test_invalid_exclude_pattern() {
        let temp_dir = TempDir::new().unwrap();
        let config = DiscoveryConfig {
            exclude_patterns: vec!["[".to_string()],
            ..DiscoveryConfig::default()
        };
        let result =
            FileDiscoverer::with_config(Arc::new(LanguageRegistry::new()), config).discover(temp_dir.path());
        assert!(matches!(result, Err(Error::ConfigError(_))));
    }
}
