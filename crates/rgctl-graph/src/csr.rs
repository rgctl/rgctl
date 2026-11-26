//! Typed bidirectional CSR topology for cache-friendly graph algorithms.
//!
//! Each directed edge stores a dense `u32` target plus a 1-byte [`EdgeType`] code
//! (~5 bytes/edge per direction). Incoming adjacency is materialized so reverse
//! traversal (blast radius, community) stays O(degree) without scanning all edges.

use crate::backend::MemoryBackend;
use crate::schema::EdgeType;
use memmap2::Mmap;
use rgctl_error::{Error, Result};
use std::collections::HashMap;
use std::sync::Arc;
use uuid::Uuid;

/// Encode [`EdgeType`] as a stable `u8` (shared with columnar snapshots).
pub fn edge_type_to_u8(t: EdgeType) -> u8 {
    match t {
        EdgeType::Calls => 0,
        EdgeType::Contains => 1,
        EdgeType::Uses => 2,
        EdgeType::Implements => 3,
        EdgeType::Extends => 4,
        EdgeType::References => 5,
        EdgeType::Instantiates => 6,
        EdgeType::Modifies => 7,
        EdgeType::UsesConfig => 8,
        EdgeType::DefinedIn => 9,
        EdgeType::DependsOn => 10,
        EdgeType::IncludesRole => 11,
        EdgeType::DependsOnRole => 12,
        EdgeType::ExecutesTask => 13,
        EdgeType::NotifiesHandler => 14,
        EdgeType::IncludesPlaybook => 15,
        EdgeType::RendersTemplate => 16,
        EdgeType::DependsOnCookbook => 17,
        EdgeType::IncludesRecipe => 18,
        EdgeType::DeclaresResource => 19,
        EdgeType::UsesTemplate => 20,
        EdgeType::DefinesAttribute => 21,
        EdgeType::NotifiesResource => 22,
        EdgeType::DependsOnModule => 23,
        EdgeType::IncludesClass => 24,
        EdgeType::InheritsClass => 25,
        EdgeType::RequiresResource => 26,
        EdgeType::UsesFact => 27,
        EdgeType::AnnotatedWith => 28,
        EdgeType::Permits => 29,
        // 30 was EdgeType::Violates (Kantra); reserved retired slot — do not reuse.
        EdgeType::Overrides => 31,
        EdgeType::ExposesRoute => 32,
        EdgeType::Injects => 33,
        EdgeType::CoChangesWith => 47,
        EdgeType::Unknown => 255,
    }
}

/// Decode a stable `u8` edge-type code.
pub fn edge_type_from_u8(v: u8) -> Result<EdgeType> {
    Ok(match v {
        0 => EdgeType::Calls,
        1 => EdgeType::Contains,
        2 => EdgeType::Uses,
        3 => EdgeType::Implements,
        4 => EdgeType::Extends,
        5 => EdgeType::References,
        6 => EdgeType::Instantiates,
        7 => EdgeType::Modifies,
        8 => EdgeType::UsesConfig,
        9 => EdgeType::DefinedIn,
        10 => EdgeType::DependsOn,
        11 => EdgeType::IncludesRole,
        12 => EdgeType::DependsOnRole,
        13 => EdgeType::ExecutesTask,
        14 => EdgeType::NotifiesHandler,
        15 => EdgeType::IncludesPlaybook,
        16 => EdgeType::RendersTemplate,
        17 => EdgeType::DependsOnCookbook,
        18 => EdgeType::IncludesRecipe,
        19 => EdgeType::DeclaresResource,
        20 => EdgeType::UsesTemplate,
        21 => EdgeType::DefinesAttribute,
        22 => EdgeType::NotifiesResource,
        23 => EdgeType::DependsOnModule,
        24 => EdgeType::IncludesClass,
        25 => EdgeType::InheritsClass,
        26 => EdgeType::RequiresResource,
        27 => EdgeType::UsesFact,
        28 => EdgeType::AnnotatedWith,
        29 => EdgeType::Permits,
        // Retired Violates (Kantra) — reserved slot; Overrides stays 31.
        30 => {
            return Err(Error::SerdeError(format!(
                "retired edge type code {v} (former Violates)"
            )));
        }
        31 => EdgeType::Overrides,
        32 => EdgeType::ExposesRoute,
        33 => EdgeType::Injects,
        47 => EdgeType::CoChangesWith,
        255 => EdgeType::Unknown,
        _ => return Err(Error::SerdeError(format!("unknown edge type code {v}"))),
    })
}

