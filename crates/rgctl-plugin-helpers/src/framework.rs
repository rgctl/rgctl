//! Declarative framework route/DI catalog matcher.
//!
//! Rules are compiled from `frameworks/*.toml` into `OUT_DIR/frameworks.bin` by `build.rs`.
//! [`apply_framework_catalog`] reuses already-extracted symbols/relations and an optional
//! existing tree-sitter root — it never calls `Parser::new()`.

#[path = "framework_schema.rs"]
mod framework_schema;

pub use framework_schema::{
    CompiledFrameworkCatalog, FrameworkRuleKind, FrameworkRuleSpec, LanguageFrameworkSpec,
    MethodRouteAnnotation,
};

use framework_schema::CompiledFrameworkCatalog as Catalog;
use rgctl_plugin_api::{Relation, RelationType, SourceLocation, Symbol, SymbolType};
use rustc_hash::{FxHashMap, FxHashSet};
use serde_json::{Map, Value, json};
use std::path::Path;
use std::sync::OnceLock;
use tree_sitter::Node;

/// Process-wide built-in catalog (indexed by language id).
static BUILTIN: OnceLock<RuntimeFrameworkCatalog> = OnceLock::new();

/// Indexed rules for one language with O(1) trigger sets for fast-reject.
#[derive(Debug, Clone)]
pub struct IndexedLanguageRules {
    pub rules: Vec<FrameworkRuleSpec>,
    pub annotation_triggers: FxHashSet<String>,
    pub call_triggers: FxHashSet<String>,
    pub wrapper_types: FxHashSet<String>,
    pub provider_calls: FxHashSet<String>,
    pub router_callee_names: FxHashSet<String>,
    pub has_route_call: bool,
    pub has_param_wrapper: bool,
    pub has_param_call: bool,
}

/// Runtime catalog keyed by language id.
#[derive(Debug, Clone)]
pub struct RuntimeFrameworkCatalog {
    pub version: u32,
    pub by_language: FxHashMap<String, IndexedLanguageRules>,
}

impl RuntimeFrameworkCatalog {
    fn from_compiled(catalog: Catalog) -> Self {
        let mut by_language = FxHashMap::default();
        for lang in catalog.languages {
            by_language.insert(lang.language.clone(), index_language(&lang));
        }
        Self {
            version: catalog.version,
            by_language,
        }
    }
}

fn index_language(spec: &LanguageFrameworkSpec) -> IndexedLanguageRules {
    let mut annotation_triggers = FxHashSet::default();
    let mut call_triggers = FxHashSet::default();
    let mut wrapper_types = FxHashSet::default();
    let mut provider_calls = FxHashSet::default();
    let mut router_callee_names = FxHashSet::default();
    let mut has_route_call = false;
    let mut has_param_wrapper = false;
    let mut has_param_call = false;

    for rule in &spec.rules {
        for a in &rule.class_prefix_annotations {
            annotation_triggers.insert(simple_name(a));
        }
        for m in &rule.method_annotations {
            annotation_triggers.insert(simple_name(&m.name));
        }
        for a in &rule.annotations {
            annotation_triggers.insert(simple_name(a));
        }
        for c in &rule.call_callees {
            call_triggers.insert(simple_name(c));
        }
        for r in &rule.router_callees {
            router_callee_names.insert(simple_name(&r.name));
        }
        for w in &rule.wrapper_types {
            wrapper_types.insert(simple_name(w));
        }
        for p in &rule.provider_calls {
            provider_calls.insert(simple_name(p));
        }
        match rule.kind {
            FrameworkRuleKind::RouteCall => has_route_call = true,
            FrameworkRuleKind::ParamWrapperInject => has_param_wrapper = true,
            FrameworkRuleKind::ParamCallInject => has_param_call = true,
            _ => {}
        }
    }

    IndexedLanguageRules {
        rules: spec.rules.clone(),
        annotation_triggers,
        call_triggers,
        wrapper_types,
        provider_calls,
        router_callee_names,
        has_route_call,
        has_param_wrapper,
        has_param_call,
    }
}

fn builtin_catalog() -> &'static RuntimeFrameworkCatalog {
    BUILTIN.get_or_init(|| {
        let bytes: &'static [u8] = include_bytes!(env!("RGCTL_FRAMEWORKS_BIN"));
        let catalog: Catalog =
            bincode::deserialize(bytes).expect("deserialize built-in frameworks.bin");
        RuntimeFrameworkCatalog::from_compiled(catalog)
    })
}

/// Load built-in catalog, optionally merging `.rgctl/frameworks.d/*.toml` with BLAKE3 cache.
pub fn load_with_repo_overrides(repo_root: Option<&Path>) -> RuntimeFrameworkCatalog {
    let builtin = builtin_catalog().clone();
    let Some(root) = repo_root else {
        return builtin;
    };
    let override_dir = root.join(".rgctl").join("frameworks.d");
    if !override_dir.is_dir() {
        return builtin;
    }

    let Ok(entries) = std::fs::read_dir(&override_dir) else {
        return builtin;
    };
    let mut paths: Vec<_> = entries
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("toml"))
        .collect();
    paths.sort();
    if paths.is_empty() {
        return builtin;
    }

    let mut hasher = blake3::Hasher::new();
    for path in &paths {
        hasher.update(path.to_string_lossy().as_bytes());
        if let Ok(bytes) = std::fs::read(path) {
            hasher.update(&bytes);
        }
    }
    let digest = hasher.finalize().to_hex();
    let cache_dir = root.join(".rgctl").join("cache");
    let cache_bin = cache_dir.join("frameworks.bin");
    let cache_hash = cache_dir.join("frameworks.blake3");

    if cache_bin.is_file()
        && cache_hash.is_file()
        && std::fs::read_to_string(&cache_hash)
            .ok()
            .is_some_and(|h| h.trim() == digest.as_str())
        && let Ok(bytes) = std::fs::read(&cache_bin)
        && let Ok(catalog) = bincode::deserialize::<Catalog>(&bytes)
    {
        return RuntimeFrameworkCatalog::from_compiled(catalog);
    }

    // Merge: start from built-in compiled catalog, replace/extend by language from overrides.
    let bytes: &'static [u8] = include_bytes!(env!("RGCTL_FRAMEWORKS_BIN"));
    let mut catalog: Catalog =
        bincode::deserialize(bytes).expect("deserialize built-in frameworks.bin");
    for path in &paths {
        let Ok(text) = std::fs::read_to_string(path) else {
            continue;
        };
        let Ok(spec) = toml::from_str::<LanguageFrameworkSpec>(&text) else {
            // Overrides require toml at runtime — soft-skip malformed files.
            // TODO: surface parse errors when a diagnostics channel exists.
            continue;
        };
        if let Some(existing) = catalog
            .languages
            .iter_mut()
            .find(|l| l.language == spec.language)
        {
            existing.rules.extend(spec.rules);
        } else {
            catalog.languages.push(spec);
        }
    }

    let _ = std::fs::create_dir_all(&cache_dir);
    if let Ok(out) = bincode::serialize(&catalog) {
        let _ = std::fs::write(&cache_bin, &out);
        let _ = std::fs::write(&cache_hash, digest.as_str());
    }
    RuntimeFrameworkCatalog::from_compiled(catalog)
}

