//! Exact static automation resolution. Only compact indexes remain resident.
//! Reference-owning nodes are staged on disk in spill mode, then finalized once
//! eligible file/root/callable indexes exist. No snapshot mutation or AST retention.
use super::*;
use bincode::Options;
use rgctl_plugin_api::{SourceReference, SourceReferenceKind as Kind};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::File;
use std::io::{BufReader, BufWriter, Read, Write};

#[derive(Default, Clone, serde::Serialize, serde::Deserialize)]
pub(super) struct TemplateIndex {
    files: BTreeMap<String, (Uuid, String)>,
    roots: BTreeMap<String, Uuid>,
    syntax: BTreeMap<String, rgctl_plugin_api::JinjaSyntax>,
    roles: BTreeMap<String, Uuid>,
    macros: BTreeMap<String, Vec<Macro>>,
    handlers: BTreeMap<String, Vec<Handler>>,
    references: BTreeMap<Uuid, (String, Vec<SourceReference>)>,
    excluded_classifications: BTreeMap<String, (String, String)>,
}
#[derive(Clone, serde::Serialize, serde::Deserialize)]
struct Macro {
    id: Uuid,
    name: String,
    scope: String,
    exported: bool,
}
#[derive(Clone, serde::Serialize, serde::Deserialize)]
struct Handler {
    id: Uuid,
    file: String,
    name: String,
    topics: Vec<String>,
}

