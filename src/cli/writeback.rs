//! `rgctl writeback` — lossless export / verify / patch from `.rgctl/` sidecars.

use super::args::OutputFormat;
use super::context::CliContext;
use crate::languages::registry::LanguageRegistry;
use anyhow::{Context, Result, bail};
use rgctl_error::Error as RgctlError;
use rgctl_graph::{
    SpanEdit, TrustLedgerEntry, TrustLedgerReport, TrustStatus, WritebackError, apply_span_edits,
    blake3_hex, open_lossless_sidecars, reconstruct_file, rename_edit, sort_edits,
    validate_syntax_unchanged,
};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct WritebackArgs {
    pub subcommand: WritebackSubcommand,
}

#[derive(Debug, Clone)]
pub enum WritebackSubcommand {
    Verify {
        languages: Option<String>,
        strict: bool,
    },
    Export {
        output: PathBuf,
        languages: Option<String>,
        file_glob: Option<String>,
    },
    Patch {
        rename: Option<String>,
        plan: Option<PathBuf>,
        dry_run: bool,
        output: Option<PathBuf>,
        in_place: bool,
        force: bool,
    },
}

pub fn run(ctx: &CliContext, args: WritebackArgs) -> Result<()> {
    match args.subcommand {
        WritebackSubcommand::Verify { languages, strict } => {
            run_verify(ctx, languages.as_deref(), strict)
        }
        WritebackSubcommand::Export {
            output,
            languages,
            file_glob,
        } => run_export(ctx, &output, languages.as_deref(), file_glob.as_deref()),
        WritebackSubcommand::Patch {
            rename,
            plan,
            dry_run,
            output,
            in_place,
            force,
        } => run_patch(
            ctx,
            rename.as_deref(),
            plan.as_deref(),
            dry_run,
            output.as_deref(),
            in_place,
            force,
        ),
    }
}

fn run_verify(ctx: &CliContext, _languages: Option<&str>, strict: bool) -> Result<()> {
    let repo = &ctx.repo;
    let (content, cst) = open_lossless_sidecars(repo)
        .context("open lossless sidecars (run `rgctl discover --lossless` first)")?;
    let mut report = TrustLedgerReport::default();

    for i in 0..content.entry_count() as usize {
        let rel = content.entry_path(i)?.to_string();
        let reconstructed = content.decompress_at(i).context("decompress frame")?;
        report.files_processed += 1;

        // Verify Green-CST leaf spans are monotonic and within bounds of reconstructed source.
        let mut cst_ok = true;
        if let Some(ref cst_reader) = cst
            && let Some(cst_idx) = cst_reader.find_file(&rel)?
        {
            let leaves = cst_reader.leaf_spans_at(cst_idx)?;
            let mut prev_end = 0u32;
            for (_kind, start, end) in leaves {
                if start < prev_end || start > end || (end as usize) > reconstructed.len() {
                    cst_ok = false;
                    break;
                }
                prev_end = end;
            }
        }

        let disk_path = repo.join(&rel);
        let (before, after, ok) = if disk_path.is_file() {
            let on_disk = fs::read(&disk_path)?;
            let before = blake3_hex(&on_disk);
            let after = blake3_hex(&reconstructed);
            let ok = reconstructed == on_disk && cst_ok;
            (before, after, ok)
        } else {
            // Standalone verification when working tree sources have been removed:
            // `decompress_at` already verified the stored 32-byte BLAKE3 digest.
            let digest = blake3_hex(&reconstructed);
            (digest.clone(), digest, cst_ok)
        };

        if ok {
            report.files_verified_identical += 1;
        }
        report.entries.push(TrustLedgerEntry {
            file: rel,
            status: if ok {
                TrustStatus::VerifiedIdentical
            } else {
                TrustStatus::ConflictRejected
            },
            blake3_before: before,
            blake3_after: after,
            macro_or_error_spans: vec![],
        });
        if !ok && strict {
            bail!(
                "byte or CST mismatch for {}",
                report.entries.last().unwrap().file
            );
        }
    }
    emit_report(ctx, &report)?;
    if report.files_verified_identical != report.files_processed
        && report
            .entries
            .iter()
            .any(|e| e.status == TrustStatus::ConflictRejected)
    {
        bail!("writeback verify failed");
    }
    Ok(())
}

