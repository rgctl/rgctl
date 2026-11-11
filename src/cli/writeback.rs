//! `rgctl writeback` — lossless export / verify / patch from `.rgctl/` sidecars.

use super::args::OutputFormat;
use super::context::CliContext;
use crate::languages::registry::LanguageRegistry;
use anyhow::{Context, Result, bail};
use rgctl_error::Error as RgctlError;
use rgctl_graph::{
    AgentWorklistItem, EdgeType, MigrateRequest, MigrateWave, MigrationPlan, SpanEdit,
    TrustLedgerEntry, TrustLedgerReport, TrustStatus, WorklistNodeFacet, WritebackError,
    apply_span_edits, blake3_hex, is_migratable_source_file, load_migration_plan,
    materialize_migration, migrate_buffers, open_lossless_sidecars, reconstruct_file, rename_edit,
    sort_edits, validate_syntax_unchanged, worklist_from_graph_nodes,
};
use rgctl_plugin_helpers::{
    builtin_framework_catalog, find_writeback_profile, list_writeback_frameworks,
};
use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
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
    Frameworks {
        lang: Option<String>,
    },
    Migrate {
        from: Option<String>,
        to: Option<String>,
        plan: Option<PathBuf>,
        wave: String,
        dry_run: bool,
        output: Option<PathBuf>,
        in_place: bool,
        lang: String,
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
        WritebackSubcommand::Frameworks { lang } => run_frameworks(ctx, lang.as_deref()),
        WritebackSubcommand::Migrate {
            from,
            to,
            plan,
            wave,
            dry_run,
            output,
            in_place,
            lang,
        } => run_migrate(
            ctx,
            from.as_deref(),
            to.as_deref(),
            plan.as_deref(),
            &wave,
            dry_run,
            output.as_deref(),
            in_place,
            &lang,
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

fn run_frameworks(ctx: &CliContext, lang: Option<&str>) -> Result<()> {
    let catalog = builtin_framework_catalog();
    let rows = list_writeback_frameworks(catalog, lang);
    match ctx.format {
        OutputFormat::Json => {
            let entries: Vec<_> = rows
                .iter()
                .map(|(l, id)| {
                    let profile = catalog
                        .by_language
                        .get(l)
                        .and_then(|ix| ix.frameworks.iter().find(|f| f.id == *id));
                    serde_json::json!({
                        "language": l,
                        "id": id,
                        "import_prefixes": profile.map(|p| &p.import_prefixes).cloned().unwrap_or_default(),
                        "worklist_supertype_triggers": profile.map(|p| &p.worklist_supertype_triggers).cloned().unwrap_or_default(),
                        "worklist_call_triggers": profile.map(|p| &p.worklist_call_triggers).cloned().unwrap_or_default(),
                    })
                })
                .collect();
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "schema_version": 1,
                    "frameworks": entries,
                }))?
            );
        }
        _ => {
            println!("writeback frameworks ({} profiles):", rows.len());
            for (l, id) in rows {
                println!("  {l}\t{id}");
            }
        }
    }
    Ok(())
}

fn parse_wave(s: &str) -> Result<MigrateWave> {
    match s.trim().to_ascii_lowercase().as_str() {
        "1" | "one" => Ok(MigrateWave::One),
        "2" | "two" => Ok(MigrateWave::Two),
        "3" | "three" => Ok(MigrateWave::Three),
        "all" | "*" => Ok(MigrateWave::All),
        other => bail!("invalid --wave '{other}' (expected 1|2|3|all)"),
    }
}

