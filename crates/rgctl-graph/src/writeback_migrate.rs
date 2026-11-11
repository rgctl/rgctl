//! Two-layer framework migration write-back (catalog pivot + LLM plan) with 3-wave splicing.

#![allow(missing_docs)]

use crate::writeback::{
    EditReason, SpanEdit, TrustLedgerEntry, TrustLedgerReport, TrustStatus, WritebackError,
    apply_span_edits, blake3_hex, sort_edits, validate_syntax_unchanged,
};
use rgctl_error::{Error, Result};
use rgctl_plugin_helpers::{
    AnnotationTemplate, FrameworkWritebackProfile, FrameworkWritebackTemplates,
    builtin_framework_catalog, find_writeback_profile, list_writeback_frameworks, parse_source,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};

/// Which structural waves to run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MigrateWave {
    One,
    Two,
    Three,
    All,
}

impl MigrateWave {
    pub fn includes(self, wave: u8) -> bool {
        match self {
            Self::All => true,
            Self::One => wave == 1,
            Self::Two => wave == 2,
            Self::Three => wave == 3,
        }
    }
}

/// Layer-2 declarative migration plan (`--plan`).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MigrationPlan {
    #[serde(default)]
    pub name: String,
    #[serde(default = "default_java")]
    pub language: String,
    #[serde(default)]
    pub namespace_rewrites: BTreeMap<String, String>,
    #[serde(default)]
    pub import_rewrites: BTreeMap<String, String>,
    #[serde(default)]
    pub annotation_rewrites: Vec<PlanAnnotationRewrite>,
    #[serde(default)]
    pub worklist_triggers: PlanWorklistTriggers,
}

fn default_java() -> String {
    "java".into()
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PlanAnnotationRewrite {
    pub from: String,
    pub to: String,
    #[serde(default)]
    pub imports: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PlanWorklistTriggers {
    #[serde(default)]
    pub annotations: Vec<String>,
    #[serde(default)]
    pub supertypes: Vec<String>,
    #[serde(default)]
    pub calls: Vec<String>,
}

/// One agent worklist item (Tier D / non-template).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentWorklistItem {
    pub symbol: String,
    pub file: String,
    pub start_line: Option<usize>,
    pub end_line: Option<usize>,
    pub start_byte: Option<u32>,
    pub end_byte: Option<u32>,
    pub reason: String,
    pub caller_count: usize,
    pub blast_radius_order: usize,
}

/// Aggregate migrate report (extends Trust Ledger fields).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MigrationWritebackReport {
    pub recipe_or_plan: String,
    pub from_framework: Option<String>,
    pub to_framework: Option<String>,
    pub wave1_edits: usize,
    pub wave2_edits: usize,
    pub wave3_edits: usize,
    pub legacy_imports_remaining: usize,
    pub trust: TrustLedgerReport,
    pub agent_worklist: Vec<AgentWorklistItem>,
}

/// Inputs for a catalog (`--from/--to`) or plan migration over in-memory buffers.
#[derive(Debug, Clone)]
pub struct MigrateRequest {
    pub language: String,
    pub from_framework: Option<String>,
    pub to_framework: Option<String>,
    pub plan: Option<MigrationPlan>,
    pub wave: MigrateWave,
    /// Repo-relative path → source bytes.
    pub files: BTreeMap<String, Vec<u8>>,
    /// Optional graph-derived worklist seeds: (symbol, file, start_line, end_line, start_byte, end_byte, reason).
    pub worklist_seeds: Vec<AgentWorklistItem>,
}