#[derive(Debug, Clone)]
enum CsrStorage {
    Owned {
        row_ptr: Vec<u32>,
        targets: Vec<u32>,
        edge_types: Vec<u8>,
        in_row_ptr: Vec<u32>,
        in_targets: Vec<u32>,
        in_edge_types: Vec<u8>,
    },
    #[cfg(target_endian = "little")]
    Mmap {
        mmap: Arc<Mmap>,
        node_count: usize,
        edge_count: usize,
        off_row_ptr: usize,
        off_in_row_ptr: usize,
        off_targets: usize,
        off_in_targets: usize,
        off_edge_types: usize,
        off_in_edge_types: usize,
    },
}

/// Compressed sparse row adjacency with parallel edge-type codes (both directions).
///
/// Within each vertex `u`, both outgoing (`out_neighbors`) and incoming
/// (`in_neighbors`) slices are sorted by `(edge_type_code, neighbor_index)`, so
/// single-`EdgeType` queries (`out_neighbors_of_type` / `in_neighbors_of_type`)
/// slice contiguous `&[u32]` sub-ranges in `O(log deg(u))` without scanning
/// unrelated edge types.
#[derive(Debug, Clone)]
pub struct CodeGraphCsr {
    storage: CsrStorage,
}

#[inline]
fn sort_parallel_by_type_then_target(
    targets: &mut [u32],
    types: &mut [u8],
    scratch: &mut Vec<(u8, u32)>,
) {
    let len = targets.len();
    if len <= 1 {
        return;
    }
    if len <= 32 {
        for i in 1..len {
            let ty = types[i];
            let tgt = targets[i];
            let key = (ty, tgt);
            let mut j = i;
            while j > 0 && (types[j - 1], targets[j - 1]) > key {
                types[j] = types[j - 1];
                targets[j] = targets[j - 1];
                j -= 1;
            }
            types[j] = ty;
            targets[j] = tgt;
        }
        return;
    }
    scratch.clear();
    scratch.reserve(len);
    for i in 0..len {
        scratch.push((types[i], targets[i]));
    }
    scratch.sort_unstable();
    for (i, &(ty, tgt)) in scratch.iter().enumerate() {
        types[i] = ty;
        targets[i] = tgt;
    }
}

#[inline]
fn sort_parallel_with_pad_by_type_then_target(
    targets: &mut [u32],
    types: &mut [u8],
    pads: &mut [[u8; 7]],
    scratch: &mut Vec<(u8, u32, [u8; 7])>,
) {
    let len = targets.len();
    if len <= 1 {
        return;
    }
    if len <= 32 {
        for i in 1..len {
            let ty = types[i];
            let tgt = targets[i];
            let pad = pads[i];
            let key = (ty, tgt);
            let mut j = i;
            while j > 0 && (types[j - 1], targets[j - 1]) > key {
                types[j] = types[j - 1];
                targets[j] = targets[j - 1];
                pads[j] = pads[j - 1];
                j -= 1;
            }
            types[j] = ty;
            targets[j] = tgt;
            pads[j] = pad;
        }
        return;
    }
    scratch.clear();
    scratch.reserve(len);
    for i in 0..len {
        scratch.push((types[i], targets[i], pads[i]));
    }
    scratch.sort_unstable_by_key(|a| (a.0, a.1));
    for (i, &(ty, tgt, pad)) in scratch.iter().enumerate() {
        types[i] = ty;
        targets[i] = tgt;
        pads[i] = pad;
    }
}

fn decode_u32_le_vec(bytes: &[u8], count: usize) -> Vec<u32> {
    let mut out = Vec::with_capacity(count);
    for &chunk in bytes.as_chunks::<4>().0.iter().take(count) {
        out.push(u32::from_le_bytes(chunk));
    }
    out
}

impl CodeGraphCsr {
    /// Construct an owned CSR from pre-validated parallel arrays.
    pub(crate) fn from_owned_parts(
        row_ptr: Vec<u32>,
        targets: Vec<u32>,
        edge_types: Vec<u8>,
        in_row_ptr: Vec<u32>,
        in_targets: Vec<u32>,
        in_edge_types: Vec<u8>,
    ) -> Self {
        Self {
            storage: CsrStorage::Owned {
                row_ptr,
                targets,
                edge_types,
                in_row_ptr,
                in_targets,
                in_edge_types,
            },
        }
    }