/// A stream of length-prefixed nodes. The scratch buffer is reused at write/read.
#[derive(Default)]
pub(super) struct PendingNodes {
    memory: Vec<Node>,
    disk: Option<(PathBuf, BufWriter<File>)>,
    scratch: Vec<u8>,
}
impl PendingNodes {
    pub(super) fn push(&mut self, node: Node, spill: Option<&SegmentedSpill>) -> Result<()> {
        if let Some(spill) = spill {
            if self.disk.is_none() {
                let path = spill.dir().join("template-nodes.seg");
                self.disk = Some((path.clone(), BufWriter::new(File::create(path)?)));
            }
            self.scratch.clear();
            bincode::serialize_into(&mut self.scratch, &node)
                .map_err(|e| Error::SerdeError(e.to_string()))?;
            if let Some((_, writer)) = &mut self.disk {
                writer.write_all(&(self.scratch.len() as u64).to_le_bytes())?;
                writer.write_all(&self.scratch)?;
            }
        } else {
            self.memory.push(node);
        }
        Ok(())
    }
    fn visit(&mut self, mut visit: impl FnMut(Node) -> Result<()>) -> Result<()> {
        if let Some((path, writer)) = &mut self.disk {
            writer.flush()?;
            let mut reader = BufReader::new(File::open(path)?);
            loop {
                let mut len = [0u8; 8];
                let count = reader.read(&mut len[..1])?;
                if count == 0 {
                    break;
                }
                reader.read_exact(&mut len[1..])?;
                let len = u64::from_le_bytes(len);
                if len > 32 * 1024 * 1024 {
                    return Err(Error::SerdeError("oversized staged template node".into()));
                }
                self.scratch.resize(len as usize, 0);
                reader.read_exact(&mut self.scratch)?;
                let node = bincode::deserialize(&self.scratch)
                    .map_err(|e| Error::SerdeError(e.to_string()))?;
                visit(node)?;
            }
        } else {
            for node in &self.memory {
                visit(node.clone())?;
            }
        }
        Ok(())
    }
}
fn key(path: &Path) -> String {
    normalize_file_key(&path.to_string_lossy())
}
fn safe_join(root: &Path, target: &str) -> Option<String> {
    if target.is_empty() || !rgctl_registry::template_options::safe_source_relative(target) {
        return None;
    }
    Some(key(&root.join(target)))
}
fn refs(node: &Node) -> Result<Vec<SourceReference>> {
    node.get_property("source_references_json")
        .map(serde_json::from_str)
        .transpose()
        .map(|refs| refs.unwrap_or_default())
        .map_err(|e| Error::SerdeError(e.to_string()))
}
fn scope(node: &Node) -> Option<String> {
    node.get_property("role_root")
        .map(|root| format!("role:{}", key(Path::new(root))))
        .or_else(|| {
            node.get_property("parent_qualified_name")
                .map(|qn| qn.split("::section:").next().unwrap_or(qn).to_owned())
        })
}
impl GraphBuilder {
    pub(super) fn index_template_file(&mut self, path: &Path, id: Uuid, language: &str) {
        if !matches!(language, "ansible" | "jinja") {
            return;
        }
        self.templates
            .files
            .insert(key(path), (id, language.to_owned()));
    }
    pub(super) fn index_template_symbol(&mut self, node: &Node) {
        let language = node.get_property("language").unwrap_or_default();
        if !matches!(language, "ansible" | "jinja") {
            return;
        }
        let Some(file) = node.file_path.as_deref() else {
            return;
        };
        let file = key(Path::new(file));
        match node.get_property("kind").unwrap_or_default() {
            "jinja_template" if node.get_property("embedded_in").is_none() => {
                self.templates.roots.insert(file.clone(), node.id);
                if let Some(raw) = node.get_property("syntax_profile_json")
                    && let Ok(profile) = serde_json::from_str(raw)
                {
                    self.templates.syntax.insert(file.clone(), profile);
                }
            }
            "ansible_source" | "ansible_playbook" => {
                self.templates.roots.insert(file.clone(), node.id);
            }
            "jinja_macro" => {
                if node.qualified_name.is_some() {
                    self.templates
                        .macros
                        .entry(file.clone())
                        .or_default()
                        .push(Macro {
                            id: node.id,
                            name: node.name.to_string(),
                            scope: node
                                .get_property("lexical_scope")
                                .unwrap_or_default()
                                .to_owned(),
                            exported: node.get_property("exported") == Some("true"),
                        });
                }
            }
            "ansible_handler" => {
                if let Some(scope) = scope(node) {
                    self.templates
                        .handlers
                        .entry(scope)
                        .or_default()
                        .push(Handler {
                            id: node.id,
                            file: file.clone(),
                            name: node.name.to_string(),
                            topics: node
                                .get_property("listen_topics_json")
                                .and_then(|raw| serde_json::from_str(raw).ok())
                                .unwrap_or_default(),
                        });
                }
            }
            _ => {}
        }
        let role = node
            .get_property("role_root")
            .map(PathBuf::from)
            .or_else(|| {
                self.template_ansible_enabled
                    .unwrap_or(true)
                    .then(|| rgctl_registry::template_options::role_root(Path::new(&file)))
                    .flatten()
            });
        if let Some(root) = role {
            let root = key(&root);
            if !self.templates.roles.contains_key(&root) {
                let role = Node::new(
                    NodeType::AnsibleRole,
                    Path::new(&root)
                        .file_name()
                        .and_then(|name| name.to_str())
                        .unwrap_or("role"),
                )
                .with_file_path(root.clone())
                .with_qualified_name(format!("ansible:role:{root}"))
                .with_property("language".into(), "ansible".into())
                .with_property("role_root".into(), root.clone());
                let id = role.id;
                self.templates.roles.insert(root.clone(), id);
                let active = self.active_tracker_key.take();
                self.commit_node(role);
                self.active_tracker_key = active;
            }
        }
    }

