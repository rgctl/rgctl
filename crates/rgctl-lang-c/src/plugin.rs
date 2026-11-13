//! C language plugin using Tree-sitter.

use crate::single_pass::{c_ast_handlers, extract_all_single_pass};
use rgctl_plugin_api::*;
use rgctl_plugin_helpers::record_symbol_spans;
use std::path::Path;
use tree_sitter::Node;

/// File-scoped qualified name: `{file_stem}::{symbol}`.
///
/// Headers and `.c` files that share a stem (e.g. `cart.h` / `cart.c`) may emit
/// duplicate qualified names — disambiguate with `file_path` or the full
/// `{file}::{qualified_name}` symbol key in the graph.
fn file_qualified_name(file_path: &str, symbol_name: &str) -> String {
    let stem = Path::new(file_path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown");
    format!("{stem}::{symbol_name}")
}

/// Strip angle brackets / quotes from a `#include` path token.
fn normalize_include_path(raw: &str) -> String {
    let trimmed = raw.trim();
    let stripped = trimmed
        .trim_start_matches('#')
        .trim_start_matches("include")
        .trim();
    stripped
        .trim_matches(|c| c == '<' || c == '>' || c == '"')
        .trim()
        .to_string()
}

pub(crate) fn include_path_from_node(node: Node, source: &[u8]) -> Result<String> {
    if let Some(path) = node.child_by_field_name("path") {
        let raw = path.utf8_text(source)?;
        return Ok(normalize_include_path(raw));
    }
    let raw = node.utf8_text(source)?;
    Ok(normalize_include_path(raw))
}

/// C language plugin.
pub struct CPlugin;

impl CPlugin {
    /// Create a new C plugin.
    pub fn new() -> Result<Self> {
        // Validate grammar at registration; per-file parse reuses a thread-local parser.
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_c::LANGUAGE.into())
            .map_err(|e| Error::PluginError(format!("Failed to set C grammar: {e}")))?;
        Ok(Self)
    }

    pub(crate) fn parse(&self, file_path: &Path, source: &[u8]) -> Result<tree_sitter::Tree> {
        parse_with_pool("c", tree_sitter_c::LANGUAGE.into(), source).map_err(|e| {
            Error::ParseError {
                file: file_path.to_path_buf(),
                line: 0,
                message: e,
            }
        })
    }

    pub(crate) fn extract_function(
        &self,
        node: Node,
        source: &[u8],
        file_path: &str,
    ) -> Result<Option<Symbol>> {
        let Some(name) = function_name_from_node(node, source) else {
            return Ok(None);
        };

        let parameters = extract_parameters(node, source)?;

        let mut symbol = Symbol {
            name: name.clone(),
            symbol_type: SymbolType::Function,
            qualified_name: Some(file_qualified_name(file_path, &name)),
            location: source_location(node, file_path),
            signature: Some(first_line(node, source)),
            return_type: None,
            parameters,
            fields: vec![],
            modifiers: vec![],
            documentation: None,
            metadata: serde_json::json!({ "language": "c" }),
            ..Default::default()
        };
        record_symbol_spans(&mut symbol, node, function_name_node(node));
        symbol = rgctl_plugin_helpers::callable_identity::symbol(
            symbol,
            node,
            source,
            rgctl_plugin_helpers::callable_identity::Policy::C,
        );
        Ok(Some(symbol))
    }

    /// F1: struct fields from `field_declaration` in `field_declaration_list`.
    /// F2: C has no real constructors — do not invent fake ctor symbols.
    pub(crate) fn extract_struct(
        &self,
        node: Node,
        source: &[u8],
        file_path: &str,
        typedef_name: Option<&str>,
    ) -> Result<Symbol> {
        let struct_tag = struct_name(node, source);
        let name = typedef_name
            .map(str::to_string)
            .or_else(|| struct_tag.clone())
            .ok_or_else(|| Error::ParseError {
                file: file_path.into(),
                line: node.start_position().row + 1,
                message: "Struct missing name".to_string(),
            })?;

        let fields = extract_struct_fields(node, source)?;

        let mut metadata = serde_json::json!({ "language": "c" });
        if let (Some(alias), Some(tag)) = (typedef_name, struct_tag.as_deref()) {
            if alias != tag {
                metadata["underlying_type"] = serde_json::Value::String(tag.to_string());
            }
        }

        let mut symbol = Symbol {
            name: name.clone(),
            symbol_type: SymbolType::Struct,
            qualified_name: Some(file_qualified_name(file_path, &name)),
            location: source_location(node, file_path),
            signature: None,
            return_type: None,
            parameters: vec![],
            fields,
            modifiers: vec![],
            documentation: None,
            metadata,
            ..Default::default()
        };
        record_symbol_spans(&mut symbol, node, node.child_by_field_name("name"));
        Ok(symbol)
    }

    pub(crate) fn extract_enum(
        &self,
        node: Node,
        source: &[u8],
        file_path: &str,
    ) -> Result<Symbol> {
        self.extract_enum_with_typedef(node, source, file_path, None)
    }

    pub(crate) fn extract_enum_with_typedef(
        &self,
        node: Node,
        source: &[u8],
        file_path: &str,
        typedef_name: Option<&str>,
    ) -> Result<Symbol> {
        let enum_tag = node
            .child_by_field_name("name")
            .and_then(|n| n.utf8_text(source).ok().map(str::to_string));
        let name = typedef_name
            .map(str::to_string)
            .or_else(|| enum_tag.clone())
            .ok_or_else(|| Error::ParseError {
                file: file_path.into(),
                line: node.start_position().row + 1,
                message: "Enum missing name".to_string(),
            })?;

        let mut metadata = serde_json::json!({ "language": "c" });
        if let (Some(alias), Some(tag)) = (typedef_name, enum_tag.as_deref()) {
            if alias != tag {
                metadata["underlying_type"] = serde_json::Value::String(tag.to_string());
            }
        }

        let mut symbol = Symbol {
            name: name.clone(),
            symbol_type: SymbolType::Enum,
            qualified_name: Some(file_qualified_name(file_path, &name)),
            location: source_location(node, file_path),
            signature: None,
            return_type: None,
            parameters: vec![],
            fields: crate::depth::extract_enumerator_fields(node, source),
            modifiers: vec![],
            documentation: None,
            metadata,
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
        let mut symbols = Vec::new();
        self.traverse(root, source, &file_path.to_string_lossy(), &mut symbols)?;
        Ok(symbols)
    }

    fn relations_from_tree(
        &self,
        root: Node,
        source: &[u8],
        file_path: &Path,
        symbols: &[Symbol],
    ) -> Result<Vec<Relation>> {
        let mut relations = Vec::new();
        walk_calls(
            root,
            source,
            file_path,
            symbols,
            C_CALL_KINDS,
            "c",
            &mut relations,
        );
        Ok(relations)
    }

    /// Iterative tree traversal using an explicit stack to prevent stack overflows on deep ASTs.
    fn traverse(
        &self,
        root: Node,
        source: &[u8],
        file_path: &str,
        symbols: &mut Vec<Symbol>,
    ) -> Result<()> {
        const MAX_DEPTH: usize = 2048;
        let mut stack = vec![(root, 0usize)];

        while let Some((node, depth)) = stack.pop() {
            if depth > MAX_DEPTH {
                tracing::warn!(
                    file = %file_path,
                    depth = depth,
                    "AST depth limit exceeded during traversal; skipping deep branches"
                );
                continue;
            }

            match node.kind() {
                "function_definition" | "declaration" => {
                    if let Some(sym) = self.extract_function(node, source, file_path)? {
                        symbols.push(sym);
                    }
                }
                "struct_specifier" => {
                    if struct_name(node, source).is_some() {
                        symbols.push(self.extract_struct(node, source, file_path, None)?);
                    }
                }
                "enum_specifier" => {
                    if node.child_by_field_name("name").is_some() {
                        symbols.push(self.extract_enum(node, source, file_path)?);
                    }
                }
                "type_definition" => {
                    let typedef_name = type_definition_typedef_name(node, source);
                    let mut cursor = node.walk();
                    for child in node.children(&mut cursor) {
                        if child.kind() == "struct_specifier"
                            && (struct_name(child, source).is_some() || typedef_name.is_some())
                        {
                            symbols.push(self.extract_struct(
                                child,
                                source,
                                file_path,
                                typedef_name.as_deref(),
                            )?);
                        } else if child.kind() == "enum_specifier"
                            && (child.child_by_field_name("name").is_some()
                                || typedef_name.is_some())
                        {
                            symbols.push(self.extract_enum_with_typedef(
                                child,
                                source,
                                file_path,
                                typedef_name.as_deref(),
                            )?);
                        } else if child.kind() == "union_specifier"
                            && (child.child_by_field_name("name").is_some()
                                || typedef_name.is_some())
                        {
                            let fields = extract_struct_fields(child, source).unwrap_or_default();
                            if let Some(sym) = crate::depth::extract_union_specifier_named(
                                child,
                                source,
                                file_path,
                                fields,
                                typedef_name.as_deref(),
                            ) {
                                symbols.push(sym);
                            }
                        }
                    }
                }
                "preproc_include" | "preprocessor_include" => {
                    let path = include_path_from_node(node, source)?;
                    let mut symbol = Symbol {
                        name: path.clone(),
                        symbol_type: SymbolType::Import,
                        qualified_name: None,
                        location: source_location(node, file_path),
                        signature: None,
                        return_type: None,
                        parameters: vec![],
                        fields: vec![],
                        modifiers: vec![],
                        documentation: None,
                        metadata: serde_json::json!({
                            "language": "c",
                            "kind": "include",
                        }),
                        ..Default::default()
                    };
                    record_symbol_spans(&mut symbol, node, node.child_by_field_name("name"));
                    symbols.push(symbol);
                }
                _ => {}
            }

            let child_count = node.child_count();
            for i in (0..child_count).rev() {
                if let Some(child) = node.child(i) {
                    stack.push((child, depth + 1));
                }
            }
        }
        Ok(())
    }
}

impl LanguagePlugin for CPlugin {
    fn language_id(&self) -> &str {
        "c"
    }

    fn file_extensions(&self) -> Vec<&str> {
        vec!["c", "h"]
    }

    fn grammar(&self) -> Option<tree_sitter::Language> {
        Some(tree_sitter_c::LANGUAGE.into())
    }

    fn extract_symbols(&self, file_path: &Path, source: &[u8]) -> Result<Vec<Symbol>> {
        let tree = self.parse(file_path, source)?;
        self.symbols_from_tree(tree.root_node(), source, file_path)
    }

    fn extract_relations(
        &self,
        file_path: &Path,
        source: &[u8],
        symbols: &[Symbol],
    ) -> Result<Vec<Relation>> {
        let tree = self.parse(file_path, source)?;
        self.relations_from_tree(tree.root_node(), source, file_path, symbols)
    }

    fn extract_all(&self, file_path: &Path, source: &[u8]) -> Result<ExtractAllResult> {
        extract_all_single_pass(self, file_path, source)
    }

    fn declared_ast_handlers(&self) -> Option<&AstHandlerTable> {
        Some(c_ast_handlers())
    }

    fn calculate_complexity(
        &self,
        symbol: &Symbol,
        _source: &[u8],
    ) -> Result<Option<ComplexityMetrics>> {
        // Prefer metrics harvested during single-pass `extract_all`.
        if let Some(metrics) = complexity_from_metadata(symbol) {
            return Ok(Some(metrics));
        }
        // C has no dedicated complexity re-walk; leave None when metadata absent.
        Ok(None)
    }
}

fn function_name_from_node(node: Node, source: &[u8]) -> Option<String> {
    match node.kind() {
        "function_definition" => {
            let decl = node.child_by_field_name("declarator")?;
            name_from_declarator(decl, source)
        }
        "declaration" => {
            // Only function prototypes have a `function_declarator` parameter list;
            // plain variable declarations (`int x = 0;`) must not become Function symbols.
            find_parameter_list(node)?;
            let decl = node.child_by_field_name("declarator")?;
            name_from_declarator(decl, source)
        }
        _ => None,
    }
}

/// Walk a function/declaration node's declarator chain to its `parameter_list`.
fn find_parameter_list(node: Node) -> Option<Node> {
    let mut current = node.child_by_field_name("declarator")?;
    const MAX_DEPTH: usize = 512;
    for _ in 0..MAX_DEPTH {
        if current.kind() == "function_declarator" {
            return current.child_by_field_name("parameters");
        }
        current = current.child_by_field_name("declarator")?;
    }
    None
}

/// F3: typed parameters from `parameter_list` / `parameter_declaration`.
fn extract_parameters(node: Node, source: &[u8]) -> Result<Vec<Parameter>> {
    let mut parameters = Vec::new();
    let Some(params_node) = find_parameter_list(node) else {
        return Ok(parameters);
    };

    let mut cursor = params_node.walk();
    for child in params_node.children(&mut cursor) {
        if child.kind() != "parameter_declaration" {
            continue;
        }
        let param_type = child
            .child_by_field_name("type")
            .and_then(|n| n.utf8_text(source).ok().map(str::to_string));
        let name = child
            .child_by_field_name("declarator")
            .and_then(|d| name_from_declarator(d, source));
        if let Some(name) = name {
            parameters.push(Parameter {
                name,
                param_type,
                default_value: None,
            });
        }
    }
    Ok(parameters)
}

pub(crate) fn extract_struct_fields(struct_node: Node, source: &[u8]) -> Result<Vec<Field>> {
    let mut fields = Vec::new();
    let Some(body) = struct_node.child_by_field_name("body") else {
        return Ok(fields);
    };
    if body.kind() != "field_declaration_list" {
        return Ok(fields);
    }

    let mut cursor = body.walk();
    for child in body.children(&mut cursor) {
        if child.kind() != "field_declaration" {
            continue;
        }
        let field_type = child
            .child_by_field_name("type")
            .and_then(|n| n.utf8_text(source).ok().map(str::to_string));

        for i in 0..child.child_count() {
            if child.field_name_for_child(i as u32) != Some("declarator") {
                continue;
            }
            let Some(declarator) = child.child(i) else {
                continue;
            };
            if let Some(name) = name_from_declarator(declarator, source) {
                fields.push(Field {
                    source_span: rgctl_plugin_helpers::field_source_span(child, source, &name),
                    name,
                    field_type: field_type.clone(),
                    visibility: None,
                });
            }
        }
    }
    Ok(fields)
}

/// Identifier / type / field node nested under a C declarator chain.
fn name_node_from_declarator(root: Node) -> Option<Node> {
    const MAX_DEPTH: usize = 512;
    let mut stack = vec![(root, 0usize)];

    while let Some((node, depth)) = stack.pop() {
        if depth > MAX_DEPTH {
            continue;
        }

        match node.kind() {
            "identifier" | "type_identifier" | "field_identifier" => {
                return Some(node);
            }
            "function_declarator"
            | "pointer_declarator"
            | "array_declarator"
            | "parenthesized_declarator" => {
                if let Some(inner) = node.child_by_field_name("declarator") {
                    stack.push((inner, depth + 1));
                } else {
                    let mut cursor = node.walk();
                    let children: Vec<Node> = node.children(&mut cursor).collect();
                    for child in children.into_iter().rev() {
                        if child.is_named() {
                            stack.push((child, depth + 1));
                        }
                    }
                }
            }
            _ => {
                let mut cursor = node.walk();
                let children: Vec<Node> = node.children(&mut cursor).collect();
                for child in children.into_iter().rev() {
                    if child.is_named() {
                        stack.push((child, depth + 1));
                    }
                }
            }
        }
    }
    None
}

fn name_from_declarator(root: Node, source: &[u8]) -> Option<String> {
    name_node_from_declarator(root).and_then(|n| n.utf8_text(source).ok().map(str::to_string))
}

fn function_name_node(node: Node) -> Option<Node> {
    match node.kind() {
        "function_definition" | "declaration" => {
            let decl = node.child_by_field_name("declarator")?;
            name_node_from_declarator(decl)
        }
        _ => None,
    }
}

pub(crate) fn struct_name(node: Node, source: &[u8]) -> Option<String> {
    node.child_by_field_name("name")
        .and_then(|n| n.utf8_text(source).ok().map(str::to_string))
}

/// Typedef alias from `type_definition` (`typedef struct Foo Bar` → `Bar`).
pub(crate) fn type_definition_typedef_name(node: Node, source: &[u8]) -> Option<String> {
    if node.kind() != "type_definition" {
        return None;
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "type_identifier" {
            return child.utf8_text(source).ok().map(str::to_string);
        }
    }
    None
}

pub(crate) fn source_location(node: Node, file_path: &str) -> SourceLocation {
    SourceLocation {
        file: file_path.to_string(),
        start_line: node.start_position().row + 1,
        end_line: node.end_position().row + 1,
        start_column: node.start_position().column,
        end_column: node.end_position().column,
    }
}

fn first_line(node: Node, source: &[u8]) -> String {
    node.utf8_text(source)
        .unwrap_or("")
        .lines()
        .next()
        .unwrap_or("")
        .trim()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use rgctl_extraction::graph_builder::GraphBuilder;
    use std::path::Path;

    #[test]
    fn test_file_qualified_name_cart_init() {
        assert_eq!(
            file_qualified_name("cart.c", "cart_init"),
            "cart::cart_init"
        );
    }

    #[test]
    fn test_normalize_system_include() {
        assert_eq!(normalize_include_path("#include <stdio.h>"), "stdio.h");
    }

    #[test]
    fn test_normalize_local_include() {
        assert_eq!(normalize_include_path("#include \"cart.h\""), "cart.h");
    }

    #[test]
    fn test_extract_c_function_qualified_name() {
        let source = br#"int init(void) { return 0; }"#;
        let plugin = CPlugin::new().unwrap();
        let symbols = plugin.extract_symbols(Path::new("cart.c"), source).unwrap();
        let init = symbols.iter().find(|s| s.name == "init").expect("init");
        assert_eq!(init.qualified_name.as_deref(), Some("cart::init"));
    }

    #[test]
    fn test_blast_radius_disambiguates_same_name_across_files() {
        let plugin = CPlugin::new().unwrap();
        let a_src = br#"int init(void) { return 1; }"#;
        let b_src = br#"int init(void) { return 2; }"#;
        let path_a = Path::new("a/init.c");
        let path_b = Path::new("b/init.c");

        let syms_a = plugin.extract_symbols(path_a, a_src).unwrap();
        let syms_b = plugin.extract_symbols(path_b, b_src).unwrap();

        let mut builder = GraphBuilder::new();
        let file_a = builder.ensure_file_node(path_a);
        let file_b = builder.ensure_file_node(path_b);
        builder.add_symbol(&syms_a[0], file_a);
        builder.add_symbol(&syms_b[0], file_b);
        builder.build_resolution_indexes();

        let rel = Relation {
            from: "init::init".to_string(),
            to: "init::init".to_string(),
            relation_type: RelationType::Calls,
            location: SourceLocation {
                file: path_a.to_string_lossy().to_string(),
                start_line: 1,
                end_line: 1,
                start_column: 0,
                end_column: 1,
            },
            metadata: serde_json::json!({}),
            to_qualified_hint: None,
            to_type_hint: None,
            ..Default::default()
        };
        let before = builder.edge_count();
        builder.add_relation(&rel).unwrap();
        assert_eq!(
            builder.edge_count(),
            before + 1,
            "qualified from should resolve within its file"
        );
    }

    #[test]
    fn test_typedef_struct_cart_alias() {
        let source = br#"typedef struct Cart Cart;"#;
        let plugin = CPlugin::new().unwrap();
        let symbols = plugin.extract_symbols(Path::new("cart.h"), source).unwrap();
        let cart = symbols
            .iter()
            .find(|s| s.name == "Cart" && s.symbol_type == SymbolType::Struct)
            .expect("Cart typedef struct");
        assert_eq!(cart.qualified_name.as_deref(), Some("cart::Cart"));
    }

    #[test]
    fn test_call_expression_child_kinds_documented() {
        let source = br#"
void foo(void (*handler)(void)) {
    bar();
    handler();
    (*handler)();
}
void bar(void) {}
"#;
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_c::LANGUAGE.into())
            .unwrap();
        let tree = parser.parse(source, None).unwrap();
        let root = tree.root_node();
        let mut stack = vec![root];
        let mut kinds = Vec::new();
        while let Some(node) = stack.pop() {
            if node.kind() == "call_expression" {
                if let Some(func) = node.child_by_field_name("function") {
                    kinds.push(func.kind().to_string());
                }
            }
            for i in 0..node.child_count() {
                if let Some(child) = node.child(i) {
                    stack.push(child);
                }
            }
        }
        kinds.sort();
        kinds.dedup();
        assert!(
            kinds.iter().any(|k| k == "identifier"),
            "expected identifier callee, got {kinds:?}"
        );
        assert!(
            kinds.iter().any(|k| k == "parenthesized_expression"),
            "expected parenthesized_expression for (*handler)(), got {kinds:?}"
        );
    }

    #[test]
    fn test_extract_c_function_and_struct() {
        let source = br#"
#include <stdio.h>

struct Cart {
    int user_id;
};

int add(int a, int b) {
    return a + b;
}
"#;
        let plugin = CPlugin::new().unwrap();
        let symbols = plugin.extract_symbols(Path::new("cart.c"), source).unwrap();
        assert!(symbols.iter().any(|s| s.name == "add"));
        assert!(symbols.iter().any(|s| s.name == "Cart"));
    }

    #[test]
    fn test_extract_c_struct_fields() {
        let source = br#"
struct Cart {
    int user_id;
    char *name;
};
"#;
        let plugin = CPlugin::new().unwrap();
        let symbols = plugin.extract_symbols(Path::new("cart.c"), source).unwrap();
        let cart = symbols
            .iter()
            .find(|s| s.name == "Cart" && s.symbol_type == SymbolType::Struct)
            .expect("Cart struct");
        assert!(
            cart.fields
                .iter()
                .any(|f| f.name == "user_id" && f.field_type.as_deref() == Some("int")),
            "fields: {:?}",
            cart.fields
        );
        assert!(
            cart.fields.iter().any(|f| f.name == "name"),
            "fields: {:?}",
            cart.fields
        );
    }

    #[test]
    fn test_extract_c_typed_parameters() {
        let source = br#"
int add(int a, int b) {
    return a + b;
}
"#;
        let plugin = CPlugin::new().unwrap();
        let symbols = plugin.extract_symbols(Path::new("math.c"), source).unwrap();
        let add = symbols.iter().find(|s| s.name == "add").expect("add");
        assert_eq!(add.parameters.len(), 2);
        assert_eq!(add.parameters[0].name, "a");
        assert_eq!(add.parameters[0].param_type.as_deref(), Some("int"));
        assert_eq!(add.parameters[1].name, "b");
        assert_eq!(add.parameters[1].param_type.as_deref(), Some("int"));
    }

    #[test]
    fn test_extract_relations_calls() {
        let source = br#"
void foo(void) {
    bar();
    baz(1);
}

void bar(void) {}
int baz(int x) { return x; }
"#;
        let plugin = CPlugin::new().unwrap();
        let path = Path::new("example.c");
        let symbols = plugin.extract_symbols(path, source).unwrap();
        let relations = plugin.extract_relations(path, source, &symbols).unwrap();
        assert!(
            relations
                .iter()
                .any(|r| matches!(r.relation_type, RelationType::Calls)),
            "expected Calls relations, got {relations:?}"
        );
        let foo = symbols.iter().find(|s| s.name == "foo").unwrap();
        assert!(
            relations
                .iter()
                .any(|r| r.from == foo.qualified_name.as_deref().unwrap()),
            "Calls should use qualified caller name"
        );
    }

    #[test]
    fn test_extract_fn_pointer_and_unresolved_calls() {
        let source = br#"
void callee(void) {}

void dispatch(void (*handler)(void)) {
    handler();
    (*handler)();
    callee();
}
"#;
        let plugin = CPlugin::new().unwrap();
        let path = Path::new("handlers.c");
        let symbols = plugin.extract_symbols(path, source).unwrap();
        let relations = plugin.extract_relations(path, source, &symbols).unwrap();
        let unresolved: Vec<_> = relations
            .iter()
            .filter(|r| r.metadata.get("unresolved").and_then(|v| v.as_bool()) == Some(true))
            .collect();
        assert!(
            unresolved.len() >= 2,
            "expected unresolved fn-pointer calls, got {relations:?}"
        );
        assert!(
            relations.iter().any(|r| r.to == "handlers::callee"),
            "direct call should resolve to qualified callee"
        );
    }

    #[test]
    fn test_include_node_kinds() {
        let source = br#"#include <stdio.h>"#;
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_c::LANGUAGE.into())
            .unwrap();
        let tree = parser.parse(source, None).unwrap();
        let mut kinds = Vec::new();
        let mut stack = vec![tree.root_node()];
        while let Some(node) = stack.pop() {
            kinds.push(node.kind().to_string());
            for i in 0..node.child_count() {
                if let Some(child) = node.child(i) {
                    stack.push(child);
                }
            }
        }
        eprintln!("include kinds: {kinds:?}");
        assert!(kinds.iter().any(|k| k.contains("include")));
    }

    #[test]
    fn test_extract_import_symbols_normalized() {
        let source = br#"
#include <stdio.h>
#include "cart.h"
"#;
        let plugin = CPlugin::new().unwrap();
        let symbols = plugin.extract_symbols(Path::new("main.c"), source).unwrap();
        assert!(symbols.iter().any(|s| {
            s.symbol_type == SymbolType::Import
                && s.name == "stdio.h"
                && s.metadata.get("kind").and_then(|v| v.as_str()) == Some("include")
        }));
        assert!(
            symbols
                .iter()
                .any(|s| { s.symbol_type == SymbolType::Import && s.name == "cart.h" })
        );
    }

    #[test]
    fn test_extract_function_prototype() {
        let source = br#"int checkout(int user_id);"#;
        let plugin = CPlugin::new().unwrap();
        let symbols = plugin
            .extract_symbols(Path::new("order.h"), source)
            .unwrap();
        assert!(symbols.iter().any(|s| s.name == "checkout"));
    }

    #[test]
    fn test_main_c_extracts_many_calls() {
        let path = Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../rgctl-tests/ecommerce-c/src/main.c"
        ));
        let source = std::fs::read(path).unwrap();
        let plugin = CPlugin::new().unwrap();
        let symbols = plugin.extract_symbols(path, &source).unwrap();
        let relations = plugin.extract_relations(path, &source, &symbols).unwrap();
        let calls: Vec<_> = relations
            .iter()
            .filter(|r| r.relation_type == RelationType::Calls)
            .collect();
        assert!(
            calls.len() >= 8,
            "main.c should emit many Calls relations, got {calls:?}"
        );
    }

    #[test]
    fn test_adf_admin_deep_ast_does_not_stack_overflow() {
        let path = Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../example/linux/drivers/crypto/intel/qat/qat_common/adf_admin.c"
        ));
        if !path.exists() {
            return;
        }
        let src = std::fs::read(path).unwrap();
        let plugin = CPlugin::new().unwrap();
        let symbols = plugin.extract_symbols(path, &src).unwrap();
        assert!(!symbols.is_empty());
        let _relations = plugin.extract_relations(path, &src, &symbols).unwrap();
    }
}
