//! Macro-to-micro hand-off: blast radius impact zone → interprocedural slices.

use crate::blast_radius_scc::{BlastRadiusEngine, BlastRadiusResult};
use crate::callgraph::CallGraph;
use crate::interprocedural_cfg::InterproceduralCFG;
use crate::interprocedural_slicing::{InterproceduralSlice, InterproceduralSlicer};
use crate::pdg::ProgramDependenceGraph;
use crate::slicing::SliceCriterion;
use rbuilder_error::Result;
use rbuilder_graph::backend::{GraphBackend, MemoryBackend};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use uuid::Uuid;

/// Seed for an interprocedural slice derived from a call-site parameter mapping.
#[derive(Debug, Clone)]
pub struct SliceHandoffSeed {
    /// Callee function receiving the argument.
    pub callee_id: Uuid,
    /// Callee function name.
    pub callee_name: String,
    /// Direct caller invoking the callee.
    pub caller_id: Uuid,
    /// Caller function name.
    pub caller_name: String,
    /// Callee formal parameter name.
    pub param_name: String,
    /// Zero-based index of the callee formal parameter.
    pub param_index: usize,
    /// Call-site source line (when known).
    pub call_site_line: usize,
}

/// Full trace from blast radius through to line-level slices.
#[derive(Debug, Clone)]
pub struct BlastSliceTrace {
    /// Entry symbol analyzed for blast radius.
    pub symbol_name: String,
    /// Blast-radius result for the entry symbol.
    pub blast: BlastRadiusResult,
    /// Interprocedural slice seeds derived from call-site hand-offs.
    pub handoffs: Vec<SliceHandoffSeed>,
    /// Resolved slices as `(function_id, function_name, slice)`.
    pub slices: Vec<(Uuid, String, InterproceduralSlice)>,
}

/// Resolve slice seeds from blast-radius direct callers into callee parameters.
pub fn resolve_handoff_seeds(
    backend: &MemoryBackend,
    blast: &BlastRadiusResult,
    symbol_id: Uuid,
) -> Result<Vec<SliceHandoffSeed>> {
    let call_graph = CallGraph::from_backend(backend)?;
    let symbol_name = backend
        .get_node(symbol_id)?
        .map(|n| n.name.clone())
        .unwrap_or_else(|| symbol_id.to_string());

    let mut seeds = Vec::new();
    for &caller_id in &blast.direct_caller_ids {
        let caller_name = backend
            .get_node(caller_id)?
            .map(|n| n.name.clone())
            .unwrap_or_default();
        let edges = call_graph.call_edges_between(caller_id, symbol_id);
        let params: Vec<String> = call_graph
            .parameter_names(symbol_id)
            .iter()
            .cloned()
            .collect();

        for edge in edges {
            for (param_index, param_name) in params.iter().enumerate() {
                seeds.push(SliceHandoffSeed {
                    callee_id: symbol_id,
                    callee_name: symbol_name.clone(),
                    caller_id,
                    caller_name: caller_name.clone(),
                    param_name: param_name.clone(),
                    param_index,
                    call_site_line: edge.call_site,
                });
            }
            if params.is_empty() {
                seeds.push(SliceHandoffSeed {
                    callee_id: symbol_id,
                    callee_name: symbol_name.clone(),
                    caller_id,
                    caller_name: caller_name.clone(),
                    param_name: "input".into(),
                    param_index: 0,
                    call_site_line: edge.call_site,
                });
            }
        }
    }

    for &impact_id in &blast.impact_zone_ids {
        if impact_id == symbol_id || blast.direct_caller_ids.contains(&impact_id) {
            continue;
        }
        let impact_name = backend
            .get_node(impact_id)?
            .map(|n| n.name.clone())
            .unwrap_or_default();
        for &caller_id in call_graph.callers(impact_id).iter() {
            if !blast.impact_zone_ids.contains(&caller_id) && caller_id != symbol_id {
                continue;
            }
            let caller_name = backend
                .get_node(caller_id)?
                .map(|n| n.name.clone())
                .unwrap_or_default();
            for edge in call_graph.call_edges_between(caller_id, impact_id) {
                let params: Vec<String> = call_graph
                    .parameter_names(impact_id)
                    .iter()
                    .cloned()
                    .collect();
                for (param_index, param_name) in params.iter().enumerate() {
                    seeds.push(SliceHandoffSeed {
                        callee_id: impact_id,
                        callee_name: impact_name.clone(),
                        caller_id,
                        caller_name: caller_name.clone(),
                        param_name: param_name.clone(),
                        param_index,
                        call_site_line: edge.call_site,
                    });
                }
            }
        }
    }

    Ok(dedupe_seeds(seeds))
}

/// Keep hand-off seeds for a single callee parameter index.
pub fn filter_handoff_seeds_by_index(
    seeds: &[SliceHandoffSeed],
    param_index: usize,
) -> Vec<SliceHandoffSeed> {
    seeds
        .iter()
        .filter(|s| s.param_index == param_index)
        .cloned()
        .collect()
}

/// Resolve seeds and retain only the requested parameter indices (e.g. mutated args only).
pub fn resolve_handoff_seeds_for_indices(
    backend: &MemoryBackend,
    blast: &BlastRadiusResult,
    symbol_id: Uuid,
    param_indices: &[usize],
) -> Result<Vec<SliceHandoffSeed>> {
    let all = resolve_handoff_seeds(backend, blast, symbol_id)?;
    Ok(all
        .into_iter()
        .filter(|s| param_indices.contains(&s.param_index))
        .collect())
}