/// Apply 3-wave migration to in-memory file buffers; returns patched bytes + report.
pub fn migrate_buffers(
    req: &MigrateRequest,
) -> std::result::Result<(BTreeMap<String, Vec<u8>>, MigrationWritebackReport), WritebackError> {
    let catalog = builtin_framework_catalog();
    let src_profile = req
        .from_framework
        .as_deref()
        .and_then(|id| find_writeback_profile(catalog, &req.language, id));
    let dst_profile = req
        .to_framework
        .as_deref()
        .and_then(|id| find_writeback_profile(catalog, &req.language, id));
    let plan = req.plan.as_ref();
    if plan.is_none() {
        let valid_ids: Vec<String> = list_writeback_frameworks(catalog, Some(&req.language))
            .into_iter()
            .map(|(_, id)| id)
            .collect();
        let valid_list = if valid_ids.is_empty() {
            "none".to_string()
        } else {
            valid_ids.join(", ")
        };
        if src_profile.is_none() {
            return Err(WritebackError::Other(Error::SerdeError(format!(
                "unknown source framework '{}' for language '{}'. Valid catalog frameworks for {}: [{}]. Run `rgctl -f json writeback frameworks --lang {}` to inspect catalog profiles, or use `--plan <migration-plan.json>` for custom/uncataloged frameworks.",
                req.from_framework.as_deref().unwrap_or(""),
                req.language,
                req.language,
                valid_list,
                req.language
            ))));
        }
        if dst_profile.is_none() {
            return Err(WritebackError::Other(Error::SerdeError(format!(
                "unknown target framework '{}' for language '{}'. Valid catalog frameworks for {}: [{}]. Run `rgctl -f json writeback frameworks --lang {}` to inspect catalog profiles, or use `--plan <migration-plan.json>` for custom/uncataloged frameworks.",
                req.to_framework.as_deref().unwrap_or(""),
                req.language,
                req.language,
                valid_list,
                req.language
            ))));
        }
    }

    let mut report = MigrationWritebackReport {
        recipe_or_plan: req
            .plan
            .as_ref()
            .map(|p| p.name.clone())
            .unwrap_or_else(|| {
                format!(
                    "{}→{}",
                    src_profile
                        .map(|p| p.id.as_str())
                        .or(req.from_framework.as_deref())
                        .unwrap_or("?"),
                    dst_profile
                        .map(|p| p.id.as_str())
                        .or(req.to_framework.as_deref())
                        .unwrap_or("?")
                )
            }),
        from_framework: src_profile
            .map(|p| p.id.clone())
            .or_else(|| req.from_framework.clone()),
        to_framework: dst_profile
            .map(|p| p.id.clone())
            .or_else(|| req.to_framework.clone()),
        ..Default::default()
    };

    let mut out_files = BTreeMap::new();

    for (path, source) in &req.files {
        if !is_migratable_source_file(path, &req.language) {
            out_files.insert(path.clone(), source.clone());
            continue;
        }
        report.trust.files_processed += 1;
        let mut buf = source.clone();
        let before = blake3_hex(&buf);
        let mut file_edits = 0usize;
        let mut file_needed: BTreeSet<String> = BTreeSet::new();

        // Wave 1 — imports & namespace prefixes
        if req.wave.includes(1) {
            let (patched, edits, imports) = wave1_imports(&buf, src_profile, dst_profile, plan)?;
            report.wave1_edits += edits;
            file_edits += edits;
            file_needed.extend(imports);
            buf = patched;
        }

        // Wave 2 — class / field / method annotations
        if req.wave.includes(2) {
            let (patched, edits, imports) =
                wave2_annotations(&buf, src_profile, dst_profile, plan)?;
            report.wave2_edits += edits;
            file_edits += edits;
            file_needed.extend(imports);
            buf = patched;
        }

        // Wave 3 — parameter annotations
        if req.wave.includes(3) {
            let (patched, edits, imports) =
                wave3_param_annotations(&buf, src_profile, dst_profile, plan)?;
            report.wave3_edits += edits;
            file_edits += edits;
            file_needed.extend(imports);
            buf = patched;
        }

        // Insert any still-missing target imports (post-wave, sorted).
        if !file_needed.is_empty() {
            buf = ensure_imports(&buf, &file_needed);
        }

        let after = blake3_hex(&buf);
        let is_java = path.ends_with(".java");
        let syn = validate_syntax_unchanged(
            source,
            &buf,
            |b| Ok(is_java && java_buffer_has_error(b)),
            false,
        );
        match syn {
            Ok(()) => {
                report.trust.edits_applied += file_edits;
                report.trust.bytes_written += buf.len() as u64;
                report.trust.entries.push(TrustLedgerEntry {
                    file: path.clone(),
                    status: TrustStatus::PatchedVerified,
                    blake3_before: before,
                    blake3_after: after,
                    macro_or_error_spans: vec![],
                });
                out_files.insert(path.clone(), buf);
            }
            Err(WritebackError::SyntaxErrorRejected) => {
                report.trust.entries.push(TrustLedgerEntry {
                    file: path.clone(),
                    status: TrustStatus::SyntaxErrorRejected,
                    blake3_before: before,
                    blake3_after: after,
                    macro_or_error_spans: vec![],
                });
                out_files.insert(path.clone(), source.clone());
            }
            Err(e) => return Err(e),
        }
    }

    // Count remaining legacy import prefixes from source profile.
    if let Some(src) = src_profile.as_ref() {
        for (path, bytes) in &out_files {
            if !is_migratable_source_file(path, &req.language) {
                continue;
            }
            let text = String::from_utf8_lossy(bytes);
            for line in text.lines() {
                let t = line.trim();
                if t.starts_with("import ")
                    && src
                        .import_prefixes
                        .iter()
                        .any(|prefix| t.contains(prefix.as_str()))
                {
                    report.legacy_imports_remaining += 1;
                }
            }
        }
    }

    report.agent_worklist = req.worklist_seeds.clone();
    for fallback in generate_worklist_from_buffers(&req.files, src_profile, plan) {
        let already_present = report.agent_worklist.iter().any(|w| {
            w.file == fallback.file
                && (w.symbol == fallback.symbol
                    || w.reason == fallback.reason
                    || (w.start_line.is_some() && w.start_line == fallback.start_line))
        });
        if !already_present {
            report.agent_worklist.push(fallback);
        }
    }
    // Assign blast_radius_order (sorted by caller_count desc, then symbol, then file).
    report.agent_worklist.sort_by(|a, b| {
        b.caller_count
            .cmp(&a.caller_count)
            .then(a.symbol.cmp(&b.symbol))
            .then(a.file.cmp(&b.file))
    });
    for (i, item) in report.agent_worklist.iter_mut().enumerate() {
        item.blast_radius_order = i + 1;
    }

    Ok((out_files, report))
}

/// Check whether `path` is a source file for `language`.
pub fn is_migratable_source_file(path: &str, language: &str) -> bool {
    match language {
        "java" => path.ends_with(".java"),
        "kotlin" => path.ends_with(".kt") || path.ends_with(".kts"),
        "python" => path.ends_with(".py"),
        "typescript" => path.ends_with(".ts") || path.ends_with(".tsx"),
        "javascript" => path.ends_with(".js") || path.ends_with(".jsx") || path.ends_with(".mjs"),
        "rust" => path.ends_with(".rs"),
        "csharp" => path.ends_with(".cs"),
        "go" => path.ends_with(".go"),
        "php" => path.ends_with(".php"),
        "ruby" => path.ends_with(".rb"),
        _ => path.ends_with(".java"),
    }
}

