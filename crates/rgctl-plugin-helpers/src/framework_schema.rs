//! Shared framework catalog schema (compiled by `build.rs`, loaded by `framework.rs`).
//!
//! TOML format (one file per language under `frameworks/`):
//! ```toml
//! language = "python"
//! [[rules]]
//! id = "fastapi.flask.route_decorator"
//! framework = "fastapi"
//! kind = "RouteDecorator"
//! tags = ["role:endpoint", "protocol:http"]
//! vocab = "endpoint endpoints route routes http rest api handler"
//!
//! [[frameworks]]
//! id = "fastapi"
//! import_prefixes = ["fastapi."]
//! [frameworks.writeback.method_route.GET]
//! template = "@router.get(\"{path}\")"
//! imports = ["fastapi"]
//! ```

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Compiled multi-language framework catalog written to `OUT_DIR/frameworks.bin`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompiledFrameworkCatalog {
    pub version: u32,
    pub languages: Vec<LanguageFrameworkSpec>,
}

/// Framework rules for a single language id (e.g. `"python"`, `"java"`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LanguageFrameworkSpec {
    pub language: String,
    pub rules: Vec<FrameworkRuleSpec>,
    /// Bidirectional writeback emitter profiles (`[[frameworks]]` in TOML).
    #[serde(default)]
    pub frameworks: Vec<FrameworkWritebackProfile>,
}

/// Bidirectional writeback profile for one framework id (e.g. `"spring"`, `"quarkus"`).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FrameworkWritebackProfile {
    /// Framework identifier (e.g., "spring", "quarkus", "fastapi").
    pub id: String,
    /// Import prefixes owned by this framework (e.g., `["org.springframework."]`).
    #[serde(default)]
    pub import_prefixes: Vec<String>,
    /// Direct namespace prefix rewrites (e.g., `{"javax." => "jakarta."}`).
    #[serde(default)]
    pub namespace_rewrites: BTreeMap<String, String>,
    /// Supertypes (`Extends`/`Implements`) that require agent worklist entries.
    #[serde(default)]
    pub worklist_supertype_triggers: Vec<String>,
    /// Callees (`Calls`) that require agent worklist entries.
    #[serde(default)]
    pub worklist_call_triggers: Vec<String>,
    /// Emitter templates keyed by normalized graph concept.
    #[serde(default)]
    pub writeback: FrameworkWritebackTemplates,
}

/// Emitter templates for normalized graph concepts → target syntax.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FrameworkWritebackTemplates {
    pub controller_stereotype: Option<AnnotationTemplate>,
    pub service_stereotype: Option<AnnotationTemplate>,
    pub repository_stereotype: Option<AnnotationTemplate>,
    pub field_inject: Option<AnnotationTemplate>,
    pub config_inject: Option<AnnotationTemplate>,
    pub transactional: Option<AnnotationTemplate>,
    pub class_route_prefix: Option<AnnotationTemplate>,
    /// Keyed by HTTP verb (`"GET"`, `"POST"`, …).
    #[serde(default)]
    pub method_route: BTreeMap<String, AnnotationTemplate>,
    /// Keyed by parameter binding kind (`"path"`, `"query"`, `"header"`, `"body"`).
    #[serde(default)]
    pub param_bind: BTreeMap<String, AnnotationTemplate>,
}

/// One annotation/decorator/macro template plus required imports.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnnotationTemplate {
    /// Template string supporting `{path}`, `{key}`, `{default_kv}`, `{name}`, `{indent}`.
    pub template: String,
    /// Required import FQN(s) to insert in Wave 1 when this template is emitted.
    #[serde(default)]
    pub imports: Vec<String>,
}

/// One declarative matching rule.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FrameworkRuleSpec {
    pub id: String,
    pub framework: String,
    pub kind: FrameworkRuleKind,
    #[serde(default)]
    pub class_prefix_annotations: Vec<String>,
    #[serde(default)]
    pub method_annotations: Vec<MethodRouteAnnotation>,
    #[serde(default)]
    pub annotations: Vec<String>,
    #[serde(default)]
    pub call_callees: Vec<String>,
    #[serde(default)]
    pub router_callees: Vec<MethodRouteAnnotation>,
    #[serde(default)]
    pub wrapper_types: Vec<String>,
    #[serde(default)]
    pub provider_calls: Vec<String>,
    #[serde(default)]
    pub target_symbols: Vec<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub vocab: String,
    #[serde(default)]
    pub target_vocab: Option<String>,
}

/// HTTP method binding for a decorator or router call name.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MethodRouteAnnotation {
    pub name: String,
    #[serde(default)]
    pub method: Option<String>,
    #[serde(default)]
    pub method_arg: Option<String>,
    #[serde(default)]
    pub default_method: Option<String>,
}

/// Matcher primitive kind (PascalCase in TOML: `kind = "RouteDecorator"`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FrameworkRuleKind {
    RouteDecorator,
    RouteCall,
    FieldAnnotationInject,
    CtorOrStereotypeInject,
    ParamWrapperInject,
    ParamCallInject,
    ConceptTagger,
}
