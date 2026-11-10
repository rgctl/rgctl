//! Java language plugin
//!
//! Extracts classes, interfaces, enums, annotations, records, modules/packages,
//! constructors (including compact/record constructors), static/instance
//! initializers, fields, and their relationships (calls, inheritance,
//! annotations, instantiation, method references, ctor chaining, imports)
//! from Java source using a single Tree-sitter CST walk per phase.

use crate::single_pass::{extract_all_single_pass, java_ast_handlers};
use rgctl_plugin_api::*;
use rgctl_plugin_api::{Error, Result};
use rgctl_plugin_helpers::{ComplexityCalculator, record_call_site_spans, record_symbol_spans};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::path::Path;
use tree_sitter::{Node, Parser};

/// Call node kinds for deferred `Calls` flush (Java tree-sitter grammar).
pub(crate) const JAVA_CALL_KINDS: &[&str] = &["method_invocation"];

/// Tree-sitter node kinds that introduce a new named type scope for
/// qualified-name purposes (`Outer.Inner` nesting).
const TYPE_DECL_KINDS: &[&str] = &[
    "class_declaration",
    "interface_declaration",
    "record_declaration",
    "enum_declaration",
    "annotation_type_declaration",
];

/// Decision-point node kinds counted toward cyclomatic/cognitive complexity.
pub(crate) const BRANCH_KINDS: &[&str] = &[
    "if_statement",
    "while_statement",
    "for_statement",
    "enhanced_for_statement",
    "do_statement",
    "catch_clause",
];

/// Node kinds that add one level of nesting depth for complexity purposes.
const NESTING_CONTAINER_KINDS: &[&str] = &[
    "if_statement",
    "while_statement",
    "for_statement",
    "enhanced_for_statement",
    "do_statement",
    "block",
];

/// Per-file extraction context: package prefix, anonymous-class synthetic
/// names, and instance-initializer counters. Rebuilt once per symbols or
/// relations walk (both walks may share one tree via `extract_all`).
pub(crate) struct ExtractCtx {
    package: Option<String>,
    /// Simple import name → fully qualified type (e.g. `OncePerRequestFilter` → `org.springframework...`).
    imports: HashMap<String, String>,
    /// Static import member → owner FQN (e.g. `emptyList` → `java.util.Collections`
    /// or `PI` → `java.lang.Math` from `import static …`).
    static_imports: HashMap<String, String>,
    /// Maps an anonymous class's `class_body` node id to a synthetic owner
    /// name (`$AnonymousN`), assigned in document order.
    anon_names: HashMap<usize, String>,
    /// Per-owner counters for instance initializer blocks (`Type.<initblock>N`).
    initblock_counters: RefCell<HashMap<String, usize>>,
    /// Per-enclosing-callable counters for lambda expressions (`Owner.$lambda$N`),
    /// assigned in document order (pre-order DFS) so the symbol pass and the
    /// relations pass (each re-parsing the source independently) agree on `N`.
    lambda_counters: RefCell<HashMap<String, usize>>,
}

impl ExtractCtx {
    pub(crate) fn new(root: Node, source: &[u8]) -> Self {
        let (imports, static_imports) = Self::collect_imports(root, source);
        Self {
            package: Self::find_package_name(root, source),
            imports,
            static_imports,
            anon_names: Self::collect_anonymous_names(root),
            initblock_counters: RefCell::new(HashMap::new()),
            lambda_counters: RefCell::new(HashMap::new()),
        }
    }

    fn import_qualified_hint(&self, simple: &str) -> Option<String> {
        self.imports.get(simple).cloned()
    }

    /// Resolve a raw type name to an FQN using imports, same-file types, and package.
    ///
    /// Strips generics (`List<Order>` → `List`) and array dims (`Order[]` → `Order`).
    pub(crate) fn resolve_type_fqn(
        &self,
        raw_type: &str,
        same_file_types: &HashSet<String>,
    ) -> Option<String> {
        let base = strip_java_type_sugar(raw_type);
        if base.is_empty() {
            return None;
        }
        if base.contains('.') {
            return Some(base);
        }
        if let Some(fqn) = self.imports.get(&base) {
            return Some(fqn.clone());
        }
        if same_file_types.contains(&base) {
            return Some(self.qualify_type(&base));
        }
        // Same-package fallback for unqualified names that look like types.
        if base
            .chars()
            .next()
            .map(|c| c.is_ascii_uppercase())
            .unwrap_or(false)
        {
            return Some(self.qualify_type(&base));
        }
        None
    }

    /// Collect type imports and static imports separately.
    fn collect_imports(
        root: Node,
        source: &[u8],
    ) -> (HashMap<String, String>, HashMap<String, String>) {
        let mut imports = HashMap::new();
        let mut static_imports = HashMap::new();
        let mut stack = vec![root];
        while let Some(node) = stack.pop() {
            if node.kind() == "import_declaration" {
                let text = node.utf8_text(source).unwrap_or("").trim().to_string();
                let is_static = text.starts_with("import static") || {
                    let mut c = node.walk();
                    node.children(&mut c).any(|ch| ch.kind() == "static")
                };
                let mut cursor = node.walk();
                if let Some(name_node) = node
                    .children(&mut cursor)
                    .find(|c| c.kind() == "identifier" || c.kind() == "scoped_identifier")
                    && let Ok(fqn) = name_node.utf8_text(source)
                {
                    let simple = fqn.rsplit('.').next().unwrap_or(fqn).to_string();
                    if is_static {
                        // `import static a.b.Util.method` → member `method` owned by `a.b.Util`
                        // `import static a.b.Util.*` → skip (wildcard; no single member key)
                        if simple != "*" {
                            let owner = fqn
                                .rsplit_once('.')
                                .map(|(o, _)| o.to_string())
                                .unwrap_or_else(|| fqn.to_string());
                            static_imports.insert(simple, owner);
                        }
                    } else if simple != "*" {
                        imports.insert(simple, fqn.to_string());
                    }
                }
            }
            let mut cursor = node.walk();
            for child in node
                .children(&mut cursor)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
            {
                stack.push(child);
            }
        }
        (imports, static_imports)
    }

    fn find_package_name(root: Node, source: &[u8]) -> Option<String> {
        let mut cursor = root.walk();
        let package_node = root
            .children(&mut cursor)
            .find(|c| c.kind() == "package_declaration")?;
        let mut pcursor = package_node.walk();

        package_node
            .children(&mut pcursor)
            .find(|c| c.kind() == "identifier" || c.kind() == "scoped_identifier")
            .and_then(|n| n.utf8_text(source).ok())
            .map(str::to_string)
    }

    /// Pre-order scan assigning `$AnonymousN` to every anonymous class body
    /// (the `class_body` of an `object_creation_expression`), keyed by node id
    /// so later lookups don't need to re-derive numbering.
    fn collect_anonymous_names(root: Node) -> HashMap<usize, String> {
        let mut map = HashMap::new();
        let mut counter = 0usize;
        let mut stack = vec![root];
        while let Some(node) = stack.pop() {
            if node.kind() == "object_creation_expression" {
                let mut cursor = node.walk();
                let anon_body = node
                    .children(&mut cursor)
                    .find(|c| c.kind() == "class_body");
                if let Some(body) = anon_body {
                    counter += 1;
                    map.insert(body.id(), format!("$Anonymous{counter}"));
                }
            }
            let mut cursor = node.walk();
            let children: Vec<Node> = node.children(&mut cursor).collect();
            for child in children.into_iter().rev() {
                stack.push(child);
            }
        }
        map
    }

    fn qualify_type(&self, path: &str) -> String {
        match &self.package {
            Some(pkg) => format!("{pkg}.{path}"),
            None => path.to_string(),
        }
    }
}

/// Strip generics / array / varargs sugar from a Java type string.
fn strip_java_type_sugar(raw: &str) -> String {
    raw.trim()
        .split('<')
        .next()
        .unwrap_or(raw)
        .trim_end_matches("[]")
        .trim_end_matches("...")
        .trim()
        .to_string()
}

/// Strip surrounding quotes from a Java string/char literal; leave other tokens.
fn strip_java_string_literal(raw: &str) -> String {
    let t = raw.trim();
    if t.len() >= 2 {
        let b = t.as_bytes();
        if (b[0] == b'"' && b[t.len() - 1] == b'"') || (b[0] == b'\'' && b[t.len() - 1] == b'\'') {
            return t[1..t.len() - 1].to_string();
        }
    }
    t.to_string()
}

fn source_location(node: Node, file_path: &str) -> SourceLocation {
    SourceLocation {
        file: file_path.to_string(),
        start_line: node.start_position().row + 1,
        end_line: node.end_position().row + 1,
        start_column: node.start_position().column,
        end_column: node.end_position().column,
    }
}

/// Reuse the current parser's token positions; translate byte columns once on extraction workers.
fn source_name_range(node: Node, source: &[u8]) -> Option<serde_json::Value> {
    let start = node.start_position();
    let end = node.end_position();
    let first =
        std::str::from_utf8(&source[node.start_byte() - start.column..node.start_byte()]).ok()?;
    let last = std::str::from_utf8(&source[node.end_byte() - end.column..node.end_byte()]).ok()?;
    Some(serde_json::json!({
        "start_line": start.row, "start_character": first.encode_utf16().count(),
        "end_line": end.row, "end_character": last.encode_utf16().count(),
    }))
}

fn apply_name_range(node: Node, source: &[u8], metadata: &mut serde_json::Value) {
    if let Some(name) = node.child_by_field_name("name")
        && let Some(range) = source_name_range(name, source)
    {
        metadata["source_name_range"] = range;
    }
}

fn first_line_text(node: Node, source: &[u8]) -> Result<String> {
    Ok(node
        .utf8_text(source)?
        .lines()
        .next()
        .unwrap_or("")
        .trim()
        .to_string())
}

/// Java language plugin using Tree-sitter.
pub struct JavaPlugin {
    _parser: Parser,
}