    /// Finalize exact automation dependencies after all file workers have drained.
    pub fn resolve_template_references(&mut self) -> Result<()> {
        let mut staged = std::mem::take(&mut self.pending_templates);
        let contexts = self.template_contexts(&mut staged)?;
        staged.visit(|mut node| {
            let mut references = refs(&node)?;
            for reference in &mut references {
                let (targets, reason, candidates) =
                    self.resolve_reference(reference, &node, &contexts);
                reference.properties.insert(
                    "resolution".into(),
                    if targets.is_empty() {
                        "unresolved"
                    } else {
                        "resolved"
                    }
                    .into(),
                );
                if let Some(reason) = reason {
                    reference.properties.insert("reason".into(), reason.into());
                }
                reference.properties.insert(
                    "candidate_paths_json".into(),
                    serde_json::to_string(&candidates)
                        .map_err(|e| Error::SerdeError(e.to_string()))?,
                );
                reference.properties.insert(
                    "resolved_targets_json".into(),
                    serde_json::to_string(&targets)
                        .map_err(|e| Error::SerdeError(e.to_string()))?,
                );
                let edge = match reference.kind {
                    Kind::JinjaExtends => EdgeType::Extends,
                    Kind::JinjaInclude | Kind::TaskInclude => EdgeType::DependsOn,
                    Kind::JinjaImport => EdgeType::Uses,
                    Kind::MacroCall => EdgeType::Calls,
                    Kind::AnsibleRole => EdgeType::IncludesRole,
                    Kind::RoleDependency => EdgeType::DependsOnRole,
                    Kind::PlaybookInclude => EdgeType::IncludesPlaybook,
                    Kind::TemplateRender => EdgeType::RendersTemplate,
                    Kind::HandlerNotify => EdgeType::NotifiesHandler,
                };
                let from = if reference.kind == Kind::RoleDependency {
                    reference
                        .context
                        .role_root
                        .as_ref()
                        .and_then(|root| self.templates.roles.get(&key(Path::new(root))))
                        .copied()
                        .unwrap_or(node.id)
                } else {
                    node.id
                };
                for target in targets {
                    self.add_edge(from, target, edge);
                }
            }
            if let Some(file) = node.file_path.as_deref() {
                self.templates
                    .references
                    .insert(node.id, (key(Path::new(file)), references.clone()));
            }
            node.properties.insert(
                "source_references_json".into(),
                serde_json::to_string(&references).map_err(|e| Error::SerdeError(e.to_string()))?,
            );
            // Index/tracker work happened when staged; append only the final payload.
            self.emit_node(node);
            Ok(())
        })?;
        let files = std::mem::take(&mut self.templates.files);
        for (file, (file_id, language)) in &files {
            if (language == "ansible" || self.template_ansible_enabled.unwrap_or(true))
                && let Some(root) = rgctl_registry::template_options::role_root(Path::new(&file))
                && let Some(role) = self.templates.roles.get(&key(&root)).copied()
            {
                self.add_edge(role, *file_id, EdgeType::Contains);
            }
        }
        self.templates.files = files;
        if let Some((path, writer)) = staged.disk.take() {
            drop(writer);
            std::fs::remove_file(path)?;
        }
        Ok(())
    }

    fn template_contexts(
        &self,
        staged: &mut PendingNodes,
    ) -> Result<BTreeMap<String, BTreeSet<String>>> {
        let mut contexts: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        // Compact include facts only, used to propagate known caller contexts.
        let mut includes: Vec<_> = self
            .templates
            .references
            .values()
            .flat_map(|(_, references)| references.iter())
            .filter(|reference| {
                matches!(reference.kind, Kind::TaskInclude | Kind::PlaybookInclude)
                    && !reference.dynamic
            })
            .cloned()
            .collect();
        staged.visit(|node| {
            for reference in refs(&node)? {
                if matches!(reference.kind, Kind::TaskInclude | Kind::PlaybookInclude)
                    && !reference.dynamic
                {
                    includes.push(reference);
                }
            }
            Ok(())
        })?;
        // Monotone closure over caller paths; a cycle cannot invent a context.
        for _ in 0..=includes.len() {
            let mut changed = false;
            for reference in &includes {
                let source = key(Path::new(&reference.location.file));
                let mut callers = contexts.get(&source).cloned().unwrap_or_default();
                if let Some(playbook) = &reference.context.playbook {
                    callers.insert(playbook.clone());
                }
                let (paths, _) = self.reference_paths(reference, &contexts);
                for path in paths {
                    if self.templates.files.contains_key(&path) {
                        let entry = contexts.entry(path).or_default();
                        let before = entry.len();
                        entry.extend(callers.iter().cloned());
                        changed |= before != entry.len();
                    }
                }
            }
            if !changed {
                break;
            }
        }
        Ok(contexts)
    }

    /// Safe prospective paths, intersected with walk eligibility by the scheduler.
    pub fn template_candidate_paths(&mut self) -> Result<BTreeSet<String>> {
        let mut staged = std::mem::take(&mut self.pending_templates);
        let contexts = self.template_contexts(&mut staged)?;
        let mut paths = BTreeSet::new();
        staged.visit(|node| {
            for reference in refs(&node)? {
                if !reference.dynamic
                    && matches!(
                        reference.kind,
                        Kind::TemplateRender
                            | Kind::JinjaInclude
                            | Kind::JinjaImport
                            | Kind::JinjaExtends
                            | Kind::MacroCall
                    )
                {
                    paths.extend(self.reference_paths(&reference, &contexts).0);
                }
            }
            Ok(())
        })?;
        self.pending_templates = staged;
        Ok(paths)
    }