fn wave1_imports(
    source: &[u8],
    src: Option<&FrameworkWritebackProfile>,
    dst: Option<&FrameworkWritebackProfile>,
    plan: Option<&MigrationPlan>,
) -> std::result::Result<(Vec<u8>, usize, BTreeSet<String>), WritebackError> {
    let text = String::from_utf8_lossy(source);
    let mut edits: Vec<SpanEdit> = Vec::new();
    let needed: BTreeSet<String> = BTreeSet::new();

    let mut rewrites: BTreeMap<String, String> = BTreeMap::new();
    if let Some(plan) = plan {
        rewrites.extend(plan.namespace_rewrites.clone());
        rewrites.extend(plan.import_rewrites.clone());
    }
    if let Some(s) = src {
        rewrites.extend(s.namespace_rewrites.clone());
    }

    // Collect legacy annotation imports that should be stripped when switching frameworks
    // (`src.id != dst.id`), since Wave 2 / Wave 3 will add the exact target imports needed.
    let mut strip_exact_imports: BTreeSet<String> = BTreeSet::new();
    let mut strip_import_prefixes: Vec<&'static str> = Vec::new();
    if let (Some(s), Some(d)) = (src, dst)
        && s.id != d.id
    {
        collect_template_imports(&s.writeback, &mut strip_exact_imports);
        if s.id == "spring" {
            strip_import_prefixes.extend([
                "org.springframework.web.bind.annotation.",
                "org.springframework.beans.factory.annotation.",
                "org.springframework.stereotype.",
                "org.springframework.transaction.annotation.",
            ]);
        } else if matches!(s.id.as_str(), "javaee" | "openliberty")
            && matches!(d.id.as_str(), "quarkus" | "jakarta-ee" | "openliberty")
        {
            for fqn in [
                "javax.ejb.Stateless",
                "javax.ejb.Singleton",
                "javax.ejb.EJB",
                "javax.ejb.TransactionAttribute",
                "javax.ejb.TransactionAttributeType",
                "jakarta.ejb.Stateless",
                "jakarta.ejb.Singleton",
                "jakarta.ejb.EJB",
                "jakarta.ejb.TransactionAttribute",
                "jakarta.ejb.TransactionAttributeType",
            ] {
                strip_exact_imports.insert(fqn.into());
            }
        }
    }

    let bytes = source;
    let mut offset = 0usize;
    for line in text.split_inclusive('\n') {
        let line_start = offset;
        offset += line.len();
        let trimmed = line.trim_start();
        if !trimmed.starts_with("import ") {
            continue;
        }
        let imported_fqn = trimmed
            .strip_prefix("import ")
            .unwrap_or("")
            .trim_start()
            .strip_prefix("static ")
            .unwrap_or_else(|| trimmed.strip_prefix("import ").unwrap_or(""))
            .trim()
            .trim_end_matches(';')
            .trim();

        let should_strip = strip_exact_imports.contains(imported_fqn)
            || strip_import_prefixes
                .iter()
                .any(|p| imported_fqn.starts_with(p))
            || (src.is_some_and(|s| {
                imported_fqn.ends_with(".*")
                    && s.import_prefixes
                        .iter()
                        .any(|p| imported_fqn.starts_with(p.as_str()))
            }));

        if should_strip {
            let end = line_start + line.len();
            edits.push(SpanEdit {
                start_byte: line_start as u32,
                end_byte: end as u32,
                replacement: Vec::new(),
                reason: EditReason::ImportRewrite,
            });
            continue;
        }

        let mut rewritten = line.to_string();
        let mut changed = false;
        for (from, to) in &rewrites {
            if rewritten.contains(from.as_str()) {
                rewritten = rewritten.replace(from, to);
                changed = true;
            }
        }
        if changed {
            let end = line_start + line.len();
            edits.push(SpanEdit {
                start_byte: line_start as u32,
                end_byte: end as u32,
                replacement: rewritten.into_bytes(),
                reason: EditReason::ImportRewrite,
            });
        }
    }

    // Global namespace rewrite for remaining javax→jakarta type refs (outside comments/strings/imports).
    if !rewrites.is_empty() {
        let mut body_edits = namespace_body_edits(bytes, &rewrites, &edits);
        edits.append(&mut body_edits);
    }

    sort_edits(&mut edits);
    let patched = apply_span_edits(source, &edits)?;
    Ok((patched, edits.len(), needed))
}

fn namespace_body_edits(
    source: &[u8],
    rewrites: &BTreeMap<String, String>,
    existing: &[SpanEdit],
) -> Vec<SpanEdit> {
    let excluded = collect_java_excluded_ranges(source);
    let mut edits = Vec::new();
    for (from, to) in rewrites {
        let from_b = from.as_bytes();
        if from_b.is_empty() || source.len() < from_b.len() {
            continue;
        }
        let mut i = 0usize;
        while let Some(rel) = source[i..].windows(from_b.len()).position(|w| w == from_b) {
            let start = i + rel;
            let end = start + from_b.len();
            i = end;
            if existing
                .iter()
                .any(|e| start < e.end_byte as usize && end > e.start_byte as usize)
            {
                continue;
            }
            if excluded.iter().any(|&(es, ee)| start < ee && end > es) {
                continue;
            }
            edits.push(SpanEdit {
                start_byte: start as u32,
                end_byte: end as u32,
                replacement: to.as_bytes().to_vec(),
                reason: EditReason::ImportRewrite,
            });
        }
    }
    edits
}