/// Join route prefix + subpath into a canonical path (`/a/b`, no trailing slash unless `/`).
pub fn normalize_route_path(prefix: &str, subpath: &str) -> String {
    let trim_quotes = |s: &str| {
        s.trim()
            .trim_matches(|c| c == '"' || c == '\'' || c == '`')
            .trim()
            .to_string()
    };
    let p = trim_quotes(prefix);
    let s = trim_quotes(subpath);
    let mut parts: Vec<&str> = Vec::new();
    for piece in [p.as_str(), s.as_str()] {
        for seg in piece.split('/') {
            if !seg.is_empty() {
                parts.push(seg);
            }
        }
    }
    if parts.is_empty() {
        return "/".to_string();
    }
    format!("/{}", parts.join("/"))
}

/// Format `(display, route::DISPLAY)` for an HTTP method + path.
pub fn format_route_target(method: &str, path: &str) -> (String, String) {
    let verb = method.trim().to_uppercase();
    let verb = if verb.is_empty() {
        "ANY".to_string()
    } else {
        verb
    };
    let norm = if path.starts_with('/') {
        path.to_string()
    } else {
        normalize_route_path("", path)
    };
    let display = format!("{verb} {norm}");
    let hint = format!("route::{display}");
    (display, hint)
}

/// Apply compiled framework rules for `lang_id` onto extracted symbols/relations.
///
/// Fast-rejects when the language has no rules, when both vectors are empty, or when no
/// annotation/call/param triggers match — pure C files pay ~0 AST walk.
pub fn apply_framework_catalog(
    lang_id: &str,
    source: &[u8],
    root_node: Option<Node<'_>>,
    symbols: &mut [Symbol],
    relations: &mut Vec<Relation>,
) {
    let catalog = builtin_catalog();
    let Some(indexed) = catalog.by_language.get(lang_id) else {
        return;
    };
    if symbols.is_empty() && relations.is_empty() {
        return;
    }

    let mut has_ann = false;
    let mut has_call = false;
    let mut has_router = false;
    let mut has_wrapper_param = false;
    let mut has_provider_param = false;
    let has_path_literal = indexed.has_route_call && source_has_route_path_literal(source);

    for rel in relations.iter() {
        match rel.relation_type {
            RelationType::AnnotatedWith => {
                let simple = annotation_simple_name(&rel.to);
                if indexed.annotation_triggers.contains(simple.as_str()) {
                    has_ann = true;
                }
            }
            RelationType::Calls => {
                let simple = annotation_simple_name(&rel.to);
                if indexed.call_triggers.contains(simple.as_str()) {
                    has_call = true;
                }
                if has_path_literal && indexed.router_callee_names.contains(simple.as_str()) {
                    has_router = true;
                }
            }
            _ => {}
        }
        if has_ann && has_call && has_router {
            break;
        }
    }
    // Top-level route registrations (e.g. Express `app.get('/x', fn)` at module scope)
    // may not emit a function-scoped `Calls` relation in every language plugin.
    if !has_router && has_path_literal && root_node.is_some() {
        has_router = true;
    }

    if indexed.has_param_wrapper || indexed.has_param_call {
        for sym in symbols.iter() {
            if !matches!(sym.symbol_type, SymbolType::Function | SymbolType::Method) {
                continue;
            }
            for param in &sym.parameters {
                if indexed.has_param_wrapper
                    && let Some(ty) = param.param_type.as_deref()
                    && type_mentions_wrappers(ty, &indexed.wrapper_types)
                {
                    has_wrapper_param = true;
                }
                if indexed.has_param_call {
                    if let Some(def) = param.default_value.as_deref()
                        && text_mentions_providers(def, &indexed.provider_calls)
                    {
                        has_provider_param = true;
                    }
                    if let Some(ty) = param.param_type.as_deref()
                        && text_mentions_providers(ty, &indexed.provider_calls)
                    {
                        has_provider_param = true;
                    }
                }
            }
            if has_wrapper_param && has_provider_param {
                break;
            }
        }
    }

    let any_trigger = has_ann || has_call || has_router || has_wrapper_param || has_provider_param;
    if !any_trigger {
        return;
    }

    let walk_route_calls = indexed.has_route_call && has_router;

    // Snapshot annotated/call relations for matching (we'll append new relations).
    let existing: Vec<Relation> = relations.clone();

    let mut new_relations: Vec<Relation> = Vec::new();

    for rule in &indexed.rules {
        match rule.kind {
            FrameworkRuleKind::RouteDecorator => {
                apply_route_decorator(rule, symbols, &existing, &mut new_relations);
            }
            FrameworkRuleKind::RouteCall => {
                if walk_route_calls {
                    apply_route_call(
                        rule,
                        source,
                        root_node,
                        symbols,
                        &existing,
                        &mut new_relations,
                    );
                }
            }
            FrameworkRuleKind::FieldAnnotationInject => {
                apply_field_annotation_inject(rule, symbols, &existing, &mut new_relations);
            }
            FrameworkRuleKind::CtorOrStereotypeInject => {
                apply_ctor_or_stereotype_inject(rule, symbols, &existing, &mut new_relations);
            }
            FrameworkRuleKind::ParamWrapperInject => {
                if has_wrapper_param {
                    apply_param_wrapper_inject(rule, symbols, &mut new_relations);
                }
            }
            FrameworkRuleKind::ParamCallInject => {
                if has_provider_param {
                    apply_param_call_inject(rule, symbols, &mut new_relations);
                }
            }
            FrameworkRuleKind::ConceptTagger => {
                apply_concept_tagger(rule, symbols, &existing);
            }
        }
    }

    // Stamp `target_vocab` onto same-file injected provider symbols immediately
    // (works in both in-memory and segmented spill modes).
    for rel in &new_relations {
        if rel.relation_type == RelationType::Injects
            && let Some(tv) = rel
                .metadata
                .get("target_vocab")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
            && let Some(target_sym) = find_symbol_mut(symbols, &rel.to)
        {
            stamp_tags_vocab(target_sym, &[], tv);
        }
    }

    relations.extend(new_relations);
}

