//! Stream CFG/PDG dashboard export from per-function `.analysis.bin` files (no monolithic archive load).

use crate::cfg_export::{
    CFG_ARCHIVE_BUNDLE_NAME, CFG_DETAIL_DIR, CFG_DETAIL_INLINE_LIMIT, CFG_INDEX_FILE,
    CfgExportSummary, CfgFunctionEntry, CfgIndexPayload, cfg_detail_light, write_empty_cfg_index,
};
use crate::cfg_record_pack::{
    CFG_RECORD_DATA_FILE, CFG_RECORD_INDEX_FILE, CfgRecordPackWriter, SLICE_RECORD_DATA_FILE,
    SLICE_RECORD_INDEX_FILE,
};
use crate::dataflow_export::DataflowFunctionEntry;
use crate::export_util::{link_or_copy, write_json_compact};
use crate::function_meta::{function_meta_map_with_index, resolve_function_meta};
use crate::slice_export::{
    SLICE_DETAIL_DIR, SLICE_INDEX_FILE, SliceBundlePayload, SliceExportSummary, SliceFunctionEntry,
    export_pdg, function_line_span, write_empty_slice_index,
};
use crate::source_catalog::{
    SourceCatalog, SourceFileEntry, build_source_catalog, write_source_index,
};
use rayon::prelude::*;
use rgctl_analysis::cfg_pdg_archive::CfgPdgArchive;
use rgctl_analysis::storage::{AnalysisIndexEntry, AnalysisStorage};
use rgctl_graph::graph_source::GraphSource;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;
use std::sync::Arc;
use uuid::Uuid;

/// When function count exceeds this, skip per-function `slice/<uuid>.json` and serve from
/// `slice.record_{index,data}.bin` (same threshold as CFG detail inline limit).
pub(crate) const SLICE_DETAIL_INLINE_LIMIT: usize = 5_000;

pub struct StreamedAnalysisExport {
    pub slice: SliceExportSummary,
    pub cfg: CfgExportSummary,
    pub dataflow: Vec<DataflowFunctionEntry>,
    /// Shared source catalog for kantra / other exporters (paths already written under `sources/`).
    pub source_catalog: SourceCatalog,
}

/// Lightweight work item: JSON payloads already serialized on the Rayon worker so the
/// main thread never holds CFG/PDG ASTs (avoids multi-second `Vec<Option<ExportWork>>` drop).
struct ExportWork {
    function_id: Uuid,
    slice_entry: SliceFunctionEntry,
    cfg_entry: CfgFunctionEntry,
    dataflow_entry: DataflowFunctionEntry,
    /// Compact JSON for slice bundle (always present when export succeeds).
    slice_bytes: Vec<u8>,
    /// Compact JSON for CFG detail when writing the record pack (archive_only mode).
    cfg_bytes: Option<Vec<u8>>,
}

/// Export slice + CFG indexes and details by loading one function analysis at a time.
#[allow(dead_code)] // Public convenience wrapper; production path uses `_with_index`.
pub fn export_cfg_slice_from_storage(
    graph: GraphSource<'_>,
    repo_root: &Path,
    out_dir: &Path,
) -> Result<StreamedAnalysisExport, String> {
    export_cfg_slice_from_storage_with_index(graph, repo_root, out_dir, None)
}

/// Analysis index as loaded from `analysis_index.bin` (`stable_key` → entry).
pub type AnalysisIndexMap = HashMap<String, AnalysisIndexEntry>;