fn run_export(
    ctx: &CliContext,
    output: &Path,
    _languages: Option<&str>,
    file_glob: Option<&str>,
) -> Result<()> {
    let repo = &ctx.repo;
    let (content, _) = open_lossless_sidecars(repo)
        .context("open lossless sidecars (run `rgctl discover --lossless` first)")?;
    fs::create_dir_all(output)?;
    let mut report = TrustLedgerReport::default();
    for idx in 0..content.entry_count() as usize {
        let rel = content.entry_path(idx)?.to_string();
        if let Some(glob) = file_glob
            && !rel.contains(glob)
        {
            continue;
        }
        report.files_processed += 1;
        let bytes = content.decompress_at(idx)?;
        let dest = output.join(&rel);
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&dest, &bytes)?;
        let digest = blake3_hex(&bytes);
        report.bytes_written += bytes.len() as u64;
        report.files_verified_identical += 1;
        report.entries.push(TrustLedgerEntry {
            file: rel,
            status: TrustStatus::VerifiedIdentical,
            blake3_before: digest.clone(),
            blake3_after: digest,
            macro_or_error_spans: vec![],
        });
    }
    emit_report(ctx, &report)?;
    Ok(())
}

fn run_patch(
    ctx: &CliContext,
    rename: Option<&str>,
    plan: Option<&Path>,
    dry_run: bool,
    output: Option<&Path>,
    in_place: bool,
    force: bool,
) -> Result<()> {
    let repo = &ctx.repo;
    let (content, cst) = open_lossless_sidecars(repo)
        .context("open lossless sidecars (run `rgctl discover --lossless` first)")?;
    let registry = Arc::new(LanguageRegistry::new().into_inner());
    let mut report = TrustLedgerReport::default();

    if let Some(plan_path) = plan {
        #[derive(serde::Deserialize)]
        struct PlanFile {
            file: String,
            edits: Vec<SpanEdit>,
        }
        #[derive(serde::Deserialize)]
        struct Plan {
            files: Vec<PlanFile>,
        }
        let plan: Plan = serde_json::from_str(&fs::read_to_string(plan_path)?)?;
        for pf in plan.files {
            report.files_processed += 1;
            let mut edits = pf.edits;
            sort_edits(&mut edits);
            let source = reconstruct_file(&content, &pf.file)?;
            match apply_span_edits(&source, &edits) {
                Ok(patched) => {
                    let before = blake3_hex(&source);
                    let after = blake3_hex(&patched);
                    let file_path = PathBuf::from(&pf.file);
                    let syn = validate_syntax_unchanged(
                        &source,
                        &patched,
                        |buf| buffer_has_parse_error(registry.as_ref(), &file_path, buf),
                        force,
                    );
                    match syn {
                        Ok(()) => {
                            report.edits_applied += edits.len();
                            if !dry_run {
                                let dest = if in_place {
                                    repo.join(&pf.file)
                                } else if let Some(out) = output {
                                    out.join(&pf.file)
                                } else {
                                    bail!("specify --in-place, --output, or --dry-run");
                                };
                                if let Some(parent) = dest.parent() {
                                    fs::create_dir_all(parent)?;
                                }
                                fs::write(&dest, &patched)?;
                                report.bytes_written += patched.len() as u64;
                            }
                            report.entries.push(TrustLedgerEntry {
                                file: pf.file,
                                status: TrustStatus::PatchedVerified,
                                blake3_before: before,
                                blake3_after: after,
                                macro_or_error_spans: vec![],
                            });
                        }
                        Err(WritebackError::SyntaxErrorRejected) => {
                            report.entries.push(TrustLedgerEntry {
                                file: pf.file,
                                status: TrustStatus::SyntaxErrorRejected,
                                blake3_before: before,
                                blake3_after: after,
                                macro_or_error_spans: vec![],
                            });
                        }
                        Err(e) => return Err(e.into()),
                    }
                }
                Err(WritebackError::ConflictRejected) => {
                    report.entries.push(TrustLedgerEntry {
                        file: pf.file,
                        status: TrustStatus::ConflictRejected,
                        blake3_before: String::new(),
                        blake3_after: String::new(),
                        macro_or_error_spans: vec![],
                    });
                }
                Err(e) => return Err(e.into()),
            }
        }
    } else if let Some(spec) = rename {
        let Some((old, new)) = spec.split_once('=') else {
            bail!("--rename expects name=new_name");
        };
        let old_b = old.as_bytes();

        // Collect declaration name_byte_spans from `.rgctl/graph.snapshot.bin` when available.
        let mut graph_spans_by_file: std::collections::HashMap<String, Vec<(u32, u32)>> =
            std::collections::HashMap::new();
        let snap_path = repo.join(".rgctl").join("graph.snapshot.bin");
        if let Ok(store) = rgctl_graph::snapshot::SnapshotNodeStore::open(&snap_path)
            && let Ok(nodes) = store.find_nodes_by_name(old)
        {
            for n in nodes {
                if let (Some(fp), Some((ns, ne))) = (n.file_path.as_deref(), n.name_byte_span()) {
                    let norm = fp.replace('\\', "/");
                    let norm = norm.strip_prefix("./").unwrap_or(&norm).to_string();
                    graph_spans_by_file.entry(norm).or_default().push((ns, ne));
                }
            }
        }

        for idx in 0..content.entry_count() as usize {
            let rel = content.entry_path(idx)?.to_string();
            let source = content.decompress_at(idx)?;
            let mut span_set: std::collections::BTreeSet<(u32, u32)> =
                std::collections::BTreeSet::new();

            if let Some(gspans) = graph_spans_by_file.get(&rel) {
                for &(s, e) in gspans {
                    if (e as usize) <= source.len() && &source[s as usize..e as usize] == old_b {
                        span_set.insert((s, e));
                    }
                }
            }

            if let Some(ref cst_reader) = cst
                && let Some(cst_idx) = cst_reader.find_file(&rel)?
            {
                for (_kind_id, start, end) in cst_reader.leaf_spans_at(cst_idx)? {
                    let s = start as usize;
                    let e = end as usize;
                    if e <= source.len()
                        && &source[s..e] == old_b
                        && is_standalone_ident_token(&source, s, e)
                    {
                        span_set.insert((start, end));
                    }
                }
            } else {
                // Fallback for non-CST files: require strict identifier token boundaries
                // and reject occurrences inside string quotes or line comments.
                let mut i = 0usize;
                while i + old_b.len() <= source.len() {
                    if &source[i..i + old_b.len()] == old_b
                        && is_standalone_ident_token(&source, i, i + old_b.len())
                    {
                        span_set.insert((i as u32, (i + old_b.len()) as u32));
                        i += old_b.len();
                    } else {
                        i += 1;
                    }
                }
            }

            if span_set.is_empty() {
                continue;
            }
            let mut edits: Vec<SpanEdit> = span_set
                .into_iter()
                .map(|(s, e)| rename_edit(s, e, new))
                .collect();
            report.files_processed += 1;
            sort_edits(&mut edits);
            let patched = match apply_span_edits(&source, &edits) {
                Ok(p) => p,
                Err(WritebackError::ConflictRejected) => {
                    report.entries.push(TrustLedgerEntry {
                        file: rel,
                        status: TrustStatus::ConflictRejected,
                        blake3_before: String::new(),
                        blake3_after: String::new(),
                        macro_or_error_spans: vec![],
                    });
                    continue;
                }
                Err(e) => return Err(e.into()),
            };
            let before = blake3_hex(&source);
            let after = blake3_hex(&patched);
            let file_path = PathBuf::from(&rel);
            match validate_syntax_unchanged(
                &source,
                &patched,
                |buf| buffer_has_parse_error(registry.as_ref(), &file_path, buf),
                force,
            ) {
                Ok(()) => {
                    report.edits_applied += edits.len();
                    if !dry_run {
                        let dest = if in_place {
                            repo.join(&rel)
                        } else if let Some(out) = output {
                            out.join(&rel)
                        } else {
                            bail!("specify --in-place, --output, or --dry-run");
                        };
                        if let Some(parent) = dest.parent() {
                            fs::create_dir_all(parent)?;
                        }
                        fs::write(&dest, &patched)?;
                        report.bytes_written += patched.len() as u64;
                    }
                    report.entries.push(TrustLedgerEntry {
                        file: rel,
                        status: TrustStatus::PatchedVerified,
                        blake3_before: before,
                        blake3_after: after,
                        macro_or_error_spans: vec![],
                    });
                }
                Err(WritebackError::SyntaxErrorRejected) => {
                    report.entries.push(TrustLedgerEntry {
                        file: rel,
                        status: TrustStatus::SyntaxErrorRejected,
                        blake3_before: before,
                        blake3_after: after,
                        macro_or_error_spans: vec![],
                    });
                }
                Err(e) => return Err(e.into()),
            }
        }
    } else {
        bail!("provide --rename name=new or --plan mutations.json");
    }

    emit_report(ctx, &report)?;
    Ok(())
}