    /// Construct a zero-copy CSR backed by a little-endian mmap region (or decode
    /// into owned vectors if unaligned / big-endian).
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn from_mmap(
        mmap: Arc<Mmap>,
        node_count: usize,
        edge_count: usize,
        off_row_ptr: usize,
        off_in_row_ptr: usize,
        off_targets: usize,
        off_in_targets: usize,
        off_edge_types: usize,
        off_in_edge_types: usize,
    ) -> Result<Self> {
        let ptr_bytes = node_count
            .checked_add(1)
            .and_then(|n| n.checked_mul(4))
            .ok_or_else(|| Error::SerdeError("CSR row_ptr byte length overflow".into()))?;
        let edge_u32_bytes = edge_count
            .checked_mul(4)
            .ok_or_else(|| Error::SerdeError("CSR target byte length overflow".into()))?;

        let check_range = |start: usize, len: usize, label: &str| -> Result<usize> {
            let end = start
                .checked_add(len)
                .ok_or_else(|| Error::SerdeError(format!("CSR {label} offset overflow")))?;
            if end > mmap.len() {
                return Err(Error::SerdeError(format!(
                    "CSR {label} out of range ({end} > {})",
                    mmap.len()
                )));
            }
            Ok(end)
        };

        let end_rp = check_range(off_row_ptr, ptr_bytes, "row_ptr")?;
        let end_irp = check_range(off_in_row_ptr, ptr_bytes, "in_row_ptr")?;
        let end_t = check_range(off_targets, edge_u32_bytes, "targets")?;
        let end_it = check_range(off_in_targets, edge_u32_bytes, "in_targets")?;
        let end_et = check_range(off_edge_types, edge_count, "edge_types")?;
        let end_iet = check_range(off_in_edge_types, edge_count, "in_edge_types")?;

        let read_u32 = |off: usize| -> u32 {
            let b = &mmap[off..off + 4];
            u32::from_le_bytes([b[0], b[1], b[2], b[3]])
        };
        let rp_first = read_u32(off_row_ptr);
        let rp_last = read_u32(off_row_ptr + node_count * 4);
        let irp_first = read_u32(off_in_row_ptr);
        let irp_last = read_u32(off_in_row_ptr + node_count * 4);
        if rp_first != 0
            || rp_last as usize != edge_count
            || irp_first != 0
            || irp_last as usize != edge_count
        {
            return Err(Error::SerdeError(
                "CSR header endpoints inconsistent with edge_count".into(),
            ));
        }

        #[cfg(target_endian = "little")]
        {
            let base = mmap.as_ptr() as usize;
            let aligned = ((base + off_row_ptr)
                | (base + off_in_row_ptr)
                | (base + off_targets)
                | (base + off_in_targets))
                & 3
                == 0;
            if aligned {
                let _ = (end_rp, end_irp, end_t, end_it, end_et, end_iet);
                return Ok(Self {
                    storage: CsrStorage::Mmap {
                        mmap,
                        node_count,
                        edge_count,
                        off_row_ptr,
                        off_in_row_ptr,
                        off_targets,
                        off_in_targets,
                        off_edge_types,
                        off_in_edge_types,
                    },
                });
            }
        }

        Ok(Self::from_owned_parts(
            decode_u32_le_vec(&mmap[off_row_ptr..end_rp], node_count + 1),
            decode_u32_le_vec(&mmap[off_targets..end_t], edge_count),
            mmap[off_edge_types..end_et].to_vec(),
            decode_u32_le_vec(&mmap[off_in_row_ptr..end_irp], node_count + 1),
            decode_u32_le_vec(&mmap[off_in_targets..end_it], edge_count),
            mmap[off_in_edge_types..end_iet].to_vec(),
        ))
    }

    /// Outgoing row-pointer slice of length `node_count + 1`.
    #[inline]
    pub fn row_ptr(&self) -> &[u32] {
        match &self.storage {
            CsrStorage::Owned { row_ptr, .. } => row_ptr,
            #[cfg(target_endian = "little")]
            CsrStorage::Mmap {
                mmap,
                node_count,
                off_row_ptr,
                ..
            } => {
                // SAFETY: alignment & bounds verified at construction in `from_mmap`.
                unsafe {
                    std::slice::from_raw_parts(
                        mmap.as_ptr().add(*off_row_ptr) as *const u32,
                        *node_count + 1,
                    )
                }
            }
        }
    }

