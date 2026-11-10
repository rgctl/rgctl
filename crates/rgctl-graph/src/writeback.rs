//! Piece-table span splicing and lossless write-back Trust Ledger.

use crate::cst_archive::CstArchiveReader;
use crate::lossless_content::LosslessContentReader;
use rgctl_error::{Error, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Why a span was replaced.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EditReason {
    /// Identifier / symbol rename.
    Rename,
    /// Import / type path rewrite.
    ImportRewrite,
    /// Call-site rewrite.
    CallSite,
    /// Explicit plan entry.
    Plan,
    /// Other / unspecified.
    Other,
}

/// One non-overlapping byte-range replacement.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpanEdit {
    /// Inclusive start byte in the original buffer.
    pub start_byte: u32,
    /// Exclusive end byte in the original buffer.
    pub end_byte: u32,
    /// Replacement bytes.
    #[serde(with = "serde_bytes_cow")]
    pub replacement: Vec<u8>,
    /// Edit rationale.
    pub reason: EditReason,
}

mod serde_bytes_cow {
    use serde::Serializer;
    use serde::{Deserialize, Deserializer};

    // Serde `with` modules must take `&Vec<u8>` for a `Vec<u8>` field.
    #[allow(clippy::ptr_arg)]
    pub fn serialize<S: Serializer>(bytes: &Vec<u8>, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&String::from_utf8_lossy(bytes))
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<u8>, D::Error> {
        let s = String::deserialize(d)?;
        Ok(s.into_bytes())
    }
}

/// Write-back failure modes.
#[derive(Debug)]
pub enum WritebackError {
    /// Overlapping or unsorted edits.
    ConflictRejected,
    /// Post-splice tree-sitter introduced new ERROR/MISSING nodes.
    SyntaxErrorRejected,
    /// Underlying I/O / format error.
    Other(Error),
}

impl std::fmt::Display for WritebackError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ConflictRejected => write!(f, "overlapping or unsorted span edits"),
            Self::SyntaxErrorRejected => write!(f, "syntax error introduced by splice"),
            Self::Other(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for WritebackError {}

impl From<Error> for WritebackError {
    fn from(value: Error) -> Self {
        Self::Other(value)
    }
}

/// Per-file Trust Ledger status.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TrustStatus {
    /// Reconstructed bytes match on-disk source.
    VerifiedIdentical,
    /// Span edits applied and post-splice syntax validated.
    PatchedVerified,
    /// File not present in the lossless content store.
    SkippedNonTier1,
    /// Overlapping or unsorted edits rejected.
    ConflictRejected,
    /// Splice introduced new tree-sitter ERROR/MISSING nodes.
    SyntaxErrorRejected,
    /// Original already had parse errors; patched buffer allowed through.
    ParseWarningPassthrough,
}

/// One Trust Ledger row.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrustLedgerEntry {
    /// Repo-relative path.
    pub file: String,
    /// Outcome.
    pub status: TrustStatus,
    /// BLAKE3 hex before (empty when skipped).
    pub blake3_before: String,
    /// BLAKE3 hex after.
    pub blake3_after: String,
    /// Macro / ERROR spans observed.
    pub macro_or_error_spans: Vec<(u32, u32)>,
}

/// Aggregate Trust Ledger report.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TrustLedgerReport {
    /// Files considered by verify / export / patch.
    pub files_processed: usize,
    /// Files whose reconstructed bytes matched on disk (or were exported identically).
    pub files_verified_identical: usize,
    /// Total bytes written by export / patch.
    pub bytes_written: u64,
    /// Number of span edits successfully applied.
    pub edits_applied: usize,
    /// Per-file ledger rows.
    pub entries: Vec<TrustLedgerEntry>,
}

/// Apply sorted, disjoint span edits (piece-table materialization).
pub fn apply_span_edits(
    source: &[u8],
    edits: &[SpanEdit],
) -> std::result::Result<Vec<u8>, WritebackError> {
    if edits.is_empty() {
        return Ok(source.to_vec());
    }
    for i in 1..edits.len() {
        if edits[i - 1].end_byte > edits[i].start_byte {
            return Err(WritebackError::ConflictRejected);
        }
    }
    for e in edits {
        if e.start_byte > e.end_byte || e.end_byte as usize > source.len() {
            return Err(WritebackError::ConflictRejected);
        }
    }
    let removed: usize = edits
        .iter()
        .map(|e| (e.end_byte - e.start_byte) as usize)
        .sum();
    let added: usize = edits.iter().map(|e| e.replacement.len()).sum();
    let mut out = Vec::with_capacity(source.len() + added - removed);
    let mut cursor = 0usize;
    for e in edits {
        let start = e.start_byte as usize;
        out.extend_from_slice(&source[cursor..start]);
        out.extend_from_slice(&e.replacement);
        cursor = e.end_byte as usize;
    }
    out.extend_from_slice(&source[cursor..]);
    Ok(out)
}

