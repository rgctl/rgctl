//! Parked unresolved / external-stub relations for cross-repo `rgctl merge`.

use rgctl_error::{Error, Result};
use rgctl_plugin_api::RelationType;
use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;
use uuid::Uuid;

/// Magic bytes for `.rgctl/parked_relations.bin`.
pub const PARKED_RELATIONS_MAGIC: &[u8; 4] = b"RBPK";
/// Schema version for parked relation archives (v2 interns repeated caller/file/target strings).
pub const PARKED_RELATIONS_VERSION: u32 = 2;
/// Relative artifact name under `.rgctl/`.
pub const PARKED_RELATIONS_FILE: &str = "parked_relations.bin";

const OPT_STR_NONE: u32 = u32::MAX;

/// Unresolved or external-stubbed relation captured during single-repo discover.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ParkedRelationRecord {
    /// UUID of the caller/source node in this repository.
    pub from_node_id: Uuid,
    /// Qualified name of the caller/source node.
    pub from_qualified_name: String,
    /// Repo-relative file path of the caller/source node.
    pub from_file: String,
    /// Raw target symbol name from `Relation.to`.
    pub to_name: String,
    /// Optional qualified hint from `Relation.to_qualified_hint`.
    pub to_qualified_hint: Option<String>,
    /// Optional receiver/parameter type hint.
    pub to_type_hint: Option<String>,
    /// Module/package path imported in `from_file` that binds the target.
    pub import_module_hint: Option<String>,
    /// Original relation kind (`Calls`, `Uses`, `Extends`, `Implements`, `Injects`).
    pub relation_type: RelationType,
    /// 1-based call-site line.
    pub call_site_line: u32,
    /// External stub UUID when GraphBuilder emitted `<external>` for this target.
    pub stub_node_id: Option<Uuid>,
}

#[derive(Serialize)]
struct InternedParkedArchiveRef<'a> {
    strings: Vec<&'a str>,
    records: Vec<CompactParkedRecord>,
}

#[derive(Deserialize)]
struct InternedParkedArchiveOwned {
    strings: Vec<String>,
    records: Vec<CompactParkedRecord>,
}

#[derive(Serialize, Deserialize)]
struct CompactParkedRecord {
    from_node_id: Uuid,
    from_qualified_name: u32,
    from_file: u32,
    to_name: u32,
    to_qualified_hint: u32,
    to_type_hint: u32,
    import_module_hint: u32,
    relation_type: RelationType,
    call_site_line: u32,
    stub_node_id: Option<Uuid>,
}

/// Serialize parked relations as `RBPK` + little-endian version + string-interned bincode payload.
pub fn save_parked_relations(path: &Path, records: &[ParkedRelationRecord]) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    fn intern_str<'a>(
        s: &'a str,
        strings: &mut Vec<&'a str>,
        pool: &mut FxHashMap<&'a str, u32>,
    ) -> u32 {
        if let Some(&id) = pool.get(s) {
            id
        } else {
            let id = strings.len() as u32;
            strings.push(s);
            pool.insert(s, id);
            id
        }
    }

    let mut strings: Vec<&str> = Vec::with_capacity(records.len() / 2 + 16);
    let mut pool: FxHashMap<&str, u32> =
        FxHashMap::with_capacity_and_hasher(records.len() / 2 + 16, Default::default());

    let mut compact: Vec<CompactParkedRecord> = Vec::with_capacity(records.len());
    for rec in records {
        let from_qualified_name = intern_str(&rec.from_qualified_name, &mut strings, &mut pool);
        let from_file = intern_str(&rec.from_file, &mut strings, &mut pool);
        let to_name = intern_str(&rec.to_name, &mut strings, &mut pool);
        let to_qualified_hint = rec
            .to_qualified_hint
            .as_deref()
            .map(|s| intern_str(s, &mut strings, &mut pool))
            .unwrap_or(OPT_STR_NONE);
        let to_type_hint = rec
            .to_type_hint
            .as_deref()
            .map(|s| intern_str(s, &mut strings, &mut pool))
            .unwrap_or(OPT_STR_NONE);
        let import_module_hint = rec
            .import_module_hint
            .as_deref()
            .map(|s| intern_str(s, &mut strings, &mut pool))
            .unwrap_or(OPT_STR_NONE);
        compact.push(CompactParkedRecord {
            from_node_id: rec.from_node_id,
            from_qualified_name,
            from_file,
            to_name,
            to_qualified_hint,
            to_type_hint,
            import_module_hint,
            relation_type: rec.relation_type,
            call_site_line: rec.call_site_line,
            stub_node_id: rec.stub_node_id,
        });
    }
    drop(pool);

    let archive = InternedParkedArchiveRef {
        strings,
        records: compact,
    };
    let file = File::create(path)?;
    let mut writer = std::io::BufWriter::with_capacity(256 * 1024, file);
    writer.write_all(PARKED_RELATIONS_MAGIC)?;
    writer.write_all(&PARKED_RELATIONS_VERSION.to_le_bytes())?;
    bincode::serialize_into(&mut writer, &archive)
        .map_err(|e| Error::SerdeError(format!("parked_relations serialize: {e}")))?;
    writer.flush()?;
    Ok(())
}