    /// Incoming row-pointer slice of length `node_count + 1`.
    #[inline]
    pub fn in_row_ptr(&self) -> &[u32] {
        match &self.storage {
            CsrStorage::Owned { in_row_ptr, .. } => in_row_ptr,
            #[cfg(target_endian = "little")]
            CsrStorage::Mmap {
                mmap,
                node_count,
                off_in_row_ptr,
                ..
            } => {
                // SAFETY: alignment & bounds verified at construction in `from_mmap`.
                unsafe {
                    std::slice::from_raw_parts(
                        mmap.as_ptr().add(*off_in_row_ptr) as *const u32,
                        *node_count + 1,
                    )
                }
            }
        }
    }

    /// Flattened outgoing target indices of length `edge_count`.
    #[inline]
    pub fn targets(&self) -> &[u32] {
        match &self.storage {
            CsrStorage::Owned { targets, .. } => targets,
            #[cfg(target_endian = "little")]
            CsrStorage::Mmap {
                mmap,
                edge_count,
                off_targets,
                ..
            } => {
                // SAFETY: alignment & bounds verified at construction in `from_mmap`.
                unsafe {
                    std::slice::from_raw_parts(
                        mmap.as_ptr().add(*off_targets) as *const u32,
                        *edge_count,
                    )
                }
            }
        }
    }

    /// Flattened incoming source indices of length `edge_count`.
    #[inline]
    pub fn in_targets(&self) -> &[u32] {
        match &self.storage {
            CsrStorage::Owned { in_targets, .. } => in_targets,
            #[cfg(target_endian = "little")]
            CsrStorage::Mmap {
                mmap,
                edge_count,
                off_in_targets,
                ..
            } => {
                // SAFETY: alignment & bounds verified at construction in `from_mmap`.
                unsafe {
                    std::slice::from_raw_parts(
                        mmap.as_ptr().add(*off_in_targets) as *const u32,
                        *edge_count,
                    )
                }
            }
        }
    }

    /// Flattened outgoing edge-type codes parallel to [`Self::targets`].
    #[inline]
    pub fn edge_types(&self) -> &[u8] {
        match &self.storage {
            CsrStorage::Owned { edge_types, .. } => edge_types,
            #[cfg(target_endian = "little")]
            CsrStorage::Mmap {
                mmap,
                edge_count,
                off_edge_types,
                ..
            } => &mmap[*off_edge_types..*off_edge_types + *edge_count],
        }
    }

    /// Flattened incoming edge-type codes parallel to [`Self::in_targets`].
    #[inline]
    pub fn in_edge_types(&self) -> &[u8] {
        match &self.storage {
            CsrStorage::Owned { in_edge_types, .. } => in_edge_types,
            #[cfg(target_endian = "little")]
            CsrStorage::Mmap {
                mmap,
                edge_count,
                off_in_edge_types,
                ..
            } => &mmap[*off_in_edge_types..*off_in_edge_types + *edge_count],
        }
    }

    /// Number of nodes (`row_ptr.len() - 1`).
    #[inline]
    pub fn node_count(&self) -> usize {
        self.row_ptr().len().saturating_sub(1)
    }

    /// Number of directed edges (outgoing).
    #[inline]
    pub fn edge_count(&self) -> usize {
        self.targets().len()
    }

    /// Outgoing `(target, type_code)` slice for node `u`.
    #[inline]
    pub fn out_neighbors(&self, u: u32) -> (&[u32], &[u8]) {
        let rp = self.row_ptr();
        let targets = self.targets();
        let types = self.edge_types();
        let u = u as usize;
        if u + 1 >= rp.len() {
            return (&[], &[]);
        }
        let start = (rp[u] as usize).min(targets.len());
        let end = (rp[u + 1] as usize).clamp(start, targets.len());
        (&targets[start..end], &types[start..end])
    }

    /// Incoming `(source, type_code)` slice for node `u`.
    #[inline]
    pub fn in_neighbors(&self, u: u32) -> (&[u32], &[u8]) {
        let irp = self.in_row_ptr();
        let targets = self.in_targets();
        let types = self.in_edge_types();
        let u = u as usize;
        if u + 1 >= irp.len() {
            return (&[], &[]);
        }
        let start = (irp[u] as usize).min(targets.len());
        let end = (irp[u + 1] as usize).clamp(start, targets.len());
        (&targets[start..end], &types[start..end])
    }

    /// Contiguous outgoing target slice for `u` restricted to `edge_type` (`O(log deg(u))`).
    #[inline]
    pub fn out_neighbors_of_type(&self, u: u32, edge_type: EdgeType) -> &[u32] {
        let code = edge_type_to_u8(edge_type);
        let (targets, types) = self.out_neighbors(u);
        if types.is_empty() {
            return &[];
        }
        let lo = types.partition_point(|&c| c < code);
        let hi = lo + types[lo..].partition_point(|&c| c == code);
        &targets[lo..hi]
    }

