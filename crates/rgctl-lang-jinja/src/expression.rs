//! Neutral expression facts shared by Jinja templates and Ansible YAML.

use crate::ParsedTemplate;
use serde::{Deserialize, Serialize};
use std::ops::Range;
use tree_sitter::Node;

/// A variable read and its complete static/dynamic access spelling.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VariableAccess {
    /// Root context or lexical variable name.
    pub name: String,
    /// Original access spelling, such as hostvars[target]['ansible_host'].
    pub access: String,
    /// Span in the original expression/template bytes.
    pub range: Range<usize>,
}

/// Invocation classification without importing or executing a plugin.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InvocationKind {
    /// Jinja/Ansible filter application.
    Filter,
    /// Jinja/Ansible test application.
    Test,
    /// Runtime lookup/query invocation.
    Lookup,
    /// Potential macro/context/builtin call; repository resolution is separate.
    Call,
}

/// An invocation's source spelling and argument evidence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Invocation {
    /// Invocation category.
    pub kind: InvocationKind,
    /// Qualified filter/test/callee spelling.
    pub name: String,
    /// Source spellings of arguments, without evaluating them.
    pub arguments: Vec<String>,
    /// Decoded literal arguments, or `None` when an argument is an expression.
    #[serde(default)]
    pub literal_arguments: Vec<Option<String>>,
    /// Literal lookup-plugin name when one is supplied.
    pub plugin: Option<String>,
    /// Whether a test was negated with "is not".
    pub negated: bool,
    /// Original source span.
    pub range: Range<usize>,
}

/// Syntax-level reads and invocations, independent of lexical binding.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExpressionFacts {
    /// Variable access occurrences, including dynamic index inputs.
    pub variables: Vec<VariableAccess>,
    /// Calls, filters, tests, and lookups.
    pub invocations: Vec<Invocation>,
}

pub(crate) fn text<'a>(parsed: &'a ParsedTemplate, node: Node<'_>) -> &'a str {
    node.utf8_text(&parsed.normalized).unwrap_or("")
}