/// Reconstruct a file from the lossless content store (CST optional for verify).
pub fn reconstruct_file(content: &LosslessContentReader, file_path: &str) -> Result<Vec<u8>> {
    let idx = content
        .find_path(file_path)?
        .ok_or_else(|| Error::SerdeError(format!("missing lossless frame for {file_path}")))?;
    content.decompress_at(idx)
}

/// Verify reconstructed bytes match on-disk source (BLAKE3 + length + content).
pub fn verify_file_bytes(reconstructed: &[u8], on_disk: &[u8]) -> bool {
    reconstructed == on_disk
}

/// Hex BLAKE3 of bytes.
pub fn blake3_hex(bytes: &[u8]) -> String {
    blake3::hash(bytes).to_hex().to_string()
}

/// Build a rename `SpanEdit` for an identifier span.
pub fn rename_edit(start_byte: u32, end_byte: u32, new_name: &str) -> SpanEdit {
    SpanEdit {
        start_byte,
        end_byte,
        replacement: new_name.as_bytes().to_vec(),
        reason: EditReason::Rename,
    }
}

/// Paths used by write-back under a repo root.
pub fn lossless_artifact_paths(repo_root: &Path) -> (PathBuf, PathBuf) {
    let rgctl = repo_root.join(".rgctl");
    (
        rgctl.join(crate::lossless_content::LOSSLESS_CONTENT_STORE_FILE),
        rgctl.join(crate::cst_archive::CST_ARCHIVE_FILE),
    )
}

/// Open lossless sidecars when present.
pub fn open_lossless_sidecars(
    repo_root: &Path,
) -> Result<(LosslessContentReader, Option<CstArchiveReader>)> {
    let (content_path, cst_path) = lossless_artifact_paths(repo_root);
    let content = LosslessContentReader::open(&content_path)?;
    let cst = if cst_path.exists() {
        Some(CstArchiveReader::open(&cst_path)?)
    } else {
        None
    };
    Ok((content, cst))
}

/// Validate that a spliced buffer does not introduce new tree-sitter errors.
///
/// `parse` should use `rgctl_plugin_helpers::parse_source` (thread-local parser).
pub fn validate_syntax_unchanged<F>(
    original: &[u8],
    patched: &[u8],
    mut parse: F,
    force: bool,
) -> std::result::Result<(), WritebackError>
where
    F: FnMut(&[u8]) -> std::result::Result<bool, Error>,
{
    let orig_err = parse(original).map_err(WritebackError::Other)?;
    let new_err = parse(patched).map_err(WritebackError::Other)?;
    if new_err && !orig_err && !force {
        return Err(WritebackError::SyntaxErrorRejected);
    }
    Ok(())
}

/// Helper to sort edits by start_byte (stable).
pub fn sort_edits(edits: &mut [SpanEdit]) {
    edits.sort_by_key(|e| e.start_byte);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn piece_table_preserves_unmodified_spans() {
        let src = b"fn foo() { bar(); }\n";
        let edits = [rename_edit(3, 6, "baz")]; // foo -> baz
        let out = apply_span_edits(src, &edits).unwrap();
        assert_eq!(&out, b"fn baz() { bar(); }\n");
    }

    #[test]
    fn overlapping_edits_rejected() {
        let src = b"abcdefghij";
        let edits = [
            SpanEdit {
                start_byte: 0,
                end_byte: 5,
                replacement: b"X".to_vec(),
                reason: EditReason::Other,
            },
            SpanEdit {
                start_byte: 3,
                end_byte: 7,
                replacement: b"Y".to_vec(),
                reason: EditReason::Other,
            },
        ];
        assert!(matches!(
            apply_span_edits(src, &edits),
            Err(WritebackError::ConflictRejected)
        ));
    }

    #[test]
    fn syntax_validation_rejects_new_errors() {
        let original = b"fn ok() {}";
        let patched = b"fn @bad() {}";
        let r = validate_syntax_unchanged(original, patched, |buf| Ok(buf.contains(&b'@')), false);
        assert!(matches!(r, Err(WritebackError::SyntaxErrorRejected)));
    }
}
