//! Revision checks prevent unchanged source from reusing obsolete lowering.

use super::*;
use crate::cfg_builder::build_cfg_for_function;

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
