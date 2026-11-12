//! Language-selected callable identity contracts, independent of display
//! signatures, parameter spelling, body text and source coordinates.
use rgctl_plugin_api::{Relation, RelationType, Result, Symbol, SymbolType};

pub mod java;
use std::collections::{BTreeMap, HashMap};
use tree_sitter::Node;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Policy {
    Java,
    C,
    Cpp,
    Csharp,
    Go,
    Groovy,
    Kotlin,
    Swift,
    Scala,
    Dart,
    Rust,
    Python,
    Ruby,
    Php,
    Javascript,
    Typescript,
    Elixir,
    Lua,
    Bash,
    Sql,
    Puppet,
}
impl Policy {
    fn typed(self) -> bool {
        matches!(
            self,
            Self::Java
                | Self::Cpp
                | Self::Csharp
                | Self::Groovy
                | Self::Kotlin
                | Self::Swift
                | Self::Scala
                | Self::Sql
        )
    }
    fn dynamic(self) -> bool {
        matches!(
            self,
            Self::Python | Self::Ruby | Self::Php | Self::Javascript | Self::Lua | Self::Bash
        )
    }
}
fn text<'a>(node: Node<'_>, source: &'a [u8]) -> &'a str {
    node.utf8_text(source).unwrap_or("")
}
fn kids(node: Node<'_>) -> Vec<Node<'_>> {
    let mut cursor = node.walk();
    node.named_children(&mut cursor).collect()
}
fn normalized(node: Node<'_>, source: &[u8], omit: Option<Node<'_>>) -> String {
    let mut out = String::new();
    let mut stack = vec![node];
    while let Some(n) = stack.pop() {
        if omit.is_some_and(|omit| omit.id() == n.id())
            || matches!(
                n.kind(),
                "comment"
                    | "line_comment"
                    | "block_comment"
                    | "annotation"
                    | "marker_annotation"
                    | "attribute_list"
            )
        {
            continue;
        }
        if n.child_count() == 0
            || matches!(n.kind(), "string" | "string_literal" | "character_literal")
        {
            out.push_str(text(n, source));
        } else {
            for i in (0..n.child_count()).rev() {
                if let Some(c) = n.child(i) {
                    stack.push(c);
                }
            }
        }
    }
    out
}
fn header_nodes(node: Node<'_>, wanted: impl Fn(Node<'_>) -> bool) -> Vec<Node<'_>> {
    let mut result = Vec::new();
    let mut stack = kids(node);
    stack.reverse();
    while let Some(n) = stack.pop() {
        if wanted(n) {
            result.push(n);
            continue;
        }
        if matches!(
            n.kind(),
            "body"
                | "block"
                | "statement_block"
                | "compound_statement"
                | "function_body"
                | "class_body"
                | "declaration_list"
                | "template_body"
                | "do_block"
        ) {
            continue;
        }
        let mut children = kids(n);
        children.reverse();
        stack.extend(children);
    }
    result
}
fn parameter_groups(node: Node<'_>, policy: Policy) -> Vec<Vec<Node<'_>>> {
    if policy == Policy::Swift {
        return vec![
            kids(node)
                .into_iter()
                .filter(|n| n.kind() == "parameter")
                .collect(),
        ];
    }
    header_nodes(node, |n| {
        matches!(
            n.kind(),
            "parameter_list"
                | "formal_parameters"
                | "function_value_parameters"
                | "parameters"
                | "function_arguments"
                | "formal_parameter_list"
                | "parameter_clause"
        )
    })
    .into_iter()
    .map(|n| {
        kids(n)
            .into_iter()
            .filter(|n| !matches!(n.kind(), "comment" | "line_comment" | "block_comment"))
            .collect()
    })
    .collect()
}
fn c_declarator(node: Node<'_>, source: &[u8], policy: Policy) -> String {
    if matches!(node.kind(), "identifier" | "field_identifier") {
        return String::new();
    }
    if node.kind() == "parameter_list" {
        return format!(
            "({})",
            kids(node)
                .into_iter()
                .filter(|n| n.kind() != "comment")
                .map(|p| parameter_type(p, source, policy))
                .collect::<Vec<_>>()
                .join(",")
        );
    }
    let mut result = String::new();
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "comment" {
            continue;
        }
        if child.is_named() {
            result.push_str(&c_declarator(child, source, policy));
        } else {
            result.push_str(text(child, source));
        }
    }
    if node.child_count() == 0 {
        result.push_str(text(node, source));
    }
    result
}
fn parameter_type(parameter: Node<'_>, source: &[u8], policy: Policy) -> String {
    if matches!(parameter.kind(), "self_parameter") {
        return "self".into();
    }
    let ty = parameter.child_by_field_name("type").or_else(|| {
        kids(parameter).into_iter().rev().find(|n| {
            matches!(
                n.kind(),
                "type"
                    | "user_type"
                    | "type_identifier"
                    | "predefined_type"
                    | "primitive_type"
                    | "array_type"
                    | "function_type"
                    | "tuple_type"
                    | "nullable_type"
                    | "optional_type"
                    | "int"
                    | "keyword_text"
            )
        })
    });
    let mut result = ty.map(|n| normalized(n, source, None)).unwrap_or_else(|| {
        if policy == Policy::Sql {
            let children = kids(parameter);
            let skip = children
                .first()
                .filter(|n| n.kind() == "identifier" && children.len() > 1)
                .copied();
            normalized(parameter, source, skip)
        } else {
            "unknown".into()
        }
    });
    if policy == Policy::Cpp || policy == Policy::C {
        let mut cursor = parameter.walk();
        for child in parameter.children(&mut cursor) {
            if child.kind() == "type_qualifier" {
                result = format!("{}{result}", normalized(child, source, None));
            }
        }
        if let Some(declarator) = parameter.child_by_field_name("declarator") {
            result.push_str(&c_declarator(declarator, source, policy));
        }
    }
    result = result.trim_start_matches(':').to_owned();
    if policy == Policy::Csharp {
        for child in kids(parameter) {
            if matches!(text(child, source), "ref" | "out" | "in") {
                result = format!("{}:{result}", text(child, source));
            }
        }
    }
    if policy == Policy::Swift {
        let label = parameter
            .child_by_field_name("external_name")
            .or_else(|| {
                kids(parameter)
                    .into_iter()
                    .find(|n| n.kind() == "simple_identifier")
            })
            .map(|n| text(n, source))
            .unwrap_or("_");
        result = format!("{label}:{result}");
    }
    if policy == Policy::Sql {
        result = result.to_lowercase();
        if result == "int" || result == "int4" {
            result = "integer".into();
        }
    }
    result
}
fn has_body(node: Node<'_>) -> bool {
    node.child_by_field_name("body").is_some()
        || kids(node).iter().any(|n| {
            n.kind().ends_with("_body")
                || n.kind().ends_with("_block")
                || matches!(
                    n.kind(),
                    "block"
                        | "compound_statement"
                        | "statement_block"
                        | "function_body"
                        | "do_block"
                        | "keywords"
                )
        })
}
fn generic_context(node: Node<'_>, source: &[u8]) -> (Vec<String>, Vec<(String, String)>, String) {
    let mut containers = header_nodes(node, |n| {
        matches!(
            n.kind(),
            "type_parameters"
                | "type_parameter_list"
                | "template_parameter_list"
                | "generic_parameters"
                | "generic_parameter_clause"
        )
    });
    let parent = node.parent().filter(|p| p.kind() == "template_declaration");
    if let Some(parent) = parent {
        containers.extend(
            kids(parent)
                .into_iter()
                .filter(|n| n.kind() == "template_parameter_list"),
        );
    }
    let mut shapes = Vec::new();
    let mut bindings = Vec::new();
    for container in containers {
        for parameter in kids(container)
            .into_iter()
            .filter(|n| n.kind() != "comment")
        {
            let name = parameter
                .child_by_field_name("name")
                .or_else(|| parameter.child_by_field_name("declarator"))
                .or_else(|| {
                    kids(parameter).into_iter().find(|n| {
                        matches!(
                            n.kind(),
                            "identifier" | "type_identifier" | "simple_identifier"
                        )
                    })
                });
            let shape = if parameter.kind().contains("type_parameter")
                || parameter.kind() == "identifier"
                || parameter.kind() == "type_identifier"
            {
                "type".into()
            } else {
                parameter
                    .child_by_field_name("type")
                    .map(|n| normalized(n, source, None))
                    .unwrap_or_else(|| "type".into())
            };
            if let Some(name) = name {
                bindings.push((text(name, source).into(), format!("$T{}", shapes.len())));
            }
            shapes.push(shape);
        }
    }
    let constraints = parent
        .into_iter()
        .flat_map(kids)
        .chain(kids(node))
        .filter(|n| n.kind() == "requires_clause")
        .map(|n| normalized(n, source, None))
        .collect::<String>();
    (shapes, bindings, constraints)
}
fn rename_type_variables(value: &str, bindings: &[(String, String)]) -> String {
    let mut out = String::new();
    let mut start = 0;
    for (index, ch) in value.char_indices() {
        if !ch.is_alphanumeric() && ch != '_' && ch != '$' {
            let word = &value[start..index];
            out.push_str(
                bindings
                    .iter()
                    .find(|(name, _)| name == word)
                    .map(|(_, value)| value.as_str())
                    .unwrap_or(word),
            );
            out.push(ch);
            start = index + ch.len_utf8();
        }
    }
    let word = &value[start..];
    out.push_str(
        bindings
            .iter()
            .find(|(name, _)| name == word)
            .map(|(_, value)| value.as_str())
            .unwrap_or(word),
    );
    out
}