fn apply_route_decorator(
    rule: &FrameworkRuleSpec,
    symbols: &mut [Symbol],
    existing: &[Relation],
    out: &mut Vec<Relation>,
) {
    let prefix_anns: FxHashSet<String> = rule
        .class_prefix_annotations
        .iter()
        .map(|a| simple_name(a))
        .collect();
    let method_map: FxHashMap<String, &MethodRouteAnnotation> = rule
        .method_annotations
        .iter()
        .map(|m| (simple_name(&m.name), m))
        .collect();

    // class_qn -> route prefix
    let mut class_prefixes: FxHashMap<String, String> = FxHashMap::default();
    for rel in existing {
        if rel.relation_type != RelationType::AnnotatedWith {
            continue;
        }
        let ann = annotation_simple_name(&rel.to);
        if prefix_anns.contains(&ann) {
            let path = first_string_arg(&rel.metadata).unwrap_or_default();
            class_prefixes.insert(rel.from.clone(), path);
        }
    }

    for rel in existing {
        if rel.relation_type != RelationType::AnnotatedWith {
            continue;
        }
        let ann = annotation_simple_name(&rel.to);
        let Some(mra) = method_map.get(&ann) else {
            continue;
        };
        // Class-level `@RequestMapping("/api")` / `@Controller("users")` are
        // prefixes only — do not emit a route for the type itself.
        if prefix_anns.contains(&ann) {
            let is_type = find_symbol(symbols, &rel.from).is_some_and(|s| {
                matches!(
                    s.symbol_type,
                    SymbolType::Class | SymbolType::Struct | SymbolType::Interface
                )
            });
            if is_type {
                continue;
            }
        }
        let method = resolve_http_method(mra, &rel.metadata);
        let sub = first_string_arg(&rel.metadata).unwrap_or_else(|| "/".to_string());
        let enclosing = enclosing_class_qn(&rel.from);
        let prefix = enclosing
            .as_ref()
            .and_then(|c| class_prefixes.get(c))
            .map(String::as_str)
            .unwrap_or("");
        let path = normalize_route_path(prefix, &sub);
        let (display, hint) = format_route_target(&method, &path);
        let loc = rel.location.clone();
        out.push(exposes_route_relation_with_spans(
            &rel.from,
            &display,
            &hint,
            &method,
            &path,
            rule,
            loc,
            rel.call_site_start_byte,
            rel.call_site_byte_len,
        ));
        if let Some(sym) = find_symbol_mut(symbols, &rel.from) {
            stamp_tags_vocab(sym, &rule.tags, &rule.vocab);
        }
        if let Some(cls) = enclosing
            && let Some(sym) = find_symbol_mut(symbols, &cls)
        {
            stamp_tags_vocab(sym, &rule.tags, &rule.vocab);
        }
    }
}