/// Collect byte ranges of AST nodes where namespace rewrites must not occur
/// (`line_comment`, `block_comment`, `string_literal`, `character_literal`, `import_declaration`).
fn collect_java_excluded_ranges(source: &[u8]) -> Vec<(usize, usize)> {
    let lang: tree_sitter::Language = tree_sitter_java::LANGUAGE.into();
    let Ok(tree) = parse_source(source, Path::new("Migrate.java"), lang) else {
        return Vec::new();
    };
    let mut ranges = Vec::new();
    let mut stack = vec![tree.root_node()];
    while let Some(node) = stack.pop() {
        match node.kind() {
            "line_comment" | "block_comment" | "string_literal" | "character_literal"
            | "import_declaration" => {
                ranges.push((node.start_byte(), node.end_byte()));
                continue;
            }
            _ => {}
        }
        for i in 0..node.child_count() {
            if let Some(c) = node.child(i) {
                stack.push(c);
            }
        }
    }
    ranges
}

fn wave2_annotations(
    source: &[u8],
    src: Option<&FrameworkWritebackProfile>,
    dst: Option<&FrameworkWritebackProfile>,
    plan: Option<&MigrationPlan>,
) -> std::result::Result<(Vec<u8>, usize, BTreeSet<String>), WritebackError> {
    let mut needed = BTreeSet::new();
    let mut edits = Vec::new();
    let ast_annotations = collect_java_annotations(source);

    if let Some(plan) = plan {
        for ar in &plan.annotation_rewrites {
            let from_ann = ar.from.trim_start_matches('@');
            for ann in ast_annotations
                .iter()
                .filter(|a| !a.is_param && a.simple_name == from_ann)
            {
                needed.extend(ar.imports.iter().cloned());
                edits.push(SpanEdit {
                    start_byte: ann.start_byte,
                    end_byte: ann.end_byte,
                    replacement: ar.to.as_bytes().to_vec(),
                    reason: EditReason::Plan,
                });
            }
        }
    } else if let (Some(_src), Some(dst)) = (src, dst) {
        let map = jvm_annotation_concept_map();
        for ann_node in ast_annotations.iter().filter(|a| !a.is_param) {
            let concept = if ann_node.simple_name == "RequestMapping" && ann_node.is_method_target {
                let raw = &source[ann_node.start_byte as usize..ann_node.end_byte as usize];
                let raw_str = String::from_utf8_lossy(raw);
                Some(AnnConcept::MethodRoute(extract_request_mapping_verb(
                    &raw_str,
                )))
            } else {
                map.iter()
                    .find(|(name, c)| *name == ann_node.simple_name && c.wave() == 2)
                    .map(|(_, c)| *c)
            };
            let Some(concept) = concept else {
                continue;
            };
            let Some(rendered) =
                render_concept(&dst.writeback, concept, source, &ann_node.simple_name)
            else {
                continue;
            };
            needed.extend(rendered.imports);
            let text = render_with_indent(
                &rendered.template,
                &ann_node.indent,
                source,
                ann_node.start_byte,
                ann_node.end_byte,
            );
            edits.push(SpanEdit {
                start_byte: ann_node.start_byte,
                end_byte: ann_node.end_byte,
                replacement: text.into_bytes(),
                reason: EditReason::Other,
            });
        }
    }

    sort_edits(&mut edits);
    dedupe_overlapping(&mut edits);
    let patched = apply_span_edits(source, &edits)?;
    Ok((patched, edits.len(), needed))
}

fn wave3_param_annotations(
    source: &[u8],
    _src: Option<&FrameworkWritebackProfile>,
    dst: Option<&FrameworkWritebackProfile>,
    plan: Option<&MigrationPlan>,
) -> std::result::Result<(Vec<u8>, usize, BTreeSet<String>), WritebackError> {
    let mut needed = BTreeSet::new();
    let mut edits = Vec::new();
    let ast_annotations = collect_java_annotations(source);

    if let Some(plan) = plan {
        for ar in &plan.annotation_rewrites {
            let from_ann = ar.from.trim_start_matches('@');
            for ann in ast_annotations
                .iter()
                .filter(|a| a.is_param && a.simple_name == from_ann)
            {
                needed.extend(ar.imports.iter().cloned());
                edits.push(SpanEdit {
                    start_byte: ann.start_byte,
                    end_byte: ann.end_byte,
                    replacement: ar.to.as_bytes().to_vec(),
                    reason: EditReason::Plan,
                });
            }
        }
    } else if let Some(dst) = dst {
        let map = jvm_annotation_concept_map();
        for ann_node in ast_annotations.iter().filter(|a| a.is_param) {
            let Some((_, concept)) = map
                .iter()
                .find(|(name, c)| *name == ann_node.simple_name && c.wave() == 3)
            else {
                continue;
            };
            let Some(rendered) =
                render_concept(&dst.writeback, *concept, source, &ann_node.simple_name)
            else {
                continue;
            };
            needed.extend(rendered.imports);
            let text = render_with_indent(
                &rendered.template,
                &ann_node.indent,
                source,
                ann_node.start_byte,
                ann_node.end_byte,
            );
            edits.push(SpanEdit {
                start_byte: ann_node.start_byte,
                end_byte: ann_node.end_byte,
                replacement: text.into_bytes(),
                reason: EditReason::Other,
            });
        }
    }

    sort_edits(&mut edits);
    let patched = apply_span_edits(source, &edits)?;
    Ok((patched, edits.len(), needed))
}

