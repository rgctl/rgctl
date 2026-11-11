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
//! ```

use serde::{Deserialize, Serialize};

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
