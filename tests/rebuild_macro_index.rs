//! Manual helper to rebuild caches (macro index, graph snapshot, blast engine) for an existing discover cache.

use rbuilder::analysis::{BlastEngineSnapshot, BlastRadiusEngine, MacroCallIndex};
use rbuilder::graph::backend::GraphBackend;
use rbuilder::graph::schema::NodeType;
use rbuilder::graph::{CodeGraph, PreparedGraphSnapshot};
use std::path::Path;
use std::time::Instant;

#[test]
#[ignore = "manual: rebuild caches for metasfresh"]
fn rebuild_metasfresh_caches() {
    let repo = Path::new("/Users/sshaaf/git/rust/rBuilder/example/metasfresh-4.9.8b");
    let db = repo.join(".rbuilder/graph.db");
    let json = std::fs::read_to_string(&db).expect("read graph.db");

    let load_start = Instant::now();
    let graph = CodeGraph::import_json(&json).expect("import graph");
    eprintln!("JSON load: {:.2}s", load_start.elapsed().as_secs_f64());

    let backend = graph.backend();
    let functions: Vec<_> = backend
        .all_node_ids()
        .expect("node ids")
        .into_iter()
        .filter(|id| {
            backend
                .get_node(*id)
                .ok()
                .flatten()
                .is_some_and(|n| n.node_type == NodeType::Function)
        })
        .collect();

    let snap_start = Instant::now();
    let prepared = PreparedGraphSnapshot::from_backend(backend).expect("prepare snapshot");
    let digest = prepared.content_digest.clone();
    prepared
        .write_to_path(&repo.join(".rbuilder/graph.snapshot.bin"))
        .expect("write graph snapshot");
    eprintln!("Graph snapshot: {:.2}s", snap_start.elapsed().as_secs_f64());

    let engine_start = Instant::now();
    let engine = BlastRadiusEngine::build(backend).expect("build engine");
    let blast_snap = engine.to_engine_snapshot(digest.clone());
    blast_snap
        .write_to_path(&BlastEngineSnapshot::default_path(repo))
        .expect("write blast snapshot");
    eprintln!("Blast engine build + snapshot: {:.2}s", engine_start.elapsed().as_secs_f64());

    let index = MacroCallIndex::rebuild_and_save(repo, &db, backend, &functions).expect("rebuild");
    assert!(!index.entries.is_empty());
    eprintln!("Macro index entries: {}", index.entries.len());
}