#[derive(Clone, Copy)]
enum AnnConcept {
    Controller,
    Service,
    Repository,
    FieldInject,
    ConfigInject,
    Transactional,
    ClassRoute,
    MethodRoute(&'static str),
    ParamBind(&'static str),
}

impl AnnConcept {
    fn wave(self) -> u8 {
        match self {
            Self::ParamBind(_) => 3,
            _ => 2,
        }
    }
}

fn jvm_annotation_concept_map() -> Vec<(&'static str, AnnConcept)> {
    vec![
        ("RestController", AnnConcept::Controller),
        ("Controller", AnnConcept::Controller),
        ("Service", AnnConcept::Service),
        ("Component", AnnConcept::Service),
        ("Stateless", AnnConcept::Service),
        ("Singleton", AnnConcept::Service),
        ("Repository", AnnConcept::Repository),
        ("Autowired", AnnConcept::FieldInject),
        ("EJB", AnnConcept::FieldInject),
        ("Inject", AnnConcept::FieldInject),
        ("PersistenceContext", AnnConcept::FieldInject),
        ("Value", AnnConcept::ConfigInject),
        ("ConfigProperty", AnnConcept::ConfigInject),
        ("Transactional", AnnConcept::Transactional),
        ("TransactionAttribute", AnnConcept::Transactional),
        ("RequestMapping", AnnConcept::ClassRoute),
        ("Path", AnnConcept::ClassRoute),
        ("GetMapping", AnnConcept::MethodRoute("GET")),
        ("PostMapping", AnnConcept::MethodRoute("POST")),
        ("PutMapping", AnnConcept::MethodRoute("PUT")),
        ("DeleteMapping", AnnConcept::MethodRoute("DELETE")),
        ("PatchMapping", AnnConcept::MethodRoute("PATCH")),
        ("GET", AnnConcept::MethodRoute("GET")),
        ("POST", AnnConcept::MethodRoute("POST")),
        ("PUT", AnnConcept::MethodRoute("PUT")),
        ("DELETE", AnnConcept::MethodRoute("DELETE")),
        ("PATCH", AnnConcept::MethodRoute("PATCH")),
        ("PathVariable", AnnConcept::ParamBind("path")),
        ("RequestParam", AnnConcept::ParamBind("query")),
        ("RequestHeader", AnnConcept::ParamBind("header")),
        ("RequestBody", AnnConcept::ParamBind("body")),
        ("PathParam", AnnConcept::ParamBind("path")),
        ("QueryParam", AnnConcept::ParamBind("query")),
        ("HeaderParam", AnnConcept::ParamBind("header")),
    ]
}

struct Rendered {
    template: String,
    imports: Vec<String>,
}

fn render_concept(
    wb: &FrameworkWritebackTemplates,
    concept: AnnConcept,
    source: &[u8],
    ann: &str,
) -> Option<Rendered> {
    let tmpl: Option<&AnnotationTemplate> = match concept {
        AnnConcept::Controller => wb.controller_stereotype.as_ref(),
        AnnConcept::Service => wb.service_stereotype.as_ref(),
        AnnConcept::Repository => wb.repository_stereotype.as_ref(),
        AnnConcept::FieldInject => wb.field_inject.as_ref(),
        AnnConcept::ConfigInject => wb.config_inject.as_ref(),
        AnnConcept::Transactional => wb.transactional.as_ref(),
        AnnConcept::ClassRoute => wb.class_route_prefix.as_ref(),
        AnnConcept::MethodRoute(verb) => wb.method_route.get(verb),
        AnnConcept::ParamBind(kind) => wb.param_bind.get(kind),
    };
    let tmpl = tmpl?;
    // Fill placeholders from first matching annotation occurrence later via render_with_indent.
    let _ = (source, ann);
    Some(Rendered {
        template: tmpl.template.clone(),
        imports: tmpl.imports.clone(),
    })
}

fn render_with_indent(template: &str, indent: &str, source: &[u8], start: u32, end: u32) -> String {
    let original = &source[start as usize..end as usize];
    let orig_text = String::from_utf8_lossy(original);
    let (path, name, key, default_kv) = extract_ann_placeholders(&orig_text);
    let indent_nl = if template.contains("{indent}") {
        format!("\n{indent}")
    } else {
        String::new()
    };
    let mut rendered = template
        .replace("{indent}", &indent_nl)
        .replace("{path}", &path)
        .replace("{name}", &name)
        .replace("{key}", &key)
        .replace("{default_kv}", &default_kv);
    if path.is_empty() && rendered.contains("@Path(\"\")") {
        let suffix = format!("{indent_nl}@Path(\"\")");
        if rendered.ends_with(&suffix) && rendered.len() > suffix.len() {
            rendered.truncate(rendered.len() - suffix.len());
        }
    }
    rendered
}

fn extract_request_mapping_verb(ann_text: &str) -> &'static str {
    for (needle, verb) in [
        ("RequestMethod.POST", "POST"),
        ("RequestMethod.PUT", "PUT"),
        ("RequestMethod.DELETE", "DELETE"),
        ("RequestMethod.PATCH", "PATCH"),
        ("RequestMethod.GET", "GET"),
    ] {
        if ann_text.contains(needle) {
            return verb;
        }
    }
    "GET"
}

fn extract_ann_placeholders(ann_text: &str) -> (String, String, String, String) {
    // @Value("${key:default}") / @Value("${key}")
    if let Some(inner) = ann_text.find("${").and_then(|i| {
        ann_text[i + 2..]
            .find('}')
            .map(|j| &ann_text[i + 2..i + 2 + j])
    }) {
        if let Some((k, d)) = inner.split_once(':') {
            return (
                String::new(),
                String::new(),
                k.to_string(),
                format!(", defaultValue = \"{d}\""),
            );
        }
        return (
            String::new(),
            String::new(),
            inner.to_string(),
            String::new(),
        );
    }
    // @GetMapping("/x") / @GetMapping(produces = "...", value = "/x") / @PathVariable("id")
    let name = string_literal_after_key(ann_text, "name")
        .or_else(|| string_literal_after_key(ann_text, "value"))
        .or_else(|| first_unkeyed_string_literal(ann_text))
        .unwrap_or_default();
    let path = string_literal_after_key(ann_text, "path")
        .or_else(|| string_literal_after_key(ann_text, "value"))
        .or_else(|| first_unkeyed_string_literal(ann_text))
        .unwrap_or_else(|| name.clone());
    let default = string_literal_after_key(ann_text, "defaultValue");
    let default_kv = default
        .map(|d| format!(", defaultValue = \"{d}\""))
        .unwrap_or_default();
    (path, name, String::new(), default_kv)
}

fn first_string_literal(s: &str) -> Option<String> {
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'"' {
            i += 1;
            let start = i;
            while i < bytes.len() && bytes[i] != b'"' {
                if bytes[i] == b'\\' {
                    i += 2;
                } else {
                    i += 1;
                }
            }
            return Some(String::from_utf8_lossy(&bytes[start..i]).into_owned());
        }
        i += 1;
    }
    None
}

