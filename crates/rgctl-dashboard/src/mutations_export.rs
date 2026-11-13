//! Field-mutation index for the Dataflow tab (hybrid CPG Layer F).

use rgctl_analysis::{FieldWriteIndex, FieldWriteKind};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::path::Path;

pub const MUTATIONS_INDEX_FILE: &str = "mutations_index.json";

/// Soft cap so huge monorepos do not blow the static dashboard bundle.
const MAX_WRITES_EXPORTED: usize = 25_000;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MutationsIndexPayload {
    pub schema_version: u32,
    pub available: bool,
    pub write_count: usize,
    /// Total writes in the on-disk field_write index (may exceed exported rows).
    pub indexed_write_count: usize,
    pub type_count: usize,
    /// Sorted distinct resolved receiver types (for typeahead).
    pub types: Vec<String>,
    pub writes: Vec<MutationWriteEntry>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub truncated: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MutationWriteEntry {
    pub function_id: String,
    pub function_name: String,
    pub is_constructor: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub receiver_local: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub receiver_type: Option<String>,
    pub member: String,
    pub file: String,
    pub line: usize,
    pub code_snippet: String,
    pub kind: String,
}

#[derive(Debug, Default, Clone)]
pub struct MutationsExportSummary {
    pub available: bool,
    pub write_count: usize,
    pub type_count: usize,
}

#[allow(dead_code)] // Public convenience wrapper; production path uses `_with_analysis`.
pub fn export_mutations_index(
    repo_root: &Path,
    out_dir: &Path,
) -> Result<MutationsExportSummary, String> {
    export_mutations_index_with_analysis(repo_root, out_dir, None)
}

/// Same as [`export_mutations_index`]; `analysis_index` is accepted for the shared single-load
/// export path (field-write index is still loaded from disk — mutations do not read analysis_index).
pub fn export_mutations_index_with_analysis(
    repo_root: &Path,
    out_dir: &Path,
    _analysis_index: Option<&crate::analysis_stream_export::AnalysisIndexMap>,
) -> Result<MutationsExportSummary, String> {
    let Some(index) = FieldWriteIndex::open_if_exists(repo_root).map_err(|e| e.to_string())? else {
        let empty = MutationsIndexPayload {
            schema_version: 1,
            available: false,
            write_count: 0,
            indexed_write_count: 0,
            type_count: 0,
            types: vec![],
            writes: vec![],
            truncated: None,
        };
        write_json(&out_dir.join(MUTATIONS_INDEX_FILE), &empty)?;
        return Ok(MutationsExportSummary::default());
    };

    let indexed_write_count = index.writes.len();
    let mut types = BTreeSet::new();
    for w in &index.writes {
        if let Some(t) = &w.receiver_type {
            if !t.is_empty() {
                types.insert(t.clone());
            }
        }
    }
    let type_list: Vec<String> = types.into_iter().collect();
    let type_count = type_list.len();

    let truncated = indexed_write_count > MAX_WRITES_EXPORTED;
    let exported_slice = if truncated {
        // Prefer typed, non-ctor writes when trimming.
        let mut ranked: Vec<_> = index.writes.iter().collect();
        ranked.sort_by_key(|w| {
            let unresolved = matches!(w.kind, FieldWriteKind::Unresolved);
            (unresolved, w.is_constructor, w.file.as_str(), w.line)
        });
        ranked
            .into_iter()
            .take(MAX_WRITES_EXPORTED)
            .collect::<Vec<_>>()
    } else {
        index.writes.iter().collect()
    };

    let writes: Vec<MutationWriteEntry> = exported_slice
        .into_iter()
        .map(|w| MutationWriteEntry {
            function_id: w.function_id.to_string(),
            function_name: w.function_name.clone(),
            is_constructor: w.is_constructor,
            receiver_local: w.receiver_local.clone(),
            receiver_type: w.receiver_type.clone(),
            member: w.member.clone(),
            file: w.file.clone(),
            line: w.line,
            code_snippet: w.code_snippet.clone(),
            kind: kind_label(w.kind).into(),
        })
        .collect();

    let payload = MutationsIndexPayload {
        schema_version: 1,
        available: true,
        write_count: writes.len(),
        indexed_write_count,
        type_count,
        types: type_list,
        writes,
        truncated: truncated.then_some(true),
    };
    write_json(&out_dir.join(MUTATIONS_INDEX_FILE), &payload)?;

    Ok(MutationsExportSummary {
        available: true,
        write_count: payload.write_count,
        type_count,
    })
}

fn kind_label(kind: FieldWriteKind) -> &'static str {
    match kind {
        FieldWriteKind::DirectField => "DirectField",
        FieldWriteKind::ThisField => "ThisField",
        FieldWriteKind::Unresolved => "Unresolved",
    }
}

fn write_json(path: &Path, value: &impl Serialize) -> Result<(), String> {
    crate::export_util::write_json_compact(path, value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rgctl_analysis::{FieldWrite, FieldWriteIndex, FieldWriteKind};
    use std::fs;
    use tempfile::tempdir;
    use uuid::Uuid;

    #[test]
    fn exports_empty_when_no_field_write_index() {
        let tmp = tempdir().unwrap();
        let out = tmp.path().join("dash");
        fs::create_dir_all(&out).unwrap();
        let summary = export_mutations_index(tmp.path(), &out).unwrap();
        assert!(!summary.available);
        let payload: MutationsIndexPayload =
            serde_json::from_slice(&fs::read(out.join(MUTATIONS_INDEX_FILE)).unwrap()).unwrap();
        assert!(!payload.available);
        assert!(payload.writes.is_empty());
    }

    #[test]
    fn exports_writes_and_types() {
        let tmp = tempdir().unwrap();
        let analysis = tmp.path().join(".rgctl").join("analysis");
        fs::create_dir_all(&analysis).unwrap();
        let index = FieldWriteIndex {
            version: 1,
            graph_digest: None,
            writes: vec![FieldWrite {
                function_id: Uuid::nil(),
                function_name: "priceShoppingCart".into(),
                is_constructor: false,
                receiver_local: Some("sc".into()),
                receiver_type: Some("ShoppingCart".into()),
                member: "cartTotal".into(),
                file: "ShoppingCartService.java".into(),
                line: 75,
                code_snippet: "sc.cartTotal = x".into(),
                kind: FieldWriteKind::DirectField,
            }],
        };
        index
            .write_to_path(&analysis.join("field_write.index.bin"))
            .unwrap();

        let out = tmp.path().join("dash");
        fs::create_dir_all(&out).unwrap();
        let summary = export_mutations_index(tmp.path(), &out).unwrap();
        assert!(summary.available);
        assert_eq!(summary.write_count, 1);
        assert_eq!(summary.type_count, 1);

        let payload: MutationsIndexPayload =
            serde_json::from_slice(&fs::read(out.join(MUTATIONS_INDEX_FILE)).unwrap()).unwrap();
        assert_eq!(payload.types, vec!["ShoppingCart".to_string()]);
        assert_eq!(payload.writes[0].member, "cartTotal");
    }
}