    fn ansible_path(&self, root: &Path, target: &str) -> Option<String> {
        if rgctl_registry::template_options::safe_source_relative(target) {
            return safe_join(root, target);
        }
        if target.is_empty() || target.contains('\\') || Path::new(target).is_absolute() {
            return None;
        }
        let normalize = |path: &Path| -> Option<PathBuf> {
            let mut result = PathBuf::new();
            for component in path.components() {
                match component {
                    Component::CurDir => {}
                    Component::ParentDir => {
                        if !result.pop() {
                            return None;
                        }
                    }
                    Component::Prefix(_) => return None,
                    _ => result.push(component.as_os_str()),
                }
            }
            Some(result)
        };
        let candidate = normalize(&root.join(target))?;
        let fence = normalize(self.template_repository_root.as_deref()?)?;
        candidate.starts_with(fence).then(|| key(&candidate))
    }

    fn reference_paths(
        &self,
        reference: &SourceReference,
        contexts: &BTreeMap<String, BTreeSet<String>>,
    ) -> (Vec<String>, bool) {
        let file = Path::new(&reference.location.file);
        let parent = file.parent().unwrap_or(Path::new(""));
        let mut roots = Vec::new();
        let mut unknown = false;
        match reference.kind {
            Kind::JinjaExtends | Kind::JinjaInclude | Kind::JinjaImport | Kind::MacroCall => {
                if let Some(role) = rgctl_registry::template_options::role_root(file) {
                    roots.push(role.join("templates"));
                }
                roots.extend(reference.context.template_roots.iter().map(PathBuf::from));
            }
            Kind::TemplateRender => {
                if let Some(role) = &reference.context.role_root {
                    roots.push(Path::new(role).join("templates"));
                }
                roots.push(parent.join("templates"));
                let mut callers = contexts.get(&key(file)).cloned().unwrap_or_default();
                if let Some(playbook) = &reference.context.playbook {
                    callers.insert(playbook.clone());
                }
                if reference.context.role_root.is_none()
                    && reference.context.playbook.is_none()
                    && callers.is_empty()
                {
                    unknown = true;
                }
                if callers.len() > 1 {
                    unknown = true;
                }
                for caller in callers {
                    if let Some(parent) = Path::new(&caller).parent() {
                        roots.push(parent.join("templates"));
                    }
                }
            }
            Kind::TaskInclude => {
                if let Some(role) = &reference.context.role_root {
                    roots.push(Path::new(role).join("tasks"));
                }
                roots.push(parent.to_path_buf());
            }
            Kind::PlaybookInclude => {
                roots.push(parent.to_path_buf());
            }
            _ => {}
        }
        let names = if reference.candidates.is_empty() {
            vec![reference.target.as_str()]
        } else {
            reference.candidates.iter().map(String::as_str).collect()
        };
        let mut paths = Vec::new();
        for name in names {
            for root in &roots {
                let path = if matches!(
                    reference.kind,
                    Kind::TemplateRender | Kind::TaskInclude | Kind::PlaybookInclude
                ) {
                    self.ansible_path(root, name)
                } else {
                    safe_join(root, name)
                };
                if let Some(path) = path
                    && !paths.contains(&path)
                {
                    paths.push(path);
                }
            }
        }
        (paths, unknown)
    }

