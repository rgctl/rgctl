//! Template invalidation uses compact candidate indexes, including absent paths.
//! Recovery addresses relevant persisted records through FileTracker UUIDs; it
//! does not hydrate or rescan a columnar snapshot and never parses a whole repo.
use crate::file_tracker::{EmptyAutomationState, FileTracker, relative_path, resolve_path};
use rgctl_error::Result;
use rgctl_extraction::{GraphBuilder, TemplateDependencies};
use rgctl_graph::backend::GraphBackend;
use rgctl_graph::{
    CodeGraph,
    schema::{Node, NodeType},
    snapshot::SnapshotNodeStore,
};
use rgctl_registry::LanguageRegistry;
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};

pub(crate) struct TemplatePlan {
    pub dependencies: TemplateDependencies,
    pub affected: BTreeSet<String>,
    pub role_invalidations: Vec<String>,
    pub template_targets: std::collections::HashSet<PathBuf>,
}
pub(crate) fn load_or_recover(
    registry: &LanguageRegistry,
    root: &Path,
    snapshot: &Path,
    graph: Option<&CodeGraph>,
) -> Result<(TemplateDependencies, bool)> {
    let store = snapshot
        .is_file()
        .then(|| SnapshotNodeStore::open(snapshot))
        .transpose()?;
    if let Some(store) = &store
        && let Some(deps) = TemplateDependencies::load(root, store.content_digest()?)
    {
        return Ok((deps, false));
    }
    let tracker = FileTracker::load(root)?;
    let expected_settings = TemplateDependencies::settings(registry);
    let mut stale_settings = None;
    let mut builder = GraphBuilder::new();
    let get = |id| -> Result<Option<Node>> {
        if let Some(store) = &store {
            store.get_node(id)
        } else if let Some(graph) = graph {
            graph.backend().get_node(id)
        } else {
            Ok(None)
        }
    };
    for (file, ids) in tracker.node_mapping() {
        let path = resolve_path(root, file);
        // Only candidate formats/associations can have automation metadata.
        if !registry.is_ansible_candidate(&path)
            && !registry.is_template_pool_path(&path)
            && !registry
                .get_plugin_for_file_path_only_for_discovery(&path)
                .is_some_and(|id| matches!(id.as_str(), "ansible" | "jinja"))
        {
            continue;
        }
        let file_node = Node::new(NodeType::File, path.to_string_lossy().into_owned())
            .with_file_path(path.to_string_lossy().into_owned());
        let Some(node) = get(file_node.id)? else {
            continue;
        };
        let semantic_source = matches!(
            node.get_property("language"),
            Some("ansible" | "jinja" | "yaml")
        ) || registry
            .get_plugin_for_file_path_only_for_discovery(&path)
            .is_some_and(|id| matches!(id.as_str(), "ansible" | "jinja"));
        if semantic_source
            && node.get_property("template_context_hash") != Some(expected_settings.as_str())
        {
            stale_settings = Some(
                node.get_property("template_context_hash")
                    .unwrap_or_default()
                    .to_owned(),
            );
        }
        if !matches!(node.get_property("language"), Some("ansible" | "jinja")) {
            continue;
        }
        builder.index_existing_template_node(&node)?;
        for id in ids {
            if *id != node.id
                && let Some(node) = get(*id)?
            {
                builder.index_existing_template_node(&node)?;
            }
        }
    }
    let mut dependencies = builder.take_template_dependencies(registry);
    if let Some(settings) = stale_settings {
        dependencies.set_recovered_settings(settings);
    }
    Ok((dependencies, true))
}
pub(crate) fn plan(
    registry: &LanguageRegistry,
    root: &Path,
    snapshot: &Path,
    graph: Option<&CodeGraph>,
    changed: &[PathBuf],
) -> Result<TemplatePlan> {
    let (dependencies, recovered) = load_or_recover(registry, root, snapshot, graph)?;
    let mut affected = dependencies.affected_files(changed);
    if recovered && !dependencies.settings_match(registry) {
        affected.extend(dependencies.files().map(str::to_owned));
    }
    let role_invalidations = dependencies.role_invalidations(&affected);
    let template_targets = dependencies
        .template_files()
        .filter(|path| affected.contains(*path))
        .map(PathBuf::from)
        .collect();
    Ok(TemplatePlan {
        dependencies,
        affected,
        role_invalidations,
        template_targets,
    })
}

/// Recover the compact automation sidecar through FileTracker-addressed records,
/// retaining extraction provenance so callers can safely decide snapshot reuse.
pub fn recover_template_dependencies(
    registry: &LanguageRegistry,
    root: &Path,
    snapshot: &Path,
    candidates: &[PathBuf],
    thread_count: Option<usize>,
) -> Result<TemplateDependencies> {
    let (mut dependencies, _) = load_or_recover(registry, root, snapshot, None)?;
    if dependencies.settings_match(registry) && registry.language_filter().is_some() {
        let tracker = FileTracker::load(root)?;
        let unknown: Vec<_> = candidates
            .iter()
            .filter(|path| {
                registry.is_ansible_candidate(path)
                    && relative_path(root, path)
                        .is_ok_and(|key| !tracker.file_hashes().contains_key(&key))
            })
            .cloned()
            .collect();
        let extractor = rgctl_extraction::Extractor::new(std::sync::Arc::new(registry.clone()))
            .with_languages(registry.language_filter().map(<[String]>::to_vec));
        let classified = rgctl_pipeline::parallel::par_map(thread_count, &unknown, |path| {
            let extraction = extractor.extract_file(path).ok()?;
            if !extraction.excluded {
                return None;
            }
            Some((
                path.to_string_lossy().into_owned(),
                (extraction.file_hash?, extraction.language?),
            ))
        })
        .into_iter()
        .flatten()
        .collect();
        dependencies.update_classifications(classified, candidates.iter().cloned());
    }
    let store = SnapshotNodeStore::open(snapshot)?;
    let digest = store.content_digest()?;
    dependencies.save(root, digest)?;
    if dependencies.is_empty() && dependencies.settings_match(registry) {
        let mut tracker = FileTracker::load(root)?;
        tracker.set_empty_automation(Some(EmptyAutomationState {
            digest: digest.to_owned(),
            settings: TemplateDependencies::settings(registry),
        }));
        tracker.save()?;
    }
    Ok(dependencies)
}