/// Extract a positional string literal `@GetMapping("/orders")` only if it is not preceded by `=`
/// (so `@GetMapping(produces = "application/json")` does not treat the MIME type as `{path}`).
fn first_unkeyed_string_literal(s: &str) -> Option<String> {
    let open = s.find('(')?;
    let after_open = s[open + 1..].trim_start();
    if after_open.starts_with('"') {
        first_string_literal(after_open)
    } else {
        None
    }
}

fn string_literal_after_key(s: &str, key: &str) -> Option<String> {
    let mut search_from = 0usize;
    while let Some(rel) = s[search_from..].find(key) {
        let start = search_from + rel;
        let after = start + key.len();
        search_from = after;
        if start > 0 {
            let prev = s.as_bytes()[start - 1];
            if prev.is_ascii_alphanumeric() || prev == b'_' {
                continue;
            }
        }
        let rest = s[after..].trim_start();
        if let Some(after_eq) = rest.strip_prefix('=') {
            let val = after_eq.trim_start();
            if val.starts_with('"') {
                return first_string_literal(val);
            }
        }
    }
    None
}

#[derive(Debug, Clone)]
struct ParsedJavaAnnotation {
    simple_name: String,
    start_byte: u32,
    end_byte: u32,
    indent: String,
    is_param: bool,
    is_method_target: bool,
}

/// Single-pass tree-sitter walk collecting all AST `marker_annotation` / `annotation` nodes.
/// Never matches text inside comments or string literals.
fn collect_java_annotations(source: &[u8]) -> Vec<ParsedJavaAnnotation> {
    let lang: tree_sitter::Language = tree_sitter_java::LANGUAGE.into();
    let Ok(tree) = parse_source(source, Path::new("Migrate.java"), lang) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let mut stack = vec![tree.root_node()];
    while let Some(node) = stack.pop() {
        let kind = node.kind();
        if matches!(kind, "marker_annotation" | "annotation") {
            let Some(name_node) = node.child_by_field_name("name") else {
                continue;
            };
            let Ok(raw_name) = name_node.utf8_text(source) else {
                continue;
            };
            let simple_name = raw_name
                .rsplit('.')
                .next()
                .unwrap_or(raw_name)
                .trim()
                .to_string();
            let start = node.start_byte();
            let end = node.end_byte();

            let mut is_param = false;
            let mut is_method_target = false;
            let mut p = node.parent();
            while let Some(anc) = p {
                match anc.kind() {
                    "formal_parameter" | "spread_parameter" | "receiver_parameter" => {
                        is_param = true;
                        break;
                    }
                    "method_declaration" | "constructor_declaration" => {
                        is_method_target = true;
                        break;
                    }
                    "class_declaration"
                    | "interface_declaration"
                    | "record_declaration"
                    | "enum_declaration"
                    | "field_declaration" => {
                        break;
                    }
                    _ => p = anc.parent(),
                }
            }

            let line_start = source[..start]
                .iter()
                .rposition(|&b| b == b'\n')
                .map(|idx| idx + 1)
                .unwrap_or(0);
            let indent: String = source[line_start..start]
                .iter()
                .take_while(|&&b| b == b' ' || b == b'\t')
                .map(|&b| b as char)
                .collect();

            out.push(ParsedJavaAnnotation {
                simple_name,
                start_byte: start as u32,
                end_byte: end as u32,
                indent,
                is_param,
                is_method_target,
            });
            continue;
        }
        // Push children in reverse so `out` is in source-byte order.
        for i in (0..node.child_count()).rev() {
            if let Some(c) = node.child(i) {
                stack.push(c);
            }
        }
    }
    out
}