/// Common source-span attachment for callable declarations.
fn source_span(symbol: &mut Symbol, node: Node<'_>) {
    symbol.start_byte = symbol
        .start_byte
        .or_else(|| u32::try_from(node.start_byte()).ok());
    symbol.end_byte = symbol
        .end_byte
        .or_else(|| u32::try_from(node.end_byte()).ok());
    symbol.metadata["source_start_byte"] = node.start_byte().into();
    symbol.metadata["source_end_byte"] = node.end_byte().into();
}

/// Fallible entry point for policies whose syntax validation can fail. Java
/// retains parse errors rather than substituting an empty/default identity.
pub fn try_symbol(
    mut symbol: Symbol,
    node: Node<'_>,
    source: &[u8],
    policy: Policy,
) -> Result<Symbol> {
    if policy != Policy::Java {
        return Ok(self::symbol(symbol, node, source, policy));
    }
    if !matches!(
        symbol.symbol_type,
        SymbolType::Function | SymbolType::Method
    ) {
        return Ok(symbol);
    }
    source_span(&mut symbol, node);
    match node.kind() {
        "method_declaration" | "constructor_declaration" | "compact_constructor_declaration" => {
            symbol.metadata["identity_discriminator"] = java::discriminator(node, source)?.into();
        }
        "lambda_expression" => {
            symbol.metadata["identity_discriminator"] =
                java::enclosing_discriminator(node, source)?.into();
        }
        _ => {}
    }
    Ok(symbol)
}