    /// Contiguous incoming source slice for `u` restricted to `edge_type` (`O(log deg(u))`).
    #[inline]
    pub fn in_neighbors_of_type(&self, u: u32, edge_type: EdgeType) -> &[u32] {
        let code = edge_type_to_u8(edge_type);
        let (targets, types) = self.in_neighbors(u);
        if types.is_empty() {
            return &[];
        }
        let lo = types.partition_point(|&c| c < code);
        let hi = lo + types[lo..].partition_point(|&c| c == code);
        &targets[lo..hi]
    }

    /// Iterate all directed edges as `(src, dst, EdgeType)`.
    pub fn for_each_edge<F>(&self, mut f: F) -> Result<()>
    where
        F: FnMut(u32, u32, EdgeType),
    {
        for src in 0..self.node_count() as u32 {
            let (targets, types) = self.out_neighbors(src);
            for (i, &dst) in targets.iter().enumerate() {
                f(src, dst, edge_type_from_u8(types[i])?);
            }
        }
        Ok(())
    }

    /// Build CSR from pre-indexed `(src, dst, type_code)` edges, sorting each vertex's
    /// outgoing and incoming slices by `(type_code, neighbor)`.
    pub fn from_indexed_edges(node_count: usize, edges: &[(u32, u32, u8)]) -> Self {
        let mut out_deg = vec![0u32; node_count];
        let mut in_deg = vec![0u32; node_count];
        let mut valid_count = 0usize;

        for &(src, dst, _) in edges {
            if (src as usize) < node_count && (dst as usize) < node_count {
                out_deg[src as usize] += 1;
                in_deg[dst as usize] += 1;
                valid_count += 1;
            }
        }

        let mut row_ptr = Vec::with_capacity(node_count + 1);
        let mut in_row_ptr = Vec::with_capacity(node_count + 1);
        row_ptr.push(0);
        in_row_ptr.push(0);
        for i in 0..node_count {
            row_ptr.push(row_ptr[i] + out_deg[i]);
            in_row_ptr.push(in_row_ptr[i] + in_deg[i]);
        }

        let mut targets = vec![0u32; valid_count];
        let mut edge_types = vec![0u8; valid_count];
        let mut in_targets = vec![0u32; valid_count];
        let mut in_edge_types = vec![0u8; valid_count];
        let mut out_cursor = row_ptr[..node_count].to_vec();
        let mut in_cursor = in_row_ptr[..node_count].to_vec();

        for &(src, dst, code) in edges {
            if (src as usize) >= node_count || (dst as usize) >= node_count {
                continue;
            }
            let o = out_cursor[src as usize] as usize;
            targets[o] = dst;
            edge_types[o] = code;
            out_cursor[src as usize] += 1;

            let i = in_cursor[dst as usize] as usize;
            in_targets[i] = src;
            in_edge_types[i] = code;
            in_cursor[dst as usize] += 1;
        }

        let mut scratch = Vec::new();
        for u in 0..node_count {
            let os = row_ptr[u] as usize;
            let oe = row_ptr[u + 1] as usize;
            sort_parallel_by_type_then_target(
                &mut targets[os..oe],
                &mut edge_types[os..oe],
                &mut scratch,
            );

            let is = in_row_ptr[u] as usize;
            let ie = in_row_ptr[u + 1] as usize;
            sort_parallel_by_type_then_target(
                &mut in_targets[is..ie],
                &mut in_edge_types[is..ie],
                &mut scratch,
            );
        }

        Self::from_owned_parts(
            row_ptr,
            targets,
            edge_types,
            in_row_ptr,
            in_targets,
            in_edge_types,
        )
    }

