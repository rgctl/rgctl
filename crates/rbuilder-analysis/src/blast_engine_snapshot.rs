//! Serialized SCC blast-radius engine for mmap reload without recomputation.
//!
//! Format v2 stores sparse, zstd-compressed reachability rows. Trivial rows (only
//! the SCC itself reachable) are omitted and reconstructed on load.

use bit_set::BitSet;
use memmap2::Mmap;
use rbuilder_error::{Error, Result};
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use uuid::Uuid;

use crate::blast_radius_scc::BlastRadiusEngine;

/// Magic bytes for blast engine snapshots (`RBSE`).
pub const BLAST_SNAPSHOT_MAGIC: [u8; 4] = *b"RBSE";
/// Legacy dense reachability format.
pub const BLAST_SNAPSHOT_VERSION_V1: u32 = 1;
/// Sparse + zstd-compressed reachability rows.
pub const BLAST_SNAPSHOT_VERSION: u32 = 2;
/// Default blast engine snapshot filename under `.rbuilder/`.
pub const BLAST_SNAPSHOT_FILE: &str = "blast_engine.snapshot.bin";

/// One sparse reachability row (v2).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReachabilityRow {
    /// SCC index for this row.
    pub scc_idx: u32,
    /// zstd-compressed little-endian `u64` bitset words.
    pub compressed: Vec<u8>,
}

/// Serializable blast-radius engine state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlastEngineSnapshot {
    /// Digest of the graph snapshot this engine was built from.
    pub graph_digest: String,
    /// Number of strongly connected components in the call graph.
    pub scc_count: usize,
    /// SCC DAG edges (from_idx, to_idx).
    pub dag_edges: Vec<(usize, usize)>,
    /// Member UUIDs per SCC.
    pub scc_members: Vec<Vec<Uuid>>,
    /// Display name per SCC.
    pub scc_names: Vec<String>,
    /// Function UUID → SCC index.
    pub node_to_scc: Vec<(Uuid, usize)>,
    /// v1 dense reachability (legacy).
    #[serde(default)]
    pub reachability_words: Vec<Vec<u64>>,
    /// v2 sparse compressed reachability rows.
    #[serde(default)]
    pub reachability_rows: Vec<ReachabilityRow>,
}

impl BlastEngineSnapshot {
    /// Write snapshot file with v2 header.
    pub fn write_to_path(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let payload = bincode::serialize(self).map_err(serde_err)?;
        let mut file = File::create(path)?;
        file.write_all(&BLAST_SNAPSHOT_MAGIC)?;
        file.write_all(&BLAST_SNAPSHOT_VERSION.to_le_bytes())?;
        file.write_all(&(payload.len() as u64).to_le_bytes())?;
        file.write_all(&payload)?;
        Ok(())
    }

    /// Load snapshot from disk via mmap (avoids an extra full-file buffer copy).
    pub fn load_from_path(path: &Path) -> Result<Self> {
        let file = File::open(path)?;
        let mmap = unsafe { Mmap::map(&file)? };
        parse_blast_payload(&mmap)
    }

    /// Default path under a repository root.
    pub fn default_path(repo_root: &Path) -> PathBuf {
        repo_root
            .join(rbuilder_graph::code_graph::GRAPH_DIR)
            .join(BLAST_SNAPSHOT_FILE)
    }
}

pub(crate) fn bitset_to_words(bs: &BitSet, bit_len: usize) -> Vec<u64> {
    let word_count = bit_len.div_ceil(64);
    (0..word_count)
        .map(|w| {
            let mut val = 0u64;
            for b in 0..64 {
                let idx = w * 64 + b;
                if idx >= bit_len {
                    break;
                }
                if bs.contains(idx) {
                    val |= 1u64 << b;
                }
            }
            val
        })
        .collect()
}

pub(crate) fn words_popcount(words: &[u64]) -> u32 {
    words.iter().map(|w| w.count_ones()).sum()
}

pub(crate) fn compress_words(words: &[u64]) -> Result<Vec<u8>> {
    let raw: Vec<u8> = words.iter().flat_map(|w| w.to_le_bytes()).collect();
    zstd::encode_all(raw.as_slice(), 3).map_err(|e| Error::SerdeError(format!("zstd compress: {e}")))
}

pub(crate) fn decompress_words(compressed: &[u8], word_count: usize) -> Result<Vec<u64>> {
    let raw = zstd::decode_all(compressed)
        .map_err(|e| Error::SerdeError(format!("zstd decompress: {e}")))?;
    if raw.len() != word_count * 8 {
        return Err(Error::SerdeError(format!(
            "reachability row size mismatch: expected {} bytes, got {}",
            word_count * 8,
            raw.len()
        )));
    }
    Ok(raw
        .chunks_exact(8)
        .map(|chunk| u64::from_le_bytes(chunk.try_into().unwrap()))
        .collect())
}

