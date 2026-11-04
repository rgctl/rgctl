//! Incremental per-file Kantra violation cache.

use crate::findings::KantraViolation;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

const MANIFEST_FILE: &str = "manifest.json";

/// On-disk cache manifest under `.rgctl/kantra_cache/`.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct KantraCacheManifest {
    pub ruleset_hash: String,
    #[serde(default)]
    pub files: HashMap<String, CachedFileEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedFileEntry {
    pub content_hash: String,
    pub violations: Vec<KantraViolation>,
}

/// Loaded cache with hit/miss accounting.
#[derive(Debug, Default)]
pub struct KantraFileCache {
    manifest: KantraCacheManifest,
    dirty: bool,
    pub hits: usize,
    pub misses: usize,
}

impl KantraFileCache {
    /// Load cache for a ruleset hash (empty when missing or stale).
    pub fn load(cache_dir: &Path, ruleset_hash: &str) -> Self {
        let path = cache_dir.join(MANIFEST_FILE);
        let manifest = fs::read_to_string(&path)
            .ok()
            .and_then(|s| serde_json::from_str::<KantraCacheManifest>(&s).ok())
            .filter(|m| m.ruleset_hash == ruleset_hash)
            .unwrap_or_else(|| KantraCacheManifest {
                ruleset_hash: ruleset_hash.to_string(),
                files: HashMap::new(),
            });
        Self {
            manifest,
            dirty: false,
            hits: 0,
            misses: 0,
        }
    }

    /// Lookup cached violations for a file at the given content hash.
    pub fn get(&mut self, rel_path: &str, content_hash: &str) -> Option<Vec<KantraViolation>> {
        let entry = self.manifest.files.get(rel_path)?;
        if entry.content_hash == content_hash {
            self.hits += 1;
            Some(entry.violations.clone())
        } else {
            self.misses += 1;
            None
        }
    }

    /// Store violations for a file and mark manifest dirty.
    pub fn put(
        &mut self,
        rel_path: String,
        content_hash: String,
        violations: Vec<KantraViolation>,
    ) {
        self.manifest.files.insert(
            rel_path,
            CachedFileEntry {
                content_hash,
                violations,
            },
        );
        self.dirty = true;
    }

    /// Persist manifest when dirty.
    pub fn save(&self, cache_dir: &Path) -> std::io::Result<()> {
        if !self.dirty {
            return Ok(());
        }
        fs::create_dir_all(cache_dir)?;
        let json = serde_json::to_string_pretty(&self.manifest)?;
        fs::write(cache_dir.join(MANIFEST_FILE), json)
    }
}

/// Hash file bytes for cache keys.
pub fn hash_file_content(content: &str) -> String {
    blake3::hash(content.as_bytes()).to_hex().to_string()
}

/// Ruleset fingerprint for cache invalidation.
pub fn ruleset_hash(catalog_id: &str, rule_count: usize) -> String {
    blake3::hash(format!("{catalog_id}:{rule_count}").as_bytes())
        .to_hex()
        .to_string()
}

/// Default cache directory under the artifact store.
pub fn cache_dir(store: &Path) -> PathBuf {
    rgctl_graph::paths::artifact_path(store, "kantra_cache")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_hit_and_ruleset_invalidation() {
        let dir = tempfile::tempdir().unwrap();
        let hash = ruleset_hash("test@1", 2);
        let mut cache = KantraFileCache::load(dir.path(), &hash);
        let v = vec![KantraViolation {
            rule_id: "r1".into(),
            category: None,
            file: "a.java".into(),
            line: 1,
            message: None,
            matched_by: "builtin.filecontent".into(),
            symbol: None,
            targets: Vec::new(),
            sources: Vec::new(),
            effort: None,
            enrichment: None,
        }];
        cache.put("a.java".into(), "abc".into(), v.clone());
        cache.save(dir.path()).unwrap();

        let mut reloaded = KantraFileCache::load(dir.path(), &hash);
        assert_eq!(reloaded.get("a.java", "abc"), Some(v));

        let stale = KantraFileCache::load(dir.path(), "other");
        assert!(stale.manifest.files.is_empty());
    }
}
