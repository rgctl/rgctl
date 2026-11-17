//! Revision checks prevent unchanged source from reusing obsolete lowering.

use super::*;
use crate::cfg_builder::build_cfg_for_function;
use crate::cfg_pdg_archive::{
    ARCHIVE_VERSION, CfgPdgArchive, CfgPdgArchiveMmap, CfgPdgRecord, EnsureCfgPdgOpts,
    ensure_cfg_pdg_for_source,
};
use crate::pdg::ProgramDependenceGraph;
use crate::storage::{
    ANALYSIS_INDEX_FILE, AnalysisIndexEntry, AnalysisStorage, FunctionAnalysis, analysis_cache_key,
};
use std::sync::Arc;
use uuid::Uuid;

#[test]
fn cfg_round_trip_preserves_predicates_and_rejects_unversioned_lowering() {
    let source = "fn example() { let enabled = true; if enabled { consume(); } }";
    let cfg = build_cfg_for_function("rust", source, "example").unwrap();
    let bytes = bincode::serialize(&cfg).unwrap();
    let restored: ControlFlowGraph = bincode::deserialize(&bytes).unwrap();
    assert!(
        restored
            .blocks
            .values()
            .flat_map(|b| &b.statements)
            .any(|s| s.kind == StatementKind::Branch && s.used_vars.iter().any(|v| v == "enabled"))
    );
    assert_eq!(cfg.edges.len(), restored.edges.len());
    assert_eq!(
        cfg.successors(cfg.entry),
        restored.successors(restored.entry)
    );

    #[derive(Serialize)]
    struct LegacyCfg<'a> {
        blocks: &'a HashMap<BlockId, BasicBlock>,
        edges: &'a [CfgEdge],
        entry: BlockId,
        exits: &'a [BlockId],
    }
    let legacy = LegacyCfg {
        blocks: &cfg.blocks,
        edges: &cfg.edges,
        entry: cfg.entry,
        exits: &cfg.exits,
    };
    assert!(
        bincode::deserialize::<ControlFlowGraph>(&bincode::serialize(&legacy).unwrap()).is_err()
    );
    let mut json = serde_json::to_value(&cfg).unwrap();
    json.as_object_mut().unwrap().remove("revision");
    assert!(serde_json::from_value::<ControlFlowGraph>(json).is_err());

    let mut future = bytes;
    future[4] = future[4].wrapping_add(1);
    assert!(bincode::deserialize::<ControlFlowGraph>(&future).is_err());
}

#[test]
fn lightweight_index_drops_old_semantics_before_fast_reuse_or_purge() {
    let tmp = tempfile::tempdir().unwrap();
    let storage = AnalysisStorage::new(tmp.path());
    let id = Uuid::new_v4();
    let entry = AnalysisIndexEntry {
        stable_key: "demo.rs\x1fexample\x1funchanged".into(),
        function_id: id,
        code_hash: "unchanged".into(),
        flow_count: 0,
        vulnerable_count: 0,
    };
    std::fs::write(
        tmp.path().join(ANALYSIS_INDEX_FILE),
        bincode::serialize(&vec![entry]).unwrap(),
    )
    .unwrap();
    assert!(storage.load_analysis_index().unwrap().is_empty());
    let current = FunctionAnalysis {
        function_id: id,
        function_name: "example".into(),
        file_path: "demo.rs".into(),
        code_hash: Some("unchanged".into()),
        cfg: None,
        pdg: None,
        dominance: None,
        taint: None,
    };
    storage.save_function(&current).unwrap();
    let key = analysis_cache_key(id, "unchanged");
    assert_eq!(storage.load_analysis_index().unwrap().len(), 1);
    storage
        .purge_stale_by_stable_keys(&HashSet::from([key]))
        .unwrap();
    assert!(storage.load_function(id).unwrap().is_some());
}

#[test]
fn on_demand_rebuilds_old_archive_without_preserving_stale_records() {
    let tmp = tempfile::tempdir().unwrap();
    let source = "fn example() { let enabled = true; if enabled { consume(); } }";
    let cfg = build_cfg_for_function("rust", source, "example").unwrap();
    let pdg = ProgramDependenceGraph::build(&cfg, source.as_bytes()).unwrap();
    let id = Uuid::new_v4();
    let other = Uuid::new_v4();
    let mut archive = CfgPdgArchive::default();
    for function_id in [id, other] {
        archive.insert(CfgPdgRecord {
            function_id,
            code_hash: rgctl_graph::code_index::hash_code(source),
            function_name: "example".into(),
            file_path: Some("demo.rs".into()),
            cfg: Arc::new(cfg.clone()),
            pdg: Arc::new(pdg.clone()),
        });
    }
    let path = CfgPdgArchive::default_path(tmp.path());
    archive.write_to_path(&path).unwrap();
    let mut bytes = std::fs::read(&path).unwrap();
    bytes[4..8].copy_from_slice(&3u32.to_le_bytes());
    std::fs::write(&path, bytes).unwrap();
    assert!(CfgPdgArchiveMmap::open(&path).is_err());
    assert!(CfgPdgArchive::load_from_path(&path).is_err());
    let rebuilt = ensure_cfg_pdg_for_source(EnsureCfgPdgOpts {
        repo_root: tmp.path(),
        language: "rust",
        source,
        function_name: "example",
        file_path: Some("demo.rs"),
        start_byte: None,
        end_byte: None,
        function_id: Some(id),
        persist: true,
    })
    .unwrap();
    assert!(
        rebuilt
            .pdg
            .nodes
            .values()
            .any(|n| n.statement.kind == StatementKind::Branch && n.used_vars.contains("enabled"))
    );
    let view = CfgPdgArchiveMmap::open(&path).unwrap();
    assert!(view.contains(id));
    assert!(!view.contains(other));
    assert_eq!(view.len(), 1);
    assert_eq!(
        u32::from_le_bytes(std::fs::read(&path).unwrap()[4..8].try_into().unwrap()),
        ARCHIVE_VERSION
    );
}
