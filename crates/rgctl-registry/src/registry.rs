//! Language plugin registry
//!
//! Manages all available language plugins and routes files to the appropriate plugin.

use crate::ingest_route::{IngestRoute, classify_ingest_path};
use rgctl_error::{Error, Result};
use rgctl_plugin_api::{ConfigFormatPlugin, ConfigFormatRegistrar, LanguagePlugin};
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, OnceLock};

/// Optional hook for the binary crate to register feature-gated language plugins.
static FULL_REGISTRY_BUILDER: OnceLock<fn() -> LanguageRegistry> = OnceLock::new();

/// Optional hook run before building a full registry (e.g. register the builder).
static REGISTRY_PRE_INIT: OnceLock<fn()> = OnceLock::new();

/// Register a builder that returns a registry with language plugins enabled by Cargo features.
pub fn set_full_registry_builder(builder: fn() -> LanguageRegistry) {
    let _ = FULL_REGISTRY_BUILDER.set(builder);
}

/// Register a one-time initializer (monolith sets this to wire `build.rs` registration).
pub fn set_registry_pre_init(init: fn()) {
    let _ = REGISTRY_PRE_INIT.set(init);
}

/// Create a registry using the registered builder, or config formats only if unset.
pub fn full_registry() -> LanguageRegistry {
    if let Some(init) = REGISTRY_PRE_INIT.get() {
        init();
    }
    FULL_REGISTRY_BUILDER
        .get()
        .copied()
        .unwrap_or(LanguageRegistry::with_config_formats)()
}

/// Registry for all language and config format plugins
#[derive(Clone)]
pub struct LanguageRegistry {
    repository_root: Option<std::path::PathBuf>,
    language_filter: Option<Vec<String>>,
    templates: Option<crate::template_options::CompiledOptions>,
    /// Language plugins by language ID
    language_plugins: HashMap<String, Arc<dyn LanguagePlugin>>,

    /// Config format plugins by format ID
    config_plugins: HashMap<String, Arc<dyn ConfigFormatPlugin>>,

    /// File extension to language plugin mapping
    extension_map: HashMap<String, Arc<dyn LanguagePlugin>>,

    /// File extension to config plugin mapping
    config_extension_map: HashMap<String, Arc<dyn ConfigFormatPlugin>>,
}

impl LanguageRegistry {
    /// Create an empty registry with no plugins registered.
    pub fn empty() -> Self {
        Self {
            repository_root: None,
            language_filter: None,
            templates: None,
            language_plugins: HashMap::new(),
            config_plugins: HashMap::new(),
            extension_map: HashMap::new(),
            config_extension_map: HashMap::new(),
        }
    }

    /// Create a registry with built-in config format plugins only.
    pub fn with_config_formats() -> Self {
        let mut registry = Self::empty();
        rgctl_config_formats::register_all(&mut registry);
        registry
    }

    /// Create a registry with config formats and any registered language plugins.
    pub fn new() -> Self {
        full_registry()
    }

    /// Check if a language plugin is registered.
    pub fn has_plugin(&self, language_id: &str) -> bool {
        self.language_plugins.contains_key(language_id)
    }

    /// List all language plugin IDs.
    pub fn language_plugin_ids(&self) -> Vec<String> {
        self.language_plugins.keys().cloned().collect()
    }

    /// Bind final language selection for snapshot-reuse compatibility. Extraction
    /// facts stay independent of this filter and can be reused by later selections.
    pub fn with_language_filter(mut self, mut languages: Option<Vec<String>>) -> Self {
        if let Some(ids) = &mut languages {
            for id in ids.iter_mut() {
                *id = crate::canonical_language_id(id).to_owned();
            }
            ids.sort();
            ids.dedup();
        }
        self.language_filter = languages;
        self
    }
    /// Final worker selection used to bind dependency/reuse state.
    pub fn language_filter(&self) -> Option<&[String]> {
        self.language_filter.as_deref()
    }