impl JavaPlugin {
    /// Create a new Java plugin.
    pub fn new() -> Result<Self> {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_java::LANGUAGE.into())
            .map_err(|e| Error::PluginError(format!("Failed to set Java grammar: {e}")))?;
        Ok(Self { _parser: parser })
    }

    // ---------------------------------------------------------------
    // Modifiers / annotations
    // ---------------------------------------------------------------

    /// Split a `modifiers` node's children into keyword strings (`public`,
    /// `static`, `sealed`, ...) and annotation usage nodes (`annotation` /
    /// `marker_annotation`), without any regex-on-source.
    fn split_modifiers<'a>(
        &self,
        modifiers_node: Node<'a>,
        source: &[u8],
    ) -> (Vec<String>, Vec<Node<'a>>) {
        let mut keywords = Vec::new();
        let mut annotations = Vec::new();
        let mut cursor = modifiers_node.walk();
        for child in modifiers_node.children(&mut cursor) {
            match child.kind() {
                "annotation" | "marker_annotation" => annotations.push(child),
                _ => {
                    if let Ok(text) = child.utf8_text(source) {
                        keywords.push(text.to_string());
                    }
                }
            }
        }
        (keywords, annotations)
    }

    /// Collect the keyword-only modifiers directly attached to `node`
    /// (annotations are excluded; see `split_modifiers`).
    fn collect_keyword_modifiers(&self, node: Node, source: &[u8]) -> Vec<String> {
        let mut cursor = node.walk();
        let modifiers_node = node.children(&mut cursor).find(|c| c.kind() == "modifiers");
        modifiers_node
            .map(|m| self.split_modifiers(m, source).0)
            .unwrap_or_default()
    }

    fn annotation_name_and_args(
        &self,
        ann_node: Node,
        source: &[u8],
    ) -> (String, String, Option<String>) {
        let raw_name = ann_node
            .child_by_field_name("name")
            .and_then(|n| n.utf8_text(source).ok())
            .unwrap_or("")
            .to_string();
        let simple = raw_name
            .rsplit('.')
            .next()
            .unwrap_or(raw_name.as_str())
            .to_string();
        let args = ann_node
            .child_by_field_name("arguments")
            .and_then(|n| n.utf8_text(source).ok())
            .map(str::to_string);
        (simple, raw_name, args)
    }

    /// Parse annotation argument list into a structured map (`value`, `path`,
    /// `name`, `unitName`, `method`, `produces`, `consumes`, `defaultValue`, …).
    fn parse_annotation_args_map(
        &self,
        ann_node: Node,
        source: &[u8],
    ) -> serde_json::Map<String, serde_json::Value> {
        let mut out = serde_json::Map::new();
        let Some(args_node) = ann_node.child_by_field_name("arguments") else {
            return out;
        };
        let mut cursor = args_node.walk();
        let mut saw_named = false;
        let mut positional: Option<String> = None;
        for child in args_node.children(&mut cursor) {
            match child.kind() {
                "element_value_pair" => {
                    saw_named = true;
                    let key = child
                        .child_by_field_name("key")
                        .and_then(|n| n.utf8_text(source).ok())
                        .unwrap_or("")
                        .to_string();
                    let val_node = child
                        .child_by_field_name("value")
                        .or_else(|| child.named_child(1));
                    let raw = val_node
                        .and_then(|n| n.utf8_text(source).ok())
                        .unwrap_or("")
                        .trim()
                        .to_string();
                    if key.is_empty() || raw.is_empty() {
                        continue;
                    }
                    let cleaned = strip_java_string_literal(&raw);
                    out.insert(key, serde_json::Value::String(cleaned));
                }
                "string_literal"
                | "identifier"
                | "scoped_identifier"
                | "field_access"
                | "element_value_array_initializer"
                | "annotation"
                | "marker_annotation"
                    if !saw_named && positional.is_none() =>
                {
                    let raw = child.utf8_text(source).unwrap_or("").trim().to_string();
                    if !raw.is_empty() {
                        positional = Some(strip_java_string_literal(&raw));
                    }
                }
                _ => {}
            }
        }
        if !saw_named && let Some(v) = positional {
            out.insert("value".into(), serde_json::Value::String(v));
        }
        out
    }

    #[allow(clippy::too_many_arguments)]
    fn push_annotated_with(
        &self,
        from: &str,
        ann_node: Node,
        source: &[u8],
        file_path: &Path,
        ctx: &ExtractCtx,
        same_file_types: &HashSet<String>,
        relations: &mut Vec<Relation>,
    ) {
        self.push_annotated_with_scoped(
            from,
            ann_node,
            source,
            file_path,
            ctx,
            same_file_types,
            relations,
            None,
        );
    }

    #[allow(clippy::too_many_arguments)]
    fn push_annotated_with_scoped(
        &self,
        from: &str,
        ann_node: Node,
        source: &[u8],
        file_path: &Path,
        ctx: &ExtractCtx,
        same_file_types: &HashSet<String>,
        relations: &mut Vec<Relation>,
        scope: Option<&str>,
    ) {
        let (name, raw_name, args) = self.annotation_name_and_args(ann_node, source);
        if name.is_empty() {
            return;
        }
        let fqn = ctx
            .resolve_type_fqn(&raw_name, same_file_types)
            .or_else(|| {
                if raw_name.contains('.') {
                    Some(raw_name)
                } else {
                    None
                }
            });
        let mut metadata = serde_json::json!({ "language": "java" });
        if let Some(args) = args {
            metadata["arguments"] = serde_json::Value::String(args);
        }
        let args_map = self.parse_annotation_args_map(ann_node, source);
        if !args_map.is_empty() {
            metadata["annotation_args"] = serde_json::Value::Object(args_map);
        }
        if let Some(ref fqn) = fqn {
            metadata["fqn"] = serde_json::Value::String(fqn.clone());
        }
        if let Some(scope) = scope {
            metadata["call_context"] =
                serde_json::Value::String(format!("AnnotationScope::{scope}"));
            metadata["annotation_scope"] = serde_json::Value::String(scope.to_string());
        }
        let mut rel = Relation {
            from: from.to_string(),
            to: name,
            relation_type: RelationType::AnnotatedWith,
            location: source_location(ann_node, &file_path.to_string_lossy()),
            metadata,
            to_qualified_hint: fqn,
            to_type_hint: None,
            ..Default::default()
        };
        record_call_site_spans(&mut rel, ann_node);
        relations.push(rel);
    }

    /// Simple type names declared in this file (class/interface/enum/annotation/record).
    pub(crate) fn same_file_types(symbols: &[Symbol]) -> HashSet<String> {
        symbols
            .iter()
            .filter(|s| {
                matches!(
                    s.symbol_type,
                    SymbolType::Class
                        | SymbolType::Interface
                        | SymbolType::Enum
                        | SymbolType::Annotation
                )
            })
            .map(|s| s.name.clone())
            .collect()
    }

    /// Attach `return_type_fqn` / `field_type_fqns` / param `type_fqn` metadata.
    pub(crate) fn enrich_symbol_type_fqns(
        &self,
        ctx: &ExtractCtx,
        same_file_types: &HashSet<String>,
        symbols: &mut [Symbol],
    ) {
        for sym in symbols.iter_mut() {
            if let Some(ref rt) = sym.return_type
                && let Some(fqn) = ctx.resolve_type_fqn(rt, same_file_types)
            {
                sym.metadata["return_type_fqn"] = serde_json::Value::String(fqn);
            }
            if !sym.fields.is_empty() {
                let mut map = serde_json::Map::new();
                for f in &sym.fields {
                    if let Some(ref ft) = f.field_type
                        && let Some(fqn) = ctx.resolve_type_fqn(ft, same_file_types)
                    {
                        map.insert(f.name.clone(), serde_json::Value::String(fqn));
                    }
                }
                if !map.is_empty() {
                    sym.metadata["field_type_fqns"] = serde_json::Value::Object(map);
                }
            }
            for p in &mut sym.parameters {
                if let Some(ref pt) = p.param_type
                    && let Some(fqn) = ctx.resolve_type_fqn(pt, same_file_types)
                {
                    // Parameter has no dedicated metadata slot; fold into symbol.
                    let key = format!("param_type_fqn:{}", p.name);
                    sym.metadata[key] = serde_json::Value::String(fqn);
                }
            }
        }
    }

    // ---------------------------------------------------------------
    // Generics / throws / receiver-parameter helpers
    // ---------------------------------------------------------------

    /// Raw `<T, U extends Foo>` text of a `type_parameters` child field, when
    /// present (class/interface/record/method/constructor declarations).
    fn type_parameters_text(&self, node: Node, source: &[u8]) -> Option<String> {
        node.child_by_field_name("type_parameters")
            .and_then(|n| n.utf8_text(source).ok())
            .map(str::to_string)
    }

    /// Comma-joined exception type names from an optional `throws` child
    /// (not a named field in the grammar, so found by kind among children).
    fn throws_text(&self, node: Node, source: &[u8]) -> Option<String> {
        let mut cursor = node.walk();
        let throws_node = node.children(&mut cursor).find(|c| c.kind() == "throws")?;
        let mut names = Vec::new();
        let mut tcursor = throws_node.walk();
        for t in throws_node.children(&mut tcursor) {
            if t.is_named()
                && let Ok(text) = t.utf8_text(source)
            {
                names.push(text.trim().to_string());
            }
        }
        if names.is_empty() {
            None
        } else {
            Some(names.join(", "))
        }
    }

    /// Raw text of an explicit `receiver_parameter` (`Foo this` /
    /// `Outer.Foo Outer.this`) among a method/constructor's parameters, if any.
    fn receiver_type_text(&self, node: Node, source: &[u8]) -> Option<String> {
        let params_node = node.child_by_field_name("parameters")?;
        let mut cursor = params_node.walk();
        let receiver = params_node
            .children(&mut cursor)
            .find(|c| c.kind() == "receiver_parameter")?;
        receiver.utf8_text(source).ok().map(str::to_string)
    }

    /// Populate `type_params` / `throws` / `receiver_type` metadata keys on an
    /// existing metadata object for a method/constructor-shaped node.
    fn apply_callable_metadata(&self, node: Node, source: &[u8], metadata: &mut serde_json::Value) {
        apply_name_range(node, source, metadata);
        if let Some(tp) = self.type_parameters_text(node, source) {
            metadata["type_params"] = serde_json::Value::String(tp);
        }
        if let Some(th) = self.throws_text(node, source) {
            metadata["throws"] = serde_json::Value::String(th);
        }
        if let Some(rt) = self.receiver_type_text(node, source) {
            metadata["receiver_type"] = serde_json::Value::String(rt);
        }
    }

    // ---------------------------------------------------------------
    // Qualified-name helpers (nesting, anonymous classes, packages)
    // ---------------------------------------------------------------

    /// Walk ancestors of `node`, collecting enclosing type names
    /// (`Outer.Inner`) and synthesizing `$AnonymousN` levels for anonymous
    /// class bodies, innermost first then reversed to source order.
    fn find_containing_type_name(
        &self,
        node: Node,
        source: &[u8],
        ctx: &ExtractCtx,
    ) -> Option<String> {
        let mut names = Vec::new();
        let mut current = node;
        while let Some(parent) = current.parent() {
            if TYPE_DECL_KINDS.contains(&parent.kind()) {
                if let Some(name) = parent
                    .child_by_field_name("name")
                    .and_then(|n| n.utf8_text(source).ok())
                {
                    names.push(name.to_string());
                }
            } else if parent.kind() == "class_body" {
                if let Some(anon) = ctx.anon_names.get(&parent.id()) {
                    names.push(anon.clone());
                } else if let Some(grandparent) = parent.parent() {
                    // Enum constant body (`ENUM { CONST { ... } }`): qualify by the
                    // constant's own name so members read `Enum.CONST.member`.
                    if grandparent.kind() == "enum_constant"
                        && let Some(name) = grandparent
                            .child_by_field_name("name")
                            .and_then(|n| n.utf8_text(source).ok())
                    {
                        names.push(name.to_string());
                    }
                }
            }
            current = parent;
        }
        if names.is_empty() {
            None
        } else {
            names.reverse();
            Some(names.join("."))
        }
    }

    /// Fully qualified name of a type declaration node itself (not an
    /// ancestor query): `pkg.Outer.Inner`.
    fn type_node_qualified_name(&self, node: Node, source: &[u8], ctx: &ExtractCtx) -> String {
        let name = node
            .child_by_field_name("name")
            .and_then(|n| n.utf8_text(source).ok())
            .unwrap_or("");
        let path = match self.find_containing_type_name(node, source, ctx) {
            Some(owner) => format!("{owner}.{name}"),
            None => name.to_string(),
        };
        ctx.qualify_type(&path)
    }

    fn method_qualified_name(
        &self,
        method_node: Node,
        name: &str,
        source: &[u8],
        ctx: &ExtractCtx,
    ) -> Option<String> {
        self.find_containing_type_name(method_node, source, ctx)
            .map(|owner| ctx.qualify_type(&format!("{owner}.{name}")))
    }

    /// Find the nearest enclosing type declaration node (for resolving
    /// e.g. a superclass hint for `super(...)`).
    fn find_enclosing_type_node<'a>(&self, node: Node<'a>) -> Option<Node<'a>> {
        let mut current = node;
        while let Some(parent) = current.parent() {
            if TYPE_DECL_KINDS.contains(&parent.kind()) {
                return Some(parent);
            }
            current = parent;
        }
        None
    }

    /// Qualified name of the nearest enclosing callable (method, constructor,
    /// compact constructor, or static initializer) containing `node`.
    fn find_containing_callable_qn(
        &self,
        node: Node,
        source: &[u8],
        ctx: &ExtractCtx,
    ) -> Option<String> {
        let mut current = node;
        while let Some(parent) = current.parent() {
            match parent.kind() {
                "method_declaration" => {
                    let name = parent
                        .child_by_field_name("name")
                        .and_then(|n| n.utf8_text(source).ok())?
                        .to_string();
                    return self
                        .method_qualified_name(parent, &name, source, ctx)
                        .or(Some(name));
                }
                "constructor_declaration" | "compact_constructor_declaration" => {
                    let owner = self.find_containing_type_name(parent, source, ctx)?;
                    return Some(format!("{}.<init>", ctx.qualify_type(&owner)));
                }
                "static_initializer" => {
                    let owner = self.find_containing_type_name(parent, source, ctx)?;
                    return Some(format!("{}.<clinit>", ctx.qualify_type(&owner)));
                }
                _ => {}
            }
            current = parent;
        }
        None
    }

    /// Stable pre-order index for a `lambda_expression`, scoped to its
    /// nearest enclosing callable (or enclosing type, for field/static
    /// initializer lambdas). Called identically from the symbol pass and the
    /// relations pass (each with a fresh `ExtractCtx`/counter) so both agree
    /// on `N` for `Owner.$lambda$N`, since both walk the tree in the same
    /// document order.
    fn lambda_index(&self, node: Node, source: &[u8], ctx: &ExtractCtx) -> (Option<String>, usize) {
        let owner = self
            .find_containing_callable_qn(node, source, ctx)
            .or_else(|| {
                self.find_containing_type_name(node, source, ctx)
                    .map(|o| ctx.qualify_type(&o))
            });
        let key = owner.clone().unwrap_or_else(|| "<unknown>".to_string());
        let idx = {
            let mut counters = ctx.lambda_counters.borrow_mut();
            let counter = counters.entry(key).or_insert(0);
            let current = *counter;
            *counter += 1;
            current
        };
        (owner, idx)
    }

    // ---------------------------------------------------------------
    // Symbol extraction: methods, constructors, static/instance init
    // ---------------------------------------------------------------

    pub(crate) fn extract_method(
        &self,
        node: Node,
        source: &[u8],
        file_path: &str,
        ctx: &ExtractCtx,
    ) -> Result<Symbol> {
        let name_node = node
            .child_by_field_name("name")
            .ok_or_else(|| Error::ParseError {
                file: file_path.into(),
                line: node.start_position().row + 1,
                message: "Method missing name".to_string(),
            })?;
        let name = name_node.utf8_text(source)?.to_string();

        let return_type = node
            .child_by_field_name("type")
            .and_then(|n| n.utf8_text(source).ok())
            .map(str::to_string);

        let modifiers = self.collect_keyword_modifiers(node, source);
        let qualified_name = self.method_qualified_name(node, &name, source, ctx);
        let parameters = self.extract_parameters(node, source)?;

        let mut metadata = serde_json::json!({ "language": "java" });
        self.apply_callable_metadata(node, source, &mut metadata);

        let mut symbol = Symbol {
            name,
            symbol_type: SymbolType::Function,
            qualified_name,
            location: source_location(node, file_path),
            signature: Some(first_line_text(node, source)?),
            return_type,
            parameters,
            fields: vec![],
            modifiers,
            documentation: None,
            metadata,
            ..Default::default()
        };
        record_symbol_spans(&mut symbol, node, Some(name_node));
        Ok(symbol)
    }

    pub(crate) fn extract_constructor(
        &self,
        node: Node,
        source: &[u8],
        file_path: &str,
        ctx: &ExtractCtx,
    ) -> Result<Symbol> {
        let name_node = node
            .child_by_field_name("name")
            .ok_or_else(|| Error::ParseError {
                file: file_path.into(),
                line: node.start_position().row + 1,
                message: "Constructor missing name".to_string(),
            })?;
        let name = name_node.utf8_text(source)?.to_string();

        let owner = self
            .find_containing_type_name(node, source, ctx)
            .ok_or_else(|| Error::ParseError {
                file: file_path.into(),
                line: node.start_position().row + 1,
                message: "Constructor missing containing class".to_string(),
            })?;

        let modifiers = self.collect_keyword_modifiers(node, source);
        let parameters = self.extract_parameters(node, source)?;
        let qualified_name = format!("{}.<init>", ctx.qualify_type(&owner));

        let mut metadata = serde_json::json!({
            "language": "java",
            "is_constructor": true,
        });
        self.apply_callable_metadata(node, source, &mut metadata);

        let mut symbol = Symbol {
            name,
            symbol_type: SymbolType::Function,
            qualified_name: Some(qualified_name),
            location: source_location(node, file_path),
            signature: Some(first_line_text(node, source)?),
            return_type: None,
            parameters,
            fields: vec![],
            modifiers,
            documentation: None,
            metadata,
            ..Default::default()
        };
        record_symbol_spans(&mut symbol, node, Some(name_node));
        Ok(symbol)
    }

    /// `compact_constructor_declaration` (records): `record Point(int x, int y) { public Point { ... } }`.
    pub(crate) fn extract_compact_constructor(
        &self,
        node: Node,
        source: &[u8],
        file_path: &str,
        ctx: &ExtractCtx,
    ) -> Result<Symbol> {
        let name_node = node
            .child_by_field_name("name")
            .ok_or_else(|| Error::ParseError {
                file: file_path.into(),
                line: node.start_position().row + 1,
                message: "Compact constructor missing name".to_string(),
            })?;
        let name = name_node.utf8_text(source)?.to_string();

        let owner = self
            .find_containing_type_name(node, source, ctx)
            .ok_or_else(|| Error::ParseError {
                file: file_path.into(),
                line: node.start_position().row + 1,
                message: "Compact constructor missing containing record".to_string(),
            })?;

        let modifiers = self.collect_keyword_modifiers(node, source);
        let qualified_name = format!("{}.<init>", ctx.qualify_type(&owner));

        let mut symbol = Symbol {
            name,
            symbol_type: SymbolType::Function,
            qualified_name: Some(qualified_name),
            location: source_location(node, file_path),
            signature: Some(first_line_text(node, source)?),
            return_type: None,
            parameters: vec![],
            fields: vec![],
            modifiers,
            documentation: None,
            metadata: serde_json::json!({
                "language": "java",
                "is_constructor": true,
                "is_compact_constructor": true,
            }),
            ..Default::default()
        };
        record_symbol_spans(&mut symbol, node, Some(name_node));
        Ok(symbol)
    }

    pub(crate) fn extract_static_initializer(
        &self,
        node: Node,
        source: &[u8],
        file_path: &str,
        ctx: &ExtractCtx,
    ) -> Result<Symbol> {
        let owner = self.find_containing_type_name(node, source, ctx);
        let qualified_name = owner.map(|o| format!("{}.<clinit>", ctx.qualify_type(&o)));

        let mut symbol = Symbol {
            name: "<clinit>".to_string(),
            symbol_type: SymbolType::Function,
            qualified_name,
            location: source_location(node, file_path),
            signature: Some(first_line_text(node, source)?),
            return_type: None,
            parameters: vec![],
            fields: vec![],
            modifiers: vec![],
            documentation: None,
            metadata: serde_json::json!({
                "language": "java",
                "is_static_initializer": true,
            }),
            ..Default::default()
        };
        record_symbol_spans(&mut symbol, node, node.child_by_field_name("name"));
        Ok(symbol)
    }

    /// Instance initializer block: a bare `block` that is a direct child of a
    /// `class_body` (tree-sitter-java has no dedicated node kind for these).
    pub(crate) fn extract_instance_initializer(
        &self,
        node: Node,
        source: &[u8],
        file_path: &str,
        ctx: &ExtractCtx,
    ) -> Result<Symbol> {
        let owner_qn = self
            .find_containing_type_name(node, source, ctx)
            .map(|o| ctx.qualify_type(&o));

        let idx = {
            let key = owner_qn.clone().unwrap_or_else(|| "<unknown>".to_string());
            let mut counters = ctx.initblock_counters.borrow_mut();
            let counter = counters.entry(key).or_insert(0);
            *counter += 1;
            *counter
        };
        let name = format!("<initblock>{idx}");
        let qualified_name = owner_qn.map(|o| format!("{o}.{name}"));

        let mut symbol = Symbol {
            name,
            symbol_type: SymbolType::Function,
            qualified_name,
            location: source_location(node, file_path),
            signature: Some(first_line_text(node, source)?),
            return_type: None,
            parameters: vec![],
            fields: vec![],
            modifiers: vec![],
            documentation: None,
            metadata: serde_json::json!({
                "language": "java",
                "is_instance_initializer": true,
            }),
            ..Default::default()
        };
        record_symbol_spans(&mut symbol, node, node.child_by_field_name("name"));
        Ok(symbol)
    }

    /// Extract parameters from `formal_parameters`, handling both
    /// `formal_parameter` and `spread_parameter` (varargs). Varargs names
    /// live under `variable_declarator`'s `name` field, not a top-level field.
    fn extract_parameters(&self, node: Node, source: &[u8]) -> Result<Vec<Parameter>> {
        let mut parameters = Vec::new();
        let params_node = if let Some(p) = node.child_by_field_name("parameters") {
            p
        } else {
            let mut cursor = node.walk();
            let found = node
                .children(&mut cursor)
                .find(|c| c.kind() == "formal_parameters");
            match found {
                Some(p) => p,
                None => return Ok(parameters),
            }
        };

        let mut cursor = params_node.walk();
        for child in params_node.children(&mut cursor) {
            match child.kind() {
                "formal_parameter" => {
                    let name = child
                        .child_by_field_name("name")
                        .and_then(|n| n.utf8_text(source).ok())
                        .map(str::to_string);
                    let param_type = child
                        .child_by_field_name("type")
                        .and_then(|n| n.utf8_text(source).ok())
                        .map(str::to_string);
                    if let Some(name) = name {
                        parameters.push(Parameter {
                            name,
                            param_type,
                            default_value: None,
                        });
                    }
                }
                "spread_parameter" => {
                    let mut var_name = None;
                    let mut type_text = None;
                    let mut vcursor = child.walk();
                    for gc in child.children(&mut vcursor) {
                        match gc.kind() {
                            "variable_declarator" => {
                                var_name = gc
                                    .child_by_field_name("name")
                                    .and_then(|n| n.utf8_text(source).ok())
                                    .map(str::to_string);
                            }
                            "modifiers" | "annotation" | "marker_annotation" | "..." => {}
                            _ => {
                                if type_text.is_none() {
                                    type_text = gc.utf8_text(source).ok().map(str::to_string);
                                }
                            }
                        }
                    }
                    if let Some(name) = var_name {
                        parameters.push(Parameter {
                            name,
                            param_type: type_text.map(|t| format!("{t}...")),
                            default_value: None,
                        });
                    }
                }
                _ => {}
            }
        }
        Ok(parameters)
    }

    // ---------------------------------------------------------------
    // Symbol extraction: types (class/interface/enum/annotation/record)
    // ---------------------------------------------------------------

    fn extract_class_fields(
        &self,
        class_node: Node,
        source: &[u8],
        metadata: &mut serde_json::Value,
    ) -> Result<Vec<Field>> {
        let mut fields = Vec::new();
        let mut ranges = serde_json::Map::new();
        let mut cursor = class_node.walk();
        let body = class_node
            .children(&mut cursor)
            .find(|c| c.kind() == "class_body");
        let Some(body) = body else {
            return Ok(fields);
        };

        let mut body_cursor = body.walk();
        for child in body.children(&mut body_cursor) {
            if child.kind() != "field_declaration" {
                continue;
            }
            let field_type = child
                .child_by_field_name("type")
                .and_then(|n| n.utf8_text(source).ok())
                .map(str::to_string);
            let visibility = {
                let kw = self.collect_keyword_modifiers(child, source);
                if kw.is_empty() {
                    None
                } else {
                    Some(kw.join(" "))
                }
            };

            let mut decl_cursor = child.walk();
            for field_child in child.children(&mut decl_cursor) {
                if field_child.kind() == "variable_declarator"
                    && let Some(name_node) = field_child.child_by_field_name("name")
                {
                    let name = name_node.utf8_text(source)?.to_string();
                    if let Some(range) = source_name_range(name_node, source) {
                        ranges.insert(name.clone(), range);
                    }
                    fields.push(Field {
                        name,
                        field_type: field_type.clone(),
                        visibility: visibility.clone(),
                    });
                }
            }
        }
        if !ranges.is_empty() {
            metadata["field_name_ranges"] = serde_json::Value::Object(ranges);
        }
        Ok(fields)
    }

    /// Each `enum_constant` becomes a field on the owning Enum symbol.
    fn extract_enum_fields(&self, enum_node: Node, source: &[u8]) -> Result<Vec<Field>> {
        let mut fields = Vec::new();
        let mut cursor = enum_node.walk();
        let Some(body) = enum_node
            .children(&mut cursor)
            .find(|c| c.kind() == "enum_body")
        else {
            return Ok(fields);
        };

        let mut body_cursor = body.walk();
        for child in body.children(&mut body_cursor) {
            if child.kind() != "enum_constant" {
                continue;
            }
            if let Some(name_node) = child.child_by_field_name("name")
                && let Ok(name) = name_node.utf8_text(source)
            {
                // Arguments (`FOO(1, 2)`) have no dedicated Field slot, so
                // they're folded into `visibility` alongside the constant
                // marker; bodies (`FOO { void x() {} }`) are picked up
                // separately by `traverse`'s generic recursion once
                // `find_containing_type_name` learns to qualify through
                // `enum_constant` bodies (`Enum.FOO.x`).
                let visibility = match child
                    .child_by_field_name("arguments")
                    .and_then(|n| n.utf8_text(source).ok())
                {
                    Some(args) => Some(format!("enum_constant{args}")),
                    None => Some("enum_constant".to_string()),
                };
                fields.push(Field {
                    name: name.to_string(),
                    field_type: None,
                    visibility,
                });
            }
        }
        Ok(fields)
    }

    /// `constant_declaration` inside an `interface_body` or
    /// `annotation_type_body` becomes a field on the owning Interface/Annotation
    /// symbol (mirrors `extract_class_fields`).
    fn extract_interface_constants(&self, node: Node, source: &[u8]) -> Result<Vec<Field>> {
        let mut fields = Vec::new();
        let mut cursor = node.walk();
        let Some(body) = node
            .children(&mut cursor)
            .find(|c| matches!(c.kind(), "interface_body" | "annotation_type_body"))
        else {
            return Ok(fields);
        };

        let mut body_cursor = body.walk();
        for child in body.children(&mut body_cursor) {
            if child.kind() != "constant_declaration" {
                continue;
            }
            let field_type = child
                .child_by_field_name("type")
                .and_then(|n| n.utf8_text(source).ok())
                .map(str::to_string);

            let mut decl_cursor = child.walk();
            for decl in child.children(&mut decl_cursor) {
                if decl.kind() == "variable_declarator"
                    && let Some(name_node) = decl.child_by_field_name("name")
                {
                    let name = name_node.utf8_text(source)?.to_string();
                    fields.push(Field {
                        name,
                        field_type: field_type.clone(),
                        visibility: None,
                    });
                }
            }
        }
        Ok(fields)
    }

    /// Record components (declared in the record's `parameters` field, just
    /// like a method/constructor parameter list) become fields.
    fn extract_record_fields(&self, record_node: Node, source: &[u8]) -> Result<Vec<Field>> {
        let params = self.extract_parameters(record_node, source)?;
        Ok(params
            .into_iter()
            .map(|p| Field {
                name: p.name,
                field_type: p.param_type,
                visibility: None,
            })
            .collect())
    }

    /// Generic extractor for `class_declaration` / `interface_declaration` /
    /// `enum_declaration` / `annotation_type_declaration`.
    pub(crate) fn extract_type(
        &self,
        node: Node,
        source: &[u8],
        file_path: &str,
        symbol_type: SymbolType,
        ctx: &ExtractCtx,
    ) -> Result<Symbol> {
        let name_node = node
            .child_by_field_name("name")
            .ok_or_else(|| Error::ParseError {
                file: file_path.into(),
                line: node.start_position().row + 1,
                message: "Type missing name".to_string(),
            })?;
        let name = name_node.utf8_text(source)?.to_string();

        let modifiers = self.collect_keyword_modifiers(node, source);

        let mut metadata = serde_json::json!({ "language": "java" });
        apply_name_range(node, source, &mut metadata);
        let fields = match symbol_type {
            SymbolType::Class => self.extract_class_fields(node, source, &mut metadata)?,
            SymbolType::Enum => self.extract_enum_fields(node, source)?,
            SymbolType::Interface | SymbolType::Annotation => {
                self.extract_interface_constants(node, source)?
            }
            _ => vec![],
        };

        let qualified_name = Some(self.type_node_qualified_name(node, source, ctx));

        if let Some(tp) = self.type_parameters_text(node, source) {
            metadata["type_params"] = serde_json::Value::String(tp);
        }

        let mut symbol = Symbol {
            name,
            symbol_type,
            qualified_name,
            location: source_location(node, file_path),
            signature: None,
            return_type: None,
            parameters: vec![],
            fields,
            modifiers,
            documentation: None,
            metadata,
            ..Default::default()
        };
        record_symbol_spans(&mut symbol, node, Some(name_node));
        Ok(symbol)
    }

    /// Walk type declarations and synthesize Lombok-generated members into `symbols`.
    pub(crate) fn apply_lombok_synthesis(
        &self,
        root: Node,
        source: &[u8],
        file_path: &str,
        ctx: &ExtractCtx,
        symbols: &mut Vec<Symbol>,
    ) {
        let mut stack = vec![root];
        while let Some(node) = stack.pop() {
            if matches!(node.kind(), "class_declaration" | "enum_declaration") {
                self.synthesize_lombok_for_type(node, source, file_path, ctx, symbols);
            }
            let mut cursor = node.walk();
            let children: Vec<Node> = node.children(&mut cursor).collect();
            for child in children.into_iter().rev() {
                stack.push(child);
            }
        }
    }

    fn annotation_simple_names(&self, node: Node, source: &[u8]) -> Vec<String> {
        let mut cursor = node.walk();
        let Some(modifiers) = node.children(&mut cursor).find(|c| c.kind() == "modifiers") else {
            return Vec::new();
        };
        let (_, anns) = self.split_modifiers(modifiers, source);
        anns.into_iter()
            .filter_map(|a| {
                a.child_by_field_name("name")
                    .and_then(|n| n.utf8_text(source).ok())
                    .map(|raw| raw.rsplit('.').next().unwrap_or(raw).to_string())
            })
            .collect()
    }

    fn field_is_final(visibility: Option<&str>) -> bool {
        visibility
            .map(|v| v.split_whitespace().any(|t| t == "final"))
            .unwrap_or(false)
    }

    fn field_is_static(visibility: Option<&str>) -> bool {
        visibility
            .map(|v| v.split_whitespace().any(|t| t == "static"))
            .unwrap_or(false)
    }

    fn lombok_getter_name(field_name: &str, field_type: Option<&str>) -> String {
        let is_bool = field_type
            .map(|t| {
                let base = strip_java_type_sugar(t);
                base == "boolean" || base == "Boolean"
            })
            .unwrap_or(false);
        if is_bool {
            if field_name.starts_with("is")
                && field_name.len() > 2
                && field_name
                    .chars()
                    .nth(2)
                    .map(|c| c.is_ascii_uppercase())
                    .unwrap_or(false)
            {
                return field_name.to_string();
            }
            return format!(
                "is{}{}",
                field_name
                    .chars()
                    .next()
                    .map(|c| c.to_ascii_uppercase())
                    .unwrap_or_default(),
                field_name.chars().skip(1).collect::<String>()
            );
        }
        format!(
            "get{}{}",
            field_name
                .chars()
                .next()
                .map(|c| c.to_ascii_uppercase())
                .unwrap_or_default(),
            field_name.chars().skip(1).collect::<String>()
        )
    }

    fn lombok_setter_name(field_name: &str) -> String {
        format!(
            "set{}{}",
            field_name
                .chars()
                .next()
                .map(|c| c.to_ascii_uppercase())
                .unwrap_or_default(),
            field_name.chars().skip(1).collect::<String>()
        )
    }

    fn synthesize_lombok_for_type(
        &self,
        node: Node,
        source: &[u8],
        file_path: &str,
        ctx: &ExtractCtx,
        symbols: &mut Vec<Symbol>,
    ) {
        let class_anns: HashSet<String> = self
            .annotation_simple_names(node, source)
            .into_iter()
            .collect();
        let wants_getter = ["Getter", "Data", "Value"]
            .iter()
            .any(|a| class_anns.contains(*a));
        let wants_setter = ["Setter", "Data"].iter().any(|a| class_anns.contains(*a));
        let wants_builder = class_anns.contains("Builder");
        let wants_required_ctor = ["RequiredArgsConstructor", "Data"]
            .iter()
            .any(|a| class_anns.contains(*a));
        let wants_all_ctor = ["AllArgsConstructor", "Value"]
            .iter()
            .any(|a| class_anns.contains(*a));
        let wants_no_ctor = class_anns.contains("NoArgsConstructor");
        let wants_log = ["Slf4j", "Log4j2", "Log", "CommonsLog"]
            .iter()
            .any(|a| class_anns.contains(*a));

        if !(wants_getter
            || wants_setter
            || wants_builder
            || wants_required_ctor
            || wants_all_ctor
            || wants_no_ctor
            || wants_log
            || class_anns.iter().any(|a| a == "Getter" || a == "Setter"))
        {
            // Still may have field-level @Getter/@Setter only.
            let mut has_field_lombok = false;
            if let Some(body) = {
                let mut c = node.walk();
                node.children(&mut c)
                    .find(|ch| ch.kind() == "class_body" || ch.kind() == "enum_body")
            } {
                let mut bc = body.walk();
                for child in body.children(&mut bc) {
                    if child.kind() == "field_declaration" {
                        let fans: HashSet<String> = self
                            .annotation_simple_names(child, source)
                            .into_iter()
                            .collect();
                        if fans.contains("Getter") || fans.contains("Setter") {
                            has_field_lombok = true;
                            break;
                        }
                    }
                }
            }
            if !has_field_lombok {
                return;
            }
        }

        let owner_qn = self.type_node_qualified_name(node, source, ctx);
        let owner_simple = node
            .child_by_field_name("name")
            .and_then(|n| n.utf8_text(source).ok())
            .unwrap_or("")
            .to_string();
        if owner_simple.is_empty() {
            return;
        }

        let class_idx = symbols.iter().position(|s| {
            matches!(s.symbol_type, SymbolType::Class | SymbolType::Enum)
                && (s.qualified_name.as_deref() == Some(owner_qn.as_str())
                    || s.name == owner_simple)
        });
        let Some(class_idx) = class_idx else {
            return;
        };

        let declared: HashSet<String> = symbols
            .iter()
            .filter(|s| {
                s.symbol_type == SymbolType::Function
                    && s.qualified_name
                        .as_deref()
                        .and_then(|qn| qn.rsplit_once('.').map(|(o, _)| o))
                        == Some(owner_qn.as_str())
            })
            .map(|s| s.name.clone())
            .collect();

        let has_explicit_ctor = symbols.iter().any(|s| {
            s.symbol_type == SymbolType::Function
                && s.metadata
                    .get("is_constructor")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false)
                && s.qualified_name
                    .as_deref()
                    .map(|qn| qn.starts_with(&format!("{owner_qn}.")))
                    .unwrap_or(false)
        });

        // Collect fields with annotations from AST (visibility already on class_sym.fields).
        struct LombokFieldInfo {
            name: String,
            field_type: Option<String>,
            visibility: Option<String>,
            anns: HashSet<String>,
            has_non_null: bool,
        }
        let mut field_infos: Vec<LombokFieldInfo> = Vec::new();
        if let Some(body) = {
            let mut c = node.walk();
            node.children(&mut c)
                .find(|ch| matches!(ch.kind(), "class_body" | "enum_body"))
        } {
            let mut bc = body.walk();
            for child in body.children(&mut bc) {
                if child.kind() != "field_declaration" {
                    continue;
                }
                let fans: HashSet<String> = self
                    .annotation_simple_names(child, source)
                    .into_iter()
                    .collect();
                let field_type = child
                    .child_by_field_name("type")
                    .and_then(|n| n.utf8_text(source).ok())
                    .map(str::to_string);
                let visibility = {
                    let kw = self.collect_keyword_modifiers(child, source);
                    if kw.is_empty() {
                        None
                    } else {
                        Some(kw.join(" "))
                    }
                };
                let has_non_null = fans.contains("NonNull") || fans.contains("NotNull");
                let mut dc = child.walk();
                for decl in child.children(&mut dc) {
                    if decl.kind() == "variable_declarator"
                        && let Some(name_node) = decl.child_by_field_name("name")
                        && let Ok(name) = name_node.utf8_text(source)
                    {
                        field_infos.push(LombokFieldInfo {
                            name: name.to_string(),
                            field_type: field_type.clone(),
                            visibility: visibility.clone(),
                            anns: fans.clone(),
                            has_non_null,
                        });
                    }
                }
            }
        }

        let mut synthetics = Vec::new();
        let loc = source_location(node, file_path);

        for f in &field_infos {
            if Self::field_is_static(f.visibility.as_deref()) {
                continue;
            }
            let field_getter = wants_getter || f.anns.contains("Getter");
            let field_setter = (wants_setter || f.anns.contains("Setter"))
                && !Self::field_is_final(f.visibility.as_deref());

            if field_getter {
                let gname = Self::lombok_getter_name(&f.name, f.field_type.as_deref());
                if !declared.contains(&gname) {
                    let symbol = Symbol {
                        name: gname.clone(),
                        symbol_type: SymbolType::Function,
                        qualified_name: Some(format!("{owner_qn}.{gname}")),
                        location: loc.clone(),
                        signature: Some(format!(
                            "public {} {}()",
                            f.field_type.as_deref().unwrap_or("Object"),
                            gname
                        )),
                        return_type: f.field_type.clone(),
                        parameters: vec![],
                        fields: vec![],
                        modifiers: vec!["public".into()],
                        documentation: None,
                        metadata: serde_json::json!({
                            "language": "java",
                            "lombok_synthesized": true,
                        }),
                        ..Default::default()
                    };
                    synthetics.push(symbol);
                }
            }
            if field_setter {
                let sname = Self::lombok_setter_name(&f.name);
                if !declared.contains(&sname) {
                    let symbol = Symbol {
                        name: sname.clone(),
                        symbol_type: SymbolType::Function,
                        qualified_name: Some(format!("{owner_qn}.{sname}")),
                        location: loc.clone(),
                        signature: Some(format!(
                            "public void {}({} {})",
                            sname,
                            f.field_type.as_deref().unwrap_or("Object"),
                            f.name
                        )),
                        return_type: Some("void".into()),
                        parameters: vec![Parameter {
                            name: f.name.clone(),
                            param_type: f.field_type.clone(),
                            default_value: None,
                        }],
                        fields: vec![],
                        modifiers: vec!["public".into()],
                        documentation: None,
                        metadata: serde_json::json!({
                            "language": "java",
                            "lombok_synthesized": true,
                        }),
                        ..Default::default()
                    };
                    synthetics.push(symbol);
                }
            }
        }

        if wants_builder && !declared.contains("builder") {
            let symbol = Symbol {
                name: "builder".into(),
                symbol_type: SymbolType::Function,
                qualified_name: Some(format!("{owner_qn}.builder")),
                location: loc.clone(),
                signature: Some(format!("public static {owner_simple}Builder builder()")),
                return_type: Some(format!("{owner_simple}Builder")),
                parameters: vec![],
                fields: vec![],
                modifiers: vec!["public".into(), "static".into()],
                documentation: None,
                metadata: serde_json::json!({
                    "language": "java",
                    "lombok_synthesized": true,
                }),
                ..Default::default()
            };
            synthetics.push(symbol);
        }

        let push_ctor = |params: Vec<Parameter>, synthetics: &mut Vec<Symbol>| {
            if has_explicit_ctor || declared.contains(&owner_simple) {
                return;
            }
            let symbol = Symbol {
                name: owner_simple.clone(),
                symbol_type: SymbolType::Function,
                qualified_name: Some(format!("{owner_qn}.<init>")),
                location: loc.clone(),
                signature: Some(format!("{owner_simple}(...)")),
                return_type: None,
                parameters: params,
                fields: vec![],
                modifiers: vec!["public".into()],
                documentation: None,
                metadata: serde_json::json!({
                    "language": "java",
                    "is_constructor": true,
                    "lombok_synthesized": true,
                }),
                ..Default::default()
            };
            synthetics.push(symbol);
        };

        if wants_no_ctor && !has_explicit_ctor {
            push_ctor(vec![], &mut synthetics);
        } else if wants_all_ctor && !has_explicit_ctor {
            let params: Vec<Parameter> = field_infos
                .iter()
                .filter(|f| !Self::field_is_static(f.visibility.as_deref()))
                .map(|f| Parameter {
                    name: f.name.clone(),
                    param_type: f.field_type.clone(),
                    default_value: None,
                })
                .collect();
            push_ctor(params, &mut synthetics);
        } else if wants_required_ctor && !has_explicit_ctor {
            let params: Vec<Parameter> = field_infos
                .iter()
                .filter(|f| {
                    !Self::field_is_static(f.visibility.as_deref())
                        && (Self::field_is_final(f.visibility.as_deref()) || f.has_non_null)
                })
                .map(|f| Parameter {
                    name: f.name.clone(),
                    param_type: f.field_type.clone(),
                    default_value: None,
                })
                .collect();
            push_ctor(params, &mut synthetics);
        }

        if wants_log {
            let class_sym = &mut symbols[class_idx];
            if !class_sym.fields.iter().any(|f| f.name == "log") {
                class_sym.fields.push(Field {
                    name: "log".into(),
                    field_type: Some("Logger".into()),
                    visibility: Some("private static final".into()),
                });
            }
        }

        symbols.extend(synthetics);
    }

    /// `record_declaration`: extracted as `SymbolType::Class` with
    /// `metadata.is_record: true` per D3 (records are JVM classes).
    pub(crate) fn extract_record(
        &self,
        node: Node,
        source: &[u8],
        file_path: &str,
        ctx: &ExtractCtx,
    ) -> Result<Symbol> {
        let name_node = node
            .child_by_field_name("name")
            .ok_or_else(|| Error::ParseError {
                file: file_path.into(),
                line: node.start_position().row + 1,
                message: "Record missing name".to_string(),
            })?;
        let name = name_node.utf8_text(source)?.to_string();

        let modifiers = self.collect_keyword_modifiers(node, source);
        let fields = self.extract_record_fields(node, source)?;
        let qualified_name = Some(self.type_node_qualified_name(node, source, ctx));

        let mut metadata = serde_json::json!({ "language": "java", "is_record": true });
        if let Some(tp) = self.type_parameters_text(node, source) {
            metadata["type_params"] = serde_json::Value::String(tp);
        }

        let mut symbol = Symbol {
            name,
            symbol_type: SymbolType::Class,
            qualified_name,
            location: source_location(node, file_path),
            signature: None,
            return_type: None,
            parameters: vec![],
            fields,
            modifiers,
            documentation: None,
            metadata,
            ..Default::default()
        };
        record_symbol_spans(&mut symbol, node, Some(name_node));
        Ok(symbol)
    }

    /// `annotation_type_element_declaration` (`String value() default "";`)
    /// becomes a Function under the owning Annotation type, QN `Ann.value`.
    pub(crate) fn extract_annotation_element(
        &self,
        node: Node,
        source: &[u8],
        file_path: &str,
        ctx: &ExtractCtx,
    ) -> Result<Symbol> {
        let name_node = node
            .child_by_field_name("name")
            .ok_or_else(|| Error::ParseError {
                file: file_path.into(),
                line: node.start_position().row + 1,
                message: "Annotation element missing name".to_string(),
            })?;
        let name = name_node.utf8_text(source)?.to_string();

        let return_type = node
            .child_by_field_name("type")
            .and_then(|n| n.utf8_text(source).ok())
            .map(str::to_string);
        let qualified_name = self.method_qualified_name(node, &name, source, ctx);

        let mut metadata = serde_json::json!({
            "language": "java",
            "is_annotation_element": true,
        });
        if let Some(default_value) = node
            .child_by_field_name("value")
            .and_then(|n| n.utf8_text(source).ok())
        {
            metadata["default_value"] = serde_json::Value::String(default_value.to_string());
        }

        let mut symbol = Symbol {
            name,
            symbol_type: SymbolType::Function,
            qualified_name,
            location: source_location(node, file_path),
            signature: Some(first_line_text(node, source)?),
            return_type,
            parameters: vec![],
            fields: vec![],
            modifiers: vec![],
            documentation: None,
            metadata,
            ..Default::default()
        };
        record_symbol_spans(&mut symbol, node, Some(name_node));
        Ok(symbol)
    }

    /// `lambda_expression` becomes a synthetic Function under the nearest
    /// enclosing callable, QN `Owner.$lambda$N` (pre-order index per owner).
    pub(crate) fn extract_lambda(
        &self,
        node: Node,
        source: &[u8],
        file_path: &str,
        ctx: &ExtractCtx,
    ) -> Result<Symbol> {
        let (owner, idx) = self.lambda_index(node, source, ctx);
        let name = format!("$lambda${idx}");
        let qualified_name = owner.map(|o| format!("{o}.{name}"));

        let mut symbol = Symbol {
            name,
            symbol_type: SymbolType::Function,
            qualified_name,
            location: source_location(node, file_path),
            signature: Some(first_line_text(node, source)?),
            return_type: None,
            parameters: vec![],
            fields: vec![],
            modifiers: vec![],
            documentation: None,
            metadata: serde_json::json!({ "language": "java", "is_lambda": true }),
            ..Default::default()
        };
        record_symbol_spans(&mut symbol, node, node.child_by_field_name("name"));
        Ok(symbol)
    }

    pub(crate) fn extract_package_symbol(
        &self,
        node: Node,
        source: &[u8],
        file_path: &str,
    ) -> Result<Symbol> {
        let mut cursor = node.walk();
        let name_node = node
            .children(&mut cursor)
            .find(|c| c.kind() == "identifier" || c.kind() == "scoped_identifier");
        let name = name_node
            .and_then(|n| n.utf8_text(source).ok())
            .unwrap_or("")
            .to_string();

        let mut symbol = Symbol {
            name: name.clone(),
            symbol_type: SymbolType::Module,
            qualified_name: Some(name),
            location: source_location(node, file_path),
            signature: None,
            return_type: None,
            parameters: vec![],
            fields: vec![],
            modifiers: vec![],
            documentation: None,
            metadata: serde_json::json!({ "language": "java", "java_kind": "package" }),
            ..Default::default()
        };
        record_symbol_spans(&mut symbol, node, name_node);
        Ok(symbol)
    }

    /// `module-info.java`'s `module_declaration`. Directives (`requires`,
    /// `exports`, `opens`, `uses`, `provides`) are recorded as best-effort
    /// string lists in metadata for GQL inspection (per D4).
    pub(crate) fn extract_module_symbol(
        &self,
        node: Node,
        source: &[u8],
        file_path: &str,
    ) -> Result<Symbol> {
        let name = node
            .child_by_field_name("name")
            .and_then(|n| n.utf8_text(source).ok())
            .unwrap_or("")
            .to_string();

        let mut requires = Vec::new();
        let mut exports = Vec::new();
        let mut opens = Vec::new();
        let mut uses = Vec::new();
        let mut provides = Vec::new();

        if let Some(body) = node.child_by_field_name("body") {
            let mut cursor = body.walk();
            for directive in body.children(&mut cursor) {
                match directive.kind() {
                    "requires_module_directive" => {
                        if let Some(m) = directive
                            .child_by_field_name("module")
                            .and_then(|n| n.utf8_text(source).ok())
                        {
                            requires.push(m.to_string());
                        }
                    }
                    "exports_module_directive" => {
                        if let Some(p) = directive
                            .child_by_field_name("package")
                            .and_then(|n| n.utf8_text(source).ok())
                        {
                            exports.push(p.to_string());
                        }
                    }
                    "opens_module_directive" => {
                        if let Some(p) = directive
                            .child_by_field_name("package")
                            .and_then(|n| n.utf8_text(source).ok())
                        {
                            opens.push(p.to_string());
                        }
                    }
                    "uses_module_directive" => {
                        if let Some(t) = directive
                            .child_by_field_name("type")
                            .and_then(|n| n.utf8_text(source).ok())
                        {
                            uses.push(t.to_string());
                        }
                    }
                    "provides_module_directive" => {
                        if let Some(p) = directive
                            .child_by_field_name("provided")
                            .and_then(|n| n.utf8_text(source).ok())
                        {
                            provides.push(p.to_string());
                        }
                    }
                    _ => {}
                }
            }
        }

        let mut symbol = Symbol {
            name: name.clone(),
            symbol_type: SymbolType::Module,
            qualified_name: Some(name),
            location: source_location(node, file_path),
            signature: None,
            return_type: None,
            parameters: vec![],
            fields: vec![],
            modifiers: vec![],
            documentation: None,
            metadata: serde_json::json!({
                "language": "java",
                "java_kind": "jpms",
                "requires": requires,
                "exports": exports,
                "opens": opens,
                "uses": uses,
                "provides": provides,
            }),
            ..Default::default()
        };
        record_symbol_spans(&mut symbol, node, node.child_by_field_name("name"));
        Ok(symbol)
    }

    fn symbols_from_tree(
        &self,
        root: Node,
        source: &[u8],
        file_path: &Path,
    ) -> Result<Vec<Symbol>> {
        let ctx = ExtractCtx::new(root, source);
        let mut symbols = Vec::new();
        self.traverse(
            root,
            source,
            &file_path.to_string_lossy(),
            &ctx,
            &mut symbols,
        )?;
        self.apply_lombok_synthesis(
            root,
            source,
            &file_path.to_string_lossy(),
            &ctx,
            &mut symbols,
        );
        // Emit Variable nodes for annotated fields so `find --annotation` and
        // AnnotatedWith edges resolve without requiring `--with-cfg` field materialization.
        self.emit_annotated_field_variables(
            root,
            source,
            &file_path.to_string_lossy(),
            &ctx,
            None,
            &mut symbols,
        )?;
        let same = Self::same_file_types(&symbols);
        self.enrich_symbol_type_fqns(&ctx, &same, &mut symbols);
        Ok(symbols)
    }

    /// True when a `field_declaration` carries at least one non-noise annotation
    /// (excluding Lombok/nullability/compiler markers that would inflate large corpora).
    pub(crate) fn has_meaningful_field_annotation(&self, node: Node, source: &[u8]) -> bool {
        let mut cursor = node.walk();
        let Some(modifiers) = node.children(&mut cursor).find(|c| c.kind() == "modifiers") else {
            return false;
        };
        let mut mcursor = modifiers.walk();
        for child in modifiers.children(&mut mcursor) {
            if !matches!(child.kind(), "annotation" | "marker_annotation") {
                continue;
            }
            let Some(name_node) = child.child_by_field_name("name") else {
                continue;
            };
            let Ok(raw) = name_node.utf8_text(source) else {
                continue;
            };
            let simple = raw.rsplit('.').next().unwrap_or(raw);
            if !matches!(
                simple,
                "NonNull"
                    | "NotNull"
                    | "Nullable"
                    | "Getter"
                    | "Setter"
                    | "SuppressWarnings"
                    | "Deprecated"
                    | "Override"
                    | "Serial"
                    | "ToString"
                    | "EqualsAndHashCode"
                    | "Builder"
                    | "With"
                    | "Singular"
            ) {
                return true;
            }
        }
        false
    }

    /// Push `Variable` symbols for a single annotated `field_declaration` node.
    pub(crate) fn push_field_declaration_variables(
        &self,
        node: Node,
        source: &[u8],
        file_path: &str,
        owner_qn: Option<&str>,
        symbols: &mut Vec<Symbol>,
    ) {
        if !self.has_meaningful_field_annotation(node, source) {
            return;
        }
        let field_type = node
            .child_by_field_name("type")
            .and_then(|n| n.utf8_text(source).ok())
            .map(str::to_string);
        let kw = self.collect_keyword_modifiers(node, source);
        let visibility = if kw.is_empty() {
            None
        } else {
            Some(kw.join(" "))
        };
        let mut decl_cursor = node.walk();
        for decl in node.children(&mut decl_cursor) {
            if decl.kind() != "variable_declarator" {
                continue;
            }
            let Some(name_node) = decl.child_by_field_name("name") else {
                continue;
            };
            let Ok(name) = name_node.utf8_text(source) else {
                continue;
            };
            let qn = match owner_qn {
                Some(o) if !o.is_empty() => format!("{o}.{name}"),
                _ => name.to_string(),
            };
            let mut metadata = serde_json::json!({
                "language": "java",
                "kind": "field",
            });
            if let Some(ref ft) = field_type {
                metadata["field_type"] = serde_json::Value::String(ft.clone());
            }
            if let Some(ref vis) = visibility {
                metadata["visibility"] = serde_json::Value::String(vis.clone());
            }
            if let Some(o) = owner_qn.filter(|o| !o.is_empty()) {
                metadata["member_of"] = serde_json::Value::String(o.to_string());
            }
            let mut symbol = Symbol {
                name: name.to_string(),
                symbol_type: SymbolType::Variable,
                qualified_name: Some(qn),
                location: source_location(node, file_path),
                signature: None,
                return_type: field_type.clone(),
                parameters: vec![],
                fields: vec![],
                modifiers: kw.clone(),
                documentation: None,
                metadata,
                ..Default::default()
            };
            record_symbol_spans(&mut symbol, node, Some(name_node));
            symbols.push(symbol);
        }
    }

    /// Materialize `Variable` symbols for fields that carry annotations (legacy `extract_symbols` path).
    pub(crate) fn emit_annotated_field_variables(
        &self,
        node: Node,
        source: &[u8],
        file_path: &str,
        ctx: &ExtractCtx,
        owner_qn: Option<&str>,
        symbols: &mut Vec<Symbol>,
    ) -> Result<()> {
        match node.kind() {
            "field_declaration" => {
                self.push_field_declaration_variables(node, source, file_path, owner_qn, symbols);
                return Ok(());
            }
            "class_declaration"
            | "interface_declaration"
            | "enum_declaration"
            | "record_declaration" => {
                let qn = self.type_node_qualified_name(node, source, ctx);
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    if matches!(
                        child.kind(),
                        "class_body" | "interface_body" | "enum_body" | "enum_body_declarations"
                    ) {
                        self.emit_annotated_field_variables(
                            child,
                            source,
                            file_path,
                            ctx,
                            Some(&qn),
                            symbols,
                        )?;
                    }
                }
                return Ok(());
            }
            "program"
            | "class_body"
            | "interface_body"
            | "enum_body"
            | "enum_body_declarations" => {
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    self.emit_annotated_field_variables(
                        child, source, file_path, ctx, owner_qn, symbols,
                    )?;
                }
            }
            _ => {}
        }
        Ok(())
    }

    pub(crate) fn collect_non_call_relations(
        &self,
        root: Node,
        source: &[u8],
        file_path: &Path,
        ctx: &ExtractCtx,
        symbols: &[Symbol],
        relations: &mut Vec<Relation>,
    ) -> Result<()> {
        let same_file_types = Self::same_file_types(symbols);
        self.extract_inheritance(root, source, file_path, ctx, &same_file_types, relations)?;
        self.extract_annotated_with(root, source, file_path, ctx, &same_file_types, relations)?;
        self.extract_object_creation(root, source, file_path, ctx, &same_file_types, relations)?;
        self.extract_method_references(root, source, file_path, ctx, &same_file_types, relations)?;
        self.extract_ctor_chaining(root, source, file_path, ctx, relations)?;
        self.extract_import_uses(root, source, file_path, ctx, relations)?;
        self.extract_field_access(root, source, file_path, ctx, &same_file_types, relations)?;
        self.extract_array_creation(root, source, file_path, ctx, &same_file_types, relations)?;
        self.extract_class_literal(root, source, file_path, ctx, &same_file_types, relations)?;
        self.extract_lambda_calls(root, source, file_path, ctx, relations)?;
        self.extract_throws_refs(root, source, file_path, ctx, &same_file_types, relations)?;
        self.extract_module_relations(root, source, file_path, relations)?;
        Ok(())
    }

    fn relations_from_tree(
        &self,
        root: Node,
        source: &[u8],
        file_path: &Path,
        symbols: &[Symbol],
    ) -> Result<Vec<Relation>> {
        let ctx = ExtractCtx::new(root, source);
        let mut relations = Vec::new();

        self.extract_calls(root, source, file_path, symbols, &mut relations)?;
        self.collect_non_call_relations(root, source, file_path, &ctx, symbols, &mut relations)?;

        Ok(relations)
    }

    fn traverse(
        &self,
        node: Node,
        source: &[u8],
        file_path: &str,
        ctx: &ExtractCtx,
        symbols: &mut Vec<Symbol>,
    ) -> Result<()> {
        match node.kind() {
            "method_declaration" => {
                symbols.push(self.extract_method(node, source, file_path, ctx)?)
            }
            "constructor_declaration" => {
                symbols.push(self.extract_constructor(node, source, file_path, ctx)?)
            }
            "compact_constructor_declaration" => {
                symbols.push(self.extract_compact_constructor(node, source, file_path, ctx)?)
            }
            "class_declaration" => {
                symbols.push(self.extract_type(node, source, file_path, SymbolType::Class, ctx)?)
            }
            "interface_declaration" => symbols.push(self.extract_type(
                node,
                source,
                file_path,
                SymbolType::Interface,
                ctx,
            )?),
            "enum_declaration" => {
                symbols.push(self.extract_type(node, source, file_path, SymbolType::Enum, ctx)?)
            }
            "annotation_type_declaration" => symbols.push(self.extract_type(
                node,
                source,
                file_path,
                SymbolType::Annotation,
                ctx,
            )?),
            "record_declaration" => {
                symbols.push(self.extract_record(node, source, file_path, ctx)?)
            }
            "annotation_type_element_declaration" => {
                symbols.push(self.extract_annotation_element(node, source, file_path, ctx)?)
            }
            "lambda_expression" => symbols.push(self.extract_lambda(node, source, file_path, ctx)?),
            "package_declaration" => {
                symbols.push(self.extract_package_symbol(node, source, file_path)?)
            }
            "module_declaration" => {
                symbols.push(self.extract_module_symbol(node, source, file_path)?)
            }
            "static_initializer" => {
                symbols.push(self.extract_static_initializer(node, source, file_path, ctx)?)
            }
            "block" if node.parent().map(|p| p.kind()) == Some("class_body") => {
                symbols.push(self.extract_instance_initializer(node, source, file_path, ctx)?)
            }
            "import_declaration" => {
                let text = node.utf8_text(source)?.trim().to_string();
                let mut symbol = Symbol {
                    name: text.clone(),
                    symbol_type: SymbolType::Import,
                    qualified_name: None,
                    location: SourceLocation {
                        file: file_path.to_string(),
                        start_line: node.start_position().row + 1,
                        end_line: node.end_position().row + 1,
                        start_column: 0,
                        end_column: 0,
                    },
                    signature: None,
                    return_type: None,
                    parameters: vec![],
                    fields: vec![],
                    modifiers: vec![],
                    documentation: None,
                    metadata: serde_json::json!({ "language": "java" }),
                    ..Default::default()
                };
                record_symbol_spans(&mut symbol, node, node.child_by_field_name("name"));
                symbols.push(symbol);
            }
            _ => {}
        }

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.traverse(child, source, file_path, ctx, symbols)?;
        }
        Ok(())
    }
}

