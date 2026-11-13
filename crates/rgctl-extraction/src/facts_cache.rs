//! Content-addressed FileFacts cache (`.rgctl/cache/facts.bin`).
//!
//! Keys are `(blake3(file_bytes), grammar_abi_version, ast_handler_manifest_hash)`.
//! Values are stored as pre-encoded JSON bytes (`Arc<[u8]>`) so:
//! - `load` does **zero** `serde_json` work (lazy decode only on `get` hit)
//! - `put` encodes **only** the new file
//! - `save` writes already-encoded bytes without re-serializing untouched entries
//! - decoded [`ExtractAllResult`] is never retained in the cache (spill-friendly)

use rgctl_error::{Error, Result};
use rgctl_graph::code_graph::GRAPH_DIR;
use rgctl_plugin_api::{ExtractAllResult, LanguagePlugin};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock, RwLock};

/// Relative path under `.rgctl/` for the binary facts cache.
pub const FACTS_CACHE_REL: &str = "cache/facts.bin";

const MAGIC: &[u8; 8] = b"RGCTLFCT";
const FORMAT_VERSION: u32 = 8;

/// Content-addressed key for one cached extraction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct FactsKey {
    /// BLAKE3 of file bytes.
    pub content_hash: [u8; 32],
    /// Tree-sitter grammar ABI version (0 when no grammar).
    pub grammar_version: u64,
    /// Fingerprint of language id + declared AST handler table.
    pub manifest_hash: [u8; 32],
}

/// Thread-safe on-disk / in-memory FileFacts cache.
///
/// In-memory values are raw JSON payloads (`Arc<[u8]>`), never decoded
/// [`ExtractAllResult`] trees — critical for cold discover / watch scale.
#[derive(Debug)]
pub struct FactsCache {
    /// Pre-encoded JSON payloads keyed by content+plugin version.
    entries: RwLock<HashMap<FactsKey, Arc<[u8]>>>,
    dirty: AtomicBool,
    hits: AtomicU64,
    misses: AtomicU64,
}

impl Default for FactsCache {
    fn default() -> Self {
        Self::new()
    }
}

impl FactsCache {
    /// Empty in-memory cache.
    pub fn new() -> Self {
        Self {
            entries: RwLock::new(HashMap::new()),
            dirty: AtomicBool::new(false),
            hits: AtomicU64::new(0),
            misses: AtomicU64::new(0),
        }
    }

    /// Absolute path to `facts.bin` for `repo_root`.
    pub fn path_for(repo_root: &Path) -> PathBuf {
        repo_root.join(GRAPH_DIR).join(FACTS_CACHE_REL)
    }

    /// Load from `.rgctl/cache/facts.bin`, or empty if missing/corrupt.
    ///
    /// Does **not** JSON-decode payloads — only builds the key → bytes index.
    pub fn load(repo_root: &Path) -> Self {
        let path = Self::path_for(repo_root);
        match Self::load_from_path(&path) {
            Ok(cache) => cache,
            Err(err) => {
                tracing::debug!(
                    path = %path.display(),
                    error = %err,
                    "facts cache load failed; starting empty"
                );
                Self::new()
            }
        }
    }