/// Load parked relations written by [`save_parked_relations`] (supports v1 and v2).
pub fn load_parked_relations(path: &Path) -> Result<Vec<ParkedRelationRecord>> {
    let mut file = File::open(path)?;
    let mut magic = [0u8; 4];
    file.read_exact(&mut magic)?;
    if &magic != PARKED_RELATIONS_MAGIC {
        return Err(Error::SerdeError(format!(
            "invalid parked_relations magic: {:?}",
            String::from_utf8_lossy(&magic)
        )));
    }
    let mut ver_buf = [0u8; 4];
    file.read_exact(&mut ver_buf)?;
    let version = u32::from_le_bytes(ver_buf);
    let mut payload = Vec::new();
    file.read_to_end(&mut payload)?;
    match version {
        1 => bincode::deserialize(&payload)
            .map_err(|e| Error::SerdeError(format!("parked_relations v1 deserialize: {e}"))),
        PARKED_RELATIONS_VERSION => {
            let archive: InternedParkedArchiveOwned = bincode::deserialize(&payload)
                .map_err(|e| Error::SerdeError(format!("parked_relations deserialize: {e}")))?;
            let get_str = |idx: u32| -> Result<String> {
                archive.strings.get(idx as usize).cloned().ok_or_else(|| {
                    Error::SerdeError("parked_relations string index out of bounds".into())
                })
            };
            let get_opt = |idx: u32| -> Result<Option<String>> {
                if idx == OPT_STR_NONE {
                    Ok(None)
                } else {
                    get_str(idx).map(Some)
                }
            };
            let mut out = Vec::with_capacity(archive.records.len());
            for rec in archive.records {
                out.push(ParkedRelationRecord {
                    from_node_id: rec.from_node_id,
                    from_qualified_name: get_str(rec.from_qualified_name)?,
                    from_file: get_str(rec.from_file)?,
                    to_name: get_str(rec.to_name)?,
                    to_qualified_hint: get_opt(rec.to_qualified_hint)?,
                    to_type_hint: get_opt(rec.to_type_hint)?,
                    import_module_hint: get_opt(rec.import_module_hint)?,
                    relation_type: rec.relation_type,
                    call_site_line: rec.call_site_line,
                    stub_node_id: rec.stub_node_id,
                });
            }
            Ok(out)
        }
        _ => Err(Error::SerdeError(format!(
            "unsupported parked_relations version {version}"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn parked_relations_roundtrip() {
        let dir = tempdir().unwrap();
        let path = dir.path().join(PARKED_RELATIONS_FILE);
        let records = vec![
            ParkedRelationRecord {
                from_node_id: Uuid::new_v4(),
                from_qualified_name: "src.checkout.processOrder".into(),
                from_file: "src/checkout.ts".into(),
                to_name: "UserService.charge".into(),
                to_qualified_hint: Some("UserService.charge".into()),
                to_type_hint: Some("UserService".into()),
                import_module_hint: Some("@acme/billing/user".into()),
                relation_type: RelationType::Calls,
                call_site_line: 12,
                stub_node_id: None,
            },
            ParkedRelationRecord {
                from_node_id: Uuid::new_v4(),
                from_qualified_name: "src.checkout.processOrder".into(),
                from_file: "src/checkout.ts".into(),
                to_name: "AuditService.log".into(),
                to_qualified_hint: None,
                to_type_hint: Some("AuditService".into()),
                import_module_hint: Some("@acme/billing/audit".into()),
                relation_type: RelationType::Calls,
                call_site_line: 13,
                stub_node_id: None,
            },
        ];
        save_parked_relations(&path, &records).unwrap();
        let loaded = load_parked_relations(&path).unwrap();
        assert_eq!(loaded, records);
    }
}