    /// Build CSR from pre-indexed `(src, dst, type_code, pad)` edges and return sparse
    /// `(out_edge_idx, pad)` pairs for edges with non-zero `_pad` metadata.
    pub(crate) fn from_indexed_edges_with_pads(
        node_count: usize,
        edges: &[(u32, u32, u8, [u8; 7])],
    ) -> (Self, Vec<(u32, [u8; 7])>) {
        let any_pad = edges.iter().any(|e| e.3 != [0u8; 7]);
        if !any_pad {
            let mut out_deg = vec![0u32; node_count];
            let mut in_deg = vec![0u32; node_count];
            let mut valid_count = 0usize;
            for &(src, dst, _, _) in edges {
                if (src as usize) < node_count && (dst as usize) < node_count {
                    out_deg[src as usize] += 1;
                    in_deg[dst as usize] += 1;
                    valid_count += 1;
                }
            }
            let mut row_ptr = Vec::with_capacity(node_count + 1);
            let mut in_row_ptr = Vec::with_capacity(node_count + 1);
            row_ptr.push(0);
            in_row_ptr.push(0);
            for i in 0..node_count {
                row_ptr.push(row_ptr[i] + out_deg[i]);
                in_row_ptr.push(in_row_ptr[i] + in_deg[i]);
            }
            let mut targets = vec![0u32; valid_count];
            let mut edge_types = vec![0u8; valid_count];
            let mut in_targets = vec![0u32; valid_count];
            let mut in_edge_types = vec![0u8; valid_count];
            let mut out_cursor = row_ptr[..node_count].to_vec();
            let mut in_cursor = in_row_ptr[..node_count].to_vec();
            for &(src, dst, code, _) in edges {
                if (src as usize) >= node_count || (dst as usize) >= node_count {
                    continue;
                }
                let o = out_cursor[src as usize] as usize;
                targets[o] = dst;
                edge_types[o] = code;
                out_cursor[src as usize] += 1;

                let i = in_cursor[dst as usize] as usize;
                in_targets[i] = src;
                in_edge_types[i] = code;
                in_cursor[dst as usize] += 1;
            }
            let mut scratch = Vec::new();
            for u in 0..node_count {
                let os = row_ptr[u] as usize;
                let oe = row_ptr[u + 1] as usize;
                sort_parallel_by_type_then_target(
                    &mut targets[os..oe],
                    &mut edge_types[os..oe],
                    &mut scratch,
                );
                let is = in_row_ptr[u] as usize;
                let ie = in_row_ptr[u + 1] as usize;
                sort_parallel_by_type_then_target(
                    &mut in_targets[is..ie],
                    &mut in_edge_types[is..ie],
                    &mut scratch,
                );
            }
            return (
                Self::from_owned_parts(
                    row_ptr,
                    targets,
                    edge_types,
                    in_row_ptr,
                    in_targets,
                    in_edge_types,
                ),
                Vec::new(),
            );
        }

        let mut out_deg = vec![0u32; node_count];
        let mut in_deg = vec![0u32; node_count];
        let mut valid_count = 0usize;
        for &(src, dst, _, _) in edges {
            if (src as usize) < node_count && (dst as usize) < node_count {
                out_deg[src as usize] += 1;
                in_deg[dst as usize] += 1;
                valid_count += 1;
            }
        }
        let mut row_ptr = Vec::with_capacity(node_count + 1);
        let mut in_row_ptr = Vec::with_capacity(node_count + 1);
        row_ptr.push(0);
        in_row_ptr.push(0);
        for i in 0..node_count {
            row_ptr.push(row_ptr[i] + out_deg[i]);
            in_row_ptr.push(in_row_ptr[i] + in_deg[i]);
        }
        let mut targets = vec![0u32; valid_count];
        let mut edge_types = vec![0u8; valid_count];
        let mut out_pads = vec![[0u8; 7]; valid_count];
        let mut in_targets = vec![0u32; valid_count];
        let mut in_edge_types = vec![0u8; valid_count];
        let mut out_cursor = row_ptr[..node_count].to_vec();
        let mut in_cursor = in_row_ptr[..node_count].to_vec();

        for &(src, dst, code, pad) in edges {
            if (src as usize) >= node_count || (dst as usize) >= node_count {
                continue;
            }
            let o = out_cursor[src as usize] as usize;
            targets[o] = dst;
            edge_types[o] = code;
            out_pads[o] = pad;
            out_cursor[src as usize] += 1;

            let i = in_cursor[dst as usize] as usize;
            in_targets[i] = src;
            in_edge_types[i] = code;
            in_cursor[dst as usize] += 1;
        }

        let mut pad_scratch = Vec::new();
        let mut in_scratch = Vec::new();
        for u in 0..node_count {
            let os = row_ptr[u] as usize;
            let oe = row_ptr[u + 1] as usize;
            sort_parallel_with_pad_by_type_then_target(
                &mut targets[os..oe],
                &mut edge_types[os..oe],
                &mut out_pads[os..oe],
                &mut pad_scratch,
            );
            let is = in_row_ptr[u] as usize;
            let ie = in_row_ptr[u + 1] as usize;
            sort_parallel_by_type_then_target(
                &mut in_targets[is..ie],
                &mut in_edge_types[is..ie],
                &mut in_scratch,
            );
        }

        let mut sparse_pads = Vec::new();
        for (idx, &pad) in out_pads.iter().enumerate() {
            if pad != [0u8; 7] {
                sparse_pads.push((idx as u32, pad));
            }
        }

        (
            Self::from_owned_parts(
                row_ptr,
                targets,
                edge_types,
                in_row_ptr,
                in_targets,
                in_edge_types,
            ),
            sparse_pads,
        )
    }

