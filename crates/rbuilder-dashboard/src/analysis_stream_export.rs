//! Stream CFG/PDG dashboard export from per-function `.analysis.bin` files (no monolithic archive load).

use crate::cfg_export::{
    cfg_detail_light, write_empty_cfg_index, CfgExportSummary, CfgFunctionEntry, CfgIndexPayload,
    CFG_ARCHIVE_BUNDLE_NAME, CFG_DETAIL_DIR, CFG_DETAIL_INLINE_LIMIT, CFG_INDEX_FILE,
};
use crate::export_util::{link_or_copy, write_json_compact};
use crate::function_meta::{function_meta_map, resolve_function_meta};
use crate::slice_export::{
    export_pdg, function_line_span, write_empty_slice_index, SliceExportSummary,
    SliceBundlePayload, SliceFunctionEntry, SLICE_DETAIL_DIR, SLICE_INDEX_FILE,
};
use crate::source_catalog::{ensure_source_file, write_source_index, SourceFileEntry};
use rbuilder_analysis::cfg_pdg_archive::CfgPdgArchive;
use rbuilder_analysis::storage::{AnalysisIndexEntry, AnalysisStorage};
use rbuilder_graph::backend::MemoryBackend;
use rayon::prelude::*;
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::sync::Mutex;
use uuid::Uuid;

pub struct StreamedAnalysisExport {
    pub slice: SliceExportSummary,
    pub cfg: CfgExportSummary,
}

struct ExportWork {
    slice_entry: SliceFunctionEntry,
    cfg_entry: CfgFunctionEntry,
}

/// Export slice + CFG indexes and details by loading one function analysis at a time.
pub fn export_cfg_slice_from_storage(
    backend: &MemoryBackend,
    repo_root: &Path,
    out_dir: &Path,
) -> Result<StreamedAnalysisExport, String> {
    let analysis_dir = repo_root.join(".rbuilder/analysis");
    let storage = AnalysisStorage::new(&analysis_dir);
    let index = storage.load_analysis_index().map_err(|e| e.to_string())?;

    if index.is_empty() {
        write_empty_slice_index(&out_dir.join(SLICE_INDEX_FILE))?;
        write_empty_cfg_index(&out_dir.join(CFG_INDEX_FILE))?;
        return Ok(StreamedAnalysisExport {
            slice: SliceExportSummary::default(),
            cfg: CfgExportSummary::default(),
        });
    }

    let meta_map = function_meta_map(repo_root, backend);
    let write_cfg_details = index.len() <= CFG_DETAIL_INLINE_LIMIT;

    let slice_dir = out_dir.join(SLICE_DETAIL_DIR);
    if slice_dir.exists() {
        fs::remove_dir_all(&slice_dir).map_err(|e| e.to_string())?;
    }
    fs::create_dir_all(&slice_dir).map_err(|e| e.to_string())?;

    let cfg_dir = out_dir.join(CFG_DETAIL_DIR);
    if cfg_dir.exists() {
        fs::remove_dir_all(&cfg_dir).map_err(|e| e.to_string())?;
    }
    if write_cfg_details {
        fs::create_dir_all(&cfg_dir).map_err(|e| e.to_string())?;
    }

    let source_cache = Mutex::new(HashMap::<String, SourceFileEntry>::new());
    let entries: Vec<AnalysisIndexEntry> = index.into_values().collect();

    let works: Vec<Option<ExportWork>> = entries
        .par_iter()
        .map(|entry| {
            export_one(
                entry,
                &storage,
                repo_root,
                backend,
                &meta_map,
                &slice_dir,
                &cfg_dir,
                out_dir,
                write_cfg_details,
                &source_cache,
            )
        })
        .collect();

    let mut slice_functions = Vec::new();
    let mut cfg_functions = Vec::new();
    for work in works.into_iter().flatten() {
        slice_functions.push(work.slice_entry);
        cfg_functions.push(work.cfg_entry);
    }

    slice_functions.sort_by(|a, b| a.name.cmp(&b.name));
    cfg_functions.sort_by(|a, b| a.name.cmp(&b.name));

    let catalog: Vec<_> = source_cache
        .lock()
        .map_err(|e| e.to_string())?
        .values()
        .cloned()
        .collect();
    write_source_index(out_dir, &catalog)?;

    let slice_available = !slice_functions.is_empty();
    write_json_compact(
        &out_dir.join(SLICE_INDEX_FILE),
        &crate::slice_export::SliceIndexPayload {
            schema_version: 2,
            available: slice_available,
            function_count: slice_functions.len(),
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
            function_count: cfg_functions.len(),
            functions: cfg_functions,
        },
    )?;

    Ok(StreamedAnalysisExport {
        slice: SliceExportSummary {
            available: slice_available,
            function_count: slice_functions.len(),
        },
        cfg: CfgExportSummary {
            available: cfg_available,
            function_count: slice_functions.len(),
            archive_copied,
        },
    })
}