impl LanguagePlugin for JavaPlugin {
    fn language_id(&self) -> &str {
        "java"
    }

    fn file_extensions(&self) -> Vec<&str> {
        vec!["java"]
    }

    fn grammar(&self) -> Option<tree_sitter::Language> {
        Some(tree_sitter_java::LANGUAGE.into())
    }

    fn extract_symbols(&self, file_path: &Path, source: &[u8]) -> Result<Vec<Symbol>> {
        let tree =
            parse_with_pool("java", tree_sitter_java::LANGUAGE.into(), source).map_err(|e| {
                Error::ParseError {
                    file: file_path.to_path_buf(),
                    line: 0,
                    message: e,
                }
            })?;
        self.symbols_from_tree(tree.root_node(), source, file_path)
    }

    fn extract_relations(
        &self,
        file_path: &Path,
        source: &[u8],
        symbols: &[Symbol],
    ) -> Result<Vec<Relation>> {
        let tree =
            parse_with_pool("java", tree_sitter_java::LANGUAGE.into(), source).map_err(|e| {
                Error::ParseError {
                    file: file_path.to_path_buf(),
                    line: 0,
                    message: e,
                }
            })?;
        let root = tree.root_node();
        let mut relations = self.relations_from_tree(root, source, file_path, symbols)?;
        let mut sym_copy = symbols.to_vec();
        rgctl_plugin_helpers::apply_framework_catalog(
            "java",
            source,
            Some(root),
            &mut sym_copy,
            &mut relations,
        );
        Ok(relations)
    }