fn apply_route_call(
    rule: &FrameworkRuleSpec,
    source: &[u8],
    root_node: Option<Node<'_>>,
    symbols: &mut [Symbol],
    existing: &[Relation],
    out: &mut Vec<Relation>,
) {
    let router_map: FxHashMap<String, &MethodRouteAnnotation> = rule
        .router_callees
        .iter()
        .map(|m| (simple_name(&m.name), m))
        .collect();
    let mut emitted_routes: FxHashSet<(String, String)> = FxHashSet::default();

    for rel in existing {
        if rel.relation_type != RelationType::Calls {
            continue;
        }
        let callee = annotation_simple_name(&rel.to);
        let Some(mra) = router_map.get(&callee) else {
            continue;
        };
        // Only emit from a pre-existing `Calls` relation when the language plugin
        // explicitly populated both a route path starting with `/` in `arguments`
        // and an explicit `handler` metadata field. Never default generic `.get()`
        // or `.to()` calls without argument metadata to `"/"`.
        let Some(path_raw) = first_string_arg(&rel.metadata).filter(|p| p.starts_with('/')) else {
            continue;
        };
        let Some(handler_raw) = rel
            .metadata
            .get("handler")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
        else {
            continue;
        };
        let method = resolve_http_method(mra, &rel.metadata);
        let path = normalize_route_path("", &path_raw);
        let (display, hint) = format_route_target(&method, &path);
        let handler = find_symbol(symbols, handler_raw)
            .map(symbol_qn)
            .unwrap_or_else(|| handler_raw.to_string());
        if !emitted_routes.insert((handler.clone(), display.clone())) {
            continue;
        }
        let loc = rel.location.clone();
        out.push(exposes_route_relation(
            &handler, &display, &hint, &method, &path, rule, loc,
        ));
        if let Some(sym) = find_symbol_mut(symbols, &handler) {
            stamp_tags_vocab(sym, &rule.tags, &rule.vocab);
        }
    }

    // Zero-allocation AST walk for router call expressions (`app.get('/path', handler)`,
    // `r.GET("/path", handler)`, `.route("/path", get(handler))`, etc.).
    if let Some(root) = root_node {
        let default_file = symbols
            .first()
            .map(|s| s.location.file.clone())
            .unwrap_or_default();
        walk_call_nodes(
            root,
            source,
            &router_map,
            rule,
            symbols,
            &default_file,
            &mut emitted_routes,
            out,
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn walk_call_nodes(
    node: Node<'_>,
    source: &[u8],
    router_map: &FxHashMap<String, &MethodRouteAnnotation>,
    rule: &FrameworkRuleSpec,
    symbols: &mut [Symbol],
    default_file: &str,
    emitted_routes: &mut FxHashSet<(String, String)>,
    out: &mut Vec<Relation>,
) {
    let kind = node.kind();
    let is_call = kind.contains("call") || kind == "method_invocation" || kind == "member_call";
    if is_call
        && let Some((mra, method_override, path_lit, handler_name)) =
            extract_route_call_bits(node, source, router_map)
    {
        let method = method_override
            .or_else(|| mra.method.clone())
            .or_else(|| mra.default_method.clone())
            .unwrap_or_else(|| "ANY".to_string());
        let path = normalize_route_path("", &path_lit);
        let (display, hint) = format_route_target(&method, &path);
        let from = find_symbol(symbols, &handler_name)
            .map(symbol_qn)
            .unwrap_or(handler_name);
        if emitted_routes.insert((from.clone(), display.clone())) {
            let loc = SourceLocation {
                file: default_file.to_string(),
                start_line: node.start_position().row + 1,
                end_line: node.end_position().row + 1,
                start_column: node.start_position().column,
                end_column: node.end_position().column,
            };
            out.push(exposes_route_relation(
                &from, &display, &hint, &method, &path, rule, loc,
            ));
            if let Some(sym) = find_symbol_mut(symbols, &from) {
                stamp_tags_vocab(sym, &rule.tags, &rule.vocab);
            }
        }
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        walk_call_nodes(
            child,
            source,
            router_map,
            rule,
            symbols,
            default_file,
            emitted_routes,
            out,
        );
    }
}

fn extract_route_call_bits<'a>(
    node: Node<'_>,
    source: &[u8],
    router_map: &'a FxHashMap<String, &'a MethodRouteAnnotation>,
) -> Option<(&'a MethodRouteAnnotation, Option<String>, String, String)> {
    let slice = source.get(node.start_byte()..node.end_byte())?;
    let text = std::str::from_utf8(slice).ok()?;
    let (before_paren, after_open) = text.split_once('(')?;
    // Reject huge chained prefixes before '(' unless we take the trailing receiver.callee
    let tail = before_paren.lines().last().unwrap_or(before_paren).trim();
    let callee = tail
        .rsplit(['.', ':'])
        .next()?
        .trim()
        .trim_end_matches(|c: char| !c.is_alphanumeric() && c != '_');
    if callee.is_empty() {
        return None;
    }
    let mra = *router_map.get(callee)?;

    // Check receiver (if any) to reject non-router calls like `map.get(...)`, `this.svc.get(...)`,
    // `headers.get(...)`, `Reflect.get(...)`, `axios.get(...)`, `http.get(...)`.
    if let Some((recv_raw, _)) = tail.rsplit_once(['.', ':']) {
        let recv_last = recv_raw
            .trim_end_matches(':')
            .rsplit(['.', ':', ' ', '(', '\t'])
            .next()
            .unwrap_or("")
            .trim();
        if is_non_router_receiver(recv_last) {
            return None;
        }
    }

    let args_inside = after_open
        .rsplit_once(')')
        .map(|(a, _)| a)
        .unwrap_or(after_open);
    let args = split_top_level_commas(args_inside);
    // Every supported RouteCall registration requires at least 2 arguments:
    // e.g. `app.get("/users", listUsers)`, `r.HandleFunc("/items", h)`,
    // `.route("/users", get(list_users))`, `get "/users", to: "users#index"`.
    // 1-argument calls like `client.get("/users")` are HTTP client requests, not route bindings.
    if args.len() < 2 {
        return None;
    }

    // First or second argument must contain a string literal starting with '/'.
    let path_lit = extract_first_string_literal(&args[0])
        .filter(|p| p.starts_with('/'))
        .or_else(|| {
            args.get(1)
                .and_then(|a| extract_first_string_literal(a))
                .filter(|p| p.starts_with('/'))
        })?;

    // Extract handler from the remaining arguments (including Axum `get(list_users)` or
    // Laravel `[OrderController::class, 'index']` or Rails `to: "users#index"`).
    let (method_override, handler) = extract_handler_and_method_from_args(&args)?;
    Some((mra, method_override, path_lit, handler))
}

fn is_non_router_receiver(recv: &str) -> bool {
    matches!(
        recv,
        "this"
            | "self"
            | "super"
            | "map"
            | "dict"
            | "cache"
            | "headers"
            | "header"
            | "params"
            | "param"
            | "query"
            | "cookies"
            | "cookie"
            | "env"
            | "config"
            | "settings"
            | "req"
            | "request"
            | "res"
            | "response"
            | "session"
            | "store"
            | "storage"
            | "localStorage"
            | "sessionStorage"
            | "Reflect"
            | "Object"
            | "Map"
            | "Set"
            | "WeakMap"
            | "Array"
            | "Promise"
            | "axios"
            | "fetch"
            | "http"
            | "https"
            | "client"
            | "httpClient"
            | "form"
            | "searchParams"
    )
}

fn extract_handler_and_method_from_args(args: &[String]) -> Option<(Option<String>, String)> {
    let last = args.last()?.trim();

    // Axum / router verb wrapper: `.route("/users", get(list_users))`
    if let Some((fn_part, inner)) = last.split_once('(') {
        let verb_cand = fn_part
            .rsplit(['.', ':'])
            .next()
            .unwrap_or(fn_part)
            .trim()
            .to_uppercase();
        if matches!(
            verb_cand.as_str(),
            "GET" | "POST" | "PUT" | "DELETE" | "PATCH" | "OPTIONS" | "HEAD" | "ANY"
        ) {
            let handler_inner = inner
                .rsplit_once(')')
                .map(|(a, _)| a)
                .unwrap_or(inner)
                .trim();
            let h = handler_inner
                .rsplit(['.', ':'])
                .next()
                .unwrap_or(handler_inner)
                .trim_matches(|c: char| !c.is_alphanumeric() && c != '_');
            if !h.is_empty() {
                return Some((Some(verb_cand), h.to_string()));
            }
        }
    }

    // Rails `to: "users#index"` or `"users#index"`
    if let Some(lit) = extract_first_string_literal(last) {
        if let Some((_ctrl, action)) = lit.split_once('#') {
            let act = action.trim();
            if !act.is_empty() {
                return Some((None, act.to_string()));
            }
        }
        return None;
    }

    // Laravel `[OrderController::class, 'index']`
    if last.starts_with('[')
        && last.ends_with(']')
        && let Some(action) = extract_first_string_literal(last)
        && !action.is_empty()
    {
        return Some((None, action));
    }

    if last.starts_with('"') || last.starts_with('\'') || last.starts_with('`') {
        return None;
    }

    let name = last
        .rsplit(['.', ':'])
        .next()?
        .trim_matches(|c: char| !c.is_alphanumeric() && c != '_')
        .to_string();
    if name.is_empty() {
        None
    } else {
        Some((None, name))
    }
}

fn apply_field_annotation_inject(
    rule: &FrameworkRuleSpec,
    symbols: &mut [Symbol],
    existing: &[Relation],
    out: &mut Vec<Relation>,
) {
    let anns: FxHashSet<String> = rule.annotations.iter().map(|a| simple_name(a)).collect();
    for rel in existing {
        if rel.relation_type != RelationType::AnnotatedWith {
            continue;
        }
        let ann = annotation_simple_name(&rel.to);
        if !anns.contains(&ann) {
            continue;
        }
        // from is Class.field or field qn
        let (class_qn, field_name) = split_field_qn(&rel.from);
        let field_ty = resolve_field_type(symbols, &class_qn, field_name.as_deref(), &rel.from);
        let Some(ty) = field_ty else {
            continue;
        };
        let simple_ty = strip_generics_and_prefix(&ty);
        if is_primitive(&simple_ty) {
            continue;
        }
        let loc = rel.location.clone();
        out.push(injects_relation_with_spans(
            &class_qn,
            &simple_ty,
            Some("class"),
            rule,
            loc,
            Some(&ann),
            rel.call_site_start_byte,
            rel.call_site_byte_len,
        ));
        if let Some(sym) = find_symbol_mut(symbols, &class_qn) {
            stamp_tags_vocab(sym, &rule.tags, &rule.vocab);
        }
    }
}

fn apply_ctor_or_stereotype_inject(
    rule: &FrameworkRuleSpec,
    symbols: &mut [Symbol],
    existing: &[Relation],
    out: &mut Vec<Relation>,
) {
    let anns: FxHashSet<String> = rule.annotations.iter().map(|a| simple_name(a)).collect();
    let stereotype = [
        "RestController",
        "Controller",
        "Service",
        "Component",
        "Repository",
        "Configuration",
        "Injectable",
        "ApiController",
    ];
    let stereotype_set: FxHashSet<&str> = stereotype.iter().copied().collect();

    let mut stereotype_classes: FxHashSet<String> = FxHashSet::default();
    let mut annotated_ctors: FxHashSet<String> = FxHashSet::default();

    for rel in existing {
        if rel.relation_type != RelationType::AnnotatedWith {
            continue;
        }
        let ann = annotation_simple_name(&rel.to);
        if !anns.contains(&ann) {
            continue;
        }
        if stereotype_set.contains(ann.as_str()) {
            stereotype_classes.insert(rel.from.clone());
        } else {
            // Autowired/Inject on ctor, ctor parameter, or class itself.
            // Do NOT promote a field annotation (`Class.field`) to `Class`,
            // or non-constructor methods on `Class` would be misclassified.
            annotated_ctors.insert(rel.from.clone());
            if let Some(parent) = enclosing_class_qn(&rel.from)
                && (parent.ends_with(".<init>") || parent.ends_with(".constructor"))
            {
                annotated_ctors.insert(parent);
            }
        }
    }

    let mut emitted_injects: FxHashSet<(String, String)> = FxHashSet::default();
    let mut stamp_qns: Vec<String> = Vec::new();
    for sym in symbols.iter() {
        let qn = symbol_qn(sym);
        let is_class = matches!(
            sym.symbol_type,
            SymbolType::Class | SymbolType::Struct | SymbolType::Interface
        );
        let class_qn = if is_class {
            qn.clone()
        } else {
            enclosing_class_qn(&qn).unwrap_or_else(|| qn.clone())
        };

        let class_is_stereotype = stereotype_classes.contains(&class_qn)
            || existing.iter().any(|r| {
                r.relation_type == RelationType::AnnotatedWith
                    && r.from == class_qn
                    && stereotype_set.contains(annotation_simple_name(&r.to).as_str())
                    && anns.contains(&annotation_simple_name(&r.to))
            });

        let ctor_annotated = annotated_ctors.contains(&qn) || annotated_ctors.contains(&class_qn);
        let is_ctor = !is_class && is_constructor(sym, &class_qn);

        let should_extract_params = if is_class {
            (class_is_stereotype || ctor_annotated) && !sym.parameters.is_empty()
        } else {
            is_ctor && (class_is_stereotype || ctor_annotated)
        };
        if !should_extract_params {
            continue;
        }

        let mut stamped = false;
        for param in &sym.parameters {
            let Some(ty) = param.param_type.as_deref() else {
                continue;
            };
            let simple_ty = strip_generics_and_prefix(ty);
            if is_primitive(&simple_ty) || simple_ty.is_empty() {
                continue;
            }
            if emitted_injects.insert((class_qn.clone(), simple_ty.clone())) {
                out.push(injects_relation(
                    &class_qn,
                    &simple_ty,
                    Some("class"),
                    rule,
                    sym.location.clone(),
                    None,
                ));
            }
            stamped = true;
        }
        if stamped {
            stamp_qns.push(qn);
            if class_qn != symbol_qn(sym) {
                stamp_qns.push(class_qn);
            }
        }
    }
    for qn in stamp_qns {
        if let Some(sym) = find_symbol_mut(symbols, &qn) {
            stamp_tags_vocab(sym, &rule.tags, &rule.vocab);
        }
    }
}

fn apply_param_wrapper_inject(
    rule: &FrameworkRuleSpec,
    symbols: &mut [Symbol],
    out: &mut Vec<Relation>,
) {
    let wrappers: FxHashSet<String> = rule.wrapper_types.iter().map(|w| simple_name(w)).collect();
    for sym in symbols.iter_mut() {
        if !matches!(sym.symbol_type, SymbolType::Function | SymbolType::Method) {
            continue;
        }
        let qn = symbol_qn(sym);
        let mut stamped = false;
        for param in &sym.parameters {
            let Some(ty) = param.param_type.as_deref() else {
                continue;
            };
            if let Some(inner) = unwrap_wrapper_type(ty, &wrappers) {
                out.push(injects_relation(
                    &qn,
                    &inner,
                    Some("struct"),
                    rule,
                    sym.location.clone(),
                    None,
                ));
                stamped = true;
            }
        }
        if stamped {
            stamp_tags_vocab(sym, &rule.tags, &rule.vocab);
        }
    }
}

fn apply_param_call_inject(
    rule: &FrameworkRuleSpec,
    symbols: &mut [Symbol],
    out: &mut Vec<Relation>,
) {
    let providers: FxHashSet<String> = rule.provider_calls.iter().map(|p| simple_name(p)).collect();
    for sym in symbols.iter_mut() {
        if !matches!(sym.symbol_type, SymbolType::Function | SymbolType::Method) {
            continue;
        }
        let qn = symbol_qn(sym);
        let mut stamped = false;
        for param in &sym.parameters {
            let haystacks = [param.default_value.as_deref(), param.param_type.as_deref()];
            for hay in haystacks.into_iter().flatten() {
                if let Some(provider) = extract_provider_call(hay, &providers) {
                    out.push(injects_relation(
                        &qn,
                        &provider,
                        Some("function"),
                        rule,
                        sym.location.clone(),
                        None,
                    ));
                    stamped = true;
                }
            }
        }
        if stamped {
            stamp_tags_vocab(sym, &rule.tags, &rule.vocab);
        }
    }
}

fn apply_concept_tagger(rule: &FrameworkRuleSpec, symbols: &mut [Symbol], existing: &[Relation]) {
    let anns: FxHashSet<String> = rule.annotations.iter().map(|a| simple_name(a)).collect();
    let callees: FxHashSet<String> = rule.call_callees.iter().map(|c| simple_name(c)).collect();

    let mut matched: FxHashSet<String> = FxHashSet::default();
    for rel in existing {
        match rel.relation_type {
            RelationType::AnnotatedWith if !anns.is_empty() => {
                let ann = annotation_simple_name(&rel.to);
                if anns.contains(&ann) {
                    matched.insert(rel.from.clone());
                }
            }
            RelationType::Calls if !callees.is_empty() => {
                let c = annotation_simple_name(&rel.to);
                if callees.contains(&c) {
                    matched.insert(rel.from.clone());
                }
            }
            _ => {}
        }
    }

    for name in matched {
        if let Some(sym) = find_symbol_mut(symbols, &name) {
            stamp_tags_vocab(sym, &rule.tags, &rule.vocab);
        }
    }
}

// --- helpers -----------------------------------------------------------------

fn exposes_route_relation(
    from: &str,
    display: &str,
    hint: &str,
    method: &str,
    path: &str,
    rule: &FrameworkRuleSpec,
    loc: SourceLocation,
) -> Relation {
    exposes_route_relation_with_spans(from, display, hint, method, path, rule, loc, None, None)
}

#[allow(clippy::too_many_arguments)]
fn exposes_route_relation_with_spans(
    from: &str,
    display: &str,
    hint: &str,
    method: &str,
    path: &str,
    rule: &FrameworkRuleSpec,
    loc: SourceLocation,
    call_site_start_byte: Option<u32>,
    call_site_byte_len: Option<u16>,
) -> Relation {
    Relation {
        from: from.to_string(),
        to: display.to_string(),
        relation_type: RelationType::ExposesRoute,
        location: loc,
        metadata: json!({
            "http_method": method,
            "route_path": path,
            "framework": rule.framework,
            "rule_id": rule.id,
        }),
        to_qualified_hint: Some(hint.to_string()),
        to_type_hint: Some("route".to_string()),
        call_site_start_byte,
        call_site_byte_len,
    }
}

fn injects_relation(
    from: &str,
    to: &str,
    to_type_hint: Option<&str>,
    rule: &FrameworkRuleSpec,
    loc: SourceLocation,
    annotation: Option<&str>,
) -> Relation {
    injects_relation_with_spans(from, to, to_type_hint, rule, loc, annotation, None, None)
}

#[allow(clippy::too_many_arguments)]
fn injects_relation_with_spans(
    from: &str,
    to: &str,
    to_type_hint: Option<&str>,
    rule: &FrameworkRuleSpec,
    loc: SourceLocation,
    annotation: Option<&str>,
    call_site_start_byte: Option<u32>,
    call_site_byte_len: Option<u16>,
) -> Relation {
    let mut meta = Map::new();
    meta.insert("framework".into(), Value::String(rule.framework.clone()));
    meta.insert("rule_id".into(), Value::String(rule.id.clone()));
    if let Some(tv) = &rule.target_vocab {
        meta.insert("target_vocab".into(), Value::String(tv.clone()));
    }
    if let Some(ann) = annotation.filter(|a| !a.is_empty()) {
        let ctx = if ann.starts_with('@') {
            ann.to_string()
        } else {
            format!("@{ann}")
        };
        meta.insert("call_context".into(), Value::String(ctx));
    }
    Relation {
        from: from.to_string(),
        to: to.to_string(),
        relation_type: RelationType::Injects,
        location: loc,
        metadata: Value::Object(meta),
        to_qualified_hint: None,
        to_type_hint: to_type_hint.map(str::to_string),
        call_site_start_byte,
        call_site_byte_len,
    }
}

/// Extract annotation simple name from `AnnotatedWith.to` (last segment after `.`).
pub fn annotation_simple_name(to: &str) -> String {
    simple_name(to)
}

fn simple_name(s: &str) -> String {
    s.trim()
        .trim_start_matches('@')
        .trim_start_matches('#')
        .rsplit(['.', '/', ':'])
        .next()
        .unwrap_or(s)
        .trim()
        .to_string()
}

/// Parse first string-ish token from metadata `arguments` / `annotation_args`
/// (prefer `path` then `value`, then raw `arguments` text).
pub fn first_string_arg(metadata: &Value) -> Option<String> {
    if let Some(map) = metadata.get("annotation_args").and_then(|v| v.as_object()) {
        for key in ["path", "value", "name"] {
            if let Some(s) = map
                .get(key)
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
            {
                return Some(s.to_string());
            }
        }
    }
    let args = metadata.get("arguments")?;
    if let Some(s) = args.as_str() {
        return extract_first_string_literal(s).or_else(|| {
            let t = s.trim().trim_matches(|c| c == '"' || c == '\'' || c == '`');
            if t.is_empty() {
                None
            } else {
                Some(t.to_string())
            }
        });
    }
    if let Some(arr) = args.as_array() {
        for v in arr {
            if let Some(s) = v.as_str() {
                let t = s.trim().trim_matches(|c| c == '"' || c == '\'' || c == '`');
                if !t.is_empty() {
                    return Some(t.to_string());
                }
            }
        }
    }
    None
}

fn extract_first_string_literal(s: &str) -> Option<String> {
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let q = bytes[i];
        if q == b'"' || q == b'\'' || q == b'`' {
            i += 1;
            let start = i;
            while i < bytes.len() && bytes[i] != q {
                if bytes[i] == b'\\' && i + 1 < bytes.len() {
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

fn resolve_http_method(mra: &MethodRouteAnnotation, metadata: &Value) -> String {
    if let Some(m) = &mra.method {
        return m.to_uppercase();
    }
    if mra.method_arg.is_some() {
        if let Some(method) = metadata
            .get("annotation_args")
            .and_then(|v| v.get("method"))
            .and_then(|v| v.as_str())
        {
            let upper = method.to_uppercase();
            for verb in ["GET", "POST", "PUT", "DELETE", "PATCH", "OPTIONS", "HEAD"] {
                if upper.contains(verb) {
                    return verb.to_string();
                }
            }
        }
        if let Some(args) = metadata.get("arguments").and_then(|v| v.as_str()) {
            // Look for GET/POST tokens in arguments
            for verb in ["GET", "POST", "PUT", "DELETE", "PATCH", "OPTIONS", "HEAD"] {
                if args.to_uppercase().contains(verb) {
                    return verb.to_string();
                }
            }
        }
    }
    mra.default_method
        .clone()
        .unwrap_or_else(|| "ANY".to_string())
        .to_uppercase()
}

fn stamp_tags_vocab(symbol: &mut Symbol, tags: &[String], vocab: &str) {
    let obj = ensure_object(&mut symbol.metadata);
    merge_space_tokens(obj, "framework_tags", tags.iter().map(String::as_str));
    let vocab_parts: Vec<&str> = vocab.split_whitespace().collect();
    merge_space_tokens(obj, "semantic_tags", vocab_parts.into_iter());
}

fn ensure_object(value: &mut Value) -> &mut Map<String, Value> {
    if !value.is_object() {
        *value = Value::Object(Map::new());
    }
    value.as_object_mut().expect("metadata object")
}

fn merge_space_tokens<'a>(
    obj: &mut Map<String, Value>,
    key: &str,
    tokens: impl Iterator<Item = &'a str>,
) {
    let mut set: FxHashSet<String> = FxHashSet::default();
    if let Some(existing) = obj.get(key).and_then(|v| v.as_str()) {
        for t in existing.split_whitespace() {
            set.insert(t.to_string());
        }
    }
    for t in tokens {
        let t = t.trim();
        if !t.is_empty() {
            set.insert(t.to_string());
        }
    }
    let mut sorted: Vec<_> = set.into_iter().collect();
    sorted.sort();
    obj.insert(key.to_string(), Value::String(sorted.join(" ")));
}

fn find_symbol<'a>(symbols: &'a [Symbol], name: &str) -> Option<&'a Symbol> {
    symbols.iter().find(|s| {
        s.qualified_name.as_deref() == Some(name) || s.name == name || symbol_qn(s) == name
    })
}

fn find_symbol_mut<'a>(symbols: &'a mut [Symbol], name: &str) -> Option<&'a mut Symbol> {
    symbols.iter_mut().find(|s| {
        s.qualified_name.as_deref() == Some(name) || s.name == name || symbol_qn(s) == name
    })
}

fn symbol_qn(sym: &Symbol) -> String {
    sym.qualified_name
        .clone()
        .unwrap_or_else(|| sym.name.clone())
}

fn enclosing_class_qn(qn: &str) -> Option<String> {
    if let Some((cls, _)) = qn.rsplit_once('.')
        && !cls.is_empty()
    {
        return Some(cls.to_string());
    }
    if let Some((cls, _)) = qn.rsplit_once("::")
        && !cls.is_empty()
    {
        return Some(cls.to_string());
    }
    None
}

fn split_field_qn(from: &str) -> (String, Option<String>) {
    if let Some((cls, field)) = from.rsplit_once('.') {
        return (cls.to_string(), Some(field.to_string()));
    }
    if let Some((cls, field)) = from.rsplit_once("::") {
        return (cls.to_string(), Some(field.to_string()));
    }
    (from.to_string(), None)
}

fn resolve_field_type(
    symbols: &[Symbol],
    class_qn: &str,
    field_name: Option<&str>,
    from: &str,
) -> Option<String> {
    // Direct field symbol
    if let Some(sym) = symbols.iter().find(|s| {
        matches!(s.symbol_type, SymbolType::Variable) && (symbol_qn(s) == from || s.name == from)
    }) && let Some(sig) = &sym.signature
    {
        return Some(sig.clone());
    }
    let field = field_name?;
    let class = symbols.iter().find(|s| {
        matches!(
            s.symbol_type,
            SymbolType::Class | SymbolType::Struct | SymbolType::Interface
        ) && symbol_qn(s) == class_qn
    })?;
    class
        .fields
        .iter()
        .find(|f| f.name == field)
        .and_then(|f| f.field_type.clone())
}

fn strip_generics_and_prefix(ty: &str) -> String {
    let mut s = ty.trim().to_string();
    // Unwrap nested Arc/RwLock/Mutex first for callers that already stripped wrappers
    loop {
        let before = s.clone();
        s = peel_known_containers(&s);
        if s == before {
            break;
        }
    }
    // Take innermost generic arg if present: Foo<Bar> -> Bar when single; else outer name
    if let Some(inner) = first_generic_arg(&s) {
        s = inner;
    }
    simple_name(&s)
}

fn peel_known_containers(ty: &str) -> String {
    for wrap in [
        "Arc", "RwLock", "Mutex", "Box", "Rc", "RefCell", "Provider", "List", "Optional",
    ] {
        if let Some(rest) = strip_wrapper_prefix(ty, wrap) {
            return rest;
        }
    }
    ty.to_string()
}

fn first_generic_arg(ty: &str) -> Option<String> {
    let start = ty.find('<')?;
    let end = ty.rfind('>')?;
    if end <= start + 1 {
        return None;
    }
    let inner = ty[start + 1..end].trim();
    // Take first type arg
    let first = split_top_level_commas(inner)
        .into_iter()
        .next()
        .unwrap_or_else(|| inner.to_string());
    Some(first.trim().to_string())
}

fn split_top_level_commas(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut start = 0;
    for (i, c) in s.char_indices() {
        match c {
            '<' | '(' | '[' => depth += 1,
            '>' | ')' | ']' => depth -= 1,
            ',' if depth == 0 => {
                out.push(s[start..i].trim().to_string());
                start = i + 1;
            }
            _ => {}
        }
    }
    out.push(s[start..].trim().to_string());
    out
}

fn unwrap_wrapper_type(ty: &str, wrappers: &FxHashSet<String>) -> Option<String> {
    let trimmed = ty.trim();
    // Match Wrapper<...> or path::Wrapper<...>
    for w in wrappers {
        if let Some(inner) = strip_wrapper_prefix(trimmed, w) {
            // After stripping wrapper, peel Arc/Mutex and take simple name
            let mut cur = inner;
            loop {
                let next = peel_known_containers(&cur);
                if next == cur {
                    break;
                }
                cur = next;
            }
            if cur.contains('<') {
                cur = first_generic_arg(&cur).unwrap_or(cur);
            }
            let simple = simple_name(&cur);
            if !simple.is_empty() && !is_primitive(&simple) {
                return Some(simple);
            }
        }
    }
    // Also: [FromServices] T style — param_type may be just the inner type with attribute elsewhere
    None
}

fn strip_wrapper_prefix(ty: &str, wrapper: &str) -> Option<String> {
    // Find Wrapper< or ::Wrapper<
    let lower_w = wrapper;
    let candidates = [
        format!("{lower_w}<"),
        format!("::{lower_w}<"),
        format!(".{lower_w}<"),
    ];
    let idx = candidates
        .iter()
        .find_map(|pat| ty.find(pat.as_str()).map(|i| (i, pat.len())))?;
    let start = idx.0 + idx.1 - 1; // at '<'
    let end = find_matching_angle(ty, start)?;
    Some(ty[start + 1..end].trim().to_string())
}

fn find_matching_angle(s: &str, open_idx: usize) -> Option<usize> {
    let bytes = s.as_bytes();
    if open_idx >= bytes.len() || bytes[open_idx] != b'<' {
        return None;
    }
    let mut depth = 0i32;
    for (i, &b) in bytes.iter().enumerate().skip(open_idx) {
        match b {
            b'<' => depth += 1,
            b'>' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
    }
    None
}

fn extract_provider_call(text: &str, providers: &FxHashSet<String>) -> Option<String> {
    for p in providers {
        // Depends(get_db) / Security(verify_token) / inject(UserService)
        let patterns = [format!("{p}("), format!("{p} (")];
        for pat in &patterns {
            if let Some(idx) = text.find(pat.as_str()) {
                let rest = &text[idx + pat.len()..];
                let end = rest.find([')', ',', ']']).unwrap_or(rest.len());
                let inner = rest[..end].trim();
                let name = inner
                    .rsplit(['.', ':'])
                    .next()
                    .unwrap_or(inner)
                    .trim()
                    .trim_matches(|c: char| !c.is_alphanumeric() && c != '_')
                    .to_string();
                if !name.is_empty() {
                    return Some(name);
                }
            }
        }
    }
    None
}

fn type_mentions_wrappers(ty: &str, wrappers: &FxHashSet<String>) -> bool {
    wrappers.iter().any(|w| {
        ty.contains(&format!("{w}<"))
            || ty.contains(&format!("::{w}<"))
            || ty
                .split(|c: char| !c.is_alphanumeric() && c != '_')
                .any(|p| p == w.as_str())
    })
}

fn text_mentions_providers(text: &str, providers: &FxHashSet<String>) -> bool {
    providers.iter().any(|p| text.contains(p.as_str()))
}

fn is_primitive(name: &str) -> bool {
    matches!(
        name,
        "bool"
            | "boolean"
            | "Boolean"
            | "byte"
            | "Byte"
            | "char"
            | "Character"
            | "short"
            | "Short"
            | "int"
            | "Int"
            | "Integer"
            | "long"
            | "Long"
            | "float"
            | "Float"
            | "double"
            | "Double"
            | "void"
            | "Void"
            | "unit"
            | "Unit"
            | "str"
            | "String"
            | "string"
            | "usize"
            | "isize"
            | "u8"
            | "u16"
            | "u32"
            | "u64"
            | "u128"
            | "i8"
            | "i16"
            | "i32"
            | "i64"
            | "i128"
            | "f32"
            | "f64"
            | "self"
            | "Self"
            | "this"
    )
}

fn is_constructor(sym: &Symbol, class_qn: &str) -> bool {
    if sym.metadata.get("is_constructor").and_then(|v| v.as_bool()) == Some(true) {
        return enclosing_class_qn(&symbol_qn(sym))
            .map(|c| c == class_qn || simple_name(&c) == simple_name(class_qn))
            .unwrap_or(true);
    }
    let class_simple = simple_name(class_qn);
    let qn = symbol_qn(sym);
    let in_class = enclosing_class_qn(&qn)
        .map(|c| c == class_qn || simple_name(&c) == class_simple)
        .unwrap_or(true);
    in_class
        && (sym.name == class_simple
            || sym.name == "<init>"
            || sym.name == "new"
            || sym.name == "constructor"
            || sym.name == "__init__"
            || sym.name == "initialize"
            || sym.name == "__construct"
            || qn.ends_with(".<init>"))
}

fn source_has_route_path_literal(source: &[u8]) -> bool {
    source
        .windows(2)
        .any(|w| w == b"\"/" || w == b"'/" || w == b"`/")
}

#[cfg(test)]
mod tests {
    use super::*;
    use rgctl_plugin_api::Parameter;

    fn loc() -> SourceLocation {
        SourceLocation {
            file: "t.py".into(),
            start_line: 1,
            end_line: 1,
            start_column: 0,
            end_column: 1,
        }
    }

    fn empty_sym(name: &str, qn: &str, ty: SymbolType) -> Symbol {
        Symbol {
            name: name.into(),
            symbol_type: ty,
            qualified_name: Some(qn.into()),
            location: loc(),
            signature: None,
            return_type: None,
            parameters: vec![],
            fields: vec![],
            modifiers: vec![],
            documentation: None,
            metadata: json!({}),
            ..Default::default()
        }
    }

    #[test]
    fn normalize_route_path_joins_and_strips() {
        assert_eq!(normalize_route_path("/api/", "/users/"), "/api/users");
        assert_eq!(normalize_route_path("\"/api\"", "'users'"), "/api/users");
        assert_eq!(normalize_route_path("", ""), "/");
        assert_eq!(normalize_route_path("/", "/"), "/");
    }

    #[test]
    fn format_route_target_builds_hint() {
        let (d, h) = format_route_target("get", "/users");
        assert_eq!(d, "GET /users");
        assert_eq!(h, "route::GET /users");
    }

    #[test]
    fn catalog_loads_builtin_languages() {
        let cat = builtin_catalog();
        assert!(cat.version >= 1);
        for lang in [
            "python",
            "typescript",
            "javascript",
            "java",
            "kotlin",
            "rust",
            "go",
            "csharp",
            "ruby",
            "php",
        ] {
            assert!(
                cat.by_language.contains_key(lang),
                "missing language {lang}"
            );
            let rules = &cat.by_language[lang].rules;
            assert!(!rules.is_empty(), "{lang} has no rules");
            for r in rules {
                assert!(!r.tags.is_empty(), "{} tags empty", r.id);
                assert!(!r.vocab.is_empty(), "{} vocab empty", r.id);
            }
        }
    }

    #[test]
    fn route_decorator_emits_exposes_route() {
        let mut symbols = vec![
            empty_sym("UsersController", "UsersController", SymbolType::Class),
            empty_sym("list", "UsersController.list", SymbolType::Method),
        ];
        let mut relations = vec![
            Relation {
                from: "UsersController".into(),
                to: "Controller".into(),
                relation_type: RelationType::AnnotatedWith,
                location: loc(),
                metadata: json!({"arguments": "\"/users\""}),
                to_qualified_hint: None,
                to_type_hint: None,
                ..Default::default()
            },
            Relation {
                from: "UsersController.list".into(),
                to: "Get".into(),
                relation_type: RelationType::AnnotatedWith,
                location: loc(),
                metadata: json!({"arguments": "\"/\""}),
                to_qualified_hint: None,
                to_type_hint: None,
                ..Default::default()
            },
        ];
        apply_framework_catalog("typescript", b"", None, &mut symbols, &mut relations);
        let routes: Vec<_> = relations
            .iter()
            .filter(|r| r.relation_type == RelationType::ExposesRoute)
            .collect();
        assert!(!routes.is_empty());
        assert_eq!(routes[0].to, "GET /users");
        assert_eq!(
            routes[0].to_qualified_hint.as_deref(),
            Some("route::GET /users")
        );
        let handler = symbols.iter().find(|s| s.name == "list").expect("handler");
        let tags = handler
            .metadata
            .get("framework_tags")
            .and_then(|v| v.as_str());
        assert!(tags.is_some_and(|t| t.contains("role:endpoint")));
    }

    #[test]
    fn param_call_inject_emits_injects() {
        let mut symbols = vec![{
            let mut s = empty_sym("get_users", "get_users", SymbolType::Function);
            s.parameters.push(Parameter {
                name: "db".into(),
                param_type: Some("Session".into()),
                default_value: Some("Depends(get_db)".into()),
            });
            s
        }];
        let mut relations = Vec::new();
        apply_framework_catalog("python", b"", None, &mut symbols, &mut relations);
        let injects: Vec<_> = relations
            .iter()
            .filter(|r| r.relation_type == RelationType::Injects)
            .collect();
        assert_eq!(injects.len(), 1);
        assert_eq!(injects[0].to, "get_db");
        assert_eq!(injects[0].to_type_hint.as_deref(), Some("function"));
        let tags = symbols[0]
            .metadata
            .get("semantic_tags")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        assert!(tags.contains("dependency") || tags.contains("inject"));
    }

    #[test]
    fn fast_reject_empty_c_like() {
        let mut symbols = vec![empty_sym("main", "main", SymbolType::Function)];
        let mut relations = Vec::new();
        apply_framework_catalog(
            "python",
            b"int main(){}",
            None,
            &mut symbols,
            &mut relations,
        );
        assert!(relations.is_empty());
        assert!(symbols[0].metadata.get("framework_tags").is_none());
    }

    #[test]
    fn load_overrides_without_dir_returns_builtin() {
        let cat = load_with_repo_overrides(None);
        assert!(cat.by_language.contains_key("python"));
    }

    #[test]
    fn generic_get_calls_do_not_emit_false_positive_routes() {
        let mut symbols = vec![empty_sym("fetchData", "fetchData", SymbolType::Function)];
        let mut relations = vec![Relation {
            from: "fetchData".into(),
            to: "get".into(),
            relation_type: RelationType::Calls,
            location: loc(),
            metadata: json!({"language": "typescript"}),
            to_qualified_hint: Some("map.get".into()),
            to_type_hint: None,
            ..Default::default()
        }];
        apply_framework_catalog(
            "typescript",
            b"function fetchData(map) { return map.get('/not-a-route'); }",
            None,
            &mut symbols,
            &mut relations,
        );
        assert!(
            !relations
                .iter()
                .any(|r| r.relation_type == RelationType::ExposesRoute),
            "unexpected ExposesRoute from generic .get() call: {relations:?}"
        );
    }

    #[test]
    fn field_annotation_does_not_leak_injects_onto_regular_method_params() {
        let mut class_sym = empty_sym("OrderController", "OrderController", SymbolType::Class);
        class_sym.fields.push(rgctl_plugin_api::Field {
            source_span: None,
            name: "svc".into(),
            field_type: Some("OrderService".into()),
            visibility: Some("private".into()),
        });
        let mut method_sym = empty_sym(
            "createOrder",
            "OrderController.createOrder",
            SymbolType::Method,
        );
        method_sym.parameters.push(Parameter {
            name: "dto".into(),
            param_type: Some("CreateOrderDto".into()),
            default_value: None,
        });
        let mut symbols = vec![class_sym, method_sym];
        let mut relations = vec![Relation {
            from: "OrderController.svc".into(),
            to: "Autowired".into(),
            relation_type: RelationType::AnnotatedWith,
            location: loc(),
            metadata: json!({}),
            to_qualified_hint: None,
            to_type_hint: None,
            ..Default::default()
        }];
        apply_framework_catalog("java", b"", None, &mut symbols, &mut relations);
        let injects: Vec<_> = relations
            .iter()
            .filter(|r| r.relation_type == RelationType::Injects)
            .map(|r| r.to.as_str())
            .collect();
        assert_eq!(injects, vec!["OrderService"]);
    }
}