    /// Bind immutable routing settings to a repository without changing this registry.
    pub fn for_repository(&self, root: &Path) -> Result<Self> {
        #[derive(serde::Deserialize, Default)]
        struct Config {
            #[serde(default)]
            languages: crate::TemplateOptions,
        }
        let path = root.join("rgctl.toml");
        let options = match std::fs::read_to_string(&path) {
            Ok(raw) => {
                toml::from_str::<Config>(&raw)
                    .map_err(|e| Error::ConfigError(format!("{}: {e}", path.display())))?
                    .languages
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => crate::TemplateOptions::default(),
            Err(e) => return Err(e.into()),
        };
        self.with_template_options(root, options)
    }

    /// Bind already-loaded options; useful for callers hosting multiple repositories.
    pub fn with_template_options(
        &self,
        root: &Path,
        options: crate::TemplateOptions,
    ) -> Result<Self> {
        for template_root in &options.jinja.template_roots {
            let target = root.join(template_root);
            if target.exists()
                && root.exists()
                && !target.canonicalize()?.starts_with(root.canonicalize()?)
            {
                return Err(Error::ConfigError(format!(
                    "template root escapes repository: {template_root}"
                )));
            }
        }
        let mut registry = self.clone();
        registry.repository_root = Some(root.to_path_buf());
        registry.templates = Some(crate::template_options::CompiledOptions::new(options)?);
        Ok(registry)
    }

    /// Repository fence for exact local source dependencies.
    pub fn repository_root(&self) -> Option<&Path> {
        self.repository_root.as_deref()
    }

    /// Ordered absolute loader roots resolved once from repository-relative settings.
    pub fn template_roots(&self) -> Vec<String> {
        self.template_options()
            .map(|options| {
                options
                    .jinja
                    .template_roots
                    .iter()
                    .map(|root| {
                        self.repository_root
                            .as_deref()
                            .unwrap_or(Path::new(""))
                            .join(root)
                            .to_string_lossy()
                            .into_owned()
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Immutable extraction settings, if a repository has been bound.
    pub fn template_options(&self) -> Option<&crate::TemplateOptions> {
        self.templates.as_ref().map(|compiled| &compiled.options)
    }

    /// Explicit association. Errors are surfaced by discovery before workers start.
    pub fn association(&self, path: &Path) -> Result<Option<&'static str>> {
        let relative = self
            .repository_root
            .as_ref()
            .and_then(|root| path.strip_prefix(root).ok())
            .unwrap_or(path);
        let association = self
            .templates
            .as_ref()
            .map(|compiled| compiled.association(relative))
            .transpose()?
            .flatten();
        if association.is_some()
            && matches!(
                classify_ingest_path(path),
                IngestRoute::Manifest | IngestRoute::Workflow
            )
        {
            return Err(Error::ConfigError(format!(
                "association cannot override protected ingest route: {}",
                path.display()
            )));
        }
        Ok(association)
    }

    /// Possible literal targets retained by the existing walk. Outside recognized
    /// role roots they become Jinja only when an exact source reference selects them.
    pub fn is_template_pool_path(&self, path: &Path) -> bool {
        if crate::template_options::alternate_template_dialect(path) {
            return false;
        }
        !matches!(
            classify_ingest_path(path),
            IngestRoute::Manifest | IngestRoute::Workflow
        ) && (path
            .components()
            .any(|component| component.as_os_str() == "templates")
            || self
                .template_roots()
                .iter()
                .any(|root| path.starts_with(root)))
    }

    /// YAML must reach the worker before a final language filter is applied.
    pub fn is_ansible_candidate(&self, path: &Path) -> bool {
        self.has_plugin("ansible")
            && !matches!(
                classify_ingest_path(path),
                IngestRoute::Manifest | IngestRoute::Workflow
            )
            && (path
                .extension()
                .is_some_and(|ext| ext == "yaml" || ext == "yml")
                || self
                    .get_language_plugin("ansible")
                    .is_some_and(|p| p.matches_path(&path.to_string_lossy())))
    }

    /// Standalone task files are promoted only when a literal include selects
    /// them. Inventory and protected routes retain their existing semantics.
    pub fn is_standalone_task_pool_path(&self, path: &Path) -> bool {
        self.is_ansible_candidate(path)
            && !self.is_template_pool_path(path)
            && path
                .parent()
                .is_some_and(|parent| parent.file_name().is_some_and(|name| name == "tasks"))
            && crate::template_options::role_root(path).is_none()
            && !path.components().any(|part| {
                matches!(
                    part.as_os_str().to_str(),
                    Some("inventory" | "inventories" | "group_vars" | "host_vars")
                )
            })
            && self.association(path).ok().flatten().is_none()
    }

    /// Register a language plugin
    pub fn register_language_plugin(&mut self, plugin: Arc<dyn LanguagePlugin>) {
        let id = plugin.language_id().to_string();

        self.language_plugins
            .insert(id.clone(), Arc::clone(&plugin));

        for ext in plugin.file_extensions() {
            self.extension_map
                .insert(ext.to_string(), Arc::clone(&plugin));
        }
    }

    /// Register a config format plugin
    pub fn register_config_plugin(&mut self, plugin: Arc<dyn ConfigFormatPlugin>) {
        let id = plugin.format_id().to_string();

        self.config_plugins.insert(id.clone(), Arc::clone(&plugin));

        for ext in plugin.file_extensions() {
            self.config_extension_map
                .insert(ext.to_string(), Arc::clone(&plugin));
        }
    }

    /// Get a language plugin by ID
    pub fn get_language_plugin(&self, language_id: &str) -> Option<Arc<dyn LanguagePlugin>> {
        self.language_plugins.get(language_id).cloned()
    }

    /// Get a config format plugin by ID
    pub fn get_config_plugin(&self, format_id: &str) -> Option<Arc<dyn ConfigFormatPlugin>> {
        self.config_plugins.get(format_id).cloned()
    }

    /// Get a language plugin for a file path.
    ///
    /// Manifest ingest routes never resolve to a language plugin so basenames
    /// like `build.gradle.kts` stay exclusive to Dependency extractors even when
    /// a Kotlin/Groovy plugin registers `.kts` / `.gradle`. Ordinary sources that
    /// fall through to [`IngestRoute::Ignore`] (e.g. `.kt`) still use language plugins.
    ///
    /// For `.m` files, peeks on-disk content (when readable) to choose between
    /// `objc` and `matlab`; see [`Self::get_plugin_for_file_with_source`].
    pub fn get_plugin_for_file(&self, file_path: &Path) -> Result<Arc<dyn LanguagePlugin>> {
        if file_path.extension().and_then(|e| e.to_str()) == Some("m")
            && let Ok(mut file) = std::fs::File::open(file_path)
        {
            use std::io::Read;
            let mut buf = [0u8; 4096];
            if let Ok(n) = file.read(&mut buf) {
                return self.get_plugin_for_file_with_source(file_path, &buf[..n]);
            }
        }
        self.get_plugin_for_file_path_only(file_path)
    }

    /// Like [`Self::get_plugin_for_file`], but uses `source` to disambiguate `.m`
    /// (Objective-C vs MATLAB) without a second disk read.
    pub fn get_plugin_for_file_with_source(
        &self,
        file_path: &Path,
        source: &[u8],
    ) -> Result<Arc<dyn LanguagePlugin>> {
        if matches!(
            classify_ingest_path(file_path),
            IngestRoute::Manifest | IngestRoute::Workflow
        ) {
            return Err(Error::UnsupportedLanguage(
                file_path.to_string_lossy().to_string(),
            ));
        }

        if file_path.extension().and_then(|e| e.to_str()) == Some("m") {
            let id = crate::sniff_objc_vs_matlab(source);
            if let Some(plugin) = self.get_language_plugin(id) {
                return Ok(plugin);
            }
        }

        self.get_plugin_for_file_path_only(file_path)
    }

    /// Path-only language id for walk admission; no source read.
    pub fn get_plugin_for_file_path_only_for_discovery(&self, path: &Path) -> Option<String> {
        self.get_plugin_for_file_path_only(path)
            .ok()
            .map(|plugin| plugin.language_id().to_owned())
    }

    fn get_plugin_for_file_path_only(&self, file_path: &Path) -> Result<Arc<dyn LanguagePlugin>> {
        if matches!(
            classify_ingest_path(file_path),
            IngestRoute::Manifest | IngestRoute::Workflow
        ) {
            return Err(Error::UnsupportedLanguage(
                file_path.to_string_lossy().to_string(),
            ));
        }

        if let Some(id) = self.association(file_path)?
            && let Some(plugin) = self.get_language_plugin(id)
        {
            return Ok(plugin);
        }
        if (file_path
            .extension()
            .is_some_and(|ext| matches!(ext.to_str(), Some("j2" | "jinja" | "jinja2")))
            || crate::template_options::role_template(file_path))
            && let Some(plugin) = self.get_language_plugin("jinja")
        {
            return Ok(plugin);
        }

        // `.m` is shared by Objective-C and MATLAB. Without source bytes, default to
        // objc — `get_plugin_for_file_with_source` / on-disk peek disambiguates.
        // Do not use `language_plugin_for_path` here: both plugins `matches_path` `.m`
        // and HashMap iteration order is unstable.
        if file_path.extension().and_then(|e| e.to_str()) == Some("m") {
            return self
                .get_language_plugin("objc")
                .or_else(|| self.extension_map.get("m").cloned())
                .ok_or_else(|| Error::UnsupportedLanguage("m".into()));
        }

        let path_str = file_path.to_string_lossy().replace('\\', "/");

        if let Some(plugin) = self.language_plugin_for_path(&path_str) {
            return Ok(plugin);
        }

        if let Some(ext) = file_path.extension().and_then(|e| e.to_str()) {
            self.extension_map
                .get(ext)
                .cloned()
                .ok_or_else(|| Error::UnsupportedLanguage(ext.to_string()))
        } else {
            Err(Error::UnsupportedLanguage(
                file_path.to_string_lossy().to_string(),
            ))
        }
    }

    /// Find a registered language plugin that claims `path` via [`LanguagePlugin::matches_path`].
    ///
    /// Path-heuristic plugins (empty [`LanguagePlugin::file_extensions`]) are checked first so
    /// IaC/CI routing wins over generic extension handlers (e.g. chef vs ruby on `.rb`).
    fn language_plugin_for_path(&self, path: &str) -> Option<Arc<dyn LanguagePlugin>> {
        if let Some(plugin) = self
            .language_plugins
            .values()
            .filter(|plugin| plugin.file_extensions().is_empty())
            .find(|plugin| plugin.matches_path(path))
        {
            return Some(Arc::clone(plugin));
        }
        self.language_plugins
            .values()
            .find(|plugin| plugin.matches_path(path))
            .cloned()
    }

    /// Get a config plugin for a file path
    pub fn get_config_plugin_for_file(
        &self,
        file_path: &Path,
    ) -> Result<Arc<dyn ConfigFormatPlugin>> {
        if self.get_plugin_for_file_path_only(file_path).is_ok() {
            return Err(Error::UnsupportedLanguage(
                file_path.to_string_lossy().to_string(),
            ));
        }

        match classify_ingest_path(file_path) {
            IngestRoute::Manifest | IngestRoute::Ignore => {
                return Err(Error::UnsupportedLanguage(
                    file_path.to_string_lossy().to_string(),
                ));
            }
            // Workflow uses YAML config extractors until Job/BuildStep emitters land.
            IngestRoute::Config | IngestRoute::Workflow => {}
        }

        if let Some(ext) = file_path.extension().and_then(|e| e.to_str()) {
            self.config_extension_map
                .get(ext)
                .cloned()
                .ok_or_else(|| Error::UnsupportedLanguage(ext.to_string()))
        } else {
            Err(Error::UnsupportedLanguage(
                file_path.to_string_lossy().to_string(),
            ))
        }
    }

    /// True when the path is a build manifest (Dependency extract route).
    pub fn is_manifest_file(&self, file_path: &Path) -> bool {
        classify_ingest_path(file_path) == IngestRoute::Manifest
    }

    /// Check if a file can be processed (code, config/workflow, or manifest).
    pub fn can_process_file(&self, file_path: &Path) -> bool {
        if classify_ingest_path(file_path) == IngestRoute::Manifest {
            return true;
        }
        if self.get_plugin_for_file_path_only(file_path).is_ok() {
            return true;
        }
        match classify_ingest_path(file_path) {
            IngestRoute::Config | IngestRoute::Workflow => {
                self.get_config_plugin_for_file(file_path).is_ok()
            }
            IngestRoute::Ignore | IngestRoute::Manifest => false,
        }
    }

    /// List all supported language IDs
    pub fn supported_languages(&self) -> Vec<String> {
        self.language_plugins.keys().cloned().collect()
    }

    /// List all supported config formats
    pub fn supported_config_formats(&self) -> Vec<String> {
        self.config_plugins.keys().cloned().collect()
    }

    /// List all supported file extensions
    pub fn supported_extensions(&self) -> Vec<String> {
        let mut extensions: Vec<String> = self
            .extension_map
            .keys()
            .chain(self.config_extension_map.keys())
            .cloned()
            .collect();
        extensions.sort();
        extensions.dedup();
        extensions
    }

    /// Get statistics about registered plugins
    pub fn stats(&self) -> RegistryStats {
        RegistryStats {
            language_plugins: self.language_plugins.len(),
            config_plugins: self.config_plugins.len(),
            total_extensions: self.supported_extensions().len(),
        }
    }
}

impl ConfigFormatRegistrar for LanguageRegistry {
    fn register_config_plugin(&mut self, plugin: Arc<dyn ConfigFormatPlugin>) {
        LanguageRegistry::register_config_plugin(self, plugin);
    }
}

impl Default for LanguageRegistry {
    fn default() -> Self {
        Self::with_config_formats()
    }
}

/// Statistics about the registry
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegistryStats {
    /// Number of language plugins
    pub language_plugins: usize,

    /// Number of config format plugins
    pub config_plugins: usize,

    /// Total number of supported file extensions
    pub total_extensions: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_registry() {
        let registry = LanguageRegistry::empty();
        let stats = registry.stats();
        assert_eq!(stats.language_plugins, 0);
        assert_eq!(stats.config_plugins, 0);
    }

    #[test]
    fn test_with_config_formats() {
        let registry = LanguageRegistry::with_config_formats();
        let stats = registry.stats();
        assert_eq!(stats.language_plugins, 0);
        assert_eq!(stats.config_plugins, 5);
    }

    #[test]
    fn test_config_plugins() {
        let registry = LanguageRegistry::with_config_formats();
        assert!(registry.get_config_plugin("yaml").is_some());
        assert!(registry.get_config_plugin("json").is_some());
        assert!(registry.get_config_plugin("toml").is_some());
    }

    #[test]
    fn test_can_process_config_files() {
        let registry = LanguageRegistry::with_config_formats();
        assert!(registry.can_process_file(Path::new("config.yaml")));
        assert!(registry.can_process_file(Path::new("config.yml")));
        assert!(registry.can_process_file(Path::new("config.json")));
        assert!(registry.can_process_file(Path::new("config.toml")));
    }

    #[test]
    fn test_manifest_routing_excludes_config_plugin() {
        let registry = LanguageRegistry::with_config_formats();
        assert!(registry.can_process_file(Path::new("pom.xml")));
        assert!(registry.is_manifest_file(Path::new("pom.xml")));
        assert!(
            registry
                .get_config_plugin_for_file(Path::new("pom.xml"))
                .is_err()
        );

        assert!(registry.can_process_file(Path::new("Cargo.toml")));
        assert!(registry.is_manifest_file(Path::new("Cargo.toml")));
        assert!(
            registry
                .get_config_plugin_for_file(Path::new("Cargo.toml"))
                .is_err()
        );

        assert!(registry.can_process_file(Path::new("package.json")));
        assert!(registry.is_manifest_file(Path::new("package.json")));
        assert!(
            registry
                .get_config_plugin_for_file(Path::new("package.json"))
                .is_err()
        );

        // Gradle Kotlin DSL build script stays Manifest even if a future
        // language plugin registers `.kts` (language lookup is blocked).
        assert!(registry.is_manifest_file(Path::new("app/build.gradle.kts")));
        assert!(
            registry
                .get_plugin_for_file(Path::new("app/build.gradle.kts"))
                .is_err()
        );
        assert!(registry.is_manifest_file(Path::new("build.gradle")));
        assert!(
            registry
                .get_plugin_for_file(Path::new("build.gradle"))
                .is_err()
        );
    }

    #[test]
    fn test_random_xml_not_processed() {
        let registry = LanguageRegistry::with_config_formats();
        assert!(!registry.can_process_file(Path::new("docs/foo.xml")));
    }
}
