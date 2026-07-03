use rbuilder::cli::blast_radius_output::{build_from_cache_entry, skipped_gatekeeping, NodeLookup};
use rbuilder::analysis::MacroIndexEntry;
use uuid::Uuid;

#[test]
fn test_cache_entry_omits_unresolved_topology_without_nil_uuid() {
    let caller_id = Uuid::new_v4();
    let entry = MacroIndexEntry {
        id: Uuid::new_v4(),
        symbol_name: "target".into(),
        class_name: None,
        file_path: "src/main.rs".into(),
        score: 1.0,
        direct_caller_ids: vec![caller_id],
        impact_zone_ids: vec![],
        direct_callers: vec!["caller".into()],
        impact_zone: vec![],
        language: "rust".into(),
        signature: None,
        canonical_fqn: "target".into(),
    };
    let response = build_from_cache_entry(
        &entry,
        skipped_gatekeeping(),
        NodeLookup::None,
        &entry.impact_zone_ids,
        entry.score,
        None,
    );
    assert!(response.topology.direct_callers.is_empty());
    assert!(response
        .topology
        .direct_callers
        .iter()
        .all(|c| c.id != Uuid::nil()));
}
