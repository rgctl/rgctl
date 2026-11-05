//! Parked unresolved / external-stub relations for cross-repo `rgctl merge`.

use rgctl_error::{Error, Result};
use rgctl_plugin_api::RelationType;
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;
use uuid::Uuid;

/// Magic bytes for `.rgctl/parked_relations.bin`.
pub const PARKED_RELATIONS_MAGIC: &[u8; 4] = b"RBPK";
/// Schema version for parked relation archives.
pub const PARKED_RELATIONS_VERSION: u32 = 1;
/// Relative artifact name under `.rgctl/`.
pub const PARKED_RELATIONS_FILE: &str = "parked_relations.bin";

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

/// Serialize parked relations as `RBPK` + little-endian version + bincode payload.
pub fn save_parked_relations(path: &Path, records: &[ParkedRelationRecord]) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut file = File::create(path)?;
    file.write_all(PARKED_RELATIONS_MAGIC)?;
    file.write_all(&PARKED_RELATIONS_VERSION.to_le_bytes())?;
    let payload = bincode::serialize(records)
        .map_err(|e| Error::SerdeError(format!("parked_relations serialize: {e}")))?;
    file.write_all(&payload)?;
    Ok(())
}

/// Load parked relations from `{rgctl_dir}/parked_relations.bin` (or `{repo}/.rgctl/parked_relations.bin`), or empty if missing.
pub fn read_parked_relations(rgctl_dir: &Path) -> Result<Vec<ParkedRelationRecord>> {
    let direct = rgctl_dir.join(PARKED_RELATIONS_FILE);
    if direct.is_file() {
        return load_parked_relations(&direct);
    }
    let nested = rgctl_dir.join(".rgctl").join(PARKED_RELATIONS_FILE);
    if nested.is_file() {
        return load_parked_relations(&nested);
    }
    Ok(Vec::new())
}

/// Load parked relations written by [`save_parked_relations`].
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
    if version != PARKED_RELATIONS_VERSION {
        return Err(Error::SerdeError(format!(
            "unsupported parked_relations version {version}"
        )));
    }
    let mut payload = Vec::new();
    file.read_to_end(&mut payload)?;
    bincode::deserialize(&payload)
        .map_err(|e| Error::SerdeError(format!("parked_relations deserialize: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn parked_relations_roundtrip() {
        let dir = tempdir().unwrap();
        let path = dir.path().join(PARKED_RELATIONS_FILE);
        let records = vec![ParkedRelationRecord {
            from_node_id: Uuid::new_v4(),
            from_qualified_name: "checkout".into(),
            from_file: "src/checkout.ts".into(),
            to_name: "UserService.charge".into(),
            to_qualified_hint: Some("UserService.charge".into()),
            to_type_hint: Some("UserService".into()),
            import_module_hint: Some("@acme/billing/user".into()),
            relation_type: RelationType::Calls,
            call_site_line: 12,
            stub_node_id: None,
        }];
        save_parked_relations(&path, &records).unwrap();
        let loaded = load_parked_relations(&path).unwrap();
        assert_eq!(loaded, records);
    }
}