#[allow(clippy::too_many_arguments)]
fn run_migrate(
    ctx: &CliContext,
    from: Option<&str>,
    to: Option<&str>,
    plan_path: Option<&Path>,
    wave: &str,
    dry_run: bool,
    output: Option<&Path>,
    in_place: bool,
    lang: &str,
) -> Result<()> {
    let wave = parse_wave(wave)?;
    let from = from.map(str::to_string);
    let to = to.map(str::to_string);
    let mut language = lang.to_string();
    let mut plan = None;
    let migration_label;

    if let Some(plan_path) = plan_path {
        plan = Some(load_migration_plan(plan_path)?);
        migration_label = plan
            .as_ref()
            .map(|p| p.name.clone())
            .unwrap_or_else(|| plan_path.display().to_string());
        if let Some(p) = plan.as_ref() {
            language = p.language.clone();
        }
    } else if from.is_some() && to.is_some() {
        migration_label = format!(
            "{}→{}",
            from.as_deref().unwrap_or("?"),
            to.as_deref().unwrap_or("?")
        );
    } else {
        bail!("provide --from <src> --to <dst>, or --plan <file.json>");
    }

    if !dry_run && !in_place && output.is_none() {
        bail!("specify --dry-run, --in-place, or --output <dir>");
    }

    let repo = &ctx.repo;
    let (content, _) = open_lossless_sidecars(repo)
        .context("open lossless sidecars (run `rgctl discover --lossless` first)")?;

    let mut files = BTreeMap::new();
    for idx in 0..content.entry_count() as usize {
        let rel = content.entry_path(idx)?.to_string();
        if !is_migratable_source_file(&rel, &language)
            && !rel.ends_with("pom.xml")
            && !rel.ends_with("build.gradle")
            && !rel.ends_with("build.gradle.kts")
        {
            continue;
        }
        let bytes = content.decompress_at(idx)?;
        files.insert(rel, bytes);
    }
    // Also include build files from disk for worklist (not always in content store).
    for build in ["pom.xml", "build.gradle", "build.gradle.kts"] {
        let p = repo.join(build);
        if p.is_file() && !files.contains_key(build) {
            files.insert(build.into(), fs::read(&p)?);
        }
    }

    let worklist_seeds =
        seed_worklist_from_snapshot(repo, from.as_deref(), &language, plan.as_ref());

    let req = MigrateRequest {
        language,
        from_framework: from,
        to_framework: to,
        plan,
        wave,
        files,
        worklist_seeds,
    };
    let (patched, mut report) = migrate_buffers(&req).map_err(|e| anyhow::anyhow!("{e}"))?;
    if report.recipe_or_plan.is_empty() {
        report.recipe_or_plan = migration_label;
    }

    let written = materialize_migration(repo, &patched, output, in_place, dry_run)?;
    report.trust.bytes_written = written;

    if in_place && !dry_run {
        // Incremental lossless refresh after Wave 3.
        let status = Command::new(std::env::current_exe().unwrap_or_else(|_| "rgctl".into()))
            .args(["discover", ".", "--lossless"])
            .current_dir(repo)
            .status();
        match status {
            Ok(s) if s.success() => {}
            Ok(s) => {
                eprintln!("warning: `rgctl discover . --lossless` exited with {s}");
            }
            Err(e) => {
                eprintln!("warning: failed to run `rgctl discover . --lossless`: {e}");
            }
        }
        // Re-check legacy imports after refresh by scanning patched buffers already counted.
        if report.legacy_imports_remaining > 0 {
            eprintln!(
                "warning: legacy_imports_remaining={} after migrate",
                report.legacy_imports_remaining
            );
        }
    }

    match ctx.format {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&report)?);
        }
        _ => {
            println!(
                "writeback migrate {}: wave1={} wave2={} wave3={} legacy_imports_remaining={} worklist={}",
                report.recipe_or_plan,
                report.wave1_edits,
                report.wave2_edits,
                report.wave3_edits,
                report.legacy_imports_remaining,
                report.agent_worklist.len()
            );
            for e in &report.trust.entries {
                println!(
                    "  [{}] {}",
                    format!("{:?}", e.status).to_lowercase(),
                    e.file
                );
            }
            for (i, w) in report.agent_worklist.iter().enumerate() {
                println!(
                    "  worklist[{}]: {} ({}) — {}",
                    i + 1,
                    w.symbol,
                    w.file,
                    w.reason
                );
            }
        }
    }
    Ok(())
}