    fn extract_all(&self, file_path: &Path, source: &[u8]) -> Result<ExtractAllResult> {
        extract_all_single_pass(self, file_path, source)
    }

    fn declared_ast_handlers(&self) -> Option<&AstHandlerTable> {
        Some(java_ast_handlers())
    }

    fn calculate_complexity(
        &self,
        symbol: &Symbol,
        source: &[u8],
    ) -> Result<Option<ComplexityMetrics>> {
        if symbol.symbol_type != SymbolType::Function {
            return Ok(None);
        }

        if let Some(metrics) = complexity_from_metadata(symbol) {
            return Ok(Some(metrics));
        }

        let tree =
            parse_with_pool("java", tree_sitter_java::LANGUAGE.into(), source).map_err(|e| {
                Error::ParseError {
                    file: symbol.location.file.clone().into(),
                    line: symbol.location.start_line,
                    message: e,
                }
            })?;
        let root = tree.root_node();
        let target_row = symbol.location.start_line.saturating_sub(1);

        fn find_at_row(node: Node, row: usize) -> Option<Node> {
            if matches!(
                node.kind(),
                "method_declaration"
                    | "constructor_declaration"
                    | "compact_constructor_declaration"
                    | "static_initializer"
            ) && node.start_position().row == row
            {
                return Some(node);
            }
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if let Some(found) = find_at_row(child, row) {
                    return Some(found);
                }
            }
            None
        }

        let Some(func_node) = find_at_row(root, target_row) else {
            return Ok(None);
        };

        Ok(Some(ComplexityMetrics {
            cyclomatic: self.java_cyclomatic(func_node),
            cognitive: self.java_cognitive(func_node),
            loc: ComplexityCalculator::loc(func_node),
            parameters: symbol.parameters.len(),
            nesting_depth: ComplexityCalculator::nesting_depth(func_node, NESTING_CONTAINER_KINDS),
            returns: ComplexityCalculator::return_count(func_node, "return_statement"),
        }))
    }
}

impl JavaPlugin {
    /// Cyclomatic complexity: decision points (if/while/for/do/catch, `&&`,
    /// `||`, `case` labels) + 1.
    fn java_cyclomatic(&self, node: Node) -> usize {
        let mut complexity = 1;

        fn walk(node: Node, complexity: &mut usize) {
            match node.kind() {
                k if BRANCH_KINDS.contains(&k) => *complexity += 1,
                "binary_expression" => {
                    if let Some(op) = node.child_by_field_name("operator")
                        && matches!(op.kind(), "&&" | "||")
                    {
                        *complexity += 1;
                    }
                }
                "switch_label" => {
                    let mut cursor = node.walk();
                    if node.children(&mut cursor).next().is_some() {
                        // Has an expression/pattern child => a `case`, not `default`.
                        *complexity += 1;
                    }
                }
                _ => {}
            }
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                walk(child, complexity);
            }
        }