fn ensure_imports(source: &[u8], imports: &BTreeSet<String>) -> Vec<u8> {
    let text = String::from_utf8_lossy(source);
    let existing: BTreeSet<String> = text
        .lines()
        .filter_map(|l| {
            let t = l.trim();
            t.strip_prefix("import ")
                .map(|rest| rest.trim().trim_end_matches(';').trim().to_string())
        })
        .collect();
    let mut to_add: Vec<String> = imports
        .iter()
        .filter(|i| !existing.contains(i.as_str()))
        .cloned()
        .collect();
    if to_add.is_empty() {
        return source.to_vec();
    }
    to_add.sort();
    let block: String = to_add.iter().map(|i| format!("import {i};\n")).collect();

    // Insert after last import, or after package, or at top.
    let mut last_import_end: Option<usize> = None;
    let mut package_end: Option<usize> = None;
    let mut offset = 0usize;
    for line in text.split_inclusive('\n') {
        let trimmed = line.trim_start();
        if trimmed.starts_with("package ") {
            package_end = Some(offset + line.len());
        } else if trimmed.starts_with("import ") {
            last_import_end = Some(offset + line.len());
        }
        offset += line.len();
    }
    let insert_at = last_import_end.or(package_end).unwrap_or(0);
    let mut out = Vec::with_capacity(source.len() + block.len());
    out.extend_from_slice(&source[..insert_at]);
    if insert_at > 0 && source.get(insert_at.saturating_sub(1)) != Some(&b'\n') {
        out.push(b'\n');
    }
    out.extend_from_slice(block.as_bytes());
    out.extend_from_slice(&source[insert_at..]);
    out
}

fn collect_template_imports(wb: &FrameworkWritebackTemplates, into: &mut BTreeSet<String>) {
    let mut push = |t: &Option<AnnotationTemplate>| {
        if let Some(t) = t {
            into.extend(t.imports.iter().cloned());
        }
    };
    push(&wb.controller_stereotype);
    push(&wb.service_stereotype);
    push(&wb.repository_stereotype);
    push(&wb.field_inject);
    push(&wb.config_inject);
    push(&wb.transactional);
    push(&wb.class_route_prefix);
    for t in wb.method_route.values() {
        into.extend(t.imports.iter().cloned());
    }
    for t in wb.param_bind.values() {
        into.extend(t.imports.iter().cloned());
    }
}

fn dedupe_overlapping(edits: &mut Vec<SpanEdit>) {
    if edits.is_empty() {
        return;
    }
    sort_edits(edits);
    let mut kept = Vec::with_capacity(edits.len());
    for e in edits.drain(..) {
        if let Some(last) = kept.last() {
            let last: &SpanEdit = last;
            if e.start_byte < last.end_byte {
                continue; // drop overlapping later edit
            }
        }
        kept.push(e);
    }
    *edits = kept;
}

fn java_buffer_has_error(buf: &[u8]) -> bool {
    let lang: tree_sitter::Language = tree_sitter_java::LANGUAGE.into();
    let Ok(tree) = parse_source(buf, Path::new("Migrate.java"), lang) else {
        return true;
    };
    let mut stack = vec![tree.root_node()];
    while let Some(node) = stack.pop() {
        if node.is_error() || node.is_missing() {
            return true;
        }
        for i in 0..node.child_count() {
            if let Some(c) = node.child(i) {
                stack.push(c);
            }
        }
    }
    false
}

fn generate_worklist_from_buffers(
    files: &BTreeMap<String, Vec<u8>>,
    src: Option<&FrameworkWritebackProfile>,
    plan: Option<&MigrationPlan>,
) -> Vec<AgentWorklistItem> {
    let mut triggers: Vec<String> = Vec::new();
    if let Some(s) = src {
        triggers.extend(s.worklist_supertype_triggers.clone());
        triggers.extend(s.worklist_call_triggers.clone());
    }
    if let Some(p) = plan {
        triggers.extend(p.worklist_triggers.supertypes.clone());
        triggers.extend(p.worklist_triggers.calls.clone());
        triggers.extend(p.worklist_triggers.annotations.clone());
    }
    let mut items = Vec::new();
    for (path, bytes) in files {
        // Build files always go on the worklist.
        if path.ends_with("pom.xml")
            || path.ends_with("build.gradle")
            || path.ends_with("build.gradle.kts")
        {
            items.push(AgentWorklistItem {
                symbol: path.clone(),
                file: path.clone(),
                start_line: Some(1),
                end_line: None,
                start_byte: Some(0),
                end_byte: Some(bytes.len() as u32),
                reason: "build file requires manual dependency migration".into(),
                caller_count: 0,
                blast_radius_order: 0,
            });
            continue;
        }
        if triggers.is_empty() {
            continue;
        }
        let excluded = if path.ends_with(".java") {
            collect_java_excluded_ranges(bytes)
        } else {
            Vec::new()
        };
        let text = String::from_utf8_lossy(bytes);
        for trig in &triggers {
            let mut search_from = 0usize;
            while let Some(rel) = text[search_from..].find(trig.as_str()) {
                let idx = search_from + rel;
                let end = idx + trig.len();
                search_from = end;
                if excluded.iter().any(|&(es, ee)| idx < ee && end > es) {
                    continue;
                }
                let line = text[..idx].bytes().filter(|b| *b == b'\n').count() + 1;
                items.push(AgentWorklistItem {
                    symbol: trig.clone(),
                    file: path.clone(),
                    start_line: Some(line),
                    end_line: Some(line),
                    start_byte: Some(idx as u32),
                    end_byte: Some(end as u32),
                    reason: format!("matches worklist trigger `{trig}`"),
                    caller_count: 0,
                    blast_radius_order: 0,
                });
                break;
            }
        }
    }
    items
}

