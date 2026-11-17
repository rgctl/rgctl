//! Shared semantic search execution for CLI and HTTP API.

use super::semantic::{
    CliSemanticScope, EngineBlastProvider, SemanticQueryArgs, expand_gql_neighbors,
};
use super::semantic_output::{
    SemanticHitJson, SemanticQueryJsonResponse, build_query_response, hit_from_semantic,
};
use crate::analysis::{
    AnalysisResults, BlastSummaryProvider, CommunityQueryContext, OnnxReloadOptions,
    SemanticExpandConfig, SemanticExpandMode, SemanticFusionConfig, SemanticIndex,
    expand_semantic_hits, query_communities, query_index_with_fusion,
};
use anyhow::{Context, Result, bail};
use rgctl_graph::CodeGraph;
use rgctl_graph::backend::GraphBackend;
use serde::Serialize;
use std::path::{Path, PathBuf};

/// Summary for `GET /api/semantic/status`.
#[derive(Debug, Clone, Serialize)]
pub struct SemanticStatusResponse {
    pub available: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dimensions: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub functions_indexed: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub graph_digest: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

pub fn semantic_index_path(repo: &Path) -> PathBuf {
    SemanticIndex::default_path(repo)
}

pub fn semantic_status(repo: &Path) -> SemanticStatusResponse {
    let path = semantic_index_path(repo);
    if !path.is_file() {
        return SemanticStatusResponse {
            available: false,
            model_id: None,
            dimensions: None,
            functions_indexed: None,
            graph_digest: None,
            message: Some(
                "Semantic index not found — run `rgctl semantic index` then refresh.".into(),
            ),
        };
    }

    match SemanticIndex::load(&path) {
        Ok(index) => semantic_status_for_index(&index),
        Err(err) => SemanticStatusResponse {
            available: false,
            model_id: None,
            dimensions: None,
            functions_indexed: None,
            graph_digest: None,
            message: Some(format!("Failed to load semantic index: {err}")),
        },
    }
}

/// Status for an index the caller already has in memory.
#[must_use]
pub fn semantic_status_for_index(index: &SemanticIndex) -> SemanticStatusResponse {
    SemanticStatusResponse {
        available: true,
        model_id: Some(index.model_id.clone()),
        dimensions: Some(index.dimensions),
        functions_indexed: Some(index.len()),
        graph_digest: index.graph_digest.clone(),
        message: None,
    }
}

/// Load analysis results once, for callers that hold them for the server lifetime.
pub fn load_analysis_results(repo: &Path) -> Result<Option<AnalysisResults>> {
    let path = repo.join(".rgctl/analysis_results.bin");
    if !path.is_file() {
        return Ok(None);
    }
    AnalysisResults::load(&path)
        .map(Some)
        .with_context(|| format!("load analysis results {}", path.display()))
}

/// Run a semantic query against a loaded index (CLI + HTTP).
pub fn execute_semantic_query(
    repo: &Path,
    graph: &CodeGraph,
    index: &SemanticIndex,
    args: &SemanticQueryArgs,
) -> Result<SemanticQueryJsonResponse> {
    let analysis = load_analysis_results(repo)?;
    execute_semantic_query_with_analysis(repo, graph, index, analysis.as_ref(), args)
}

/// Run a semantic query with caller-supplied analysis, so HTTP requests do not deserialize
/// `analysis_results.bin` on every query.
pub fn execute_semantic_query_with_analysis(
    repo: &Path,
    graph: &CodeGraph,
    index: &SemanticIndex,
    analysis: Option<&AnalysisResults>,
    args: &SemanticQueryArgs,
) -> Result<SemanticQueryJsonResponse> {
    validate_index_scope(index, args.scope)?;

    let reload = OnnxReloadOptions {
        model_path: args.model.clone(),
        tokenizer_path: args.tokenizer.clone(),
    };

    let fusion = SemanticFusionConfig {
        enabled: args.fusion,
        candidate_pool: args.candidate_pool.max(args.limit),
        keyword_and: args.keyword_and,
        ..SemanticFusionConfig::default()
    };

    if args.scope == CliSemanticScope::Community {
        let analysis = analysis.ok_or_else(|| {
            anyhow::anyhow!(
                "community semantic search requires analysis_results.bin (run `rgctl discover`)"
            )
        })?;
        let backend = graph.backend();
        let ctx = CommunityQueryContext::from_analysis(analysis, |uuid| {
            backend.get_node(uuid).ok().flatten().map(|n| {
                (
                    n.name.to_string(),
                    n.file_path.as_ref().map(|s| s.to_string()),
                )
            })
        });
        let labels: std::collections::HashMap<_, _> = ctx
            .communities
            .iter()
            .map(|c| (c.id, c.label.clone()))
            .collect();
        let community_hits =
            query_communities(index, analysis, &labels, &args.query, args.limit, &reload)?;
        let hits: Vec<SemanticHitJson> = community_hits
            .into_iter()
            .map(|h| SemanticHitJson {
                node_id: h.community_id.to_string(),
                name: h.label.clone(),
                qualified_name: Some(format!("community:{}", h.community_id)),
                file_path: Some(format!("{} members", h.member_count)),
                distance: h.distance,
                score: h.score,
                fused_score: None,
                ranking: Some("community".into()),
            })
            .collect();
        return Ok(build_query_response(
            &args.query,
            &index.model_id,
            index.dimensions,
            hits,
            None,
        ));
    }

    let mut hits = query_index_with_fusion(
        index,
        &args.query,
        args.limit,
        &reload,
        &fusion,
        analysis,
        Some(graph.backend()),
        Some(repo),
    )?;
    let unfiltered_hit_count = hits.len();
    apply_scope_filter(&mut hits, args.scope);
    if hits.is_empty() && unfiltered_hit_count > 0 {
        bail!(
            "semantic query scope {:?} produced no matching entries; rebuild index with `rgctl semantic index --scope {}`",
            args.scope,
            scope_flag(args.scope)
        );
    }

    let backend = graph.backend();
    let graph_digest = index.graph_digest.clone();

    let expansion = if let Some(mode) = args.expand {
        let expand_mode = match mode {
            super::semantic::CliExpandMode::Neighbors => SemanticExpandMode::Neighbors,
            super::semantic::CliExpandMode::Blast => SemanticExpandMode::Blast,
            super::semantic::CliExpandMode::Gql => SemanticExpandMode::Gql,
            super::semantic::CliExpandMode::All => SemanticExpandMode::All,
        };
        let config = SemanticExpandConfig {
            mode: expand_mode,
            call_depth: args.expand_depth.max(1),
            anchor_limit: args.limit.min(5),
            per_anchor_limit: 20,
        };
        let blast_provider = EngineBlastProvider {
            repo,
            backend,
            graph_digest: graph_digest.clone(),
        };
        let mut expansion = expand_semantic_hits(
            backend,
            &hits,
            &config,
            if matches!(
                expand_mode,
                SemanticExpandMode::Blast | SemanticExpandMode::All
            ) {
                Some(&blast_provider as &dyn BlastSummaryProvider)
            } else {
                None
            },
        )?;

        if matches!(
            expand_mode,
            SemanticExpandMode::Gql | SemanticExpandMode::All
        ) {
            expansion.gql = Some(expand_gql_neighbors(
                backend,
                &hits,
                args.expand_depth.max(1),
                config.anchor_limit,
            )?);
        }
        Some(expansion)
    } else {
        None
    };

    let hit_json: Vec<_> = hits
        .iter()
        .map(|hit| hit_from_semantic(&hit.entry, hit.distance, index.dimensions, Some(hit)))
        .collect();

    Ok(build_query_response(
        &args.query,
        &index.model_id,
        index.dimensions,
        hit_json,
        expansion,
    ))
}

fn apply_scope_filter(hits: &mut Vec<crate::analysis::SemanticHit>, scope: CliSemanticScope) {
    match scope {
        CliSemanticScope::Function => hits.retain(|hit| {
            hit.entry
                .node_type
                .as_deref()
                .is_some_and(|node_type| node_type == "Function")
                || hit.entry.semantic_tags.is_some()
        }),
        CliSemanticScope::Docs => hits.retain(|hit| {
            hit.entry
                .node_type
                .as_deref()
                .is_some_and(|node_type| node_type == "Module")
                && hit
                    .entry
                    .kind
                    .as_deref()
                    .is_some_and(|kind| kind == "heading" || kind == "code_block")
        }),
        CliSemanticScope::All | CliSemanticScope::Community => {}
    }
}

fn scope_flag(scope: CliSemanticScope) -> &'static str {
    match scope {
        CliSemanticScope::Function => "function",
        CliSemanticScope::Docs => "docs",
        CliSemanticScope::All => "all",
        CliSemanticScope::Community => "community",
    }
}