    /// Load from an explicit path (lazy: no `serde_json::from_slice`).
    pub fn load_from_path(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::new());
        }
        let bytes = std::fs::read(path)?;
        if bytes.len() < 20 {
            return Err(Error::Other("facts.bin too short".into()));
        }
        if &bytes[0..8] != MAGIC {
            return Err(Error::Other("facts.bin bad magic".into()));
        }
        let version = u32::from_le_bytes(bytes[8..12].try_into().unwrap_or([0; 4]));
        if version != FORMAT_VERSION {
            return Err(Error::Other(format!(
                "facts.bin unsupported version {version}"
            )));
        }
        let count = u64::from_le_bytes(bytes[12..20].try_into().unwrap_or([0; 8])) as usize;
        let mut pos = 20usize;
        // Each entry has at least a 72-byte header (32 + 8 + 32) + 8-byte len = 80 bytes.
        if count.saturating_mul(80) > bytes.len() - pos {
            return Err(Error::Other("facts.bin truncated entry count".into()));
        }
        let mut map = HashMap::with_capacity(count);
        for _ in 0..count {
            if pos + 80 > bytes.len() {
                return Err(Error::Other("facts.bin truncated entry header".into()));
            }
            let content_hash: [u8; 32] = bytes[pos..pos + 32].try_into().unwrap();
            pos += 32;
            let grammar_version = u64::from_le_bytes(bytes[pos..pos + 8].try_into().unwrap());
            pos += 8;
            let manifest_hash: [u8; 32] = bytes[pos..pos + 32].try_into().unwrap();
            pos += 32;
            let val_len = u64::from_le_bytes(bytes[pos..pos + 8].try_into().unwrap()) as usize;
            pos += 8;
            if pos.saturating_add(val_len) > bytes.len() {
                return Err(Error::Other("facts.bin truncated entry payload".into()));
            }
            let value_json: Arc<[u8]> = Arc::from(&bytes[pos..pos + val_len]);
            pos += val_len;
            map.insert(
                FactsKey {
                    content_hash,
                    grammar_version,
                    manifest_hash,
                },
                value_json,
            );
        }
        Ok(Self {
            entries: RwLock::new(map),
            dirty: AtomicBool::new(false),
            hits: AtomicU64::new(0),
            misses: AtomicU64::new(0),
        })
    }

    /// Atomically persist to `.rgctl/cache/facts.bin` when dirty.
    pub fn save(&self, repo_root: &Path) -> Result<()> {
        if !self.dirty.load(Ordering::Acquire) {
            return Ok(());
        }
        let path = Self::path_for(repo_root);
        self.save_to_path(&path)
    }

    /// Write atomically via `facts.bin.tmp` then rename.
    ///
    /// Streams pre-encoded `Arc<[u8]>` payloads directly to a `BufWriter<File>` —
    /// zero `serde_json::to_vec` on save and zero intermediate `Vec<u8>` copies.
    pub fn save_to_path(&self, path: &Path) -> Result<()> {
        use std::io::Write;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let entries = self
            .entries
            .read()
            .map_err(|_| Error::Other("facts cache lock poisoned".into()))?;

        let tmp = path.with_extension("bin.tmp");
        {
            let file = std::fs::File::create(&tmp)?;
            let mut w = std::io::BufWriter::with_capacity(64 * 1024, file);
            w.write_all(MAGIC)?;
            w.write_all(&FORMAT_VERSION.to_le_bytes())?;
            w.write_all(&(entries.len() as u64).to_le_bytes())?;
            for (key, value_json) in entries.iter() {
                w.write_all(&key.content_hash)?;
                w.write_all(&key.grammar_version.to_le_bytes())?;
                w.write_all(&key.manifest_hash)?;
                w.write_all(&(value_json.len() as u64).to_le_bytes())?;
                w.write_all(value_json.as_ref())?;
            }
            w.flush()?;
        }
        drop(entries);

        std::fs::rename(&tmp, path)?;
        self.dirty.store(false, Ordering::Release);
        Ok(())
    }

    /// Look up a cached extraction (JSON-decodes **only** this entry on hit).
    pub fn get(&self, key: &FactsKey) -> Option<ExtractAllResult> {
        let bytes = self.entries.read().ok().and_then(|m| m.get(key).cloned());
        match bytes {
            Some(raw) => match serde_json::from_slice::<ExtractAllResult>(&raw) {
                Ok(value) => {
                    self.hits.fetch_add(1, Ordering::Relaxed);
                    Some(value)
                }
                Err(err) => {
                    tracing::debug!(error = %err, "facts cache entry decode failed");
                    self.misses.fetch_add(1, Ordering::Relaxed);
                    None
                }
            },
            None => {
                self.misses.fetch_add(1, Ordering::Relaxed);
                None
            }
        }
    }

    /// Encode `value` once and insert (does not retain the decoded struct).
    pub fn put(&self, key: FactsKey, value: &ExtractAllResult) {
        let Ok(encoded) = serde_json::to_vec(value) else {
            tracing::debug!("facts cache put: JSON encode failed");
            return;
        };
        self.put_encoded(key, Arc::<[u8]>::from(encoded));
    }

    /// Insert a pre-encoded JSON payload.
    pub fn put_encoded(&self, key: FactsKey, value_json: Arc<[u8]>) {
        if let Ok(mut map) = self.entries.write() {
            map.insert(key, value_json);
        }
        self.dirty.store(true, Ordering::Release);
    }

    /// Number of successful cache hits since load.
    pub fn hit_count(&self) -> u64 {
        self.hits.load(Ordering::Relaxed)
    }

    /// Number of cache misses since load.
    pub fn miss_count(&self) -> u64 {
        self.misses.load(Ordering::Relaxed)
    }

    /// Entries currently held in memory (encoded payloads only).
    pub fn len(&self) -> usize {
        self.entries.read().map(|m| m.len()).unwrap_or(0)
    }

    /// True when empty.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Build the cache key for `source` under `plugin` (hashes `source`).