/// Apply the selected language contract at the existing CST extraction site.
/// Non-callable symbols are returned unchanged. Java callers use [`try_symbol`]
/// so malformed declaration types are reported as parse errors.
pub fn symbol(mut symbol: Symbol, node: Node<'_>, source: &[u8], policy: Policy) -> Symbol {
    if !matches!(
        symbol.symbol_type,
        SymbolType::Function | SymbolType::Method
    ) {
        if symbol.symbol_type == SymbolType::Variable {
            crate::spans::field_symbol_spans(&mut symbol, node, source);
        }
        return symbol;
    }
    source_span(&mut symbol, node);
    let groups = parameter_groups(node, policy);
    let parameters: Vec<Vec<_>> = groups
        .iter()
        .map(|group| {
            group
                .iter()
                .filter(|n| n.kind() != "self_parameter")
                .map(|p| parameter_type(*p, source, policy))
                .collect()
        })
        .collect();
    let parameters = if groups.is_empty() {
        vec![
            symbol
                .parameters
                .iter()
                .filter(|p| !matches!(p.name.as_str(), "self" | "&self" | "&mut self"))
                .map(|p| {
                    p.param_type
                        .as_deref()
                        .unwrap_or("unknown")
                        .chars()
                        .filter(|c| !c.is_whitespace())
                        .collect()
                })
                .collect::<Vec<String>>(),
        ]
    } else {
        parameters
    };
    let (generic_shapes, generic_bindings, constraints) = generic_context(node, source);
    let parameters: Vec<Vec<_>> = parameters
        .into_iter()
        .map(|group| {
            group
                .into_iter()
                .map(|value| rename_type_variables(&value, &generic_bindings))
                .collect()
        })
        .collect();
    if !generic_shapes.is_empty() {
        symbol.metadata["generic_arity"] = generic_shapes.len().into();
    }
    let tuple = parameters
        .iter()
        .map(|p| format!("({})", p.join(",")))
        .collect::<String>();
    symbol.metadata["parameter_identity"] = tuple.clone().into();
    let declaration = !has_body(node);
    symbol.metadata["callable_role"] = if declaration {
        "declaration"
    } else {
        "implementation"
    }
    .into();
    if policy.typed() {
        let mut identity = if generic_shapes.is_empty() {
            tuple.clone()
        } else {
            format!("generic[{}]{tuple}", generic_shapes.join(","))
        };
        if !constraints.is_empty() {
            identity.push_str(&format!(
                "requires:{}",
                rename_type_variables(&constraints, &generic_bindings)
            ));
        }
        if policy == Policy::Cpp {
            let declarator = header_nodes(node, |n| n.kind() == "function_declarator")
                .into_iter()
                .next();
            if let Some(declarator) = declarator {
                for qualifier in kids(declarator)
                    .into_iter()
                    .filter(|n| matches!(n.kind(), "type_qualifier" | "ref_qualifier"))
                {
                    identity.push_str(&normalized(qualifier, source, None));
                }
            }
        }
        if policy == Policy::Kotlin {
            let name = node.child_by_field_name("name");
            if let Some(receiver) = kids(node).into_iter().find(|n| {
                name.is_some_and(|name| n.end_byte() < name.start_byte())
                    && matches!(n.kind(), "user_type" | "nullable_type")
            }) {
                identity = format!("receiver:{}:{identity}", normalized(receiver, source, None));
            }
        }
        symbol.metadata["override_identity"] = identity.clone().into();
        if declaration {
            identity = format!("declaration:{identity}");
        }
        symbol.metadata["identity_discriminator"] = identity.into();
    }
    if policy == Policy::C && declaration {
        symbol.metadata["identity_discriminator"] = format!("declaration:{tuple}").into();
    }
    if policy == Policy::Rust {
        let mut ancestor = node.parent();
        let mut implementation = None;
        while let Some(parent) = ancestor {
            if parent.kind() == "impl_item" {
                implementation = Some(parent);
                break;
            }
            ancestor = parent.parent();
        }
        if let Some(implementation) = implementation {
            if let Some(trait_name) = implementation.child_by_field_name("trait") {
                let owner = normalized(trait_name, source, None);
                symbol.metadata["identity_discriminator"] = format!("trait:{owner}").into();
                symbol.metadata["override_owner"] = owner.into();
            } else {
                symbol.metadata["override_owner"] = "<inherent>".into();
            }
        }
        symbol.metadata["override_identity"] = tuple.clone().into();
    }
    if policy == Policy::Typescript {
        let identity = if declaration {
            format!(
                "declaration:{tuple}->{}",
                symbol
                    .return_type
                    .as_deref()
                    .unwrap_or("unknown")
                    .chars()
                    .filter(|c| !c.is_whitespace())
                    .collect::<String>()
            )
        } else {
            "implementation".into()
        };
        symbol.metadata["identity_discriminator"] = identity.into();
        symbol.metadata["override_identity"] = "".into();
    }
    if policy == Policy::Elixir {
        let arguments = node
            .child_by_field_name("arguments")
            .or_else(|| kids(node).into_iter().find(|n| n.kind() == "arguments"));
        let mut head = arguments.and_then(|n| kids(n).into_iter().next());
        while let Some(n) = head {
            if n.kind() == "binary_operator" {
                head = n.child_by_field_name("left");
            } else {
                break;
            }
        }
        if let Some(head) = head {
            let args = head
                .child_by_field_name("arguments")
                .or_else(|| kids(head).into_iter().find(|n| n.kind() == "arguments"));
            let args = args.map(kids).unwrap_or_default();
            let mut patterns = Vec::new();
            for arg in &args {
                patterns.push(if arg.kind() == "identifier" {
                    "_".into()
                } else {
                    normalized(*arg, source, None)
                });
            }
            let variables: std::collections::HashSet<_> = args
                .iter()
                .filter(|n| n.kind() == "identifier")
                .map(|n| text(*n, source))
                .collect();
            let guard_node = arguments
                .and_then(|n| kids(n).into_iter().next())
                .filter(|n| n.kind() == "binary_operator")
                .and_then(|n| n.child_by_field_name("right"));
            let mut guard = String::new();
            if let Some(guard_node) = guard_node {
                let mut stack = vec![guard_node];
                while let Some(n) = stack.pop() {
                    if n.child_count() == 0 {
                        guard.push_str(
                            if n.kind() == "identifier" && variables.contains(text(n, source)) {
                                "_"
                            } else {
                                text(n, source)
                            },
                        );
                    } else {
                        for i in (0..n.child_count()).rev() {
                            if let Some(child) = n.child(i) {
                                stack.push(child);
                            }
                        }
                    }
                }
            }
            symbol.metadata["identity_discriminator"] = format!(
                "arity:{}:clause:({}):{guard}",
                args.len(),
                patterns.join(",")
            )
            .into();
            symbol.metadata["arity"] = args.len().into();
        }
    }
    symbol
}
/// Finalize redefinition groups and annotate relation sources from exact spans.
/// This runs on extraction workers and never reparses or traverses the CST.
pub fn finish(policy: Policy, symbols: &mut [Symbol], relations: &mut [Relation]) {
    let mut groups: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    for (index, symbol) in symbols
        .iter()
        .enumerate()
        .filter(|(_, s)| matches!(s.symbol_type, SymbolType::Function | SymbolType::Method))
    {
        groups
            .entry(
                symbol
                    .qualified_name
                    .as_deref()
                    .unwrap_or(&symbol.name)
                    .into(),
            )
            .or_default()
            .push(index);
    }
    for indices in groups.values_mut() {
        indices.sort_by_key(|i| {
            symbols[*i]
                .metadata
                .get("source_start_byte")
                .and_then(|v| v.as_u64())
                .unwrap_or(0)
        });
        if policy.dynamic() && indices.len() > 1 {
            for (ordinal, &index) in indices.iter().enumerate() {
                if ordinal > 0 {
                    symbols[index].metadata["identity_discriminator"] =
                        format!("definition:{ordinal}").into();
                }
                symbols[index].metadata["override_identity"] = "".into();
                symbols[index].metadata["superseded_definition"] =
                    (ordinal + 1 < indices.len()).into();
            }
        }
    }
    let names: HashMap<String, Vec<&Symbol>> = groups
        .into_iter()
        .map(|(name, indices)| (name, indices.into_iter().map(|i| &symbols[i]).collect()))
        .collect();
    for relation in relations {
        let from_candidates = names.get(&relation.from);
        let from = from_candidates
            .and_then(|candidates| {
                let mut matches = candidates.iter().copied().filter(|s| {
                    s.location.file == relation.location.file
                        && relation
                            .metadata
                            .get("from_identity_discriminator")
                            .and_then(|v| v.as_str())
                            .is_none_or(|identity| {
                                s.metadata
                                    .get("identity_discriminator")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("")
                                    == identity
                            })
                        && s.location.start_line <= relation.location.start_line
                        && s.location.end_line >= relation.location.end_line
                        && (s.location.start_line != relation.location.start_line
                            || s.location.start_column <= relation.location.start_column)
                        && (s.location.end_line != relation.location.end_line
                            || s.location.end_column >= relation.location.end_column)
                });
                let first = matches.next()?;
                if matches.next().is_some() {
                    None
                } else {
                    Some(first)
                }
            })
            .or_else(|| from_candidates.and_then(|c| if c.len() == 1 { Some(c[0]) } else { None }));
        if let Some(from) = from
            && relation
                .metadata
                .get("from_identity_discriminator")
                .and_then(|v| v.as_str())
                .is_none()
        {
            relation.metadata["from_identity_discriminator"] = from
                .metadata
                .get("identity_discriminator")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .into();
        }
        if relation.relation_type == RelationType::Calls {
            if relation.to_qualified_hint.is_none() && !relation.to.contains(['.', ':', '#']) {
                if let Some(from) = from.and_then(|s| s.qualified_name.as_deref()) {
                    let owner = from
                        .rsplit_once("::")
                        .map(|(owner, _)| (owner, "::"))
                        .or_else(|| from.rsplit_once('.').map(|(owner, _)| (owner, ".")))
                        .or_else(|| from.rsplit_once('#').map(|(owner, _)| (owner, "#")));
                    if let Some((owner, separator)) = owner {
                        let target = format!("{owner}{separator}{}", relation.to);
                        relation.to_type_hint = Some(owner.to_owned());
                        relation.to_qualified_hint = Some(target);
                        relation.metadata["inferred_callable_owner"] = true.into();
                    }
                }
            }
            relation.metadata["exact_callable_resolution"] = true.into();
            let target = relation
                .to_qualified_hint
                .as_deref()
                .unwrap_or(&relation.to);
            if let Some(candidates) = names.get(target) {
                let implementations: Vec<_> = candidates
                    .iter()
                    .filter(|s| {
                        s.metadata["callable_role"] != "declaration"
                            && s.metadata["superseded_definition"] != true
                            && relation
                                .metadata
                                .get("explicit_generic_arity")
                                .is_none_or(|arity| s.metadata.get("generic_arity") == Some(arity))
                    })
                    .collect();
                let argument = relation
                    .metadata
                    .get("argument_identity_discriminator")
                    .and_then(|v| v.as_str());
                let matching: Vec<_> = implementations
                    .iter()
                    .filter(|s| {
                        argument.is_some()
                            && s.metadata
                                .get("parameter_identity")
                                .and_then(|v| v.as_str())
                                == argument
                    })
                    .collect();
                let selected = if let [only] = matching.as_slice() {
                    Some(**only)
                } else if let [only] = implementations.as_slice() {
                    if !policy.typed()
                        || argument.is_none()
                        || relation.metadata.get("explicit_generic_arity").is_some()
                    {
                        Some(*only)
                    } else {
                        None
                    }
                } else {
                    None
                };
                if let Some(only) = selected {
                    relation.metadata["to_identity_discriminator"] = only
                        .metadata
                        .get("identity_discriminator")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .into();
                }
            }
        }
    }
}

