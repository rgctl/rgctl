//! End-to-end discovery must not reuse source positions after a callable moves.

use rgctl::analysis::cfg_pdg_archive::{CfgPdgArchive, CfgPdgArchiveMmap};
use std::process::Command;

#[test]
fn relocated_unchanged_callable_rebuilds_its_operand_locations() {
    let tmp = tempfile::tempdir().unwrap();
    let source = "function consume(value) { return value.execute(); }\n";
    let path = tmp.path().join("app.js");
    let mut expected_id = None;
    let mut expected_hash = None;
    for prefix in ["", "// inserted before the unchanged function\n\n"] {
        let shifted = format!("{prefix}{source}");
        std::fs::write(&path, &shifted).unwrap();
        let output = Command::new(env!("CARGO_BIN_EXE_rgctl"))
            .current_dir(tmp.path())
            .args(["discover", ".", "--languages", "javascript", "--with-cfg"])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "discover failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let view = CfgPdgArchiveMmap::open(&CfgPdgArchive::default_path(tmp.path())).unwrap();
        let record = view
            .function_ids()
            .into_iter()
            .filter_map(|id| view.load_record(id).unwrap())
            .find(|r| r.function_name == "consume")
            .unwrap();
        if let Some(id) = expected_id {
            assert_eq!(
                record.function_id, id,
                "fixture must preserve canonical callable identity"
            );
        }
        if let Some(hash) = &expected_hash {
            assert_eq!(
                &record.code_hash, hash,
                "fixture must preserve the function body hash"
            );
        }
        expected_id = Some(record.function_id);
        expected_hash = Some(record.code_hash.clone());
        let facts = &record.cfg.operands;
        let call = &facts.calls[0];
        let start = shifted.find("value.execute()").unwrap();
        assert_eq!(call.span.start as usize, start);
        assert_eq!(
            &shifted[call.span.start as usize..call.span.end as usize],
            "value.execute()"
        );
        assert_eq!(facts.span.unwrap().start as usize, prefix.len());
        assert_eq!(
            facts.source_line,
            Some(if prefix.is_empty() { 1 } else { 3 })
        );
    }
}