pub(crate) fn words_to_bitset(words: &[u64], bit_len: usize) -> BitSet {
    let mut bs = BitSet::new();
    for (w, &word) in words.iter().enumerate() {
        for b in 0..64 {
            let idx = w * 64 + b;
            if idx >= bit_len {
                break;
            }
            if (word & (1u64 << b)) != 0 {
                bs.insert(idx);
            }
        }
    }
    bs
}

pub(crate) fn reachability_from_snapshot(snapshot: &BlastEngineSnapshot) -> Result<Vec<BitSet>> {
    let scc_count = snapshot.scc_count;
    let word_count = scc_count.div_ceil(64);
    let mut reachability: Vec<BitSet> = (0..scc_count)
        .map(|idx| {
            let mut bs = BitSet::new();
            bs.insert(idx);
            bs
        })
        .collect();

    if !snapshot.reachability_rows.is_empty() {
        for row in &snapshot.reachability_rows {
            let idx = row.scc_idx as usize;
            if idx >= scc_count {
                return Err(Error::SerdeError(format!(
                    "reachability row index {idx} out of range (scc_count={scc_count})"
                )));
            }
            let words = decompress_words(&row.compressed, word_count)?;
            reachability[idx] = words_to_bitset(&words, scc_count);
        }
        return Ok(reachability);
    }

    if snapshot.reachability_words.len() != scc_count {
        return Err(Error::SerdeError(format!(
            "v1 reachability row count {} != scc_count {scc_count}",
            snapshot.reachability_words.len()
        )));
    }
    for (idx, words) in snapshot.reachability_words.iter().enumerate() {
        reachability[idx] = words_to_bitset(words, scc_count);
    }
    Ok(reachability)
}

fn parse_blast_payload(bytes: &[u8]) -> Result<BlastEngineSnapshot> {
    if bytes.len() < 16 {
        return Err(Error::SerdeError("blast snapshot truncated".into()));
    }
    if bytes[0..4] != BLAST_SNAPSHOT_MAGIC {
        return Err(Error::SerdeError("invalid blast snapshot magic".into()));
    }
    let version = u32::from_le_bytes(bytes[4..8].try_into().unwrap());
    if version != BLAST_SNAPSHOT_VERSION && version != BLAST_SNAPSHOT_VERSION_V1 {
        return Err(Error::SerdeError(format!(
            "unsupported blast snapshot version {version}"
        )));
    }
    let payload_len = u64::from_le_bytes(bytes[8..16].try_into().unwrap()) as usize;
    if bytes.len() < 16 + payload_len {
        return Err(Error::SerdeError("blast snapshot payload truncated".into()));
    }
    bincode::deserialize(&bytes[16..16 + payload_len]).map_err(serde_err)
}

fn serde_err(e: bincode::Error) -> Error {
    Error::SerdeError(format!("blast snapshot: {e}"))
}

/// Load engine from disk if digest matches.
pub fn try_load_engine(repo_root: &Path, graph_digest: &str) -> Result<Option<BlastRadiusEngine>> {
    let path = BlastEngineSnapshot::default_path(repo_root);
    if !path.exists() {
        return Ok(None);
    }
    let snap = BlastEngineSnapshot::load_from_path(&path)?;
    if snap.graph_digest != graph_digest {
        return Ok(None);
    }
    BlastRadiusEngine::from_engine_snapshot(snap).map(Some)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn compress_round_trip() {
        let words = vec![0b1010u64, 0u64, 1u64 << 63];
        let compressed = compress_words(&words).unwrap();
        let restored = decompress_words(&compressed, words.len()).unwrap();
        assert_eq!(words, restored);
    }

    #[test]
    fn sparse_self_only_rows_omitted() {
        let scc_count = 4;
        let mut reachability: Vec<BitSet> = (0..scc_count)
            .map(|idx| {
                let mut bs = BitSet::new();
                bs.insert(idx);
                bs
            })
            .collect();
        reachability[2].insert(1);

        let mut rows = Vec::new();
        for (idx, bs) in reachability.iter().enumerate() {
            let words = bitset_to_words(bs, scc_count);
            if words_popcount(&words) <= 1 {
                continue;
            }
            rows.push(ReachabilityRow {
                scc_idx: idx as u32,
                compressed: compress_words(&words).unwrap(),
            });
        }
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].scc_idx, 2);

        let snap = BlastEngineSnapshot {
            graph_digest: "test".into(),
            scc_count,
            dag_edges: vec![],
            scc_members: vec![vec![]; scc_count],
            scc_names: vec!["a".into(); scc_count],
            node_to_scc: vec![],
            reachability_words: vec![],
            reachability_rows: rows,
        };
        let loaded = reachability_from_snapshot(&snap).unwrap();
        assert_eq!(loaded[2].contains(1), reachability[2].contains(1));
        assert_eq!(loaded[0].contains(0), true);
    }
}