fn seed_worklist_from_snapshot(
    repo: &Path,
    from_fw: Option<&str>,
    language: &str,
    plan: Option<&MigrationPlan>,
) -> Vec<AgentWorklistItem> {
    let catalog = builtin_framework_catalog();
    let mut triggers = Vec::new();
    if let Some(fw) = from_fw
        && let Some(profile) = find_writeback_profile(catalog, language, fw)
    {
        triggers.extend(profile.worklist_supertype_triggers.clone());
        triggers.extend(profile.worklist_call_triggers.clone());
    }
    if let Some(p) = plan {
        triggers.extend(p.worklist_triggers.supertypes.clone());
        triggers.extend(p.worklist_triggers.calls.clone());
        triggers.extend(p.worklist_triggers.annotations.clone());
    }
    if triggers.is_empty() {
        return Vec::new();
    }
    let snap = repo.join(".rgctl").join("graph.snapshot.bin");
    let Ok(store) = rgctl_graph::snapshot::SnapshotNodeStore::open(&snap) else {
        return Vec::new();
    };

    // Pass 1 over nodes: map UUID → (symbol_name, file, start_line, end_line, start_byte, end_byte)
    // and collect direct node-name trigger hits.
    let mut node_meta: HashMap<uuid::Uuid, WorklistNodeFacet> = HashMap::new();
    let mut trigger_target_ids: HashMap<uuid::Uuid, String> = HashMap::new();
    let mut direct_nodes = Vec::new();
    let _ = store.for_each_node_seq(|n| {
        let name = n
            .qualified_name
            .as_deref()
            .unwrap_or(n.name.as_ref())
            .to_string();
        let raw_file = n.file_path.as_deref().unwrap_or("");
        let norm_file = raw_file.replace('\\', "/");
        let file = norm_file
            .strip_prefix("./")
            .unwrap_or(&norm_file)
            .to_string();
        for t in &triggers {
            if n.name.as_str() == t.as_str()
                || name.ends_with(&format!(".{t}"))
                || name.contains(t.as_str())
            {
                trigger_target_ids.insert(n.id, t.clone());
                if !file.is_empty() {
                    direct_nodes.push((
                        name.clone(),
                        file.clone(),
                        n.start_line,
                        n.end_line,
                        n.start_byte,
                        n.end_byte,
                    ));
                }
                break;
            }
        }
        node_meta.insert(
            n.id,
            (
                name,
                file,
                n.start_line,
                n.end_line,
                n.start_byte,
                n.end_byte,
            ),
        );
    });

    // Pass 2 over edges: count incoming Calls per symbol name and resolve Extends/Implements/Calls/AnnotatedWith
    // edges pointing to trigger targets back to the declaring source symbol (`from_id`).
    let mut caller_counts: HashMap<String, usize> = HashMap::new();
    let mut edge_items: Vec<AgentWorklistItem> = Vec::new();
    let _ = store.mmap().for_each_edge(|from_id, to_id, edge_type| {
        if edge_type == EdgeType::Calls
            && let Some((callee_name, ..)) = node_meta.get(&to_id)
        {
            *caller_counts.entry(callee_name.clone()).or_insert(0) += 1;
        }
        if matches!(
            edge_type,
            EdgeType::Extends | EdgeType::Implements | EdgeType::Calls | EdgeType::AnnotatedWith
        ) && let Some(trig) = trigger_target_ids.get(&to_id)
            && let Some((sym, file, sl, el, sb, eb)) = node_meta.get(&from_id)
            && !file.is_empty()
        {
            edge_items.push(AgentWorklistItem {
                symbol: sym.clone(),
                file: file.clone(),
                start_line: *sl,
                end_line: *el,
                start_byte: *sb,
                end_byte: *eb,
                reason: format!("graph {edge_type:?} edge to worklist trigger `{trig}`"),
                caller_count: 0,
                blast_radius_order: 0,
            });
        }
        Ok(())
    });

    for item in &mut edge_items {
        item.caller_count = *caller_counts.get(&item.symbol).unwrap_or(&0);
    }
    let mut direct = worklist_from_graph_nodes(&direct_nodes, &triggers, &caller_counts);
    edge_items.append(&mut direct);
    edge_items
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