/// Graph node facets used when seeding an agent worklist.
/// `(symbol, file, start_line, end_line, start_byte, end_byte)`.
pub type WorklistNodeFacet = (
    String,
    String,
    Option<usize>,
    Option<usize>,
    Option<u32>,
    Option<u32>,
);

/// Seed worklist items from a loaded graph snapshot (Extends/Implements/Calls).
pub fn worklist_from_graph_nodes(
    nodes: &[WorklistNodeFacet],
    triggers: &[String],
    caller_counts: &HashMap<String, usize>,
) -> Vec<AgentWorklistItem> {
    let mut items = Vec::new();
    for (symbol, file, sl, el, sb, eb) in nodes {
        let hit = triggers
            .iter()
            .any(|t| symbol.contains(t.as_str()) || file.contains(t.as_str()));
        if !hit {
            continue;
        }
        let reason = format!("graph match for worklist trigger near `{symbol}`");
        items.push(AgentWorklistItem {
            symbol: symbol.clone(),
            file: file.clone(),
            start_line: *sl,
            end_line: *el,
            start_byte: *sb,
            end_byte: *eb,
            reason,
            caller_count: *caller_counts.get(symbol).unwrap_or(&0),
            blast_radius_order: 0,
        });
    }
    items
}

/// Load a Layer-2 plan JSON file.
pub fn load_migration_plan(path: &Path) -> Result<MigrationPlan> {
    let text = std::fs::read_to_string(path)?;
    serde_json::from_str(&text).map_err(|e| Error::SerdeError(e.to_string()))
}

/// Write migrated buffers to `--output` or `--in-place`.
pub fn materialize_migration(
    repo_root: &Path,
    patched: &BTreeMap<String, Vec<u8>>,
    output: Option<&Path>,
    in_place: bool,
    dry_run: bool,
) -> Result<u64> {
    if dry_run {
        return Ok(0);
    }
    let mut bytes = 0u64;
    for (rel, content) in patched {
        let dest: PathBuf = if in_place {
            repo_root.join(rel)
        } else if let Some(out) = output {
            out.join(rel)
        } else {
            continue;
        };
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&dest, content)?;
        bytes += content.len() as u64;
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spring_to_quarkus_controller_wave() {
        let src = br#"
package com.acme;

import org.springframework.web.bind.annotation.RestController;
import org.springframework.web.bind.annotation.RequestMapping;
import org.springframework.web.bind.annotation.GetMapping;
import org.springframework.web.bind.annotation.PathVariable;
import org.springframework.beans.factory.annotation.Autowired;
import org.springframework.beans.factory.annotation.Value;

@RestController
@RequestMapping("/api/orders")
public class OrderController {
    @Autowired
    private OrderService orders;

    @Value("${orders.max:100}")
    private int max;

    @GetMapping("/{id}")
    public String get(@PathVariable("id") Long id) {
        return "ok";
    }
}
"#;
        let mut files = BTreeMap::new();
        files.insert("OrderController.java".into(), src.to_vec());
        let req = MigrateRequest {
            language: "java".into(),
            from_framework: Some("spring".into()),
            to_framework: Some("quarkus".into()),
            plan: None,
            wave: MigrateWave::All,
            files,
            worklist_seeds: vec![],
        };
        let (out, report) = migrate_buffers(&req).expect("migrate");
        let text = String::from_utf8(out["OrderController.java"].clone()).unwrap();
        assert!(text.contains("@ApplicationScoped"), "{text}");
        assert!(text.contains("@Inject"), "{text}");
        assert!(text.contains("@ConfigProperty"), "{text}");
        assert!(text.contains("@GET") || text.contains("@Path"), "{text}");
        assert!(text.contains("@PathParam"), "{text}");
        assert!(!text.contains("@Autowired"), "{text}");
        assert!(!text.contains("@RestController"), "{text}");
        assert!(report.wave1_edits + report.wave2_edits + report.wave3_edits > 0);
    }

    #[test]
    fn plan_junit4_to_junit5() {
        let src = br#"
import org.junit.Test;
import org.junit.Before;

public class SampleTest {
    @Before
    public void setUp() {}

    @Test
    public void works() {}
}
"#;
        let plan = MigrationPlan {
            name: "junit4-to-junit5".into(),
            language: "java".into(),
            import_rewrites: BTreeMap::from([
                ("org.junit.Test".into(), "org.junit.jupiter.api.Test".into()),
                (
                    "org.junit.Before".into(),
                    "org.junit.jupiter.api.BeforeEach".into(),
                ),
            ]),
            annotation_rewrites: vec![
                PlanAnnotationRewrite {
                    from: "Before".into(),
                    to: "@BeforeEach".into(),
                    imports: vec!["org.junit.jupiter.api.BeforeEach".into()],
                },
                PlanAnnotationRewrite {
                    from: "Test".into(),
                    to: "@Test".into(),
                    imports: vec!["org.junit.jupiter.api.Test".into()],
                },
            ],
            ..Default::default()
        };
        let mut files = BTreeMap::new();
        files.insert("SampleTest.java".into(), src.to_vec());
        let req = MigrateRequest {
            language: "java".into(),
            from_framework: None,
            to_framework: None,
            plan: Some(plan),
            wave: MigrateWave::All,
            files,
            worklist_seeds: vec![],
        };
        let (out, _) = migrate_buffers(&req).expect("plan migrate");
        let text = String::from_utf8(out["SampleTest.java"].clone()).unwrap();
        assert!(text.contains("@BeforeEach"), "{text}");
        assert!(text.contains("org.junit.jupiter.api"), "{text}");
    }
}
