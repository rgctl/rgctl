//! Deduplicated source files for dashboard slice/dataflow views.

use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

pub const SOURCE_INDEX_FILE: &str = "sources_index.json";
pub const SOURCE_FILES_DIR: &str = "sources";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceIndexPayload {
    pub schema_version: u32,
    pub files: Vec<SourceFileEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceFileEntry {
    pub source_id: String,
    pub file_path: String,
    pub total_lines: usize,
}

/// In-memory map of absolute path → catalog entry, shared across exporters.
#[derive(Debug, Clone, Default)]
pub struct SourceCatalog {
    by_path: HashMap<String, SourceFileEntry>,
}

impl SourceCatalog {
    pub fn get(&self, file_path: &str) -> Option<&SourceFileEntry> {
        self.by_path.get(file_path)
    }

    pub fn entries(&self) -> Vec<SourceFileEntry> {
        self.by_path.values().cloned().collect()
    }

    pub fn insert(&mut self, entry: SourceFileEntry) {
        self.by_path.insert(entry.file_path.clone(), entry);
    }

    #[allow(dead_code)]
    pub fn len(&self) -> usize {
        self.by_path.len()
    }

    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.by_path.is_empty()
    }
}

/// Stable short id for a repository source path.
pub fn source_id_for_path(file_path: &str) -> String {
    blake3::hash(file_path.as_bytes())
        .to_hex()
        .chars()
        .take(16)
        .collect()
}

/// Build the source catalog once over unique paths (parallel copy into `out_dir/sources`).
pub fn build_source_catalog(
    out_dir: &Path,
    unique_paths: &[String],
) -> Result<SourceCatalog, String> {
    let sources_dir = out_dir.join(SOURCE_FILES_DIR);
    if !unique_paths.is_empty() {
        fs::create_dir_all(&sources_dir).map_err(|e| e.to_string())?;
    }

    let entries: Vec<SourceFileEntry> = unique_paths
        .par_iter()
        .filter_map(|file_path| copy_one_source(&sources_dir, file_path))
        .collect();

    let mut catalog = SourceCatalog::default();
    for entry in entries {
        catalog.insert(entry);
    }
    Ok(catalog)
}

fn copy_one_source(sources_dir: &Path, file_path: &str) -> Option<SourceFileEntry> {
    let text = fs::read_to_string(file_path).ok()?;
    let total_lines = text.lines().count().max(1);
    let source_id = source_id_for_path(file_path);
    let dest = sources_dir.join(format!("{source_id}.txt"));
    if !dest.is_file() {
        fs::write(&dest, &text).ok()?;
    }
    Some(SourceFileEntry {
        source_id,
        file_path: file_path.to_string(),
        total_lines,
    })
}

pub fn write_source_index(out_dir: &Path, entries: &[SourceFileEntry]) -> Result<(), String> {
    let mut files = entries.to_vec();
    files.sort_by(|a, b| a.file_path.cmp(&b.file_path));
    let payload = SourceIndexPayload {
        schema_version: 1,
        files,
    };
    super::export_util::write_json_compact(&out_dir.join(SOURCE_INDEX_FILE), &payload)
}