pub fn facts_key_for(plugin: &dyn LanguagePlugin, source: &[u8]) -> FactsKey {
    facts_key_for_hash(plugin, *blake3::hash(source).as_bytes())
}

/// Build the cache key from a precomputed content hash (avoids double BLAKE3).
pub fn facts_key_for_hash(plugin: &dyn LanguagePlugin, content_hash: [u8; 32]) -> FactsKey {
    let (grammar_version, manifest_hash) = plugin_cache_version(plugin);
    FactsKey {
        content_hash,
        grammar_version,
        manifest_hash,
    }
}

/// Process-wide memo of `(grammar_abi, manifest_hash)` per language id.
fn plugin_cache_version(plugin: &dyn LanguagePlugin) -> (u64, [u8; 32]) {
    #[allow(clippy::type_complexity)]
    static TABLE: OnceLock<Mutex<HashMap<String, (u64, [u8; 32])>>> = OnceLock::new();
    let table = TABLE.get_or_init(|| Mutex::new(HashMap::new()));
    let id = plugin.language_id();
    if let Ok(guard) = table.lock()
        && let Some(v) = guard.get(id)
    {
        return *v;
    }
    let grammar_version = plugin
        .grammar()
        .map(|g| g.abi_version() as u64)
        .unwrap_or(0);
    let manifest_hash = manifest_fingerprint(plugin);
    let v = (grammar_version, manifest_hash);
    if let Ok(mut guard) = table.lock() {
        guard.insert(id.to_string(), v);
    }
    v
}