/// Attach conservative literal/formal-parameter evidence while the call CST is
/// still available. Unknown expressions do not prove an overload selection.
pub fn call(
    node: Node<'_>,
    source: &[u8],
    policy: Policy,
    from: Option<&Symbol>,
    relations: &mut [Relation],
) {
    call_with_local_types(node, source, policy, from, relations, |_| None);
}

/// Like [`call`], with language-plugin scope evidence for local identifiers.
/// The callback does not parse source or change the common identity format.
pub fn call_with_local_types(
    node: Node<'_>,
    source: &[u8],
    policy: Policy,
    from: Option<&Symbol>,
    relations: &mut [Relation],
    local_type: impl FnMut(&str) -> Option<String>,
) {
    if policy == Policy::Java {
        let Some(from) = from else {
            return;
        };
        if let Some(identity) = from
            .metadata
            .get("identity_discriminator")
            .and_then(|v| v.as_str())
        {
            for relation in relations
                .iter_mut()
                .filter(|r| r.relation_type == RelationType::Calls)
            {
                relation.metadata["from_identity_discriminator"] = identity.into();
            }
        }
        if let Some(identity) = java::argument_identity(node, source, from, local_type) {
            attach_argument_identity(relations, &identity);
        }
        return;
    }
    if !policy.typed() && policy != Policy::Typescript {
        return;
    }
    let generics = node.child_by_field_name("type_arguments").or_else(|| {
        node.child_by_field_name("function")
            .or_else(|| node.child_by_field_name("name"))
            .and_then(|callee| {
                header_nodes(callee, |n| {
                    matches!(
                        n.kind(),
                        "type_arguments" | "type_argument_list" | "template_argument_list"
                    )
                })
                .into_iter()
                .next()
            })
            .or_else(|| {
                kids(node).into_iter().find(|n| {
                    matches!(
                        n.kind(),
                        "type_arguments" | "type_argument_list" | "template_argument_list"
                    )
                })
            })
    });
    if let Some(generics) = generics {
        let arity = kids(generics).len();
        for relation in relations.iter_mut() {
            relation.metadata["explicit_generic_arity"] = arity.into();
        }
    }
    let arguments = node.child_by_field_name("arguments").or_else(|| {
        header_nodes(node, |n| {
            matches!(
                n.kind(),
                "argument_list" | "arguments" | "value_arguments" | "value_argument_list"
            )
        })
        .into_iter()
        .next()
    });
    let Some(arguments) = arguments else {
        return;
    };
    let mut types = Vec::new();
    for argument in kids(arguments)
        .into_iter()
        .filter(|n| n.kind() != "comment")
    {
        let value = if matches!(argument.kind(), "value_argument" | "argument") {
            argument
                .child_by_field_name("value")
                .or_else(|| kids(argument).into_iter().last())
                .unwrap_or(argument)
        } else {
            argument
        };
        let raw = text(value, source);
        let capital = matches!(policy, Policy::Kotlin | Policy::Swift | Policy::Scala);
        let ty = match value.kind() {
            "integer_literal"
            | "number_literal"
            | "number"
            | "decimal_integer_literal"
            | "integer"
                if !raw.contains(['.', 'e', 'E']) =>
            {
                if raw.ends_with(['l', 'L']) {
                    if capital { "Long" } else { "long" }
                } else if matches!(policy, Policy::Typescript | Policy::Javascript) {
                    "number"
                } else if capital {
                    "Int"
                } else {
                    "int"
                }
                .to_string()
            }
            "real_literal"
            | "float_literal"
            | "decimal_floating_point_literal"
            | "number_literal"
            | "number" => {
                let number = raw.trim_end_matches(['f', 'F', 'd', 'D', 'm', 'M', 'l', 'L']);
                if number.parse::<f64>().is_err() {
                    return;
                }
                if raw.ends_with(['f', 'F']) {
                    if capital { "Float" } else { "float" }
                } else if policy == Policy::Csharp && raw.ends_with(['m', 'M']) {
                    "decimal"
                } else if policy == Policy::Cpp && raw.ends_with(['l', 'L']) {
                    "longdouble"
                } else if capital {
                    "Double"
                } else {
                    "double"
                }
                .into()
            }
            "string" | "string_literal" | "line_string_literal" => match policy {
                Policy::C | Policy::Cpp => "constchar*",
                Policy::Csharp | Policy::Typescript | Policy::Javascript => "string",
                _ => "String",
            }
            .into(),
            "true" | "false" | "boolean_literal" => match policy {
                Policy::Csharp | Policy::Cpp => "bool",
                Policy::Swift => "Bool",
                Policy::Kotlin | Policy::Scala => "Boolean",
                _ => "boolean",
            }
            .into(),
            "identifier" | "simple_identifier" => match from
                .and_then(|s| s.parameters.iter().find(|p| p.name == raw))
                .and_then(|p| p.param_type.as_ref())
            {
                Some(ty) => ty.chars().filter(|c| !c.is_whitespace()).collect(),
                None => return,
            },
            _ => return,
        };
        types.push(if policy == Policy::Swift {
            let label = argument
                .child_by_field_name("name")
                .map(|n| text(n, source))
                .unwrap_or("_");
            format!("{label}:{ty}")
        } else {
            ty
        });
    }
    attach_argument_identity(relations, &format!("({})", types.join(",")));
}

fn attach_argument_identity(relations: &mut [Relation], identity: &str) {
    for relation in relations
        .iter_mut()
        .filter(|r| r.relation_type == RelationType::Calls)
    {
        relation.metadata["argument_identity_discriminator"] = identity.into();
    }
}