/// Inferred index scope label from entry contents (for status / mismatch messages).
pub(crate) fn inferred_index_scope(index: &SemanticIndex) -> &'static str {
    let has_functions = index
        .entries
        .iter()
        .any(|e| e.node_type.as_deref() == Some("Function"));
    let has_docs = index.entries.iter().any(|e| {
        e.node_type.as_deref() == Some("Module")
            && e.kind
                .as_deref()
                .is_some_and(|kind| kind == "heading" || kind == "code_block")
    });
    match (has_functions, has_docs) {
        (true, true) => "all",
        (true, false) => "function",
        (false, true) => "docs",
        (false, false) => "unknown",
    }
}

pub(crate) fn validate_index_scope(
    index: &SemanticIndex,
    requested: CliSemanticScope,
) -> Result<()> {
    let has_functions = index
        .entries
        .iter()
        .any(|e| e.node_type.as_deref() == Some("Function"));
    let has_docs = index.entries.iter().any(|e| {
        e.node_type.as_deref() == Some("Module")
            && e.kind
                .as_deref()
                .is_some_and(|kind| kind == "heading" || kind == "code_block")
    });

    match requested {
        CliSemanticScope::Function if !has_functions && has_docs => bail!(
            "semantic query scope 'function' is incompatible with index scope '{}' — run `rgctl semantic index --scope function`",
            inferred_index_scope(index)
        ),
        CliSemanticScope::Docs if !has_docs && has_functions => bail!(
            "semantic query scope 'docs' is incompatible with index scope '{}' — run `rgctl semantic index --scope docs`",
            inferred_index_scope(index)
        ),
        _ => Ok(()),
    }
}