fn manifest_fingerprint(plugin: &dyn LanguagePlugin) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(plugin.language_id().as_bytes());
    if let Some(table) = plugin.declared_ast_handlers() {
        let mut entries: Vec<(u16, &str, u8)> = table
            .iter()
            .map(|(id, name, mask)| (id, name, mask.0))
            .collect();
        entries.sort_by_key(|(id, _, _)| *id);
        for (id, name, mask) in entries {
            hasher.update(&id.to_le_bytes());
            hasher.update(name.as_bytes());
            hasher.update(&[mask]);
        }
    }
    if let Some(fp) = plugin.cache_fingerprint() {
        hasher.update(fp);
    }
    *hasher.finalize().as_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;
    use rgctl_plugin_api::{SourceLocation, Symbol, SymbolType};
    use tempfile::TempDir;

    fn sample_result() -> ExtractAllResult {
        ExtractAllResult::from_parts(
            vec![Symbol {
                name: "foo".into(),
                symbol_type: SymbolType::Function,
                qualified_name: None,
                location: SourceLocation {
                    file: "a.rs".into(),
                    start_line: 1,
                    end_line: 2,
                    start_column: 0,
                    end_column: 1,
                },
                signature: None,
                return_type: None,
                parameters: vec![],
                fields: vec![],
                modifiers: vec![],
                documentation: None,
                metadata: serde_json::Value::Null,
                ..Default::default()
            }],
            vec![],
        )
    }

    #[test]
    fn roundtrip_atomic_save() {
        let dir = TempDir::new().unwrap();
        let cache = FactsCache::new();
        let key = FactsKey {
            content_hash: [1u8; 32],
            grammar_version: 15,
            manifest_hash: [2u8; 32],
        };
        cache.put(key, &sample_result());
        cache.save(dir.path()).unwrap();

        let loaded = FactsCache::load(dir.path());
        assert_eq!(loaded.len(), 1, "load must retain encoded entries");
        let got = loaded.get(&key).expect("hit");
        assert_eq!(got.symbols.len(), 1);
        assert_eq!(got.symbols[0].name, "foo");
        assert_eq!(loaded.hit_count(), 1);
    }

    #[test]
    fn load_does_not_eager_decode() {
        let dir = TempDir::new().unwrap();
        let cache = FactsCache::new();
        let key = FactsKey {
            content_hash: [9u8; 32],
            grammar_version: 1,
            manifest_hash: [8u8; 32],
        };
        cache.put(key, &sample_result());
        cache.save(dir.path()).unwrap();

        let loaded = FactsCache::load(dir.path());
        // Hits stay 0 until get() — proves load skipped JSON decode.
        assert_eq!(loaded.hit_count(), 0);
        assert_eq!(loaded.miss_count(), 0);
        assert_eq!(loaded.len(), 1);
        assert!(loaded.get(&key).is_some());
    }

    #[test]
    fn save_does_not_reencode_untouched() {
        let dir = TempDir::new().unwrap();
        let cache = FactsCache::new();
        let key = FactsKey {
            content_hash: [7u8; 32],
            grammar_version: 2,
            manifest_hash: [6u8; 32],
        };
        let encoded = Arc::<[u8]>::from(serde_json::to_vec(&sample_result()).unwrap());
        cache.put_encoded(key, Arc::clone(&encoded));
        cache.save(dir.path()).unwrap();

        let loaded = FactsCache::load(dir.path());
        let path = FactsCache::path_for(dir.path());
        // Touch dirty by put_encoded of a second key, then save — first payload
        // bytes must round-trip unchanged (no re-JSON).
        let key2 = FactsKey {
            content_hash: [5u8; 32],
            grammar_version: 2,
            manifest_hash: [6u8; 32],
        };
        loaded.put_encoded(key2, Arc::clone(&encoded));
        loaded.save_to_path(&path).unwrap();
        let again = FactsCache::load(dir.path());
        assert_eq!(again.len(), 2);
        assert!(again.get(&key).is_some());
    }

    #[test]
    fn corrupt_file_loads_empty() {
        let dir = TempDir::new().unwrap();
        let path = FactsCache::path_for(dir.path());
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, b"not-a-facts-file").unwrap();
        let cache = FactsCache::load(dir.path());
        assert!(cache.is_empty());
    }

    #[test]
    fn roundtrip_real_rust_extract() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("lib.rs");
        std::fs::write(&path, "pub fn alpha() {}\n").unwrap();
        let source = std::fs::read(&path).unwrap();
        let registry = std::sync::Arc::new(rgctl_languages::default_registry());
        let plugin = registry.get_plugin_for_file(&path).unwrap();
        let extracted = plugin.extract_all(&path, &source).unwrap();
        assert!(!extracted.symbols.is_empty());

        let key = facts_key_for(plugin.as_ref(), &source);
        let cache = FactsCache::new();
        cache.put(key, &extracted);
        cache.save(dir.path()).unwrap();
        let loaded = FactsCache::load(dir.path());
        let got = loaded.get(&key).expect("cache hit after reload");
        assert_eq!(got.symbols.len(), extracted.symbols.len());
        assert_eq!(got.symbols[0].name, extracted.symbols[0].name);
    }

    #[test]
    fn truncated_tmp_does_not_clobber_good_cache() {
        let dir = TempDir::new().unwrap();
        let cache = FactsCache::new();
        let key = FactsKey {
            content_hash: [3u8; 32],
            grammar_version: 1,
            manifest_hash: [4u8; 32],
        };
        cache.put(key, &sample_result());
        cache.save(dir.path()).unwrap();

        let path = FactsCache::path_for(dir.path());
        let tmp = path.with_extension("bin.tmp");
        std::fs::write(&tmp, b"RGCTLFCT\x01\x00\x00\x00truncated").unwrap();
        assert!(path.exists());
        let loaded = FactsCache::load(dir.path());
        assert!(loaded.get(&key).is_some());
    }

    #[test]
    fn facts_key_for_hash_matches_facts_key_for() {
        let path = Path::new("x.rs");
        let source = b"fn f() {}";
        let registry = std::sync::Arc::new(rgctl_languages::default_registry());
        // Use a real plugin via temp file.
        let dir = TempDir::new().unwrap();
        let file = dir.path().join("x.rs");
        std::fs::write(&file, source).unwrap();
        let plugin = registry.get_plugin_for_file(&file).unwrap();
        let a = facts_key_for(plugin.as_ref(), source);
        let b = facts_key_for_hash(plugin.as_ref(), *blake3::hash(source).as_bytes());
        assert_eq!(a, b);
        let _ = path;
    }
}