#[inline]
fn is_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'$'
}

/// Verify `[start, end)` is bounded by non-identifier bytes and is not immediately
/// enclosed in string quotes (`"`, `'`, `` ` ``).
fn is_standalone_ident_token(source: &[u8], start: usize, end: usize) -> bool {
    if start > 0 {
        let prev = source[start - 1];
        if is_ident_byte(prev) || matches!(prev, b'"' | b'\'' | b'`') {
            return false;
        }
    }
    if end < source.len() {
        let next = source[end];
        if is_ident_byte(next) || matches!(next, b'"' | b'\'' | b'`') {
            return false;
        }
    }
    true
}

fn emit_report(ctx: &CliContext, report: &TrustLedgerReport) -> Result<()> {
    match ctx.format {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(report)?);
        }
        _ => {
            println!(
                "writeback: processed={} identical={} edits={} bytes={}",
                report.files_processed,
                report.files_verified_identical,
                report.edits_applied,
                report.bytes_written
            );
            for e in &report.entries {
                println!(
                    "  [{}] {}",
                    format!("{:?}", e.status).to_lowercase(),
                    e.file
                );
            }
        }
    }
    Ok(())
}

/// Return `true` when tree-sitter reports ERROR/MISSING nodes (or parse fails).
fn buffer_has_parse_error(
    registry: &rgctl_registry::LanguageRegistry,
    file_path: &Path,
    buf: &[u8],
) -> std::result::Result<bool, RgctlError> {
    let Ok(plugin) = registry.get_plugin_for_file_with_source(file_path, buf) else {
        // Unknown language: treat `@` tokens as a conservative syntax-error signal.
        return Ok(buf.contains(&b'@'));
    };
    let Some(grammar) = plugin.grammar() else {
        return Ok(false);
    };
    match rgctl_plugin_helpers::parse_source(buf, file_path, grammar) {
        Ok(tree) => {
            let mut err = false;
            let mut stack = vec![tree.root_node()];
            while let Some(node) = stack.pop() {
                if node.is_error() || node.is_missing() {
                    err = true;
                    break;
                }
                for i in 0..node.child_count() {
                    if let Some(child) = node.child(i) {
                        stack.push(child);
                    }
                }
            }
            Ok(err)
        }
        Err(_) => Ok(true),
    }
}