    /// Build CSR from dense node order and typed UUID edges.
    ///
    /// `uuid_to_index` must cover every endpoint; unknown endpoints are skipped.
    pub fn from_typed_edges(
        node_count: usize,
        edges: &[(Uuid, Uuid, EdgeType)],
        uuid_to_index: &HashMap<Uuid, u32>,
    ) -> Self {
        let mut resolved: Vec<(u32, u32, u8)> = Vec::with_capacity(edges.len());
        for &(from, to, ty) in edges {
            let Some(&src) = uuid_to_index.get(&from) else {
                continue;
            };
            let Some(&dst) = uuid_to_index.get(&to) else {
                continue;
            };
            resolved.push((src, dst, edge_type_to_u8(ty)));
        }
        Self::from_indexed_edges(node_count, &resolved)
    }

    /// Build CSR by streaming edges from a mmap snapshot store (no full edge `Vec`).
    ///
    /// On v5+ snapshots whose `uuid_to_index` matches column order, callers should
    /// prefer [`crate::columnar_snapshot::ColumnarGraphMmap::csr_cloned`] for O(1) zero-copy.
    pub fn from_store_topology(
        store: &crate::snapshot::SnapshotNodeStore,
        uuid_to_index: &HashMap<Uuid, u32>,
    ) -> Result<Self> {
        let node_count = store.node_count();
        let mut out_deg = vec![0u32; node_count];
        let mut in_deg = vec![0u32; node_count];
        let mut resolved_count = 0usize;

        store.for_each_edge(|from, to, _ty| {
            if let (Some(&src), Some(&dst)) = (uuid_to_index.get(&from), uuid_to_index.get(&to)) {
                out_deg[src as usize] += 1;
                in_deg[dst as usize] += 1;
                resolved_count += 1;
            }
            Ok(())
        })?;

        let mut row_ptr = Vec::with_capacity(node_count + 1);
        let mut in_row_ptr = Vec::with_capacity(node_count + 1);
        row_ptr.push(0);
        in_row_ptr.push(0);
        for i in 0..node_count {
            row_ptr.push(row_ptr[i] + out_deg[i]);
            in_row_ptr.push(in_row_ptr[i] + in_deg[i]);
        }

        let mut targets = vec![0u32; resolved_count];
        let mut edge_types = vec![0u8; resolved_count];
        let mut in_targets = vec![0u32; resolved_count];
        let mut in_edge_types = vec![0u8; resolved_count];
        let mut out_cursor = row_ptr[..node_count].to_vec();
        let mut in_cursor = in_row_ptr[..node_count].to_vec();

        store.for_each_edge(|from, to, ty| {
            let Some(&src) = uuid_to_index.get(&from) else {
                return Ok(());
            };
            let Some(&dst) = uuid_to_index.get(&to) else {
                return Ok(());
            };
            let code = edge_type_to_u8(ty);
            let o = out_cursor[src as usize] as usize;
            targets[o] = dst;
            edge_types[o] = code;
            out_cursor[src as usize] += 1;

            let i = in_cursor[dst as usize] as usize;
            in_targets[i] = src;
            in_edge_types[i] = code;
            in_cursor[dst as usize] += 1;
            Ok(())
        })?;

        let mut scratch = Vec::new();
        for u in 0..node_count {
            let os = row_ptr[u] as usize;
            let oe = row_ptr[u + 1] as usize;
            sort_parallel_by_type_then_target(
                &mut targets[os..oe],
                &mut edge_types[os..oe],
                &mut scratch,
            );
            let is = in_row_ptr[u] as usize;
            let ie = in_row_ptr[u + 1] as usize;
            sort_parallel_by_type_then_target(
                &mut in_targets[is..ie],
                &mut in_edge_types[is..ie],
                &mut scratch,
            );
        }

        Ok(Self::from_owned_parts(
            row_ptr,
            targets,
            edge_types,
            in_row_ptr,
            in_targets,
            in_edge_types,
        ))
    }