/// Same as [`export_cfg_slice_from_storage`], with an optional pre-loaded analysis index.
pub fn export_cfg_slice_from_storage_with_index(
    graph: GraphSource<'_>,
    repo_root: &Path,
    out_dir: &Path,
    analysis_index: Option<&AnalysisIndexMap>,
) -> Result<StreamedAnalysisExport, String> {
    let analysis_dir = repo_root.join(".rgctl/analysis");
    let storage = AnalysisStorage::new(&analysis_dir);
    let owned_index;
    let index: &AnalysisIndexMap = match analysis_index {
        Some(idx) => idx,
        None => {
            owned_index = storage.load_analysis_index().map_err(|e| e.to_string())?;
            &owned_index
        }
    };

    if index.is_empty() {
        write_empty_slice_index(&out_dir.join(SLICE_INDEX_FILE))?;
        write_empty_cfg_index(&out_dir.join(CFG_INDEX_FILE))?;
        return Ok(StreamedAnalysisExport {
            slice: SliceExportSummary::default(),
            cfg: CfgExportSummary::default(),
            dataflow: Vec::new(),
            source_catalog: SourceCatalog::default(),
        });
    }

    let meta_map = function_meta_map_with_index(repo_root, graph, Some(index));
    let write_cfg_details = index.len() <= CFG_DETAIL_INLINE_LIMIT;
    let write_slice_details = index.len() <= SLICE_DETAIL_INLINE_LIMIT;

    let slice_dir = out_dir.join(SLICE_DETAIL_DIR);
    if slice_dir.exists() {
        fs::remove_dir_all(&slice_dir).map_err(|e| e.to_string())?;
    }
    if write_slice_details {
        fs::create_dir_all(&slice_dir).map_err(|e| e.to_string())?;
    }

    let cfg_dir = out_dir.join(CFG_DETAIL_DIR);
    if cfg_dir.exists() {
        fs::remove_dir_all(&cfg_dir).map_err(|e| e.to_string())?;
    }
    if write_cfg_details {
        fs::create_dir_all(&cfg_dir).map_err(|e| e.to_string())?;
    }

    // Collect unique absolute paths from the index before the per-function loop so sources
    // are copied once (not contended under a Mutex on every Rayon worker).
    let unique_paths = unique_source_paths(index, repo_root);
    let source_catalog = build_source_catalog(out_dir, &unique_paths)?;
    let catalog = Arc::new(source_catalog);

    let entries: Vec<&AnalysisIndexEntry> = index.values().collect();

    let works: Vec<Option<ExportWork>> = entries
        .par_iter()
        .map(|entry| {
            export_one(
                entry,
                &storage,
                repo_root,
                graph,
                &meta_map,
                &slice_dir,
                &cfg_dir,
                write_cfg_details,
                write_slice_details,
                &catalog,
            )
        })
        .collect();

    let mut slice_functions = Vec::new();
    let mut cfg_functions = Vec::new();
    let mut dataflow = Vec::new();
    let mut cfg_pack: Vec<(Uuid, Vec<u8>)> = Vec::new();
    let mut slice_pack: Vec<(Uuid, Vec<u8>)> = Vec::new();
    for work in works.into_iter().flatten() {
        if let Some(bytes) = work.cfg_bytes {
            cfg_pack.push((work.function_id, bytes));
        }
        slice_pack.push((work.function_id, work.slice_bytes));
        slice_functions.push(work.slice_entry);
        cfg_functions.push(work.cfg_entry);
        dataflow.push(work.dataflow_entry);
    }

    let record_pack_written = if !write_cfg_details && !cfg_pack.is_empty() {
        let mut pack = CfgRecordPackWriter::create(out_dir)?;
        cfg_pack.sort_by_key(|(id, _)| *id);
        for (id, bytes) in &cfg_pack {
            pack.append_bytes(*id, bytes)?;
        }
        pack.finish(out_dir)?;
        true
    } else {
        false
    };

    // Always write the slice record pack so archive consumers and future warm re-exports
    // can range-fetch; when under the inline limit we also keep per-file JSON.
    let slice_record_pack = if !slice_pack.is_empty() {
        let mut pack = CfgRecordPackWriter::create_named(out_dir, SLICE_RECORD_DATA_FILE)?;
        slice_pack.sort_by_key(|(id, _)| *id);
        for (id, bytes) in &slice_pack {
            pack.append_bytes(*id, bytes)?;
        }
        pack.finish_named(out_dir, SLICE_RECORD_INDEX_FILE)?;
        true
    } else {
        false
    };

    // When over the inline limit, drop the empty slice/ dir so the bundle stays lean.
    if !write_slice_details && slice_dir.exists() {
        let _ = fs::remove_dir_all(&slice_dir);
    }

    slice_functions.sort_by(|a, b| a.name.cmp(&b.name));
    cfg_functions.sort_by(|a, b| a.name.cmp(&b.name));

    let catalog_owned = Arc::try_unwrap(catalog).unwrap_or_else(|arc| (*arc).clone());
    let catalog_entries: Vec<SourceFileEntry> = catalog_owned.entries();
    write_source_index(out_dir, &catalog_entries)?;

    let slice_available = !slice_functions.is_empty();
    write_json_compact(
        &out_dir.join(SLICE_INDEX_FILE),
        &crate::slice_export::SliceIndexPayload {
            schema_version: 2,
            available: slice_available,
            function_count: slice_functions.len(),
            detail_mode: if write_slice_details {
                "per_file".into()
            } else {
                "archive_only".into()
            },
            record_index_path: if slice_record_pack {
                Some(SLICE_RECORD_INDEX_FILE.into())
            } else {
                None
            },
            record_data_path: if slice_record_pack {
                Some(SLICE_RECORD_DATA_FILE.into())
            } else {
                None
            },
            functions: slice_functions.clone(),
        },
    )?;

    let archive_path = CfgPdgArchive::default_path(repo_root);
    let archive_copied = if archive_path.is_file() {
        link_or_copy(&archive_path, &out_dir.join(CFG_ARCHIVE_BUNDLE_NAME))?;
        true
    } else {
        false
    };

    let cfg_available = !cfg_functions.is_empty();
    write_json_compact(
        &out_dir.join(CFG_INDEX_FILE),
        &CfgIndexPayload {
            schema_version: 2,
            available: cfg_available,
            archive_path: if archive_copied {
                Some(CFG_ARCHIVE_BUNDLE_NAME.into())
            } else {
                None
            },
            detail_mode: if write_cfg_details {
                "per_file".into()
            } else {
                "archive_only".into()
            },
            record_index_path: if record_pack_written {
                Some(CFG_RECORD_INDEX_FILE.into())
            } else {
                None
            },
            record_data_path: if record_pack_written {
                Some(CFG_RECORD_DATA_FILE.into())
            } else {
                None
            },
            function_count: cfg_functions.len(),
            functions: cfg_functions,
        },
    )?;

    Ok(StreamedAnalysisExport {
        slice: SliceExportSummary {
            available: slice_available,
            function_count: slice_functions.len(),
            record_pack: slice_record_pack && !write_slice_details,
        },
        cfg: CfgExportSummary {
            available: cfg_available,
            function_count: slice_functions.len(),
            archive_copied,
        },
        dataflow,
        source_catalog: catalog_owned,
    })
}