        walk(node, &mut complexity);
        complexity
    }

    /// Cognitive complexity: like cyclomatic but weighted by nesting depth.
    fn java_cognitive(&self, node: Node) -> usize {
        let mut cognitive = 0;

        fn walk(node: Node, cognitive: &mut usize, nesting: usize) {
            let next_nesting = match node.kind() {
                "if_statement"
                | "while_statement"
                | "for_statement"
                | "enhanced_for_statement"
                | "do_statement" => {
                    *cognitive += 1 + nesting;
                    nesting + 1
                }
                "catch_clause" => {
                    *cognitive += 1 + nesting;
                    nesting
                }
                "binary_expression" => {
                    if let Some(op) = node.child_by_field_name("operator")
                        && matches!(op.kind(), "&&" | "||")
                    {
                        *cognitive += 1;
                    }
                    nesting
                }
                _ => nesting,
            };
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                walk(child, cognitive, next_nesting);
            }
        }

        walk(node, &mut cognitive, 0);
        cognitive
    }

    /// Extract method call relationships
    fn extract_calls(
        &self,
        node: Node,
        source: &[u8],
        file_path: &Path,
        symbols: &[Symbol],
        relations: &mut Vec<Relation>,
    ) -> Result<()> {
        let mut cursor = node.walk();

        // Find the containing method for any calls we find
        let containing_method = self.find_containing_method(node, source, symbols);

        // Look for method invocations
        if node.kind() == "method_invocation"
            && let Some(from_method) = &containing_method
        {
            // Extract the method name being called
            if let Some(method_name_node) = node.child_by_field_name("name") {
                let simple_name = method_name_node.utf8_text(source).unwrap_or("").to_string();

                if !simple_name.is_empty() {
                    // Try to find the qualified name from symbols in this file
                    let local_qualified = symbols
                        .iter()
                        .find(|s| s.name == simple_name && s.symbol_type == SymbolType::Function)
                        .and_then(|s| s.qualified_name.as_ref())
                        .cloned();

                    // Best-effort: try to infer the target class from the object
                    // For example: helper.transform() → infer "Helper" class
                    let (to_qualified_hint, to_type_hint) =
                        if let Some(object_node) = node.child_by_field_name("object") {
                            self.infer_method_target(object_node, &simple_name, source, node)
                        } else {
                            (None, None)
                        };

                    relations.push(Relation {
                        from: from_method.clone(),
                        to: local_qualified.unwrap_or(simple_name.clone()),
                        relation_type: RelationType::Calls,
                        location: SourceLocation {
                            file: file_path.to_string_lossy().to_string(),
                            start_line: node.start_position().row + 1,
                            end_line: node.end_position().row + 1,
                            start_column: node.start_position().column,
                            end_column: node.end_position().column,
                        },
                        metadata: serde_json::json!({ "language": "java" }),
                        to_qualified_hint,
                        to_type_hint,
                        ..Default::default()
                    });
                }
            }
        }

        // Recurse into children
        for child in node.children(&mut cursor) {
            self.extract_calls(child, source, file_path, symbols, relations)?;
        }

        Ok(())
    }

    /// Extract inheritance relationships: `extends`/`implements` (classes),
    /// `extends` (interfaces, via `extends_interfaces`), and `permits`
    /// (sealed classes/interfaces).
    fn extract_inheritance(
        &self,
        node: Node,
        source: &[u8],
        file_path: &Path,
        ctx: &ExtractCtx,
        same_file_types: &HashSet<String>,
        relations: &mut Vec<Relation>,
    ) -> Result<()> {
        let mut cursor = node.walk();

        // Handle class declarations
        if node.kind() == "class_declaration" {
            let class_name = self.find_class_name(node, source)?;
            let owner = ctx.qualify_type(&class_name);

            // Look for "extends" clause
            if let Some(superclass) = node.child_by_field_name("superclass") {
                // The superclass node contains "extends" keyword and type_identifier
                let mut sc_cursor = superclass.walk();
                for child in superclass.children(&mut sc_cursor) {
                    if matches!(
                        child.kind(),
                        "type_identifier" | "generic_type" | "scoped_type_identifier"
                    ) {
                        let Some(parent_class) = Self::inheritance_type_name(child, source) else {
                            continue;
                        };
                        let mut rel = Relation {
                            from: owner.clone(),
                            to: parent_class.clone(),
                            relation_type: RelationType::Extends,
                            location: SourceLocation {
                                file: file_path.to_string_lossy().to_string(),
                                start_line: child.start_position().row + 1,
                                end_line: child.end_position().row + 1,
                                start_column: child.start_position().column,
                                end_column: child.end_position().column,
                            },
                            metadata: serde_json::json!({ "language": "java" }),
                            to_qualified_hint: ctx
                                .resolve_type_fqn(&parent_class, same_file_types)
                                .or_else(|| ctx.import_qualified_hint(&parent_class)),
                            to_type_hint: None,
                            ..Default::default()
                        };
                        record_call_site_spans(&mut rel, child);
                        relations.push(rel);
                    }
                }
            }

            // Look for "implements" clause
            if let Some(interfaces) = node.child_by_field_name("interfaces") {
                let mut impl_cursor = interfaces.walk();
                for interface_node in interfaces.children(&mut impl_cursor) {
                    // Handle type_list which contains the actual type identifiers
                    if interface_node.kind() == "type_list" {
                        let mut type_cursor = interface_node.walk();
                        for type_node in interface_node.children(&mut type_cursor) {
                            if matches!(
                                type_node.kind(),
                                "type_identifier" | "generic_type" | "scoped_type_identifier"
                            ) {
                                let Some(interface_name) =
                                    Self::inheritance_type_name(type_node, source)
                                else {
                                    continue;
                                };
                                let mut rel = Relation {
                                    from: owner.clone(),
                                    to: interface_name.clone(),
                                    relation_type: RelationType::Implements,
                                    location: SourceLocation {
                                        file: file_path.to_string_lossy().to_string(),
                                        start_line: type_node.start_position().row + 1,
                                        end_line: type_node.end_position().row + 1,
                                        start_column: type_node.start_position().column,
                                        end_column: type_node.end_position().column,
                                    },
                                    metadata: serde_json::json!({ "language": "java" }),
                                    to_qualified_hint: ctx
                                        .resolve_type_fqn(&interface_name, same_file_types)
                                        .or_else(|| ctx.import_qualified_hint(&interface_name)),
                                    to_type_hint: None,
                                    ..Default::default()
                                };
                                record_call_site_spans(&mut rel, type_node);
                                relations.push(rel);
                            }
                        }
                    }
                    // Also handle direct type identifiers
                    else if matches!(
                        interface_node.kind(),
                        "type_identifier" | "generic_type" | "scoped_type_identifier"
                    ) {
                        let Some(interface_name) =
                            Self::inheritance_type_name(interface_node, source)
                        else {
                            continue;
                        };
                        let mut rel = Relation {
                            from: owner.clone(),
                            to: interface_name.clone(),
                            relation_type: RelationType::Implements,
                            location: SourceLocation {
                                file: file_path.to_string_lossy().to_string(),
                                start_line: interface_node.start_position().row + 1,
                                end_line: interface_node.end_position().row + 1,
                                start_column: interface_node.start_position().column,
                                end_column: interface_node.end_position().column,
                            },
                            metadata: serde_json::json!({ "language": "java" }),
                            to_qualified_hint: ctx
                                .resolve_type_fqn(&interface_name, same_file_types)
                                .or_else(|| ctx.import_qualified_hint(&interface_name)),
                            to_type_hint: None,
                            ..Default::default()
                        };
                        record_call_site_spans(&mut rel, interface_node);
                        relations.push(rel);
                    }
                }
            }

            self.extract_permits_from(
                node,
                &owner,
                source,
                file_path,
                ctx,
                same_file_types,
                relations,
            )?;
        }

        // Handle interface declarations: `extends_interfaces` (child, not
        // field) wraps a `type_list`, and `permits` for sealed interfaces.
        if node.kind() == "interface_declaration" {
            let iface_name = self.find_class_name(node, source)?;
            let owner = ctx.qualify_type(&iface_name);

            let mut icursor = node.walk();
            if let Some(ext) = node
                .children(&mut icursor)
                .find(|c| c.kind() == "extends_interfaces")
            {
                let mut ecursor = ext.walk();
                let type_list_node = ext.children(&mut ecursor).find(|c| c.kind() == "type_list");
                if let Some(type_list) = type_list_node {
                    let mut tcursor = type_list.walk();
                    for t in type_list.children(&mut tcursor) {
                        if matches!(
                            t.kind(),
                            "type_identifier" | "generic_type" | "scoped_type_identifier"
                        ) {
                            let Some(name) = Self::inheritance_type_name(t, source) else {
                                continue;
                            };
                            let mut rel = Relation {
                                from: owner.clone(),
                                to: name.clone(),
                                relation_type: RelationType::Extends,
                                location: SourceLocation {
                                    file: file_path.to_string_lossy().to_string(),
                                    start_line: t.start_position().row + 1,
                                    end_line: t.end_position().row + 1,
                                    start_column: t.start_position().column,
                                    end_column: t.end_position().column,
                                },
                                metadata: serde_json::json!({ "language": "java" }),
                                to_qualified_hint: ctx
                                    .resolve_type_fqn(&name, same_file_types)
                                    .or_else(|| ctx.import_qualified_hint(&name)),
                                to_type_hint: None,
                                ..Default::default()
                            };
                            record_call_site_spans(&mut rel, t);
                            relations.push(rel);
                        }
                    }
                }
            }

            self.extract_permits_from(
                node,
                &owner,
                source,
                file_path,
                ctx,
                same_file_types,
                relations,
            )?;
        }

        // Recurse into children
        for child in node.children(&mut cursor) {
            self.extract_inheritance(child, source, file_path, ctx, same_file_types, relations)?;
        }

        Ok(())
    }

    /// Simple type name for inheritance relations (`Foo` from `Foo<Bar>` or `a.b.Foo`).
    fn inheritance_type_name(node: Node, source: &[u8]) -> Option<String> {
        let raw = node.utf8_text(source).ok()?.trim();
        if raw.is_empty() {
            return None;
        }
        let base = raw.split('<').next().unwrap_or(raw).trim();
        let simple = base.rsplit('.').next().unwrap_or(base).trim();
        if simple.is_empty() {
            None
        } else {
            Some(simple.to_string())
        }
    }

    /// Emit `Permits` relations for a sealed class/interface's `permits`
    /// clause (present as an optional `permits` field on both node kinds).
    #[allow(clippy::too_many_arguments)]
    fn extract_permits_from(
        &self,
        node: Node,
        owner_name: &str,
        source: &[u8],
        file_path: &Path,
        ctx: &ExtractCtx,
        same_file_types: &HashSet<String>,
        relations: &mut Vec<Relation>,
    ) -> Result<()> {
        let Some(permits) = node.child_by_field_name("permits") else {
            return Ok(());
        };
        let mut pcursor = permits.walk();
        let Some(type_list) = permits
            .children(&mut pcursor)
            .find(|c| c.kind() == "type_list")
        else {
            return Ok(());
        };
        let mut tcursor = type_list.walk();
        for t in type_list.children(&mut tcursor) {
            if matches!(
                t.kind(),
                "type_identifier" | "generic_type" | "scoped_type_identifier"
            ) && let Ok(raw) = t.utf8_text(source)
            {
                let name = raw.split('<').next().unwrap_or(raw).trim().to_string();
                let simple = name.rsplit('.').next().unwrap_or(&name).to_string();
                if !simple.is_empty() {
                    relations.push(Relation {
                        from: owner_name.to_string(),
                        to: simple.clone(),
                        relation_type: RelationType::Permits,
                        location: SourceLocation {
                            file: file_path.to_string_lossy().to_string(),
                            start_line: t.start_position().row + 1,
                            end_line: t.end_position().row + 1,
                            start_column: t.start_position().column,
                            end_column: t.end_position().column,
                        },
                        metadata: serde_json::json!({ "language": "java" }),
                        to_qualified_hint: ctx.resolve_type_fqn(&name, same_file_types),
                        to_type_hint: None,
                        ..Default::default()
                    });
                }
            }
        }
        Ok(())
    }

    /// Nearest owning symbol for a type-use annotation (`annotated_type`),
    /// walking up to the first enclosing parameter/field/method/local.
    /// Parameters and locals attach to their enclosing method (no Variable /
    /// parameter node exists) so AnnotatedWith edges resolve in the graph.
    fn annotated_type_owner(&self, node: Node, source: &[u8], ctx: &ExtractCtx) -> Option<String> {
        let mut current = node;
        while let Some(parent) = current.parent() {
            match parent.kind() {
                "formal_parameter" | "spread_parameter" => {
                    return self.find_containing_callable_qn(parent, source, ctx);
                }
                "field_declaration" => {
                    let owner = self
                        .find_containing_type_name(parent, source, ctx)
                        .map(|o| ctx.qualify_type(&o));
                    let mut c = parent.walk();
                    let field_name = parent
                        .children(&mut c)
                        .find(|n| n.kind() == "variable_declarator")
                        .and_then(|vd| vd.child_by_field_name("name"))
                        .and_then(|n| n.utf8_text(source).ok())?;
                    return match owner {
                        Some(o) => Some(format!("{o}.{field_name}")),
                        None => Some(field_name.to_string()),
                    };
                }
                "method_declaration" => {
                    let name = parent
                        .child_by_field_name("name")
                        .and_then(|n| n.utf8_text(source).ok())?;
                    return self
                        .method_qualified_name(parent, name, source, ctx)
                        .or(Some(name.to_string()));
                }
                "local_variable_declaration" => {
                    return self.find_containing_callable_qn(parent, source, ctx);
                }
                _ => {}
            }
            current = parent;
        }
        None
    }

    /// Walk annotation usage sites (types, methods, constructors, fields,
    /// parameters) and emit `AnnotatedWith` relations. Argument list text is
    /// preserved in relation metadata when present.
    fn extract_annotated_with(
        &self,
        node: Node,
        source: &[u8],
        file_path: &Path,
        ctx: &ExtractCtx,
        same_file_types: &HashSet<String>,
        relations: &mut Vec<Relation>,
    ) -> Result<()> {
        let from = match node.kind() {
            "class_declaration"
            | "interface_declaration"
            | "enum_declaration"
            | "record_declaration"
            | "annotation_type_declaration" => {
                Some(self.type_node_qualified_name(node, source, ctx))
            }
            "method_declaration" => node
                .child_by_field_name("name")
                .and_then(|n| n.utf8_text(source).ok())
                .map(str::to_string)
                .map(|name| {
                    self.method_qualified_name(node, &name, source, ctx)
                        .unwrap_or(name)
                }),
            "constructor_declaration" | "compact_constructor_declaration" => self
                .find_containing_type_name(node, source, ctx)
                .map(|owner| format!("{}.<init>", ctx.qualify_type(&owner))),
            "field_declaration" => {
                let owner = self
                    .find_containing_type_name(node, source, ctx)
                    .map(|o| ctx.qualify_type(&o));
                let mut c = node.walk();
                let field_name = node
                    .children(&mut c)
                    .find(|n| n.kind() == "variable_declarator")
                    .and_then(|vd| vd.child_by_field_name("name"))
                    .and_then(|n| n.utf8_text(source).ok())
                    .map(str::to_string);
                match (owner, field_name) {
                    (Some(o), Some(f)) => Some(format!("{o}.{f}")),
                    (None, Some(f)) => Some(f),
                    _ => None,
                }
            }
            "formal_parameter" => {
                // Attach to enclosing callable — parameter QNs are not graph nodes.
                self.find_containing_callable_qn(node, source, ctx)
            }
            _ => None,
        };

        if let Some(from) = from {
            let scope = match node.kind() {
                "formal_parameter" | "spread_parameter" => Some("Parameter"),
                "field_declaration" => Some("Field"),
                "method_declaration"
                | "constructor_declaration"
                | "compact_constructor_declaration" => Some("Method"),
                "class_declaration"
                | "interface_declaration"
                | "enum_declaration"
                | "record_declaration"
                | "annotation_type_declaration" => Some("Type"),
                _ => None,
            };
            let mut cursor = node.walk();
            let modifiers_node = node.children(&mut cursor).find(|c| c.kind() == "modifiers");
            if let Some(modifiers_node) = modifiers_node {
                let (_, annotations) = self.split_modifiers(modifiers_node, source);
                for ann in annotations {
                    self.push_annotated_with_scoped(
                        &from,
                        ann,
                        source,
                        file_path,
                        ctx,
                        same_file_types,
                        relations,
                        scope,
                    );
                }
            }
        }

        // Type-use annotations (`List<@NonNull String>`, `@NonNull Foo[]`, ...):
        // attach to the nearest owning parameter/field/method/local per D5.
        if node.kind() == "annotated_type"
            && let Some(from) = self.annotated_type_owner(node, source, ctx)
        {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if matches!(child.kind(), "annotation" | "marker_annotation") {
                    self.push_annotated_with(
                        &from,
                        child,
                        source,
                        file_path,
                        ctx,
                        same_file_types,
                        relations,
                    );
                }
            }
        }

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.extract_annotated_with(child, source, file_path, ctx, same_file_types, relations)?;
        }
        Ok(())
    }

    /// `new Type(...)` → `Instantiates` from the enclosing callable to the
    /// created type.
    fn extract_object_creation(
        &self,
        node: Node,
        source: &[u8],
        file_path: &Path,
        ctx: &ExtractCtx,
        same_file_types: &HashSet<String>,
        relations: &mut Vec<Relation>,
    ) -> Result<()> {
        if node.kind() == "object_creation_expression"
            && let Some(from) = self.find_containing_callable_qn(node, source, ctx)
            && let Some(type_node) = node.child_by_field_name("type")
            && let Ok(raw) = type_node.utf8_text(source)
        {
            let simple = raw.split('<').next().unwrap_or(raw).trim();
            let last_segment = simple.rsplit('.').next().unwrap_or(simple);
            if !last_segment.is_empty() {
                let fqn = ctx.resolve_type_fqn(simple, same_file_types);
                relations.push(Relation {
                    from,
                    to: last_segment.to_string(),
                    relation_type: RelationType::Instantiates,
                    location: source_location(node, &file_path.to_string_lossy()),
                    metadata: serde_json::json!({ "language": "java" }),
                    to_qualified_hint: fqn,
                    to_type_hint: Some(last_segment.to_string()),
                    ..Default::default()
                });
            }
        }

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.extract_object_creation(
                child,
                source,
                file_path,
                ctx,
                same_file_types,
                relations,
            )?;
        }
        Ok(())
    }

    /// `Type::method` / `obj::method` / `Type::new` → best-effort `Calls`.
    fn extract_method_references(
        &self,
        node: Node,
        source: &[u8],
        file_path: &Path,
        ctx: &ExtractCtx,
        same_file_types: &HashSet<String>,
        relations: &mut Vec<Relation>,
    ) -> Result<()> {
        if node.kind() == "method_reference"
            && let Some(from) = self.find_containing_callable_qn(node, source, ctx)
        {
            let mut cursor = node.walk();
            let children: Vec<Node> = node.children(&mut cursor).collect();
            if let (Some(first), Some(last)) = (children.first(), children.last()) {
                let owner_raw = first.utf8_text(source).ok().map(str::to_string);
                let method_name = if last.kind() == "new" {
                    "<init>".to_string()
                } else if last.kind() == "identifier" {
                    last.utf8_text(source).unwrap_or("").to_string()
                } else {
                    String::new()
                };
                if !method_name.is_empty() {
                    let owner_fqn = owner_raw.as_ref().and_then(|o| {
                        ctx.resolve_type_fqn(o, same_file_types)
                            .or_else(|| Some(o.clone()))
                    });
                    let to_type_hint = owner_fqn
                        .as_ref()
                        .map(|o| strip_java_type_sugar(o))
                        .or(owner_raw.clone());
                    let to_qualified_hint =
                        owner_fqn.as_ref().map(|o| format!("{o}.{method_name}"));
                    relations.push(Relation {
                            from,
                            to: method_name,
                            relation_type: RelationType::Calls,
                            location: source_location(node, &file_path.to_string_lossy()),
                            metadata: serde_json::json!({ "language": "java", "method_reference": true }),
                            to_qualified_hint,
                            to_type_hint,
                         ..Default::default() });
                }
            }
        }

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.extract_method_references(
                child,
                source,
                file_path,
                ctx,
                same_file_types,
                relations,
            )?;
        }
        Ok(())
    }

    /// `this(...)` / `super(...)` explicit constructor invocation → `Calls`
    /// toward a `<init>` hint.
    fn extract_ctor_chaining(
        &self,
        node: Node,
        source: &[u8],
        file_path: &Path,
        ctx: &ExtractCtx,
        relations: &mut Vec<Relation>,
    ) -> Result<()> {
        if node.kind() == "explicit_constructor_invocation"
            && let Some(ctor_field) = node.child_by_field_name("constructor")
            && let Some(from) = self.find_containing_callable_qn(node, source, ctx)
        {
            let to = if ctor_field.kind() == "super" {
                let super_name = self
                    .find_enclosing_type_node(node)
                    .and_then(|t| t.child_by_field_name("superclass"))
                    .and_then(|sc| {
                        let mut c = sc.walk();

                        sc.children(&mut c).find(|n| {
                            matches!(
                                n.kind(),
                                "type_identifier" | "generic_type" | "scoped_type_identifier"
                            )
                        })
                    })
                    .and_then(|n| n.utf8_text(source).ok())
                    .map(|s| s.split('<').next().unwrap_or(s).to_string())
                    .unwrap_or_else(|| "super".to_string());
                format!("{super_name}.<init>")
            } else {
                let owner = self
                    .find_enclosing_type_node(node)
                    .and_then(|t| t.child_by_field_name("name"))
                    .and_then(|n| n.utf8_text(source).ok())
                    .unwrap_or("")
                    .to_string();
                format!("{owner}.<init>")
            };

            relations.push(Relation {
                from,
                to,
                relation_type: RelationType::Calls,
                location: source_location(node, &file_path.to_string_lossy()),
                metadata: serde_json::json!({
                    "language": "java",
                    "ctor_chain": ctor_field.kind(),
                }),
                to_qualified_hint: None,
                to_type_hint: None,
                ..Default::default()
            });
        }

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.extract_ctor_chaining(child, source, file_path, ctx, relations)?;
        }
        Ok(())
    }

    /// `field_access` (`this.status`, `obj.status`, `Type.field`) →
    /// `References` from the enclosing callable toward a best-effort field
    /// target. `this.field` resolves the owning type; `identifier.field`
    /// tries the same field/local type lookup used by `infer_method_target`.
    fn extract_field_access(
        &self,
        node: Node,
        source: &[u8],
        file_path: &Path,
        ctx: &ExtractCtx,
        same_file_types: &HashSet<String>,
        relations: &mut Vec<Relation>,
    ) -> Result<()> {
        if node.kind() == "field_access"
            && let Some(field_node) = node.child_by_field_name("field")
        {
            let field_name = field_node.utf8_text(source).unwrap_or("");
            if field_node.kind() == "identifier"
                && !field_name.is_empty()
                && let Some(from) = self.find_containing_callable_qn(node, source, ctx)
            {
                let (to_qualified_hint, to_type_hint) = match node.child_by_field_name("object") {
                    Some(obj) if obj.kind() == "this" => self
                        .find_enclosing_type_node(node)
                        .and_then(|t| t.child_by_field_name("name"))
                        .and_then(|n| n.utf8_text(source).ok())
                        .map(|owner| {
                            let owner_fqn = ctx
                                .resolve_type_fqn(owner, same_file_types)
                                .unwrap_or_else(|| owner.to_string());
                            (
                                Some(format!("{owner_fqn}.{field_name}")),
                                Some(strip_java_type_sugar(&owner_fqn)),
                            )
                        })
                        .unwrap_or((None, None)),
                    Some(obj) if obj.kind() == "identifier" => {
                        let obj_text = obj.utf8_text(source).unwrap_or("");
                        self.find_containing_class_node(node)
                            .and_then(|c| self.find_field_type(c, obj_text, source))
                            .or_else(|| self.find_local_variable_type(node, obj_text, source))
                            .map(|owner| {
                                let owner_fqn = ctx
                                    .resolve_type_fqn(&owner, same_file_types)
                                    .unwrap_or(owner);
                                (
                                    Some(format!("{owner_fqn}.{field_name}")),
                                    Some(strip_java_type_sugar(&owner_fqn)),
                                )
                            })
                            .unwrap_or((None, None))
                    }
                    _ => (None, None),
                };
                relations.push(Relation {
                    from,
                    to: field_name.to_string(),
                    relation_type: RelationType::References,
                    location: source_location(node, &file_path.to_string_lossy()),
                    metadata: serde_json::json!({ "language": "java" }),
                    to_qualified_hint,
                    to_type_hint,
                    ..Default::default()
                });
            }
        }

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.extract_field_access(child, source, file_path, ctx, same_file_types, relations)?;
        }
        Ok(())
    }

    /// `new Type[n]` → `Instantiates` from the enclosing callable toward the
    /// array's element type.
    fn extract_array_creation(
        &self,
        node: Node,
        source: &[u8],
        file_path: &Path,
        ctx: &ExtractCtx,
        same_file_types: &HashSet<String>,
        relations: &mut Vec<Relation>,
    ) -> Result<()> {
        if node.kind() == "array_creation_expression"
            && let Some(from) = self.find_containing_callable_qn(node, source, ctx)
            && let Some(type_node) = node.child_by_field_name("type")
            && let Ok(raw) = type_node.utf8_text(source)
        {
            let simple = raw.split('<').next().unwrap_or(raw).trim();
            let last = simple.rsplit('.').next().unwrap_or(simple);
            if !last.is_empty() {
                let fqn = ctx.resolve_type_fqn(simple, same_file_types);
                relations.push(Relation {
                    from,
                    to: last.to_string(),
                    relation_type: RelationType::Instantiates,
                    location: source_location(node, &file_path.to_string_lossy()),
                    metadata: serde_json::json!({ "language": "java", "array": true }),
                    to_qualified_hint: fqn,
                    to_type_hint: Some(last.to_string()),
                    ..Default::default()
                });
            }
        }

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.extract_array_creation(child, source, file_path, ctx, same_file_types, relations)?;
        }
        Ok(())
    }

    /// `Foo.class` → `References` from the enclosing callable toward `Foo`.
    fn extract_class_literal(
        &self,
        node: Node,
        source: &[u8],
        file_path: &Path,
        ctx: &ExtractCtx,
        same_file_types: &HashSet<String>,
        relations: &mut Vec<Relation>,
    ) -> Result<()> {
        if node.kind() == "class_literal"
            && let Some(from) = self.find_containing_callable_qn(node, source, ctx)
            && let Some(type_node) = node.named_child(0)
            && let Ok(raw) = type_node.utf8_text(source)
        {
            let simple = raw.split('<').next().unwrap_or(raw).trim();
            let last = simple.rsplit('.').next().unwrap_or(simple);
            if !last.is_empty() {
                let fqn = ctx.resolve_type_fqn(simple, same_file_types);
                relations.push(Relation {
                    from,
                    to: last.to_string(),
                    relation_type: RelationType::References,
                    location: source_location(node, &file_path.to_string_lossy()),
                    metadata: serde_json::json!({ "language": "java", "class_literal": true }),
                    to_qualified_hint: fqn,
                    to_type_hint: Some(last.to_string()),
                    ..Default::default()
                });
            }
        }

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.extract_class_literal(child, source, file_path, ctx, same_file_types, relations)?;
        }
        Ok(())
    }

    /// `lambda_expression` → `Calls` from the enclosing callable toward the
    /// synthetic lambda Function symbol (see `extract_lambda`). Uses the same
    /// `lambda_index` scheme so `to` matches the QN emitted during symbol
    /// extraction.
    fn extract_lambda_calls(
        &self,
        node: Node,
        source: &[u8],
        file_path: &Path,
        ctx: &ExtractCtx,
        relations: &mut Vec<Relation>,
    ) -> Result<()> {
        if node.kind() == "lambda_expression" {
            let (owner, idx) = self.lambda_index(node, source, ctx);
            if let Some(from) = owner {
                let to = format!("{from}.$lambda${idx}");
                relations.push(Relation {
                    from,
                    to,
                    relation_type: RelationType::Calls,
                    location: source_location(node, &file_path.to_string_lossy()),
                    metadata: serde_json::json!({ "language": "java", "is_lambda_call": true }),
                    to_qualified_hint: None,
                    to_type_hint: None,
                    ..Default::default()
                });
            }
        }

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.extract_lambda_calls(child, source, file_path, ctx, relations)?;
        }
        Ok(())
    }

    /// Best-effort `References` from a method/constructor to each simple name
    /// in its `throws` clause (property is always recorded on the symbol via
    /// `apply_callable_metadata`; edges are emitted whenever a throws clause
    /// exists, per D2 "edges when unique").
    fn extract_throws_refs(
        &self,
        node: Node,
        source: &[u8],
        file_path: &Path,
        ctx: &ExtractCtx,
        same_file_types: &HashSet<String>,
        relations: &mut Vec<Relation>,
    ) -> Result<()> {
        let from = match node.kind() {
            "method_declaration" => node
                .child_by_field_name("name")
                .and_then(|n| n.utf8_text(source).ok())
                .and_then(|name| self.method_qualified_name(node, name, source, ctx)),
            "constructor_declaration" => self
                .find_containing_type_name(node, source, ctx)
                .map(|owner| format!("{}.<init>", ctx.qualify_type(&owner))),
            _ => None,
        };

        if let (Some(from), Some(throws)) = (from, self.throws_text(node, source)) {
            for raw in throws.split(", ") {
                let simple = raw.rsplit('.').next().unwrap_or(raw).trim();
                if simple.is_empty() {
                    continue;
                }
                let fqn = ctx.resolve_type_fqn(raw.trim(), same_file_types);
                relations.push(Relation {
                    from: from.clone(),
                    to: simple.to_string(),
                    relation_type: RelationType::References,
                    location: source_location(node, &file_path.to_string_lossy()),
                    metadata: serde_json::json!({ "language": "java", "throws": true }),
                    to_qualified_hint: fqn,
                    to_type_hint: Some(simple.to_string()),
                    ..Default::default()
                });
            }
        }

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.extract_throws_refs(child, source, file_path, ctx, same_file_types, relations)?;
        }
        Ok(())
    }

    /// `module_declaration` directives → graph relations alongside the
    /// metadata already recorded by `extract_module_symbol`: `requires` →
    /// `DependsOn`; `uses` → `Uses`; `provides X with Y` → `Uses` toward the
    /// service and each provider; `exports`/`opens` → `Defines` toward the
    /// package (documented equivalent for GQL per D7).
    fn extract_module_relations(
        &self,
        node: Node,
        source: &[u8],
        file_path: &Path,
        relations: &mut Vec<Relation>,
    ) -> Result<()> {
        if node.kind() == "module_declaration" {
            let module_name = node
                .child_by_field_name("name")
                .and_then(|n| n.utf8_text(source).ok())
                .unwrap_or("")
                .to_string();

            if !module_name.is_empty()
                && let Some(body) = node.child_by_field_name("body")
            {
                let mut cursor = body.walk();
                for directive in body.children(&mut cursor) {
                    self.extract_module_directive_relations(
                        directive,
                        source,
                        file_path,
                        &module_name,
                        relations,
                    );
                }
            }
        }

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.extract_module_relations(child, source, file_path, relations)?;
        }
        Ok(())
    }

    fn extract_module_directive_relations(
        &self,
        directive: Node,
        source: &[u8],
        file_path: &Path,
        module_name: &str,
        relations: &mut Vec<Relation>,
    ) {
        let loc = source_location(directive, &file_path.to_string_lossy());
        match directive.kind() {
            "requires_module_directive" => {
                if let Some(m) = directive
                    .child_by_field_name("module")
                    .and_then(|n| n.utf8_text(source).ok())
                {
                    relations.push(Relation {
                        from: module_name.to_string(),
                        to: m.to_string(),
                        relation_type: RelationType::DependsOn,
                        location: loc,
                        metadata: serde_json::json!({ "language": "java", "jpms": "requires" }),
                        to_qualified_hint: Some(m.to_string()),
                        to_type_hint: None,
                        ..Default::default()
                    });
                }
            }
            "uses_module_directive" => {
                if let Some(t) = directive
                    .child_by_field_name("type")
                    .and_then(|n| n.utf8_text(source).ok())
                {
                    let simple = t.rsplit('.').next().unwrap_or(t);
                    relations.push(Relation {
                        from: module_name.to_string(),
                        to: simple.to_string(),
                        relation_type: RelationType::Uses,
                        location: loc,
                        metadata: serde_json::json!({ "language": "java", "jpms": "uses" }),
                        to_qualified_hint: Some(t.to_string()),
                        to_type_hint: None,
                        ..Default::default()
                    });
                }
            }
            "provides_module_directive" => {
                let mut seen_provided = false;
                let mut cursor = directive.walk();
                for child in directive.children(&mut cursor) {
                    if !matches!(child.kind(), "identifier" | "scoped_identifier") {
                        continue;
                    }
                    let Ok(text) = child.utf8_text(source) else {
                        continue;
                    };
                    let simple = text.rsplit('.').next().unwrap_or(text);
                    let kind = if seen_provided {
                        "provides_with"
                    } else {
                        "provides"
                    };
                    seen_provided = true;
                    relations.push(Relation {
                        from: module_name.to_string(),
                        to: simple.to_string(),
                        relation_type: RelationType::Uses,
                        location: loc.clone(),
                        metadata: serde_json::json!({ "language": "java", "jpms": kind }),
                        to_qualified_hint: Some(text.to_string()),
                        to_type_hint: None,
                        ..Default::default()
                    });
                }
            }
            "exports_module_directive" | "opens_module_directive" => {
                if let Some(p) = directive
                    .child_by_field_name("package")
                    .and_then(|n| n.utf8_text(source).ok())
                {
                    relations.push(Relation {
                        from: module_name.to_string(),
                        to: p.to_string(),
                        relation_type: RelationType::Defines,
                        location: loc,
                        metadata: serde_json::json!({ "language": "java", "jpms": directive.kind() }),
                        to_qualified_hint: Some(p.to_string()),
                        to_type_hint: None,
                     ..Default::default() });
                }
            }
            _ => {}
        }
    }

    /// `import a.b.C;` → `Uses` from the file's package/module owner (or the
    /// file path itself when no package is declared) to the imported name.
    fn extract_import_uses(
        &self,
        node: Node,
        source: &[u8],
        file_path: &Path,
        ctx: &ExtractCtx,
        relations: &mut Vec<Relation>,
    ) -> Result<()> {
        if node.kind() == "import_declaration" {
            let mut cursor = node.walk();
            let name_node = node
                .children(&mut cursor)
                .find(|c| c.kind() == "identifier" || c.kind() == "scoped_identifier");
            if let Some(name_node) = name_node
                && let Ok(fqn) = name_node.utf8_text(source)
            {
                let simple = fqn.rsplit('.').next().unwrap_or(fqn).to_string();
                let owner = ctx
                    .package
                    .clone()
                    .unwrap_or_else(|| file_path.to_string_lossy().to_string());
                let mut rel = Relation {
                    from: owner,
                    to: simple,
                    relation_type: RelationType::Uses,
                    location: source_location(node, &file_path.to_string_lossy()),
                    metadata: serde_json::json!({ "language": "java", "fqn": fqn, "import": true }),
                    to_qualified_hint: Some(fqn.to_string()),
                    to_type_hint: None,
                    ..Default::default()
                };
                record_call_site_spans(&mut rel, name_node);
                relations.push(rel);
            }
        }

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.extract_import_uses(child, source, file_path, ctx, relations)?;
        }
        Ok(())
    }

    /// Find the fully qualified name of the method containing a given node
    fn find_containing_method(
        &self,
        node: Node,
        source: &[u8],
        _symbols: &[Symbol],
    ) -> Option<String> {
        let mut current = node;
        let mut method_name = None;
        let mut class_name = None;

        // Find method name first
        while let Some(parent) = current.parent() {
            if parent.kind() == "method_declaration" && method_name.is_none() {
                let mut cursor = parent.walk();
                for child in parent.children(&mut cursor) {
                    if child.kind() == "identifier" {
                        method_name = child.utf8_text(source).ok().map(|s| s.to_string());
                        break;
                    }
                }
            }
            if parent.kind() == "class_declaration" && class_name.is_none() {
                let mut cursor = parent.walk();
                for child in parent.children(&mut cursor) {
                    if child.kind() == "identifier" {
                        class_name = child.utf8_text(source).ok().map(|s| s.to_string());
                        break;
                    }
                }
            }
            current = parent;
        }

        // Return qualified name if both found, otherwise just method name
        match (class_name, method_name) {
            (Some(class), Some(method)) => Some(format!("{}.{}", class, method)),
            (None, Some(method)) => Some(method),
            _ => None,
        }
    }

    /// Find the name of a class
    fn find_class_name(&self, node: Node, source: &[u8]) -> Result<String> {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "identifier" {
                return Ok(child.utf8_text(source)?.to_string());
            }
        }
        Err(Error::ParseError {
            file: "unknown".into(),
            line: node.start_position().row + 1,
            message: "Class missing name".to_string(),
        })
    }

    /// Flush deferred `method_invocation` nodes with ExtractCtx-aware receiver resolution.
    pub(crate) fn flush_pending_calls(
        &self,
        pending: &[(Node, usize)],
        source: &[u8],
        file_path: &Path,
        extract_ctx: &ExtractCtx,
        symbols: &[Symbol],
        relations: &mut Vec<Relation>,
    ) {
        let same_file_types: HashSet<String> = symbols
            .iter()
            .filter(|s| {
                matches!(
                    s.symbol_type,
                    SymbolType::Class
                        | SymbolType::Interface
                        | SymbolType::Enum
                        | SymbolType::Annotation
                )
            })
            .map(|s| s.name.clone())
            .collect();

        for &(node, from_idx) in pending {
            let Some(from_fn) = symbols.get(from_idx) else {
                continue;
            };
            let Some(name_node) = node.child_by_field_name("name") else {
                continue;
            };
            let Ok(callee) = name_node.utf8_text(source).map(str::trim) else {
                continue;
            };
            if callee.is_empty() {
                continue;
            }

            let from = from_fn
                .qualified_name
                .clone()
                .unwrap_or_else(|| from_fn.name.clone());

            let (to_type_hint, to_qualified_hint) = self.resolve_call_receiver(
                node,
                source,
                extract_ctx,
                symbols,
                from_fn,
                callee,
                &same_file_types,
            );

            let same_file_matches: Vec<_> = symbols
                .iter()
                .filter(|s| {
                    s.name == callee
                        && s.symbol_type == SymbolType::Function
                        && s.location.file == file_path.to_string_lossy()
                })
                .collect();
            let local_target = match same_file_matches.as_slice() {
                [only] => only
                    .qualified_name
                    .clone()
                    .unwrap_or_else(|| callee.to_string()),
                _ => to_qualified_hint
                    .clone()
                    .unwrap_or_else(|| callee.to_string()),
            };

            relations.push(Relation {
                from,
                to: local_target,
                relation_type: RelationType::Calls,
                location: source_location(node, &file_path.to_string_lossy()),
                metadata: serde_json::json!({ "language": "java" }),
                to_qualified_hint,
                to_type_hint,
                ..Default::default()
            });
        }
    }

    /// Resolve `object.method` / bare `method` receivers for single-pass Calls.
    #[allow(clippy::too_many_arguments)]
    fn resolve_call_receiver(
        &self,
        call: Node,
        source: &[u8],
        extract_ctx: &ExtractCtx,
        symbols: &[Symbol],
        from_fn: &Symbol,
        callee: &str,
        same_file_types: &HashSet<String>,
    ) -> (Option<String>, Option<String>) {
        let owner_fqn = from_fn
            .qualified_name
            .as_deref()
            .and_then(|qn| qn.rsplit_once('.').map(|(o, _)| o.to_string()));
        let owner_simple = owner_fqn.as_deref().map(strip_java_type_sugar).or_else(|| {
            from_fn
                .qualified_name
                .as_deref()
                .and_then(|qn| qn.rsplit('.').nth(1).map(|s| s.to_string()))
        });

        let Some(object) = call.child_by_field_name("object") else {
            // Bare call: static import member, else enclosing type.
            if let Some(owner) = extract_ctx.static_imports.get(callee) {
                let simple = strip_java_type_sugar(owner);
                return (Some(simple.clone()), Some(format!("{owner}.{callee}")));
            }
            let qh = owner_fqn
                .as_ref()
                .map(|o| format!("{o}.{callee}"))
                .or_else(|| owner_simple.as_ref().map(|o| format!("{o}.{callee}")));
            return (owner_simple.or(owner_fqn), qh);
        };

        let Ok(obj_text) = object.utf8_text(source).map(str::trim) else {
            return (None, None);
        };

        if object.kind() == "this" || obj_text == "this" {
            let qh = owner_fqn
                .as_ref()
                .map(|o| format!("{o}.{callee}"))
                .or_else(|| owner_simple.as_ref().map(|o| format!("{o}.{callee}")));
            return (owner_simple.or(owner_fqn), qh);
        }
        if object.kind() == "super" || obj_text == "super" {
            return (owner_simple.clone(), Some(format!("super.{callee}")));
        }

        // `this.field.method()` / `recv.field.method()` via field_access receiver.
        if object.kind() == "field_access" {
            if let Some(field_node) = object.child_by_field_name("field")
                && let Ok(field_name) = field_node.utf8_text(source)
            {
                let obj_obj = object.child_by_field_name("object");
                let field_owner_is_this = obj_obj
                    .map(|o| o.kind() == "this" || o.utf8_text(source).ok() == Some("this"))
                    .unwrap_or(false);
                if field_owner_is_this || obj_obj.is_none() {
                    if let Some(owner_name) = owner_fqn.as_ref().or(owner_simple.as_ref())
                        && let Some(class_sym) = symbols.iter().find(|s| {
                            matches!(
                                s.symbol_type,
                                SymbolType::Class | SymbolType::Interface | SymbolType::Enum
                            ) && (s.name == strip_java_type_sugar(owner_name)
                                || s.qualified_name.as_deref() == Some(owner_name.as_str()))
                        })
                        && let Some(ft) = class_sym
                            .fields
                            .iter()
                            .find(|f| f.name == field_name)
                            .and_then(|f| f.field_type.as_deref())
                    {
                        let fqn = extract_ctx
                            .resolve_type_fqn(ft, same_file_types)
                            .unwrap_or_else(|| strip_java_type_sugar(ft));
                        let simple = strip_java_type_sugar(&fqn);
                        return (Some(simple.clone()), Some(format!("{fqn}.{callee}")));
                    }
                }
            }
        }

        if object.kind() == "identifier" {
            // Formal parameters.
            if let Some(param_ty) = from_fn
                .parameters
                .iter()
                .find(|p| p.name == obj_text)
                .and_then(|p| p.param_type.as_deref())
            {
                let fqn = extract_ctx
                    .resolve_type_fqn(param_ty, same_file_types)
                    .unwrap_or_else(|| strip_java_type_sugar(param_ty));
                let simple = strip_java_type_sugar(&fqn);
                return (Some(simple), Some(format!("{fqn}.{callee}")));
            }

            // Locals / try-resource / foreach / catch / pattern (AST walk).
            if let Some(type_name) = self.find_local_variable_type(call, obj_text, source) {
                let fqn = extract_ctx
                    .resolve_type_fqn(&type_name, same_file_types)
                    .unwrap_or(type_name);
                let simple = strip_java_type_sugar(&fqn);
                return (Some(simple), Some(format!("{fqn}.{callee}")));
            }

            // Enclosing class fields.
            if let Some(owner_name) = owner_fqn.as_ref().or(owner_simple.as_ref())
                && let Some(class_sym) = symbols.iter().find(|s| {
                    matches!(
                        s.symbol_type,
                        SymbolType::Class | SymbolType::Interface | SymbolType::Enum
                    ) && (s.name == strip_java_type_sugar(owner_name)
                        || s.qualified_name.as_deref() == Some(owner_name.as_str()))
                })
                && let Some(ft) = class_sym
                    .fields
                    .iter()
                    .find(|f| f.name == obj_text)
                    .and_then(|f| f.field_type.as_deref())
            {
                let fqn = extract_ctx
                    .resolve_type_fqn(ft, same_file_types)
                    .unwrap_or_else(|| strip_java_type_sugar(ft));
                let simple = strip_java_type_sugar(&fqn);
                return (Some(simple), Some(format!("{fqn}.{callee}")));
            }

            // Static type receiver: PascalCase or imported type.
            let looks_like_type = obj_text
                .chars()
                .next()
                .map(|c| c.is_ascii_uppercase())
                .unwrap_or(false)
                || extract_ctx.imports.contains_key(obj_text);
            if looks_like_type {
                let fqn = extract_ctx
                    .resolve_type_fqn(obj_text, same_file_types)
                    .unwrap_or_else(|| obj_text.to_string());
                let simple = strip_java_type_sugar(&fqn);
                return (Some(simple), Some(format!("{fqn}.{callee}")));
            }
        }

        (None, None)
    }

    /// Best-effort attempt to infer the target class for a method call.
    ///
    /// For example, given `helper.transform()`:
    /// - Looks for a field/variable named "helper"
    /// - Extracts its type (e.g., "Helper")
    /// - Returns ("Helper.transform", "Helper")
    ///
    /// This is a heuristic and may not always be accurate:
    /// - Doesn't follow type inference through assignments
    /// - Doesn't resolve imports to fully qualified names
    /// - Assumes simple field/variable declarations
    fn infer_method_target(
        &self,
        object_node: Node,
        method_name: &str,
        source: &[u8],
        call_site: Node,
    ) -> (Option<String>, Option<String>) {
        // Get the object name (e.g., "helper" from "helper.transform()")
        let object_name = match object_node.utf8_text(source) {
            Ok(name) => name,
            Err(_) => {
                return (None, None);
            }
        };

        // Look for field declaration or variable declaration with this name
        // Walk up to the containing class
        let containing_class = self.find_containing_class_node(call_site);

        if let Some(class_node) = containing_class {
            // Look for field declarations in the class
            if let Some(type_name) = self.find_field_type(class_node, object_name, source) {
                let qualified_hint = format!("{}.{}", type_name, method_name);
                return (Some(qualified_hint), Some(type_name));
            }
        }

        // Fallback: check for local variable declarations
        if let Some(type_name) = self.find_local_variable_type(call_site, object_name, source) {
            let qualified_hint = format!("{}.{}", type_name, method_name);
            return (Some(qualified_hint), Some(type_name));
        }

        (None, None)
    }

    /// Find the class_declaration node containing the given node
    fn find_containing_class_node<'a>(&self, node: Node<'a>) -> Option<Node<'a>> {
        let mut current = node;
        while let Some(parent) = current.parent() {
            if parent.kind() == "class_declaration" {
                return Some(parent);
            }
            current = parent;
        }
        None
    }

    /// Find the type of a field in a class
    /// For example: `private Helper helper = new Helper();` → returns "Helper"
    fn find_field_type(&self, class_node: Node, field_name: &str, source: &[u8]) -> Option<String> {
        // Find the class_body node first
        let mut cursor = class_node.walk();
        let class_body = class_node
            .children(&mut cursor)
            .find(|child| child.kind() == "class_body");

        let class_body = class_body?;

        // Now search inside the class_body for field_declaration nodes
        let mut body_cursor = class_body.walk();
        for child in class_body.children(&mut body_cursor) {
            if child.kind() == "field_declaration" {
                // Look for the type and declarator
                let mut field_cursor = child.walk();
                let mut type_name = None;
                let mut found_field = false;

                for field_child in child.children(&mut field_cursor) {
                    // Extract the type
                    if field_child.kind() == "type_identifier"
                        || field_child.kind() == "generic_type"
                    {
                        type_name = field_child.utf8_text(source).ok().map(|s| {
                            // Remove generics if present (e.g., "List<String>" → "List")
                            s.split('<').next().unwrap_or(s).to_string()
                        });
                    }

                    // Check if this is the field we're looking for
                    if field_child.kind() == "variable_declarator"
                        && let Some(name_node) = field_child.child_by_field_name("name")
                        && let Ok(name) = name_node.utf8_text(source)
                        && name == field_name
                    {
                        found_field = true;
                    }
                }

                if found_field && type_name.is_some() {
                    return type_name;
                }
            }
        }
        None
    }

    /// Find the type of a local variable
    /// For example: `Helper h = ...; h.transform()` → returns "Helper"
    fn find_local_variable_type(
        &self,
        start_node: Node,
        var_name: &str,
        source: &[u8],
    ) -> Option<String> {
        // Walk up to find the containing method
        let mut current = start_node;
        while let Some(parent) = current.parent() {
            if parent.kind() == "method_declaration" || parent.kind() == "constructor_declaration" {
                // Search for local_variable_declaration in this method
                return self.search_local_variables(parent, var_name, source);
            }
            current = parent;
        }
        None
    }

    /// Search for local variable declarations in a method
    fn search_local_variables(
        &self,
        method_node: Node,
        var_name: &str,
        source: &[u8],
    ) -> Option<String> {
        fn search_recursive(node: Node, var_name: &str, source: &[u8]) -> Option<String> {
            if node.kind() == "local_variable_declaration" {
                let mut type_name = None;
                let mut found_var = false;

                let mut local_cursor = node.walk();
                for child in node.children(&mut local_cursor) {
                    if child.kind() == "type_identifier" || child.kind() == "generic_type" {
                        type_name = child
                            .utf8_text(source)
                            .ok()
                            .map(|s| s.split('<').next().unwrap_or(s).to_string());
                    }

                    if child.kind() == "variable_declarator"
                        && let Some(name_node) = child.child_by_field_name("name")
                        && let Ok(name) = name_node.utf8_text(source)
                        && name == var_name
                    {
                        found_var = true;
                    }
                }

                if found_var && type_name.is_some() {
                    return type_name;
                }
            }

            // Recurse into children
            let mut child_cursor = node.walk();
            for child in node.children(&mut child_cursor) {
                if let Some(result) = search_recursive(child, var_name, source) {
                    return Some(result);
                }
            }

            None
        }

        search_recursive(method_node, var_name, source)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn symbols_of(source: &[u8], file: &str) -> Vec<Symbol> {
        let plugin = JavaPlugin::new().unwrap();
        plugin.extract_symbols(Path::new(file), source).unwrap()
    }

    fn relations_of(source: &[u8], file: &str) -> Vec<Relation> {
        let plugin = JavaPlugin::new().unwrap();
        let symbols = plugin.extract_symbols(Path::new(file), source).unwrap();
        plugin
            .extract_relations(Path::new(file), source, &symbols)
            .unwrap()
    }

    // ---------------------------------------------------------------
    // Existing behavior (must keep passing)
    // ---------------------------------------------------------------

    #[test]
    fn test_extract_java_class_and_method() {
        let source = br#"
public class UserService {
    public String authenticate(String token) {
        return token;
    }
}
"#;
        let symbols = symbols_of(source, "UserService.java");
        assert!(symbols.iter().any(|s| s.name == "UserService"));
        assert!(symbols.iter().any(|s| s.name == "authenticate"));
        let auth = symbols.iter().find(|s| s.name == "authenticate").unwrap();
        assert_eq!(auth.parameters.len(), 1);
        assert_eq!(auth.parameters[0].name, "token");
        assert_eq!(auth.parameters[0].param_type.as_deref(), Some("String"));
    }

    #[test]
    fn test_extract_java_fields_and_constructor() {
        let source = br#"
public class OrderDTO {
    private String orderId;
    private String status;

    public OrderDTO(String orderId, String status) {
        this.orderId = orderId;
        this.status = status;
    }

    public void markProcessed() {
        this.status = "PROCESSED";
    }
}
"#;
        let symbols = symbols_of(source, "OrderDTO.java");
        let class = symbols
            .iter()
            .find(|s| s.name == "OrderDTO" && s.symbol_type == SymbolType::Class)
            .expect("class");
        assert!(class.fields.iter().any(|f| f.name == "orderId"));
        assert!(class.fields.iter().any(|f| f.name == "status"));
        let ctor = symbols
            .iter()
            .find(|s| {
                s.symbol_type == SymbolType::Function
                    && s.metadata.get("is_constructor").and_then(|v| v.as_bool()) == Some(true)
            })
            .expect("constructor");
        assert_eq!(ctor.qualified_name.as_deref(), Some("OrderDTO.<init>"));
        assert_eq!(ctor.parameters.len(), 2);
    }

    #[test]
    fn test_extract_relations_calls() {
        let source = br#"
public class Example {
    public void foo() {
        bar();
    }
    public void bar() {}
}
"#;
        let relations = relations_of(source, "Example.java");
        assert!(
            !relations.is_empty(),
            "Should extract at least one relation"
        );
        assert!(
            relations
                .iter()
                .any(|r| matches!(r.relation_type, RelationType::Calls)),
            "Should extract a Calls relation"
        );
    }

    #[test]
    fn test_extract_relations_implements() {
        let source = br#"public class ServiceImpl implements Service {}"#;
        let relations = relations_of(source, "Service.java");
        assert!(
            relations
                .iter()
                .any(|r| matches!(r.relation_type, RelationType::Implements)),
            "Should extract an Implements relation"
        );
    }

    #[test]
    fn test_extract_relations_extends() {
        let source = br#"public class DerivedClass extends BaseClass {}"#;
        let relations = relations_of(source, "Base.java");
        assert!(
            relations
                .iter()
                .any(|r| matches!(r.relation_type, RelationType::Extends)),
            "Should extract an Extends relation"
        );
    }

    #[test]
    fn test_jwt_filter_extends_once_per_request() {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../rgctl-tests/ecommerce-java/src/main/java/com/example/ecommerce/security/JwtAuthenticationFilter.java"
        );
        let source = std::fs::read(path).expect("jwt filter fixture");
        let relations = relations_of(&source, "JwtAuthenticationFilter.java");
        assert!(
            relations.iter().any(|r| {
                matches!(r.relation_type, RelationType::Extends)
                    && r.to == "OncePerRequestFilter"
                    && r.from.contains("JwtAuthenticationFilter")
            }),
            "extends relations: {:?}",
            relations
                .iter()
                .filter(|r| matches!(r.relation_type, RelationType::Extends))
                .collect::<Vec<_>>()
        );
    }

    // ---------------------------------------------------------------
    // 1. @interface -> Annotation
    // ---------------------------------------------------------------

    #[test]
    fn test_annotation_type_declaration() {
        let source = br#"
public @interface AddOnStartup {
    String description() default "";
}
"#;
        let symbols = symbols_of(source, "AddOnStartup.java");
        let ann = symbols
            .iter()
            .find(|s| s.name == "AddOnStartup")
            .expect("annotation symbol");
        assert_eq!(ann.symbol_type, SymbolType::Annotation);
    }

    // ---------------------------------------------------------------
    // 2. Method @Override + @RequestMapping(path="/x") -> AnnotatedWith x2
    // ---------------------------------------------------------------

    #[test]
    fn test_method_annotations_and_modifiers() {
        let source = br#"
public class Controller {
    @Override
    @RequestMapping(path = "/x")
    public void handle() {}
}
"#;
        let symbols = symbols_of(source, "Controller.java");
        let method = symbols.iter().find(|s| s.name == "handle").expect("method");
        assert!(method.modifiers.iter().any(|m| m == "public"));
        assert!(!method.modifiers.iter().any(|m| m.contains('@')));

        let relations = relations_of(source, "Controller.java");
        let annotated: Vec<_> = relations
            .iter()
            .filter(|r| matches!(r.relation_type, RelationType::AnnotatedWith))
            .collect();
        assert_eq!(annotated.len(), 2, "expected two AnnotatedWith relations");
        assert!(annotated.iter().any(|r| r.to == "Override"));
        let req_mapping = annotated
            .iter()
            .find(|r| r.to == "RequestMapping")
            .expect("RequestMapping annotation relation");
        let args = req_mapping
            .metadata
            .get("arguments")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        assert!(args.contains("path"));
    }

    // ---------------------------------------------------------------
    // 3. Field @AddOnStartup -> AnnotatedWith
    // ---------------------------------------------------------------

    #[test]
    fn test_field_annotation() {
        let source = br#"
public class Service {
    @AddOnStartup(description = "x")
    private String name;
}
"#;
        let relations = relations_of(source, "Service.java");
        assert!(
            relations
                .iter()
                .any(|r| matches!(r.relation_type, RelationType::AnnotatedWith)
                    && r.to == "AddOnStartup"),
            "expected AnnotatedWith relation to AddOnStartup"
        );
        let plugin = JavaPlugin::new().unwrap();
        let result = plugin
            .extract_all(Path::new("Service.java"), source)
            .unwrap();
        assert!(
            result.symbols.iter().any(|s| {
                s.symbol_type == SymbolType::Variable
                    && s.name == "name"
                    && s.qualified_name.as_deref() == Some("Service.name")
            }),
            "annotated field should emit a Variable symbol: {:?}",
            result.symbols
        );
    }

    #[test]
    fn field_di_annotations_emit_annotated_with_args_and_injects_context() {
        let source = br#"
public class Greeter {
    @PersistenceContext(unitName = "primary")
    private EntityManager em;

    @ConfigProperty(name = "app.greeting")
    private String greeting;

    @Inject
    private OrderService orders;
}
"#;
        let plugin = JavaPlugin::new().unwrap();
        let result = plugin
            .extract_all(Path::new("Greeter.java"), source)
            .unwrap();

        let persistence = result
            .relations
            .iter()
            .find(|r| {
                r.relation_type == RelationType::AnnotatedWith
                    && r.to == "PersistenceContext"
                    && r.from.contains("em")
            })
            .expect("AnnotatedWith PersistenceContext on field");
        let args = persistence
            .metadata
            .get("arguments")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        assert!(
            args.contains("unitName"),
            "expected unitName in annotation args, got {args:?}"
        );

        let config = result
            .relations
            .iter()
            .find(|r| {
                r.relation_type == RelationType::AnnotatedWith
                    && r.to == "ConfigProperty"
                    && r.from.contains("greeting")
            })
            .expect("AnnotatedWith ConfigProperty on field");
        let config_args = config
            .metadata
            .get("arguments")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        assert!(
            config_args.contains("name"),
            "expected name in ConfigProperty args, got {config_args:?}"
        );

        assert!(
            result
                .symbols
                .iter()
                .any(|s| { s.symbol_type == SymbolType::Variable && s.name == "em" })
                && result
                    .symbols
                    .iter()
                    .any(|s| { s.symbol_type == SymbolType::Variable && s.name == "greeting" }),
            "expected Variable symbols for annotated fields"
        );

        let injects: Vec<_> = result
            .relations
            .iter()
            .filter(|r| r.relation_type == RelationType::Injects)
            .collect();
        assert!(
            injects.iter().any(|r| {
                r.to.contains("EntityManager")
                    && r.metadata.get("call_context").and_then(|v| v.as_str())
                        == Some("@PersistenceContext")
            }),
            "expected Injects EntityManager with @PersistenceContext call_context, got {injects:?}"
        );
        assert!(
            injects.iter().any(|r| {
                r.to.contains("OrderService")
                    && r.metadata.get("call_context").and_then(|v| v.as_str()) == Some("@Inject")
            }),
            "expected Injects OrderService with @Inject call_context, got {injects:?}"
        );
    }

    #[test]
    fn parameter_annotations_carry_scope_spans_and_annotation_args() {
        let source = br#"
import org.springframework.web.bind.annotation.GetMapping;
import org.springframework.web.bind.annotation.PathVariable;
import org.springframework.web.bind.annotation.RequestParam;
import org.springframework.web.bind.annotation.RequestMapping;
import org.springframework.web.bind.annotation.RequestMethod;

public class Orders {
    @RequestMapping(path = "/api", method = RequestMethod.GET, produces = "application/json")
    @GetMapping("/orders/{id}")
    public String get(
        @PathVariable("id") Long id,
        @RequestParam(name = "filter", defaultValue = "all") String filter
    ) {
        return filter;
    }
}
"#;
        let plugin = JavaPlugin::new().unwrap();
        let result = plugin
            .extract_all(Path::new("Orders.java"), source)
            .unwrap();

        let path_var = result
            .relations
            .iter()
            .find(|r| {
                r.relation_type == RelationType::AnnotatedWith
                    && r.to == "PathVariable"
                    && r.metadata.get("call_context").and_then(|v| v.as_str())
                        == Some("AnnotationScope::Parameter")
            })
            .expect("PathVariable parameter AnnotatedWith");
        assert!(path_var.call_site_start_byte.is_some());
        assert!(path_var.call_site_byte_len.is_some());
        let path_args = path_var
            .metadata
            .get("annotation_args")
            .and_then(|v| v.as_object())
            .expect("PathVariable annotation_args");
        assert_eq!(path_args.get("value").and_then(|v| v.as_str()), Some("id"));

        let req_param = result
            .relations
            .iter()
            .find(|r| r.relation_type == RelationType::AnnotatedWith && r.to == "RequestParam")
            .expect("RequestParam");
        let rp_args = req_param
            .metadata
            .get("annotation_args")
            .and_then(|v| v.as_object())
            .expect("RequestParam annotation_args");
        assert_eq!(rp_args.get("name").and_then(|v| v.as_str()), Some("filter"));
        assert_eq!(
            rp_args.get("defaultValue").and_then(|v| v.as_str()),
            Some("all")
        );

        let req_mapping = result
            .relations
            .iter()
            .find(|r| r.relation_type == RelationType::AnnotatedWith && r.to == "RequestMapping")
            .expect("RequestMapping");
        let rm_args = req_mapping
            .metadata
            .get("annotation_args")
            .and_then(|v| v.as_object())
            .expect("RequestMapping annotation_args");
        assert_eq!(rm_args.get("path").and_then(|v| v.as_str()), Some("/api"));
        assert!(
            rm_args
                .get("method")
                .and_then(|v| v.as_str())
                .is_some_and(|m| m.contains("GET")),
            "expected method=RequestMethod.GET, got {rm_args:?}"
        );
        assert_eq!(
            rm_args.get("produces").and_then(|v| v.as_str()),
            Some("application/json")
        );
        assert!(req_mapping.call_site_start_byte.is_some());

        let imports: Vec<_> = result
            .relations
            .iter()
            .filter(|r| {
                r.relation_type == RelationType::Uses
                    && r.metadata
                        .get("import")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false)
            })
            .collect();
        assert!(!imports.is_empty());
        assert!(imports.iter().all(|r| r.call_site_start_byte.is_some()));
    }

    // ---------------------------------------------------------------
    // 4. Record + compact ctor + method
    // ---------------------------------------------------------------

    #[test]
    fn test_record_with_compact_constructor_and_method() {
        let source = br#"
public record Point(int x, int y) {
    public Point {
        if (x < 0) throw new IllegalArgumentException();
    }
    public int sum() {
        return x + y;
    }
}
"#;
        let symbols = symbols_of(source, "Point.java");
        let record = symbols
            .iter()
            .find(|s| s.name == "Point" && s.symbol_type == SymbolType::Class)
            .expect("record class symbol");
        assert_eq!(
            record.metadata.get("is_record").and_then(|v| v.as_bool()),
            Some(true)
        );
        assert!(record.fields.iter().any(|f| f.name == "x"));
        assert!(record.fields.iter().any(|f| f.name == "y"));

        let ctor = symbols
            .iter()
            .find(|s| s.metadata.get("is_constructor").and_then(|v| v.as_bool()) == Some(true))
            .expect("compact constructor");
        assert_eq!(ctor.qualified_name.as_deref(), Some("Point.<init>"));

        let sum = symbols
            .iter()
            .find(|s| s.name == "sum")
            .expect("sum method");
        assert_eq!(sum.qualified_name.as_deref(), Some("Point.sum"));
    }

    // ---------------------------------------------------------------
    // 5. Enum constants as fields + method
    // ---------------------------------------------------------------

    #[test]
    fn test_enum_constants_and_method() {
        let source = br#"
public enum Color {
    RED, GREEN;
    public String lower() {
        return name();
    }
}
"#;
        let symbols = symbols_of(source, "Color.java");
        let color = symbols
            .iter()
            .find(|s| s.name == "Color" && s.symbol_type == SymbolType::Enum)
            .expect("enum symbol");
        assert!(color.fields.iter().any(|f| f.name == "RED"));
        assert!(color.fields.iter().any(|f| f.name == "GREEN"));
        assert!(symbols.iter().any(|s| s.name == "lower"));
    }

    // ---------------------------------------------------------------
    // 6. package -> Module (java_kind package); class QN includes package
    // ---------------------------------------------------------------

    #[test]
    fn test_package_declaration_and_qualified_class_name() {
        let source = br#"
package com.example.app;

public class Foo {}
"#;
        let symbols = symbols_of(source, "Foo.java");
        let module = symbols
            .iter()
            .find(|s| s.symbol_type == SymbolType::Module && s.name == "com.example.app")
            .expect("package module symbol");
        assert_eq!(
            module.metadata.get("java_kind").and_then(|v| v.as_str()),
            Some("package")
        );

        let foo = symbols
            .iter()
            .find(|s| s.name == "Foo" && s.symbol_type == SymbolType::Class)
            .expect("Foo class");
        assert_eq!(foo.qualified_name.as_deref(), Some("com.example.app.Foo"));
    }

    // ---------------------------------------------------------------
    // 7. module-info requires/exports
    // ---------------------------------------------------------------

    #[test]
    fn test_module_declaration() {
        let source = br#"
module com.example.app {
    requires java.base;
    exports com.example.app;
}
"#;
        let symbols = symbols_of(source, "module-info.java");
        let module = symbols
            .iter()
            .find(|s| s.symbol_type == SymbolType::Module && s.name == "com.example.app")
            .expect("jpms module symbol");
        assert_eq!(
            module.metadata.get("java_kind").and_then(|v| v.as_str()),
            Some("jpms")
        );
        let requires: Vec<&str> = module
            .metadata
            .get("requires")
            .and_then(|v| v.as_array())
            .map(|a| a.iter().filter_map(|x| x.as_str()).collect())
            .unwrap_or_default();
        assert!(requires.contains(&"java.base"));
        let exports: Vec<&str> = module
            .metadata
            .get("exports")
            .and_then(|v| v.as_array())
            .map(|a| a.iter().filter_map(|x| x.as_str()).collect())
            .unwrap_or_default();
        assert!(exports.contains(&"com.example.app"));
    }

    // ---------------------------------------------------------------
    // 8. sealed permits edges
    // ---------------------------------------------------------------

    #[test]
    fn test_sealed_permits() {
        let source = br#"
public sealed class Shape permits Circle, Square {}
final class Circle extends Shape {}
final class Square extends Shape {}
"#;
        let relations = relations_of(source, "Shape.java");
        let permits: Vec<_> = relations
            .iter()
            .filter(|r| matches!(r.relation_type, RelationType::Permits))
            .collect();
        assert!(
            permits
                .iter()
                .any(|r| r.from == "Shape" && r.to == "Circle")
        );
        assert!(
            permits
                .iter()
                .any(|r| r.from == "Shape" && r.to == "Square")
        );

        // Subclass Extends relations still exist.
        assert!(
            relations
                .iter()
                .any(|r| matches!(r.relation_type, RelationType::Extends)
                    && r.from == "Circle"
                    && r.to == "Shape")
        );
    }

    // ---------------------------------------------------------------
    // 9. interface extends B, C
    // ---------------------------------------------------------------

    #[test]
    fn test_interface_extends_multiple() {
        let source = br#"
public interface B {}
public interface C {}
public interface A extends B, C {}
"#;
        let relations = relations_of(source, "A.java");
        let extends: Vec<_> = relations
            .iter()
            .filter(|r| matches!(r.relation_type, RelationType::Extends) && r.from == "A")
            .collect();
        assert!(extends.iter().any(|r| r.to == "B"));
        assert!(extends.iter().any(|r| r.to == "C"));
    }

    // ---------------------------------------------------------------
    // 10. new ArrayList -> Instantiates
    // ---------------------------------------------------------------

    #[test]
    fn test_object_creation_instantiates() {
        let source = br#"
import java.util.ArrayList;

public class Factory {
    public void make() {
        new ArrayList<String>();
    }
}
"#;
        let relations = relations_of(source, "Factory.java");
        assert!(
            relations
                .iter()
                .any(|r| matches!(r.relation_type, RelationType::Instantiates)
                    && r.to == "ArrayList"),
            "expected Instantiates relation to ArrayList"
        );
    }

    // ---------------------------------------------------------------
    // 11. String::length -> Calls
    // ---------------------------------------------------------------

    #[test]
    fn test_method_reference_calls() {
        let source = br#"
import java.util.function.ToIntFunction;

public class Lengths {
    public void run() {
        ToIntFunction<String> f = String::length;
    }
}
"#;
        let relations = relations_of(source, "Lengths.java");
        let call = relations
            .iter()
            .find(|r| matches!(r.relation_type, RelationType::Calls) && r.to == "length")
            .expect("expected Calls relation for method reference");
        assert_eq!(call.to_type_hint.as_deref(), Some("String"));
    }

    // ---------------------------------------------------------------
    // 12. Child(){ super(1); } -> Calls-like relation
    // ---------------------------------------------------------------

    #[test]
    fn test_explicit_super_constructor_invocation() {
        let source = br#"
public class Base {
    public Base(int x) {}
}
public class Child extends Base {
    public Child() {
        super(1);
    }
}
"#;
        let relations = relations_of(source, "Child.java");
        assert!(
            relations
                .iter()
                .any(|r| matches!(r.relation_type, RelationType::Calls)
                    && r.from == "Child.<init>"
                    && r.to.contains("<init>")),
            "expected ctor-chaining Calls relation from Child.<init>"
        );
    }

    // ---------------------------------------------------------------
    // 13. import Uses
    // ---------------------------------------------------------------

    #[test]
    fn test_import_uses_relation() {
        let source = br#"
import java.util.List;

public class Repo {}
"#;
        let symbols = symbols_of(source, "Repo.java");
        assert!(symbols.iter().any(|s| s.symbol_type == SymbolType::Import));

        let relations = relations_of(source, "Repo.java");
        let uses = relations
            .iter()
            .find(|r| matches!(r.relation_type, RelationType::Uses))
            .expect("expected a Uses relation for the import");
        assert_eq!(uses.to, "List");
        assert_eq!(uses.to_qualified_hint.as_deref(), Some("java.util.List"));
    }

    // ---------------------------------------------------------------
    // 14. varargs
    // ---------------------------------------------------------------

    #[test]
    fn test_varargs_parameter() {
        let source = br#"
public class Logger {
    public void log(String... args) {}
}
"#;
        let symbols = symbols_of(source, "Logger.java");
        let method = symbols.iter().find(|s| s.name == "log").expect("method");
        assert_eq!(method.parameters.len(), 1);
        assert_eq!(method.parameters[0].name, "args");
        assert!(
            method.parameters[0]
                .param_type
                .as_deref()
                .unwrap_or("")
                .contains("String")
        );
    }

    // ---------------------------------------------------------------
    // 15. nested Outer.Inner.i QN
    // ---------------------------------------------------------------

    #[test]
    fn test_nested_class_qualified_name() {
        let source = br#"
class Outer {
    class Inner {
        void i() {}
    }
}
"#;
        let symbols = symbols_of(source, "Outer.java");
        let method = symbols.iter().find(|s| s.name == "i").expect("method i");
        assert_eq!(method.qualified_name.as_deref(), Some("Outer.Inner.i"));
    }

    // ---------------------------------------------------------------
    // 16. anonymous Runnable run NOT Outer.run
    // ---------------------------------------------------------------

    #[test]
    fn test_anonymous_class_method_qn_honesty() {
        let source = br#"
class Outer {
    void m() {
        Runnable r = new Runnable() {
            public void run() {}
        };
    }
}
"#;
        let symbols = symbols_of(source, "Outer.java");
        let run = symbols
            .iter()
            .find(|s| s.name == "run")
            .expect("run method");
        assert_ne!(run.qualified_name.as_deref(), Some("Outer.run"));
        assert!(
            run.qualified_name
                .as_deref()
                .unwrap_or("")
                .starts_with("Outer.$Anonymous")
        );
    }

    // ---------------------------------------------------------------
    // 17. complexity > 1 for if/else method
    // ---------------------------------------------------------------

    #[test]
    fn test_calculate_complexity_branching_method() {
        let source = br#"
public class Checker {
    public int check(int x) {
        if (x > 0) {
            return 1;
        } else {
            return -1;
        }
    }
}
"#;
        let plugin = JavaPlugin::new().unwrap();
        let symbols = plugin
            .extract_symbols(Path::new("Checker.java"), source)
            .unwrap();
        let method = symbols.iter().find(|s| s.name == "check").unwrap();
        let metrics = plugin
            .calculate_complexity(method, source)
            .unwrap()
            .expect("complexity metrics");
        assert!(metrics.cyclomatic > 1, "expected cyclomatic > 1");
        assert!(metrics.loc > 0, "expected loc > 0");
    }

    // ---------------------------------------------------------------
    // java-grammar-remainder: member remainder
    // ---------------------------------------------------------------

    #[test]
    fn test_interface_constant_field() {
        let source = br#"
public interface Svc {
    int MAX = 10;
}
"#;
        let symbols = symbols_of(source, "Svc.java");
        let svc = symbols
            .iter()
            .find(|s| s.name == "Svc" && s.symbol_type == SymbolType::Interface)
            .expect("interface symbol");
        let max = svc
            .fields
            .iter()
            .find(|f| f.name == "MAX")
            .expect("MAX field");
        assert!(max.field_type.as_deref().unwrap_or("").contains("int"));
    }

    #[test]
    fn test_annotation_element_with_default() {
        let source = br#"
public @interface Ann {
    String value() default "";
}
"#;
        let symbols = symbols_of(source, "Ann.java");
        let value = symbols
            .iter()
            .find(|s| s.name == "value" && s.symbol_type == SymbolType::Function)
            .expect("annotation element symbol");
        assert!(
            value
                .qualified_name
                .as_deref()
                .unwrap_or("")
                .ends_with("Ann.value")
        );
        assert_eq!(
            value
                .metadata
                .get("is_annotation_element")
                .and_then(|v| v.as_bool()),
            Some(true)
        );
        assert_eq!(
            value.metadata.get("default_value").and_then(|v| v.as_str()),
            Some("\"\"")
        );
    }

    #[test]
    fn test_enum_constant_with_method_body() {
        let source = br#"
enum E {
    A { void x() {} };
}
"#;
        let symbols = symbols_of(source, "E.java");
        let e = symbols
            .iter()
            .find(|s| s.name == "E" && s.symbol_type == SymbolType::Enum)
            .expect("enum symbol");
        assert!(e.fields.iter().any(|f| f.name == "A"));
        let x = symbols
            .iter()
            .find(|s| s.name == "x")
            .expect("x method in constant body");
        assert!(x.qualified_name.as_deref().unwrap_or("").contains("E.A.x"));
    }

    #[test]
    fn test_enum_constant_with_arguments() {
        let source = br#"
enum E2 {
    A(1, 2);
    E2(int a, int b) {}
}
"#;
        let symbols = symbols_of(source, "E2.java");
        let e2 = symbols
            .iter()
            .find(|s| s.name == "E2" && s.symbol_type == SymbolType::Enum)
            .expect("enum symbol");
        let a = e2.fields.iter().find(|f| f.name == "A").expect("A field");
        let visibility = a.visibility.as_deref().unwrap_or("");
        assert!(visibility.starts_with("enum_constant"));
        assert!(visibility.contains("1, 2"));
    }

    // ---------------------------------------------------------------
    // java-grammar-remainder: generics + throws
    // ---------------------------------------------------------------

    #[test]
    fn test_generic_class_and_method_type_params() {
        let source = br#"
class Box<T extends Comparable<T>> {
    <U> void put(U u) {}
}
"#;
        let symbols = symbols_of(source, "Box.java");
        let class = symbols
            .iter()
            .find(|s| s.name == "Box" && s.symbol_type == SymbolType::Class)
            .expect("class symbol");
        assert!(
            class
                .metadata
                .get("type_params")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .contains('T')
        );

        let put = symbols
            .iter()
            .find(|s| s.name == "put")
            .expect("put method");
        assert!(
            put.metadata
                .get("type_params")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .contains('U')
        );
    }

    #[test]
    fn test_method_throws_property_and_reference() {
        let source = br#"
public class Reader {
    public void read() throws java.io.IOException {}
}
"#;
        let symbols = symbols_of(source, "Reader.java");
        let read = symbols
            .iter()
            .find(|s| s.name == "read")
            .expect("read method");
        assert!(
            read.metadata
                .get("throws")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .contains("IOException")
        );

        let relations = relations_of(source, "Reader.java");
        assert!(
            relations
                .iter()
                .any(|r| matches!(r.relation_type, RelationType::References)
                    && r.to == "IOException"),
            "expected References relation to IOException"
        );
    }

    // ---------------------------------------------------------------
    // java-grammar-remainder: expression refs
    // ---------------------------------------------------------------

    #[test]
    fn test_field_access_this_emits_references() {
        let source = br#"
public class Order {
    private String status;
    public void process() {
        this.status = "DONE";
    }
}
"#;
        let relations = relations_of(source, "Order.java");
        let field_ref = relations
            .iter()
            .find(|r| matches!(r.relation_type, RelationType::References) && r.to == "status")
            .expect("expected References relation to status");
        assert_eq!(field_ref.to_qualified_hint.as_deref(), Some("Order.status"));
    }

    #[test]
    fn test_field_access_via_variable_emits_references() {
        let source = br#"
public class Holder {
    String status;
}
public class Order {
    public void process(Holder obj) {
        obj.status = "DONE";
    }
}
"#;
        let relations = relations_of(source, "Order.java");
        assert!(
            relations
                .iter()
                .any(|r| matches!(r.relation_type, RelationType::References) && r.to == "status"),
            "expected References relation to status via obj.status"
        );
    }

    #[test]
    fn test_array_creation_instantiates() {
        let source = br#"
public class Factory {
    public void make() {
        new String[10];
    }
}
"#;
        let relations = relations_of(source, "Factory.java");
        assert!(
            relations
                .iter()
                .any(|r| matches!(r.relation_type, RelationType::Instantiates) && r.to == "String"),
            "expected Instantiates relation to String"
        );
    }

    #[test]
    fn test_class_literal_references() {
        let source = br#"
public class Factory {
    public void make() {
        Object o = Foo.class;
    }
}
"#;
        let relations = relations_of(source, "Factory.java");
        assert!(
            relations
                .iter()
                .any(|r| matches!(r.relation_type, RelationType::References) && r.to == "Foo"),
            "expected References relation to Foo"
        );
    }

    #[test]
    fn test_receiver_parameter_metadata() {
        let source = br#"
public class Outer {
    class Inner {
        void m(Outer.Inner this) {}
    }
}
"#;
        let symbols = symbols_of(source, "Outer.java");
        let m = symbols.iter().find(|s| s.name == "m").expect("m method");
        assert!(
            m.metadata
                .get("receiver_type")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .contains("Inner")
        );
    }

    // ---------------------------------------------------------------
    // java-grammar-remainder: lambdas
    // ---------------------------------------------------------------

    #[test]
    fn test_lambda_symbol_and_calls_relation() {
        let source = br#"
public class Printer {
    public void run() {
        xs.forEach(s -> System.out.println(s));
    }
}
"#;
        let symbols = symbols_of(source, "Printer.java");
        let lambda = symbols
            .iter()
            .find(|s| {
                s.qualified_name
                    .as_deref()
                    .unwrap_or("")
                    .contains("$lambda$")
            })
            .expect("expected a lambda Function symbol");
        assert_eq!(
            lambda.metadata.get("is_lambda").and_then(|v| v.as_bool()),
            Some(true)
        );

        let relations = relations_of(source, "Printer.java");
        assert!(
            relations
                .iter()
                .any(|r| matches!(r.relation_type, RelationType::Calls)
                    && r.from == "Printer.run"
                    && r.to.contains("$lambda$")),
            "expected Calls relation from Printer.run to the lambda"
        );
    }

    // ---------------------------------------------------------------
    // java-grammar-remainder: type-use annotations + JPMS
    // ---------------------------------------------------------------

    #[test]
    fn test_type_use_annotation_emits_annotated_with() {
        let source = br#"
import java.util.List;
public class C {
    void m(List<@NonNull String> xs) {}
}
"#;
        let relations = relations_of(source, "C.java");
        assert!(
            relations
                .iter()
                .any(|r| matches!(r.relation_type, RelationType::AnnotatedWith)
                    && r.from == "C.m"
                    && r.to == "NonNull"),
            "expected AnnotatedWith from method QN C.m to NonNull, got {:?}",
            relations
                .iter()
                .filter(|r| matches!(r.relation_type, RelationType::AnnotatedWith))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_parameter_declaration_annotation_attaches_to_method() {
        let source = br#"
public class C {
    void m(@Deprecated String xs) {}
}
"#;
        let relations = relations_of(source, "C.java");
        assert!(
            relations
                .iter()
                .any(|r| matches!(r.relation_type, RelationType::AnnotatedWith)
                    && r.from == "C.m"
                    && r.to == "Deprecated"),
            "expected AnnotatedWith from C.m to Deprecated (not method(param))"
        );
    }

    #[test]
    fn test_module_requires_and_provides_relations() {
        let source = br#"
module M {
    requires java.base;
    uses com.a.Other;
    provides com.a.Spi with com.a.SpiImpl;
}
"#;
        let relations = relations_of(source, "module-info.java");
        assert!(
            relations
                .iter()
                .any(|r| matches!(r.relation_type, RelationType::DependsOn)
                    && r.from == "M"
                    && r.to == "java.base"),
            "expected DependsOn relation to java.base"
        );
        assert!(
            relations
                .iter()
                .any(|r| matches!(r.relation_type, RelationType::Uses)
                    && r.from == "M"
                    && r.to == "Other"),
            "expected Uses relation to Other (uses directive)"
        );
        assert!(
            relations
                .iter()
                .any(|r| matches!(r.relation_type, RelationType::Uses)
                    && r.from == "M"
                    && r.to == "Spi"),
            "expected Uses relation to Spi (provides service)"
        );
        assert!(
            relations
                .iter()
                .any(|r| matches!(r.relation_type, RelationType::Uses)
                    && r.from == "M"
                    && r.to == "SpiImpl"),
            "expected Uses relation to SpiImpl (provides provider)"
        );
    }

    #[test]
    fn spring_extract_all_emits_exposes_route_injects_and_reflection() {
        let source = br#"
@RestController
@RequestMapping("/api")
public class OrderController {
    @Autowired
    private OrderService orderService;

    @GetMapping("/orders")
    public String list() {
        return "ok";
    }

    public void load(String name) throws Exception {
        Class.forName(name);
    }
}
"#;
        let plugin = JavaPlugin::new().unwrap();
        let result = plugin
            .extract_all(Path::new("OrderController.java"), source)
            .unwrap();
        let routes: Vec<_> = result
            .relations
            .iter()
            .filter(|r| r.relation_type == RelationType::ExposesRoute)
            .map(|r| r.to.as_str())
            .collect();
        assert!(
            routes.iter().any(|t| *t == "GET /api/orders"),
            "expected GET /api/orders, got {routes:?}"
        );
        assert!(
            result.relations.iter().any(|r| {
                r.relation_type == RelationType::Injects && r.to.contains("OrderService")
            }),
            "Injects OrderService: {:?}",
            result.relations
        );
        let ctrl = result
            .symbols
            .iter()
            .find(|s| s.name == "OrderController")
            .expect("OrderController");
        let tags = ctrl
            .metadata
            .get("semantic_tags")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        assert!(
            tags.contains("controller") || tags.contains("endpoint"),
            "controller tags={tags:?}"
        );
        let load = result
            .symbols
            .iter()
            .find(|s| s.name == "load")
            .expect("load");
        let refl = load
            .metadata
            .get("semantic_tags")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        assert!(
            refl.contains("reflection"),
            "reflection tags on load={refl:?}"
        );
    }

    #[test]
    fn test_lombok_data_synthesizes_getters_setters_and_required_ctor() {
        let source = br#"
import lombok.Data;
import lombok.NonNull;

@Data
public class Order {
    private final String id;
    @NonNull private String status;
    private int count;
}
"#;
        let symbols = symbols_of(source, "Order.java");
        let get_id = symbols.iter().find(|s| s.name == "getId").expect("getId");
        assert_eq!(
            get_id
                .metadata
                .get("lombok_synthesized")
                .and_then(|v| v.as_bool()),
            Some(true)
        );
        assert!(symbols.iter().any(|s| s.name == "getStatus"));
        assert!(symbols.iter().any(|s| s.name == "setStatus"));
        assert!(symbols.iter().any(|s| s.name == "getCount"));
        assert!(symbols.iter().any(|s| s.name == "setCount"));
        // final id → no setId
        assert!(!symbols.iter().any(|s| s.name == "setId"));
        let ctor = symbols
            .iter()
            .find(|s| {
                s.metadata
                    .get("is_constructor")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false)
                    && s.metadata
                        .get("lombok_synthesized")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false)
            })
            .expect("lombok ctor");
        let param_names: Vec<_> = ctor.parameters.iter().map(|p| p.name.as_str()).collect();
        assert!(param_names.contains(&"id"));
        assert!(param_names.contains(&"status"));
    }

    #[test]
    fn test_lombok_builder_and_slf4j_and_call_resolution() {
        let source = br#"
import lombok.Builder;
import lombok.Getter;
import lombok.RequiredArgsConstructor;
import lombok.extern.slf4j.Slf4j;

@Getter
@Builder
@RequiredArgsConstructor
@Slf4j
public class Customer {
    private final String name;
    private int age;
}

class Service {
    void use(Customer c) {
        String n = c.getName();
        Customer.builder();
        new Customer("x");
        log.info(n);
    }
}
"#;
        let plugin = JavaPlugin::new().unwrap();
        let result = plugin
            .extract_all(Path::new("Customer.java"), source)
            .unwrap();
        assert!(
            result.symbols.iter().any(|s| s.name == "getName"
                && s.metadata
                    .get("lombok_synthesized")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false)),
            "expected synthetic getName"
        );
        assert!(result.symbols.iter().any(|s| s.name == "builder"));
        let customer = result
            .symbols
            .iter()
            .find(|s| s.name == "Customer")
            .expect("Customer");
        assert!(
            customer.fields.iter().any(|f| f.name == "log"),
            "expected synthetic log field"
        );
        assert!(
            result.relations.iter().any(|r| {
                matches!(r.relation_type, RelationType::Calls)
                    && (r.to.contains("getName")
                        || r.to_qualified_hint
                            .as_deref()
                            .map(|h| h.contains("getName"))
                            .unwrap_or(false))
            }),
            "expected Calls toward getName, got {:?}",
            result
                .relations
                .iter()
                .filter(|r| matches!(r.relation_type, RelationType::Calls))
                .collect::<Vec<_>>()
        );
        assert!(
            result.relations.iter().any(|r| {
                matches!(r.relation_type, RelationType::Calls)
                    && (r.to.contains("builder")
                        || r.to_qualified_hint
                            .as_deref()
                            .map(|h| h.contains("builder"))
                            .unwrap_or(false))
            }),
            "expected Calls toward builder()"
        );
    }
    #[test]
    fn indexed_name_ranges_skip_annotations_and_use_utf16() {
        let symbols = symbols_of(
            "package p;\n@Deprecated\nclass Example {\n int field;\n /* 😀 */ void run() {}\n}\n"
                .as_bytes(),
            "Example.java",
        );
        let class = symbols.iter().find(|s| s.name == "Example").expect("class");
        assert_eq!(
            class.location.start_line, 2,
            "full declaration span remains unchanged"
        );
        assert_eq!(
            class.metadata["source_name_range"],
            serde_json::json!({"start_line":2,"start_character":6,"end_line":2,"end_character":13})
        );
        assert_eq!(
            class.metadata["field_name_ranges"]["field"]["start_line"],
            3
        );
        let method = symbols.iter().find(|s| s.name == "run").expect("method");
        assert_eq!(
            method.metadata["source_name_range"],
            serde_json::json!({"start_line":4,"start_character":15,"end_line":4,"end_character":18})
        );
    }
}