    fn resolve_reference(
        &self,
        reference: &SourceReference,
        node: &Node,
        contexts: &BTreeMap<String, BTreeSet<String>>,
    ) -> (Vec<Uuid>, Option<&'static str>, Vec<String>) {
        if reference.dynamic {
            return (Vec::new(), Some("dynamic_target"), Vec::new());
        }
        if reference.kind == Kind::HandlerNotify {
            let Some(scope) = scope(node) else {
                return (vec![], Some("unknown_handler_scope"), vec![]);
            };
            let handlers = self
                .templates
                .handlers
                .get(&scope)
                .map(Vec::as_slice)
                .unwrap_or_default();
            let named: Vec<_> = handlers
                .iter()
                .filter(|handler| handler.name == reference.target)
                .map(|handler| handler.id)
                .collect();
            let mut targets: Vec<_> = handlers
                .iter()
                .filter(|handler| handler.topics.contains(&reference.target))
                .map(|handler| handler.id)
                .collect();
            if named.len() > 1 {
                return (vec![], Some("ambiguous_handler"), vec![]);
            }
            targets.extend(named);
            targets.sort();
            targets.dedup();
            let reason = targets.is_empty().then_some("missing_handler");
            return (
                targets,
                reason,
                reference
                    .context
                    .role_root
                    .as_ref()
                    .map(|root| key(&Path::new(root).join("handlers")))
                    .into_iter()
                    .collect(),
            );
        }
        if matches!(reference.kind, Kind::AnsibleRole | Kind::RoleDependency) {
            let file = Path::new(&reference.location.file);
            let base = reference
                .context
                .role_root
                .as_ref()
                .and_then(|root| Path::new(root).parent())
                .map(Path::to_path_buf)
                .or_else(|| file.parent().map(|parent| parent.join("roles")));
            let target = reference.target.replace('.', "/");
            let mut candidates = Vec::new();
            if let Some(base) = base
                && let Some(path) = safe_join(&base, &reference.target)
            {
                candidates.push(path);
            }
            if reference.target.matches('.').count() == 2 {
                // Only exact collection-role layouts, never global bare-name suffixes.
                let parts: Vec<_> = target.split('/').collect();
                for root in self.templates.roles.keys() {
                    let suffix = format!(
                        "ansible_collections/{}/{}/roles/{}",
                        parts[0], parts[1], parts[2]
                    );
                    if root == &suffix || root.ends_with(&format!("/{suffix}")) {
                        candidates.push(root.clone());
                    }
                }
            }
            let mut targets: Vec<_> = candidates
                .iter()
                .filter_map(|candidate| self.templates.roles.get(candidate).copied())
                .collect();
            targets.sort();
            targets.dedup();
            if targets.len() > 1 {
                return (vec![], Some("ambiguous_role"), candidates);
            }
            let reason = targets.is_empty().then_some("missing_or_external_role");
            return (targets, reason, candidates);
        }
        let local_macro = reference.kind == Kind::MacroCall
            && reference
                .properties
                .get("local_template")
                .is_some_and(|value| value == "true");
        let (paths, unknown) = if local_macro {
            (vec![key(Path::new(&reference.location.file))], false)
        } else {
            self.reference_paths(reference, contexts)
        };
        if unknown {
            return (vec![], Some("unknown_or_ambiguous_search_context"), paths);
        }
        if paths.is_empty() {
            return (vec![], Some("unknown_loader_root"), paths);
        }
        if reference.kind == Kind::TemplateRender
            && let Some(role) = &reference.context.role_root
        {
            let own_root = key(&Path::new(role).join("templates"));
            let own_known = paths
                .iter()
                .filter(|path| path.starts_with(&format!("{own_root}/")))
                .any(|path| {
                    self.templates
                        .files
                        .get(path)
                        .is_some_and(|(_, language)| language == "jinja")
                });
            if !own_known
                && paths.iter().any(|path| {
                    self.templates
                        .files
                        .get(path)
                        .is_some_and(|(_, language)| language == "jinja")
                })
            {
                return (vec![], Some("unknown_caller_role_search_stack"), paths);
            }
        }
        if reference.kind == Kind::MacroCall
            && !matches!(
                node.node_type,
                NodeType::Function | NodeType::AnsibleTask | NodeType::AnsibleHandler
            )
        {
            return (vec![], Some("non_callable_source_scope"), paths);
        }

        let wanted = match reference.kind {
            Kind::TemplateRender
            | Kind::JinjaExtends
            | Kind::JinjaInclude
            | Kind::JinjaImport
            | Kind::MacroCall => "jinja",
            _ => "ansible",
        };
        let selected = if local_macro {
            paths.first()
        } else {
            paths.iter().find(|path| {
                self.templates
                    .files
                    .get(*path)
                    .is_some_and(|(_, language)| language == wanted)
            })
        };
        let Some(path) = selected else {
            return (vec![], Some("missing_or_filtered_target"), paths);
        };
        if reference.kind == Kind::MacroCall {
            let Some(member) = &reference.member else {
                return (vec![], Some("unknown_macro_member"), paths);
            };
            let macros = self
                .templates
                .macros
                .get(path)
                .map(Vec::as_slice)
                .unwrap_or_default();
            let mut matches: Vec<_> = macros
                .iter()
                .filter(|mac| {
                    if mac.name != *member {
                        return false;
                    }
                    if local_macro {
                        let lexical = reference
                            .properties
                            .get("lexical_scope")
                            .map(String::as_str)
                            .unwrap_or_default();
                        lexical == mac.scope || lexical.starts_with(&format!("{}::", mac.scope))
                    } else {
                        mac.exported
                    }
                })
                .collect();
            if local_macro {
                let longest = matches.iter().map(|mac| mac.scope.len()).max().unwrap_or(0);
                matches.retain(|mac| mac.scope.len() == longest);
            }
            if matches.len() != 1 {
                return (
                    vec![],
                    Some(if matches.is_empty() {
                        "missing_or_inaccessible_macro"
                    } else {
                        "ambiguous_macro"
                    }),
                    paths,
                );
            }
            return (vec![matches[0].id], None, paths);
        }
        let target = if reference.kind == Kind::TemplateRender {
            self.templates.files.get(path).map(|(id, _)| *id)
        } else {
            self.templates.roots.get(path).copied()
        };
        let reason = if reference.kind == Kind::TemplateRender {
            self.templates
                .syntax
                .get(path)
                .and_then(|syntax| serde_json::to_value(syntax).ok())
                .and_then(|syntax| {
                    reference
                        .properties
                        .iter()
                        .any(|(key, value)| {
                            syntax.get(key).is_some_and(|known| {
                                known.as_str().is_some_and(|known| known != value)
                            })
                        })
                        .then_some("external_profile_unresolved")
                })
        } else {
            None
        };
        (
            target.into_iter().collect(),
            reason.or_else(|| target.is_none().then_some("missing_target_root")),
            paths,
        )
    }
}