pub(crate) fn children(node: Node<'_>) -> Vec<Node<'_>> {
    let mut cursor = node.walk();
    let mut children = Vec::with_capacity(node.named_child_count());
    children.extend(node.named_children(&mut cursor));
    children
}

pub(crate) fn peel(mut node: Node<'_>) -> Node<'_> {
    while node.named_child_count() == 1
        && matches!(
            node.kind(),
            "expression"
                | "binary_expression"
                | "unary_expression"
                | "primary_expression"
                | "literal"
        )
        && node.child_by_field_name("object").is_none()
        && node.child_by_field_name("function").is_none()
        && node.child_by_field_name("operator").is_none()
    {
        let Some(child) = node.named_child(0) else {
            break;
        };
        node = child;
    }
    node
}

pub(crate) fn literal_string(parsed: &ParsedTemplate, node: Node<'_>) -> Option<String> {
    let node = peel(node);
    if node.kind() != "string_literal" {
        return None;
    }
    let raw = text(parsed, node);
    if raw.len() < 2 {
        return None;
    }
    let mut decoded = String::with_capacity(raw.len() - 2);
    let mut escaped = false;
    for character in raw[1..raw.len() - 1].chars() {
        if escaped {
            match character {
                'n' => decoded.push('\n'),
                'r' => decoded.push('\r'),
                't' => decoded.push('\t'),
                '\\' | '\'' | '"' => decoded.push(character),
                other => {
                    decoded.push('\\');
                    decoded.push(other);
                }
            }
            escaped = false;
        } else if character == '\\' {
            escaped = true;
        } else {
            decoded.push(character);
        }
    }
    if escaped {
        decoded.push('\\');
    }
    Some(decoded)
}

pub(crate) fn root_identifier(mut node: Node<'_>) -> Option<Node<'_>> {
    loop {
        node = peel(node);
        if node.kind() == "identifier" {
            return Some(node);
        }
        node = node.child_by_field_name("object")?;
    }
}

fn call_parts(node: Node<'_>) -> Option<(Node<'_>, Vec<Node<'_>>)> {
    let node = peel(node);
    let callee = if node.kind() == "function_call" {
        node.named_child(0)?
    } else {
        node.child_by_field_name("function")?
    };
    let arguments = children(node)
        .into_iter()
        .filter(|child| child.kind() == "arg")
        .filter_map(|arg| {
            // Keyword names and '=' are syntax, not context inputs.
            children(arg)
                .into_iter()
                .rev()
                .find(|n| n.kind() == "expression")
        })
        .collect();
    Some((callee, arguments))
}

fn invoke<'tree>(
    parsed: &ParsedTemplate,
    node: Node<'tree>,
    kind: InvocationKind,
) -> (Invocation, Vec<Node<'tree>>) {
    let original = peel(node);
    let mut node = original;
    let negated = kind == InvocationKind::Test
        && node.kind() == "unary_expression"
        && node
            .named_child(0)
            .is_some_and(|operator| text(parsed, operator) == "not");
    if negated && let Some(operand) = node.named_child(1) {
        node = peel(operand);
    }
    let (callee, arguments) = if node.kind() == "builtin_test" {
        (
            node,
            children(node)
                .into_iter()
                .filter(|child| child.kind() == "expression")
                .collect(),
        )
    } else {
        call_parts(node).unwrap_or((node, Vec::new()))
    };
    let name = if node.kind() == "builtin_test" {
        text(parsed, callee)
            .split_whitespace()
            .next()
            .unwrap_or("")
            .to_owned()
    } else {
        text(parsed, callee).trim().to_owned()
    };
    let kind = if kind == InvocationKind::Call && matches!(name.as_str(), "lookup" | "query" | "q")
    {
        InvocationKind::Lookup
    } else {
        kind
    };
    let plugin = if kind == InvocationKind::Lookup {
        arguments
            .first()
            .and_then(|arg| literal_string(parsed, *arg))
    } else {
        None
    };
    let invocation = Invocation {
        kind,
        name,
        arguments: arguments
            .iter()
            .map(|arg| text(parsed, *arg).to_owned())
            .collect(),
        literal_arguments: arguments
            .iter()
            .map(|arg| literal_string(parsed, *arg))
            .collect(),
        plugin,
        negated,
        range: parsed.original_range(original),
    };
    (invocation, arguments)
}

/// Extract neutral reads and invocations from an expression subtree.
///
/// Error regions and literal contents are skipped. Binding to source variables
/// or project macros is the responsibility of the containing language extractor.
pub fn expression_facts(parsed: &ParsedTemplate, root: Node<'_>) -> ExpressionFacts {
    let mut facts = ExpressionFacts::default();
    let mut pending = vec![root];
    while let Some(node) = pending.pop() {
        if node.is_error() || node.is_missing() {
            continue;
        }
        let node = peel(node);
        if let Some(operator) = node.child_by_field_name("operator")
            && matches!(text(parsed, operator).trim(), "|" | "is")
        {
            if let Some(left) = node.child_by_field_name("left") {
                pending.push(left);
            }
            if let Some(right) = node.child_by_field_name("right") {
                let kind = if text(parsed, operator) == "|" {
                    InvocationKind::Filter
                } else {
                    InvocationKind::Test
                };
                let (invocation, arguments) = invoke(parsed, right, kind);
                facts.invocations.push(invocation);
                pending.extend(arguments);
            }
            continue;
        }
        if let Some((callee, _)) = call_parts(node) {
            let (invocation, arguments) = invoke(parsed, node, InvocationKind::Call);
            if let Some(object) = peel(callee).child_by_field_name("object") {
                pending.push(object);
            }
            facts.invocations.push(invocation);
            pending.extend(arguments);
            continue;
        }
        if node.child_by_field_name("object").is_some()
            && let Some(root) = root_identifier(node)
        {
            facts.variables.push(VariableAccess {
                name: text(parsed, root).to_owned(),
                access: text(parsed, node).to_owned(),
                range: parsed.original_range(node),
            });
            let mut access = node;
            while let Some(object) = peel(access).child_by_field_name("object") {
                if let Some(index) = peel(access).child_by_field_name("index") {
                    pending.push(index);
                }
                access = object;
            }
            continue;
        }
        if node.kind() == "identifier" {
            let name = text(parsed, node).to_owned();
            facts.variables.push(VariableAccess {
                access: name.clone(),
                name,
                range: parsed.original_range(node),
            });
        } else if !matches!(
            node.kind(),
            "string_literal" | "comment" | "raw_body" | "content"
        ) {
            pending.extend(children(node).into_iter().rev());
        }
    }
    facts.variables.sort_by_key(|access| access.range.start);
    facts
        .invocations
        .sort_by_key(|invocation| invocation.range.start);
    facts
}

pub(crate) fn block_filter_facts(parsed: &ParsedTemplate, node: Node<'_>) -> ExpressionFacts {
    let (invocation, arguments) = invoke(parsed, node, InvocationKind::Filter);
    let mut facts = ExpressionFacts {
        variables: Vec::new(),
        invocations: vec![invocation],
    };
    for argument in arguments {
        let argument = expression_facts(parsed, argument);
        facts.variables.extend(argument.variables);
        facts.invocations.extend(argument.invocations);
    }
    facts.variables.sort_by_key(|access| access.range.start);
    facts
        .invocations
        .sort_by_key(|invocation| invocation.range.start);
    facts
}