fn unique_source_paths(index: &AnalysisIndexMap, repo_root: &Path) -> Vec<String> {
    // Stable keys are `file\x1fname\x1fcode_hash` — collect unique files without
    // per-entry `resolve_function_meta` (that was O(functions) graph lookups).
    let mut set = HashSet::new();
    for entry in index.values() {
        let (_name, parsed_path) = parse_stable_key(&entry.stable_key);
        let Some(path) = parsed_path.filter(|p| !p.is_empty()) else {
            continue;
        };
        if Path::new(&path).is_absolute() {
            set.insert(path);
        } else {
            set.insert(repo_root.join(&path).to_string_lossy().into_owned());
        }
    }
    let mut paths: Vec<String> = set.into_iter().collect();
    paths.sort();
    paths
}

#[allow(clippy::too_many_arguments)]
fn export_one(
    entry: &AnalysisIndexEntry,
    storage: &AnalysisStorage,
    repo_root: &Path,
    graph: GraphSource<'_>,
    meta_map: &HashMap<Uuid, (String, Option<String>)>,
    slice_dir: &Path,
    cfg_dir: &Path,
    write_cfg_details: bool,
    write_slice_details: bool,
    catalog: &SourceCatalog,
) -> Option<ExportWork> {
    let analysis = storage.load_function(entry.function_id).ok()??;
    let cfg = analysis.cfg.as_ref()?;
    let pdg = analysis.pdg.as_ref()?;

    let (parsed_name, parsed_path) = parse_stable_key(&entry.stable_key);
    let record_name = if analysis.function_name.is_empty() {
        parsed_name?
    } else {
        analysis.function_name.clone()
    };
    let record_path = if analysis.file_path.is_empty() {
        parsed_path
    } else {
        Some(analysis.file_path.clone())
    };

    let (name, file_path) = resolve_function_meta(
        &entry.function_id,
        &record_name,
        &record_path,
        repo_root,
        graph,
        meta_map,
    );

    let (source_id, total_lines, start_line, end_line) = if let Some(ref path) = file_path {
        if let Some(src) = catalog.get(path) {
            let (start, end) = function_line_span(cfg);
            (
                Some(src.source_id.clone()),
                src.total_lines,
                Some(start),
                Some(end),
            )
        } else {
            (None, 1, None, None)
        }
    } else {
        (None, 1, None, None)
    };

    let pdg_export = export_pdg(pdg, cfg);
    let data_edges = pdg_export.edges.iter().filter(|e| e.kind == "data").count();
    let pdg_nodes = pdg_export.nodes.len();
    let block_count = cfg.blocks.len();
    let cfg_edge_count = cfg.edges.len();
    let cfg_detail = cfg_detail_light(&entry.function_id, &name, file_path.clone(), cfg);
    let bundle = SliceBundlePayload {
        schema_version: 2,
        function_id: entry.function_id.to_string(),
        name: name.clone(),
        file_path: file_path.clone(),
        source: None,
        source_id,
        start_line,
        end_line,
        total_lines,
        pdg: pdg_export,
    };
    // Serialize on the worker; drop CFG/PDG-backed temps before returning.
    let slice_bytes = serde_json::to_vec(&bundle).ok()?;
    drop(bundle);
    if write_slice_details {
        fs::write(
            slice_dir.join(format!("{}.json", entry.function_id)),
            &slice_bytes,
        )
        .ok()?;
    }

    let cfg_bytes = if write_cfg_details {
        let bytes = serde_json::to_vec(&cfg_detail).ok()?;
        drop(cfg_detail);
        fs::write(cfg_dir.join(format!("{}.json", entry.function_id)), &bytes).ok()?;
        None
    } else {
        let bytes = serde_json::to_vec(&cfg_detail).ok()?;
        drop(cfg_detail);
        Some(bytes)
    };
    // Drop analysis (and thus Arc CFG/PDG) before building ExportWork.
    drop(analysis);

    let function_id_str = entry.function_id.to_string();
    Some(ExportWork {
        function_id: entry.function_id,
        slice_entry: SliceFunctionEntry {
            function_id: function_id_str.clone(),
            name: name.clone(),
            file_path: file_path.clone(),
            source_lines: total_lines,
            pdg_nodes,
        },
        cfg_entry: CfgFunctionEntry {
            function_id: function_id_str.clone(),
            name: name.clone(),
            file_path: file_path.clone(),
            block_count,
            cfg_edge_count,
        },
        cfg_bytes,
        dataflow_entry: DataflowFunctionEntry {
            function_id: function_id_str,
            name,
            file_path,
            pdg_nodes,
            data_edges,
            block_count,
        },
        slice_bytes,
    })
}