/// Compact automation state bound to one completed snapshot digest. It contains
/// exact indexes, source references and reverse candidate paths, including absent
/// targets; never source bodies or ASTs. A failed/stale load is recovered from
/// persisted relevant file records or cached facts by the updater. Writers publish
/// only after the graph commit, using atomic rename; a crash leaves a stale sidecar
/// which fails the digest check rather than reviving stale edges.
#[derive(Default, Clone, serde::Serialize, serde::Deserialize)]
pub struct TemplateDependencies {
    version: u32,
    digest: String,
    settings: String,
    index: TemplateIndex,
    reverse: BTreeMap<String, BTreeSet<String>>,
    // Content classification of declined YAML candidates. This avoids repeatedly
    // treating unchanged, intentionally untracked inventories as new sources.
    excluded_classifications: BTreeMap<String, (String, String)>,
}
impl TemplateDependencies {
    /// Repository artifact path.
    pub fn path(root: &Path) -> PathBuf {
        root.join(".rgctl/template-dependencies.bin")
    }
    /// Extraction/routing revision, independent of the source-content hash.
    pub fn settings(registry: &rgctl_registry::LanguageRegistry) -> String {
        let mut hash = blake3::Hasher::new();
        hash.update(b"automation-sidecar-1");
        hash.update(include_bytes!("template_resolution.rs"));
        hash.update(include_bytes!("../discovery/mod.rs"));
        hash.update(include_bytes!("../../../rgctl-registry/src/registry.rs"));
        hash.update(include_bytes!(
            "../../../rgctl-registry/src/template_options.rs"
        ));
        if let Ok(bytes) = serde_json::to_vec(&registry.language_filter()) {
            hash.update(&bytes);
        }
        if let Some(options) = registry.template_options()
            && let Ok(bytes) = serde_json::to_vec(options)
        {
            hash.update(&bytes);
        }
        for id in ["ansible", "jinja"] {
            if let Some(plugin) = registry.get_language_plugin(id) {
                hash.update(
                    &crate::facts_cache::facts_key_for_hash(plugin.as_ref(), [0; 32]).manifest_hash,
                );
            }
        }
        hash.finalize().to_hex().to_string()
    }
    /// Read only a sidecar for exactly this graph digest. Corruption is recoverable.
    pub fn load(root: &Path, digest: &str) -> Option<Self> {
        let bytes = std::fs::read(Self::path(root)).ok()?;
        if !bytes.starts_with(b"RGCTLTPL") {
            return None;
        }
        let payload = bytes.get(8..)?;
        if payload.get(..4)? != 1u32.to_le_bytes() {
            return None;
        }
        let result: Self = bincode::DefaultOptions::new()
            .with_fixint_encoding()
            .with_limit(payload.len() as u64)
            .reject_trailing_bytes()
            .deserialize(payload)
            .ok()?;
        (result.version == 1 && result.digest == digest).then_some(result)
    }
    /// Merge worker classifications, pruning disappeared/ineligible candidates.
    pub fn update_classifications(
        &mut self,
        classified: BTreeMap<String, (String, String)>,
        eligible: impl IntoIterator<Item = PathBuf>,
    ) {
        let eligible: BTreeSet<_> = eligible.into_iter().map(|path| key(&path)).collect();
        self.excluded_classifications
            .retain(|path, _| eligible.contains(path));
        self.excluded_classifications.extend(
            classified
                .into_iter()
                .map(|(path, value)| (key(Path::new(&path)), value)),
        );
    }
    /// Hash only previously declined candidates on a warm pass. A byte/settings
    /// change returns the candidate to workers for definitive classification.
    pub fn filter_declined_candidates(
        &self,
        registry: &rgctl_registry::LanguageRegistry,
        files: &mut Vec<PathBuf>,
    ) {
        if !self.settings_match(registry) {
            return;
        }
        files.retain(|path| {
            self.excluded_classifications
                .get(&key(path))
                .is_none_or(|(hash, _)| {
                    std::fs::read(path)
                        .map(|bytes| blake3::hash(&bytes).to_hex().as_str() != hash)
                        .unwrap_or(true)
                })
        });
    }
    /// Record recovered file provenance; an unknown/mismatched stamp requests a
    /// rebuild instead of presenting old extraction as current.
    pub fn set_recovered_settings(&mut self, settings: String) {
        self.settings = settings;
    }
    /// Detect configuration/grammar changes even when all source hashes match.
    pub fn settings_match(&self, registry: &rgctl_registry::LanguageRegistry) -> bool {
        self.settings == Self::settings(registry)
    }
    /// Publish atomically after a successful snapshot write.
    pub fn save(&mut self, root: &Path, digest: &str) -> Result<()> {
        self.version = 1;
        self.digest = digest.to_owned();
        self.rebuild_reverse();
        let path = Self::path(root);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let temp = path.with_extension(format!("{}.tmp", Uuid::new_v4()));
        let mut writer = BufWriter::new(File::create(&temp)?);
        writer.write_all(b"RGCTLTPL")?;
        bincode::serialize_into(&mut writer, self).map_err(|e| Error::SerdeError(e.to_string()))?;
        writer.flush()?;
        writer.get_ref().sync_all()?;
        drop(writer);
        std::fs::rename(temp, path)?;
        Ok(())
    }
    fn rebuild_reverse(&mut self) {
        self.reverse.clear();
        for (file, references) in self.index.references.values() {
            for reference in references {
                if let Some(raw) = reference.properties.get("candidate_paths_json")
                    && let Ok(paths) = serde_json::from_str::<Vec<String>>(raw)
                {
                    for path in paths {
                        self.reverse.entry(path).or_default().insert(file.clone());
                    }
                }
            }
        }
    }
    /// Relevant source files and transitive referrers. Role declarations/handlers
    /// share a static role scope, so replay that role's cached file facts together.
    pub fn affected_files(&self, changed: &[PathBuf]) -> BTreeSet<String> {
        let mut affected: BTreeSet<_> = changed.iter().map(|path| key(path)).collect();
        loop {
            let before = affected.len();
            let seeds: Vec<_> = affected.iter().cloned().collect();
            for seed in seeds {
                for (candidate, sources) in &self.reverse {
                    if seed == *candidate || seed.starts_with(&format!("{candidate}/")) {
                        affected.extend(sources.iter().cloned());
                    }
                }
                if let Some(role) = rgctl_registry::template_options::role_root(Path::new(&seed)) {
                    let root = key(&role);
                    affected.extend(
                        self.index
                            .files
                            .keys()
                            .filter(|path| path.starts_with(&format!("{root}/")))
                            .cloned(),
                    );
                }
            }
            if affected.len() == before {
                break;
            }
        }
        affected
    }
    /// Extra invalidations for synthetic role directory nodes, which have no
    /// single task-file owner. They are rebuilt from the affected role files.
    pub fn role_invalidations(&self, paths: &BTreeSet<String>) -> Vec<String> {
        self.index
            .roles
            .keys()
            .filter(|root| {
                paths
                    .iter()
                    .any(|path| path.starts_with(&format!("{root}/")))
            })
            .cloned()
            .collect()
    }
    /// Build a delta seed without stale indexes/references from invalidated files.
    pub fn without_files(mut self, paths: &BTreeSet<String>) -> Self {
        self.excluded_classifications
            .retain(|path, _| !paths.contains(path));
        self.index.files.retain(|path, _| !paths.contains(path));
        self.index.roots.retain(|path, _| !paths.contains(path));
        self.index.syntax.retain(|path, _| !paths.contains(path));
        self.index.macros.retain(|path, _| !paths.contains(path));
        self.index.handlers.retain(|_, handlers| {
            handlers.retain(|handler| !paths.contains(&handler.file));
            !handlers.is_empty()
        });
        self.index.roles.retain(|root, _| {
            !paths
                .iter()
                .any(|path| path.starts_with(&format!("{root}/")))
        });
        self.index
            .references
            .retain(|_, (file, _)| !paths.contains(file));
        self
    }
    /// Eligible literal candidates, including currently missing targets.
    pub fn candidate_paths(&self) -> impl Iterator<Item = &str> {
        self.reverse.keys().map(String::as_str)
    }
    /// Jinja files whose association came from a literal reference.
    pub fn template_files(&self) -> impl Iterator<Item = &str> {
        self.index
            .files
            .iter()
            .filter(|(_, (_, language))| language == "jinja")
            .map(|(path, _)| path.as_str())
    }
    /// Files previously attributed to the structural languages.
    pub fn files(&self) -> impl Iterator<Item = &str> {
        self.index.files.keys().map(String::as_str)
    }
}
impl GraphBuilder {
    /// Seed exact indexes from the prior digest-bound dependency sidecar.
    pub fn seed_template_dependencies(&mut self, dependencies: TemplateDependencies) {
        self.templates = dependencies.index;
        self.templates.excluded_classifications = dependencies.excluded_classifications;
    }
    /// Recover one relevant persisted record, addressed through FileTracker IDs.
    /// No full snapshot node scan or referenced-source read is required.
    pub fn index_existing_template_node(&mut self, node: &Node) -> Result<()> {
        if node.node_type == NodeType::File {
            if let (Some(file), Some(language)) =
                (node.file_path.as_deref(), node.get_property("language"))
                && matches!(language, "ansible" | "jinja")
            {
                self.index_template_file(Path::new(file), node.id, language);
            }
        } else {
            self.index_template_symbol(node);
            let references = refs(node)?;
            if !references.is_empty()
                && let Some(file) = node.file_path.as_deref()
            {
                self.templates
                    .references
                    .insert(node.id, (key(Path::new(file)), references));
            }
        }
        Ok(())
    }
    /// Take compact state after resolution, before consuming the builder.
    pub fn take_template_dependencies(
        &mut self,
        registry: &rgctl_registry::LanguageRegistry,
    ) -> TemplateDependencies {
        let mut dependencies = TemplateDependencies {
            version: 1,
            digest: String::new(),
            settings: TemplateDependencies::settings(registry),
            excluded_classifications: std::mem::take(&mut self.templates.excluded_classifications),
            index: std::mem::take(&mut self.templates),
            reverse: BTreeMap::new(),
        };
        dependencies.rebuild_reverse();
        dependencies
    }
}