fn dedupe_seeds(seeds: Vec<SliceHandoffSeed>) -> Vec<SliceHandoffSeed> {
    let mut seen = HashSet::new();
    seeds
        .into_iter()
        .filter(|s| {
            seen.insert((
                s.caller_id,
                s.callee_id,
                s.param_index,
                s.call_site_line,
            ))
        })
        .collect()
}

/// Build slice criterion for a callee parameter (first use line in callee PDG).
pub fn criterion_for_parameter(
    backend: &MemoryBackend,
    icfg: &InterproceduralCFG,
    source_files: &HashMap<String, String>,
    callee_id: Uuid,
    param_name: &str,
) -> Result<SliceCriterion> {
    let cfg = icfg
        .get_cfg(callee_id)
        .ok_or_else(|| rbuilder_error::Error::NotFound(format!("cfg {callee_id}")))?;
    let source = source_for_function(backend, source_files, callee_id)?;
    let pdg = ProgramDependenceGraph::build(cfg, source.as_bytes())?;

    let line = pdg
        .nodes
        .values()
        .find(|n| n.used_vars.contains(param_name) || n.defined_vars.contains(param_name))
        .map(|n| n.statement.line)
        .unwrap_or(1);

    Ok(SliceCriterion {
        variable: param_name.to_string(),
        line,
    })
}

fn source_for_function(
    backend: &MemoryBackend,
    source_files: &HashMap<String, String>,
    func_id: Uuid,
) -> Result<String> {
    let file_path = backend
        .get_node(func_id)?
        .and_then(|n| n.file_path)
        .unwrap_or_default();
    if let Some(content) = source_files.get(&file_path) {
        return Ok(content.clone());
    }
    source_files
        .iter()
        .find(|(k, _)| k.ends_with(&file_path) || file_path.ends_with(k.as_str()))
        .map(|(_, v)| v.clone())
        .ok_or_else(|| rbuilder_error::Error::NotFound(format!("source for {file_path}")))
}

/// Load source files referenced by function nodes under `repo_root`.
pub fn load_source_files(backend: &MemoryBackend, repo_root: &Path) -> HashMap<String, String> {
    let mut files = HashMap::new();
    let mut paths = HashSet::new();
    if let Ok(ids) = backend.find_node_ids_by_type(rbuilder_graph::schema::NodeType::Function) {
        for id in ids {
            if let Ok(Some(node)) = backend.get_node(id) {
                if let Some(path) = &node.file_path {
                    paths.insert(path.clone());
                }
            }
        }
    }
    for path in paths {
        let abs = repo_root.join(&path);
        let read_path = if abs.exists() {
            abs
        } else {
            PathBuf::from(&path)
        };
        if read_path.exists() {
            if let Ok(content) = std::fs::read_to_string(&read_path) {
                files.insert(path, content);
            }
        }
    }
    files
}

/// Run blast radius and derive interprocedural slices for each hand-off seed.
pub fn trace_blast_to_slices(
    backend: &MemoryBackend,
    repo_root: &Path,
    symbol_name: &str,
) -> Result<BlastSliceTrace> {
    let engine = BlastRadiusEngine::build(backend)?;
    let (symbol_id, resolved_name) =
        crate::blast_radius::resolve_unique_symbol(backend, symbol_name)?;
    let blast = engine.analyze(symbol_id)?;
    trace_blast_to_slices_with_blast(backend, repo_root, symbol_id, &resolved_name, &blast)
}

/// Derive interprocedural slices from a pre-computed blast-radius result.
pub fn trace_blast_to_slices_with_blast(
    backend: &MemoryBackend,
    repo_root: &Path,
    symbol_id: Uuid,
    symbol_name: &str,
    blast: &BlastRadiusResult,
) -> Result<BlastSliceTrace> {
    let handoffs = resolve_handoff_seeds(backend, blast, symbol_id)?;

    let source_files = load_source_files(backend, repo_root);
    let archive = crate::cfg_pdg_archive::CfgPdgArchive::open_if_exists(repo_root)?;
    let icfg = if let Some(ref archive) = archive {
        archive.to_interprocedural_cfg(backend)?
    } else {
        InterproceduralCFG::build(backend, &source_files)?
    };
    let slicer = InterproceduralSlicer::new(&icfg, backend, &source_files)?;
    if let Some(ref archive) = archive {
        slicer.preload_pdgs(archive);
    }

    let mut slices = Vec::new();
    let mut sliced = HashSet::new();
    for seed in &handoffs {
        let key = (seed.callee_id, seed.param_index);
        if !sliced.insert(key) {
            continue;
        }
        let criterion = criterion_for_parameter(
            backend,
            &icfg,
            &source_files,
            seed.callee_id,
            &seed.param_name,
        )?;
        let slice = slicer.slice(seed.callee_id, criterion)?;
        slices.push((seed.callee_id, seed.callee_name.clone(), slice));
    }

    Ok(BlastSliceTrace {
        symbol_name: symbol_name.to_string(),
        blast: blast.clone(),
        handoffs,
        slices,
    })
}