fn parse_stable_key(key: &str) -> (Option<String>, Option<String>) {
    let mut parts = key.split('\x1f');
    let file = parts.next().map(str::to_string);
    let name = parts.next().map(str::to_string);
    (name, file)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rgctl_analysis::cfg_builder::build_cfg_for_function;
    use rgctl_analysis::pdg::ProgramDependenceGraph;
    use rgctl_analysis::storage::FunctionAnalysis;
    use rgctl_graph::backend::MemoryBackend;
    use std::sync::Arc;
    use tempfile::TempDir;
    use uuid::Uuid;

    #[test]
    fn streams_slice_and_cfg_from_metasfresh_cache() {
        let repo = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../example/metasfresh-4.9.8b");
        let analysis_dir = repo.join(".rgctl/analysis");
        if !analysis_dir.is_dir() {
            return;
        }
        let out = tempfile::TempDir::new().unwrap();
        let backend = MemoryBackend::new();
        let result =
            export_cfg_slice_from_storage(GraphSource::memory(&backend), &repo, out.path())
                .unwrap();
        assert!(result.slice.available, "slice export should succeed");
        assert!(result.cfg.available, "cfg export should succeed");
        assert!(
            result.slice.function_count > 10_000,
            "expected large function count, got {}",
            result.slice.function_count
        );
        assert!(out.path().join("slice_index.json").is_file());
        assert!(out.path().join("cfg_index.json").is_file());
        assert!(out.path().join("cfg_pdg.archive.bin").is_file());
        // Large corpora must use the slice record pack (no per-file JSON flood).
        assert!(
            result.slice.record_pack,
            "metasfresh should exceed SLICE_DETAIL_INLINE_LIMIT"
        );
        assert!(out.path().join(SLICE_RECORD_INDEX_FILE).is_file());
        assert!(out.path().join(SLICE_RECORD_DATA_FILE).is_file());
        assert!(
            !out.path().join("slice").is_dir(),
            "archive_only should not retain slice/ directory"
        );
    }

    #[test]
    fn streams_slice_and_cfg_from_per_function_storage() {
        let tmp = TempDir::new().unwrap();
        let repo = tmp.path().join("repo");
        let analysis_dir = repo.join(".rgctl/analysis");
        let out = tmp.path().join("dashboard");
        std::fs::create_dir_all(&analysis_dir).unwrap();
        std::fs::create_dir_all(&out).unwrap();

        let code = "fn add(a: i32, b: i32) -> i32 { a + b }";
        let cfg = build_cfg_for_function("rust", code, "add").unwrap();
        let pdg = ProgramDependenceGraph::build(&cfg, code.as_bytes()).unwrap();
        let id = Uuid::new_v4();
        let storage = AnalysisStorage::new(&analysis_dir);
        storage
            .save_function(&FunctionAnalysis {
                function_id: id,
                function_name: "add".into(),
                file_path: "src/lib.rs".into(),
                code_hash: Some("hash1".into()),
                cfg: Some(Arc::new(cfg)),
                pdg: Some(Arc::new(pdg)),
                dominance: None,
                taint: None,
            })
            .unwrap();

        let backend = MemoryBackend::new();
        let result =
            export_cfg_slice_from_storage(GraphSource::memory(&backend), &repo, &out).unwrap();
        assert!(result.slice.available);
        assert_eq!(result.slice.function_count, 1);
        assert!(result.cfg.available);
        assert_eq!(result.dataflow.len(), 1);
        assert_eq!(result.dataflow[0].function_id, id.to_string());
        assert!(out.join("slice").join(format!("{id}.json")).is_file());
        assert!(out.join("slice_index.json").is_file());
        assert!(out.join("cfg_index.json").is_file());
        assert!(out.join(SLICE_RECORD_INDEX_FILE).is_file());
        assert!(out.join(SLICE_RECORD_DATA_FILE).is_file());
    }
}