    /// Build CSR from a live backend (dense index order = `all_node_ids` encounter order).
    pub fn from_backend(backend: &MemoryBackend) -> Result<(Self, Vec<Uuid>, HashMap<Uuid, u32>)> {
        let node_ids = backend.all_node_ids()?;
        let node_count = node_ids.len();
        let mut uuid_to_index = HashMap::with_capacity(node_count);
        for (i, id) in node_ids.iter().enumerate() {
            uuid_to_index.insert(*id, i as u32);
        }
        let edges = backend.edge_topology_typed()?;
        let csr = Self::from_typed_edges(node_count, &edges, &uuid_to_index);
        Ok((csr, node_ids, uuid_to_index))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::GraphBackend;
    use crate::schema::{Edge, Node, NodeType};

    #[test]
    fn csr_bidirectional_calls() {
        let mut backend = MemoryBackend::new();
        let a = Node::new(NodeType::Function, "a");
        let b = Node::new(NodeType::Function, "b");
        let a_id = a.id;
        let b_id = b.id;
        backend.insert_node(a).unwrap();
        backend.insert_node(b).unwrap();
        backend
            .insert_edge(Edge::new(a_id, b_id, EdgeType::Calls))
            .unwrap();

        let (csr, ids, map) = CodeGraphCsr::from_backend(&backend).unwrap();
        assert_eq!(csr.node_count(), 2);
        assert_eq!(csr.edge_count(), 1);
        let ai = map[&a_id];
        let bi = map[&b_id];
        let (out_t, out_ty) = csr.out_neighbors(ai);
        assert_eq!(out_t, &[bi]);
        assert_eq!(out_ty, &[edge_type_to_u8(EdgeType::Calls)]);
        let (in_t, _) = csr.in_neighbors(bi);
        assert_eq!(in_t, &[ai]);
        assert_eq!(ids.len(), 2);
    }

    #[test]
    fn csr_store_topology_matches_typed_edges() {
        use crate::snapshot::SnapshotNodeStore;
        use tempfile::tempdir;

        let mut backend = MemoryBackend::new();
        let a = Node::new(NodeType::Function, "a");
        let b = Node::new(NodeType::Function, "b");
        let a_id = a.id;
        let b_id = b.id;
        backend.insert_node(a).unwrap();
        backend.insert_node(b).unwrap();
        backend
            .insert_edge(Edge::new(a_id, b_id, EdgeType::Calls))
            .unwrap();

        let dir = tempdir().unwrap();
        let path = dir.path().join("graph.snapshot.bin");
        crate::write_columnar_from_backend(&backend, &path).unwrap();
        let store = SnapshotNodeStore::open(&path).unwrap();
        let ids = store.all_node_ids();
        let mut uuid_to_index = HashMap::new();
        for (i, id) in ids.into_iter().enumerate() {
            uuid_to_index.insert(id, i as u32);
        }
        let edges = store.edge_topology_typed().unwrap();
        let legacy = CodeGraphCsr::from_typed_edges(store.node_count(), &edges, &uuid_to_index);
        let streamed = CodeGraphCsr::from_store_topology(&store, &uuid_to_index).unwrap();
        assert_eq!(legacy.node_count(), streamed.node_count());
        assert_eq!(legacy.edge_count(), streamed.edge_count());
        assert_eq!(legacy.targets(), streamed.targets());
        assert_eq!(legacy.edge_types(), streamed.edge_types());
    }

    #[test]
    fn csr_groups_and_sorts_slices_by_edge_type() {
        let edges = vec![
            (0u32, 3u32, edge_type_to_u8(EdgeType::References)),
            (0u32, 2u32, edge_type_to_u8(EdgeType::Calls)),
            (0u32, 1u32, edge_type_to_u8(EdgeType::Contains)),
            (0u32, 1u32, edge_type_to_u8(EdgeType::Calls)),
        ];
        let csr = CodeGraphCsr::from_indexed_edges(4, &edges);
        assert_eq!(csr.out_neighbors_of_type(0, EdgeType::Calls), &[1, 2]);
        assert_eq!(csr.out_neighbors_of_type(0, EdgeType::Contains), &[1]);
        assert_eq!(csr.out_neighbors_of_type(0, EdgeType::References), &[3]);
        assert!(csr.out_neighbors_of_type(0, EdgeType::Extends).is_empty());
        assert_eq!(csr.in_neighbors_of_type(1, EdgeType::Calls), &[0]);
        assert_eq!(csr.in_neighbors_of_type(1, EdgeType::Contains), &[0]);
    }
}