fn export_one(
    entry: &AnalysisIndexEntry,
    storage: &AnalysisStorage,
    repo_root: &Path,
    backend: &MemoryBackend,
    meta_map: &HashMap<Uuid, (String, Option<String>)>,
    slice_dir: &Path,
    cfg_dir: &Path,
    out_dir: &Path,
    write_cfg_details: bool,
    source_cache: &Mutex<HashMap<String, SourceFileEntry>>,
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
        backend,
        meta_map,
    );

    let (source_id, total_lines, start_line, end_line) =
        if let Some(ref path) = file_path {
            let mut cache = source_cache.lock().ok()?;
            if let Some(src) = ensure_source_file(out_dir, path, &mut cache) {
                let (start, end) = function_line_span(cfg);
                (
                    Some(src.source_id),
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
        pdg: pdg_export.clone(),
    };
    write_json_compact(
        &slice_dir.join(format!("{}.json", entry.function_id)),
        &bundle,
    )
    .ok()?;

    if write_cfg_details {
        let detail = cfg_detail_light(&entry.function_id, &name, file_path.clone(), cfg);
        write_json_compact(
            &cfg_dir.join(format!("{}.json", entry.function_id)),
            &detail,
        )
        .ok()?;
    }

    Some(ExportWork {
        slice_entry: SliceFunctionEntry {
            function_id: entry.function_id.to_string(),
            name: name.clone(),
            file_path,
            source_lines: total_lines,
            pdg_nodes: pdg_export.nodes.len(),
        },
        cfg_entry: CfgFunctionEntry {
            function_id: entry.function_id.to_string(),
            name,
            file_path: bundle.file_path,
            block_count: cfg.blocks.len(),
            cfg_edge_count: cfg.edges.len(),
        },
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
    use rbuilder_analysis::cfg_builder::build_cfg_for_function;
    use rbuilder_analysis::pdg::ProgramDependenceGraph;
    use rbuilder_analysis::storage::FunctionAnalysis;
    use rbuilder_graph::backend::MemoryBackend;
    use tempfile::TempDir;
    use uuid::Uuid;

    #[test]
    fn streams_slice_and_cfg_from_metasfresh_cache() {
        let repo = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../example/metasfresh-4.9.8b");
        let analysis_dir = repo.join(".rbuilder/analysis");
        if !analysis_dir.is_dir() {
            return;
        }
        let out = tempfile::TempDir::new().unwrap();
        let backend = MemoryBackend::new();
        let result =
            export_cfg_slice_from_storage(&backend, &repo, out.path()).unwrap();
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
    }

    #[test]
    fn streams_slice_and_cfg_from_per_function_storage() {
        let tmp = TempDir::new().unwrap();
        let repo = tmp.path().join("repo");
        let analysis_dir = repo.join(".rbuilder/analysis");
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
                cfg: Some(cfg),
                pdg: Some(pdg),
                dominance: None,
                taint: None,
            })
            .unwrap();

        let backend = MemoryBackend::new();
        let result =
            export_cfg_slice_from_storage(&backend, &repo, &out).unwrap();
        assert!(result.slice.available);
        assert_eq!(result.slice.function_count, 1);
        assert!(result.cfg.available);
        assert!(out.join("slice").join(format!("{id}.json")).is_file());
        assert!(out.join("slice_index.json").is_file());
        assert!(out.join("cfg_index.json").is_file());
    }
}
