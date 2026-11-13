//! Lower operand roles from the same native tree used to build a function CFG.

use super::*;
use crate::language_profile::{canonical_language_id, function_kinds_for};
use rgctl_error::{Error, Result};
use std::collections::HashMap;
use tree_sitter::Node;

const EXPRESSION_DEPTH_LIMIT: usize = 256;

fn index(value: usize) -> Result<u32> {
    u32::try_from(value).map_err(|_| Error::ParseError {
        file: "source".into(),
        line: 0,
        message: "procedural operand index exceeds u32".into(),
    })
}

fn span(node: Node<'_>) -> Result<OperandSpan> {
    Ok(OperandSpan {
        start: index(node.start_byte())?,
        end: index(node.end_byte())?,
    })
}

fn children(node: Node<'_>) -> Vec<Node<'_>> {
    let mut cursor = node.walk();
    node.named_children(&mut cursor)
        .filter(|n| n.kind() != "comment")
        .collect()
}

pub(crate) fn lower_function_operands(
    language: &str,
    function: Node<'_>,
    body: Node<'_>,
    source: &[u8],
) -> Result<FunctionOperands> {
    let language = match canonical_language_id(language).unwrap_or("") {
        "java" => OperandLanguage::Java,
        "javascript" => OperandLanguage::JavaScript,
        "typescript" | "tsx" => OperandLanguage::TypeScript,
        "python" => OperandLanguage::Python,
        "go" => OperandLanguage::Go,
        "rust" => OperandLanguage::Rust,
        _ => {
            return Ok(FunctionOperands {
                status: OperandStatus::UnsupportedLanguage,
                span: Some(span(function)?),
                source_line: Some(index(function.start_position().row + 1)?),
                ..Default::default()
            });
        }
    };
    let mut lowerer = Lowerer {
        facts: FunctionOperands {
            language,
            status: OperandStatus::Captured,
            span: Some(span(function)?),
            source_line: Some(index(function.start_position().row + 1)?),
            ..Default::default()
        },
        source,
        names: HashMap::new(),
        expressions: HashMap::new(),
        nested_kinds: function_kinds_for(match language {
            OperandLanguage::Java => "java",
            OperandLanguage::JavaScript => "javascript",
            OperandLanguage::TypeScript => "typescript",
            OperandLanguage::Python => "python",
            OperandLanguage::Go => "go",
            OperandLanguage::Rust => "rust",
            _ => unreachable!(),
        })?,
    };
    if function.has_error() {
        lowerer.diagnostic(function, OperandDiagnosticKind::MalformedSyntax)?;
    }
    lowerer.parameters(function)?;
    let mut stack = vec![body];
    while let Some(node) = stack.pop() {
        if lowerer.is_callable_boundary(node) {
            lowerer.diagnostic(node, OperandDiagnosticKind::NestedCallable)?;
            continue;
        }
        for (_, value) in local_write_sources(node) {
            if let Some(value) = value {
                lowerer.expression(value, 0)?;
            }
        }
        if let Some(condition) = node.child_by_field_name("condition") {
            lowerer.expression(condition, 0)?;
        }
        match node.kind() {
            "call_expression"
            | "call"
            | "method_invocation"
            | "object_creation_expression"
            | "new_expression" => {
                lowerer.expression(node, 0)?;
            }
            "return_statement" | "return_expression" => lowerer.return_site(node, false)?,
            _ => {}
        }
        // Reuse the traversal stack instead of allocating child vectors for
        // every AST node in the function body.
        let start = stack.len();
        let mut cursor = node.walk();
        stack.extend(
            node.named_children(&mut cursor)
                .filter(|n| n.kind() != "comment"),
        );
        stack[start..].reverse();
    }
    if matches!(
        function.kind(),
        "arrow_function" | "lambda" | "lambda_expression" | "closure_expression"
    ) && !matches!(body.kind(), "block" | "statement_block")
    {
        lowerer.return_site(body, true)?;
    } else if language == OperandLanguage::Rust
        && let Some(tail) = children(body).last().copied()
        && !matches!(
            tail.kind(),
            "let_declaration" | "expression_statement" | "return_expression" | "item"
        )
        && !lowerer.is_callable_boundary(tail)
        && !tail.kind().ends_with("_item")
    {
        lowerer.return_site(tail, true)?;
    }
    Ok(lowerer.facts)
}

struct Lowerer<'a> {
    facts: FunctionOperands,
    source: &'a [u8],
    names: HashMap<&'a str, OperandNameId>,
    expressions: HashMap<usize, ExpressionId>,
    nested_kinds: &'static [&'static str],
}

impl<'a> Lowerer<'a> {
    fn is_callable_boundary(&self, node: Node<'_>) -> bool {
        self.nested_kinds.contains(&node.kind())
            || matches!(
                node.kind(),
                "lambda_expression"
                    | "closure_expression"
                    | "lambda"
                    | "func_literal"
                    | "function_literal"
                    | "class_declaration"
                    | "class_definition"
            )
    }

    fn name(&mut self, text: &'a str) -> Result<OperandNameId> {
        if let Some(&id) = self.names.get(text) {
            return Ok(id);
        }
        let id = OperandNameId(index(self.facts.names.len())?);
        self.facts.names.push(text.into());
        self.names.insert(text, id);
        Ok(id)
    }

    fn node_name(&mut self, node: Node<'_>) -> Result<OperandNameId> {
        self.name(node.utf8_text(self.source).unwrap_or(""))
    }

    fn diagnostic(&mut self, node: Node<'_>, kind: OperandDiagnosticKind) -> Result<()> {
        self.facts.diagnostics.push(OperandDiagnostic {
            span: span(node)?,
            kind,
        });
        Ok(())
    }

    fn inputs(&mut self, values: &[ExpressionId]) -> Result<OperandRange> {
        let range = OperandRange {
            start: index(self.facts.inputs.len())?,
            len: index(values.len())?,
        };
        self.facts.inputs.extend_from_slice(values);
        Ok(range)
    }

    fn formal(
        &mut self,
        name: &'a str,
        node: Option<Node<'_>>,
        kind: FormalKind,
        default: Option<Node<'_>>,
    ) -> Result<()> {
        let id = FormalId(index(self.facts.formals.len())?);
        let name = self.name(name)?;
        let value = ExpressionId(index(self.facts.expressions.len())?);
        self.facts.expressions.push(ExpressionFact {
            span: node.map(span).transpose()?,
            kind: ExpressionKind::Formal(id),
            inputs: OperandRange::default(),
        });
        // Reserve before lowering defaults, which may contain invocations.
        self.facts.formals.push(FormalFact {
            name,
            value,
            kind,
            default_value: None,
            optional: default.is_some()
                || matches!(
                    kind,
                    FormalKind::VariadicPositional | FormalKind::VariadicKeyword
                ),
        });
        let default_value = default.map(|n| self.expression(n, 0)).transpose()?;
        self.facts.formals[id.0 as usize].default_value = default_value;
        Ok(())
    }

    fn parameters(&mut self, function: Node<'_>) -> Result<()> {
        let language = self.facts.language;
        if language == OperandLanguage::Java {
            let is_static = children(function)
                .into_iter()
                .filter(|n| n.kind() == "modifiers")
                .any(|n| {
                    let mut cursor = n.walk();
                    n.children(&mut cursor).any(|n| n.kind() == "static")
                });
            if !is_static
                && matches!(
                    function.kind(),
                    "method_declaration"
                        | "constructor_declaration"
                        | "compact_constructor_declaration"
                )
            {
                self.facts.receiver_convention = ReceiverConvention::ImplicitThis;
                self.formal("this", None, FormalKind::Receiver, None)?;
            }
        } else if matches!(
            language,
            OperandLanguage::JavaScript | OperandLanguage::TypeScript
        ) && function.kind() == "method_definition"
        {
            self.facts.receiver_convention = ReceiverConvention::ImplicitThis;
            self.formal("this", None, FormalKind::Receiver, None)?;
        } else if language == OperandLanguage::Python {
            let mut parent = function.parent();
            let decorated = parent.filter(|n| n.kind() == "decorated_definition");
            if decorated.is_some() {
                parent = parent.and_then(|n| n.parent());
            }
            if parent
                .and_then(|n| n.parent())
                .is_some_and(|n| n.kind() == "class_definition")
            {
                let decorators: Vec<_> = decorated
                    .map(children)
                    .unwrap_or_default()
                    .into_iter()
                    .filter(|n| n.kind() == "decorator")
                    .collect();
                let has_decorator = |name: &str| {
                    decorators.iter().any(|n| {
                        n.named_child(0).and_then(|n| n.utf8_text(self.source).ok()) == Some(name)
                    })
                };
                self.facts.receiver_convention = if has_decorator("staticmethod") {
                    ReceiverConvention::None
                } else if has_decorator("classmethod") {
                    ReceiverConvention::PythonClassDescriptor
                } else {
                    ReceiverConvention::PythonDescriptor
                };
                for decorator in decorators {
                    if !decorator
                        .named_child(0)
                        .and_then(|n| n.utf8_text(self.source).ok())
                        .is_some_and(|name| matches!(name, "staticmethod" | "classmethod"))
                    {
                        self.diagnostic(decorator, OperandDiagnosticKind::ParameterPattern)?;
                    }
                }
            }
        }
        if let Some(receiver) = function.child_by_field_name("receiver") {
            self.facts.receiver_convention = ReceiverConvention::Explicit;
            for param in children(receiver) {
                self.parameter(param, FormalKind::Receiver)?;
            }
        }
        let Some(parameters) = function
            .child_by_field_name("parameters")
            .or_else(|| function.child_by_field_name("parameter"))
        else {
            return Ok(());
        };
        let mut position = 0u32;
        let mut keyword_only = false;
        let nodes = if matches!(parameters.kind(), "identifier") {
            vec![parameters]
        } else {
            children(parameters)
        };
        let mut positional_start = self.facts.formals.len();
        for param in nodes {
            match param.kind() {
                "positional_separator" => {
                    for formal in &mut self.facts.formals[positional_start..] {
                        if let FormalKind::Positional(index) = formal.kind {
                            formal.kind = FormalKind::PositionalOnly(index);
                        }
                    }
                    positional_start = self.facts.formals.len();
                    continue;
                }
                "keyword_separator" => {
                    keyword_only = true;
                    continue;
                }
                "self_parameter" => {
                    self.facts.receiver_convention = ReceiverConvention::Explicit;
                    self.formal("self", Some(param), FormalKind::Receiver, None)?;
                    continue;
                }
                _ => {}
            }
            let rest = matches!(
                param.kind(),
                "rest_pattern"
                    | "spread_parameter"
                    | "variadic_parameter_declaration"
                    | "list_splat_pattern"
            ) || param
                .child_by_field_name("pattern")
                .is_some_and(|n| n.kind() == "rest_pattern");
            let kind = if param.kind() == "dictionary_splat_pattern" {
                FormalKind::VariadicKeyword
            } else if rest {
                FormalKind::VariadicPositional
            } else if keyword_only {
                FormalKind::KeywordOnly
            } else {
                FormalKind::Positional(position)
            };
            let before = self.facts.formals.len();
            self.parameter(param, kind)?;
            if matches!(kind, FormalKind::Positional(_)) {
                position = position
                    .checked_add(index(self.facts.formals.len() - before)?)
                    .ok_or_else(|| Error::SerdeError("formal index overflow".into()))?;
            }
            if language == OperandLanguage::Python && rest {
                keyword_only = true;
            }
        }
        if language == OperandLanguage::Go
            && function.child_by_field_name("result").is_some_and(|n| {
                children(n)
                    .iter()
                    .any(|n| n.child_by_field_name("name").is_some())
            })
        {
            self.diagnostic(function, OperandDiagnosticKind::ReturnConvention)?;
        }
        Ok(())
    }

    fn parameter(&mut self, param: Node<'_>, kind: FormalKind) -> Result<()> {
        // Go may declare several names under one parameter declaration.
        let mut cursor = param.walk();
        let names: Vec<_> = param.children_by_field_name("name", &mut cursor).collect();
        let names = if !names.is_empty() {
            names
        } else {
            let pattern = param
                .child_by_field_name("pattern")
                .or_else(|| param.child_by_field_name("left"))
                .or_else(|| {
                    if matches!(param.kind(), "identifier" | "self") {
                        Some(param)
                    } else {
                        None
                    }
                })
                .or_else(|| {
                    children(param).into_iter().find(|n| {
                        matches!(
                            n.kind(),
                            "identifier" | "self" | "rest_pattern" | "variable_declarator"
                        )
                    })
                });
            match pattern {
                Some(n) if matches!(n.kind(), "rest_pattern" | "variable_declarator") => n
                    .child_by_field_name("name")
                    .or_else(|| n.named_child(0))
                    .into_iter()
                    .collect(),
                Some(n) => vec![n],
                None => vec![],
            }
        };
        if names.is_empty() {
            self.diagnostic(param, OperandDiagnosticKind::ParameterPattern)?;
        }
        let default = param
            .child_by_field_name("value")
            .or_else(|| param.child_by_field_name("right"));
        for (offset, name) in names.into_iter().enumerate() {
            if !matches!(name.kind(), "identifier" | "self") {
                self.diagnostic(name, OperandDiagnosticKind::ParameterPattern)?;
                continue;
            }
            let kind = match kind {
                FormalKind::Positional(position) => FormalKind::Positional(
                    position
                        .checked_add(index(offset)?)
                        .ok_or_else(|| Error::SerdeError("formal index overflow".into()))?,
                ),
                other => other,
            };
            let text = name.utf8_text(self.source).unwrap_or("");
            self.formal(text, Some(name), kind, default)?;
            if param.kind() == "optional_parameter"
                && let Some(formal) = self.facts.formals.last_mut()
            {
                formal.optional = true;
            }
        }
        Ok(())
    }

    fn expression(&mut self, node: Node<'_>, depth: usize) -> Result<ExpressionId> {
        if let Some(&id) = self.expressions.get(&node.id()) {
            return Ok(id);
        }
        let id = ExpressionId(index(self.facts.expressions.len())?);
        self.expressions.insert(node.id(), id);
        self.facts.expressions.push(ExpressionFact {
            span: Some(span(node)?),
            kind: ExpressionKind::Unknown,
            inputs: OperandRange::default(),
        });
        if depth >= EXPRESSION_DEPTH_LIMIT {
            self.diagnostic(node, OperandDiagnosticKind::DepthLimit)?;
            return Ok(id);
        }
        let kind = node.kind();
        let mut inputs = Vec::new();
        let expression_kind = if self.is_callable_boundary(node) {
            self.diagnostic(node, OperandDiagnosticKind::NestedCallable)?;
            ExpressionKind::Unknown
        } else if matches!(
            kind,
            "call_expression"
                | "call"
                | "method_invocation"
                | "object_creation_expression"
                | "new_expression"
        ) {
            ExpressionKind::CallResult(self.call(node, id, depth + 1)?)
        } else if matches!(
            kind,
            "identifier"
                | "self"
                | "this"
                | "super"
                | "type_identifier"
                | "property_identifier"
                | "field_identifier"
        ) {
            ExpressionKind::Name(self.node_name(node)?)
        } else if let Some((receiver, member)) = member_parts(node) {
            inputs.push(self.expression(receiver, depth + 1)?);
            ExpressionKind::Member(self.node_name(member)?)
        } else if matches!(kind, "composite_literal" | "struct_expression") {
            let type_node = node
                .child_by_field_name("type")
                .or_else(|| node.child_by_field_name("name"));
            if let Some(type_node) = type_node {
                let type_value = self.expression(type_node, depth + 1)?;
                inputs.push(type_value);
                for child in children(node) {
                    if child.id() != type_node.id() {
                        inputs.push(self.expression(child, depth + 1)?);
                    }
                }
                ExpressionKind::Construction(type_value)
            } else {
                self.diagnostic(node, OperandDiagnosticKind::InvocationShape)?;
                ExpressionKind::Unknown
            }
        } else if matches!(
            kind,
            "integer"
                | "float"
                | "number"
                | "string"
                | "true"
                | "false"
                | "none"
                | "null"
                | "nil"
                | "integer_literal"
                | "decimal_integer_literal"
                | "hex_integer_literal"
                | "floating_point_literal"
                | "boolean_literal"
                | "null_literal"
                | "string_literal"
                | "interpreted_string_literal"
                | "raw_string_literal"
                | "char_literal"
                | "character_literal"
        ) && !children(node)
            .iter()
            .any(|n| n.kind().contains("interpolation") || n.kind() == "template_substitution")
        {
            ExpressionKind::Literal
        } else {
            for child in children(node) {
                inputs.push(self.expression(child, depth + 1)?);
            }
            if kind == "parenthesized_expression" {
                ExpressionKind::Parenthesized
            } else {
                ExpressionKind::Computation
            }
        };
        let inputs = self.inputs(&inputs)?;
        self.facts.expressions[id.0 as usize] = ExpressionFact {
            span: Some(span(node)?),
            kind: expression_kind,
            inputs,
        };
        Ok(id)
    }

    fn call(&mut self, node: Node<'_>, result: ExpressionId, depth: usize) -> Result<CallSiteId> {
        let id = CallSiteId(index(self.facts.calls.len())?);
        let kind = if matches!(node.kind(), "new_expression" | "object_creation_expression") {
            InvocationKind::Construction
        } else {
            InvocationKind::Call
        };
        self.facts.calls.push(CallSiteFact {
            span: span(node)?,
            callee: result,
            receiver: None,
            arguments: OperandRange::default(),
            result,
            kind,
        });
        let callee_node = node
            .child_by_field_name("function")
            .or_else(|| node.child_by_field_name("constructor"))
            .or_else(|| node.child_by_field_name("name"))
            .or_else(|| node.child_by_field_name("type"));
        let receiver_node = node.child_by_field_name("object").or_else(|| {
            callee_node
                .and_then(member_parts)
                .map(|(receiver, _)| receiver)
        });
        let receiver = receiver_node
            .map(|n| self.expression(n, depth))
            .transpose()?;
        let callee = match callee_node {
            Some(n) => self.expression(n, depth)?,
            None => {
                self.diagnostic(node, OperandDiagnosticKind::InvocationShape)?;
                result
            }
        };
        // Collect locally, then append after nested invocations have populated
        // their own argument ranges. Parent arguments stay contiguous.
        let mut arguments = Vec::new();
        if let Some(list) = node.child_by_field_name("arguments") {
            for argument in children(list) {
                let (value, kind) = match argument.kind() {
                    "keyword_argument" => {
                        let name = argument.child_by_field_name("name");
                        let value = argument.child_by_field_name("value");
                        match (name, value) {
                            (Some(name), Some(value)) => {
                                (value, ArgumentKind::Named(self.node_name(name)?))
                            }
                            _ => {
                                self.diagnostic(argument, OperandDiagnosticKind::InvocationShape)?;
                                (argument, ArgumentKind::Positional)
                            }
                        }
                    }
                    "spread_element" | "list_splat" | "variadic_argument" => (
                        argument.named_child(0).unwrap_or(argument),
                        ArgumentKind::SpreadPositional,
                    ),
                    "dictionary_splat" => (
                        argument.named_child(0).unwrap_or(argument),
                        ArgumentKind::SpreadKeywords,
                    ),
                    _ => (argument, ArgumentKind::Positional),
                };
                arguments.push(ArgumentFact {
                    value: self.expression(value, depth)?,
                    kind,
                });
            }
        }
        let range = OperandRange {
            start: index(self.facts.arguments.len())?,
            len: index(arguments.len())?,
        };
        self.facts.arguments.extend(arguments);
        self.facts.calls[id.0 as usize] = CallSiteFact {
            span: span(node)?,
            callee,
            receiver,
            arguments: range,
            result,
            kind,
        };
        Ok(id)
    }

    fn return_site(&mut self, node: Node<'_>, implicit: bool) -> Result<()> {
        let nodes = if implicit {
            vec![node]
        } else {
            let nodes = children(node);
            if self.facts.language == OperandLanguage::Go
                && nodes.first().is_some_and(|n| n.kind() == "expression_list")
            {
                children(nodes[0])
            } else {
                nodes
            }
        };
        let mut values = Vec::new();
        for value in nodes {
            values.push(self.expression(value, 0)?);
        }
        let values = self.inputs(&values)?;
        self.facts.returns.push(ReturnFact {
            span: span(node)?,
            values,
            implicit,
        });
        Ok(())
    }
}

fn member_parts(node: Node<'_>) -> Option<(Node<'_>, Node<'_>)> {
    if !matches!(
        node.kind(),
        "member_expression" | "attribute" | "selector_expression" | "field_expression"
    ) {
        return None;
    }
    let object = node
        .child_by_field_name("object")
        .or_else(|| node.child_by_field_name("operand"))
        .or_else(|| node.child_by_field_name("value"))?;
    let member = node
        .child_by_field_name("property")
        .or_else(|| node.child_by_field_name("attribute"))
        .or_else(|| node.child_by_field_name("field"))?;
    Some((object, member))
}

/// Native simple-local write operands. Complex patterns intentionally have no
/// whole-value alias mapping. This helper never parses or walks a second tree.
pub(crate) fn local_write_sources(node: Node<'_>) -> Vec<(Node<'_>, Option<Node<'_>>)> {
    let mut root = node;
    while root.kind() == "expression_statement" && root.named_child_count() == 1 {
        root = root.named_child(0).unwrap_or(root);
    }
    fn scalar(value: Node<'_>) -> Node<'_> {
        if value.kind() == "expression_list" && value.named_child_count() == 1 {
            value.named_child(0).unwrap_or(value)
        } else {
            value
        }
    }
    match root.kind() {
        "let_declaration" | "let_statement" => root
            .child_by_field_name("pattern")
            .map(|target| vec![(target, root.child_by_field_name("value").map(scalar))])
            .unwrap_or_default(),
        "var_declaration" | "var_spec_list" => children(root)
            .into_iter()
            .flat_map(local_write_sources)
            .collect(),
        "variable_declaration"
        | "lexical_declaration"
        | "local_variable_declaration"
        | "local_declaration_statement" => children(root)
            .into_iter()
            .filter(|n| n.kind() == "variable_declarator")
            .filter_map(|n| {
                n.child_by_field_name("name")
                    .map(|name| (name, n.child_by_field_name("value")))
            })
            .collect(),
        "variable_declarator" | "var_spec" => root
            .child_by_field_name("name")
            .map(|target| vec![(target, root.child_by_field_name("value").map(scalar))])
            .unwrap_or_default(),
        "assignment"
        | "assignment_expression"
        | "assignment_statement"
        | "short_var_declaration" => {
            let Some(left) = root.child_by_field_name("left") else {
                return vec![];
            };
            let Some(right) = root.child_by_field_name("right") else {
                return vec![(left, None)];
            };
            let lefts = if left.kind() == "expression_list" {
                children(left)
            } else {
                vec![left]
            };
            let rights = if right.kind() == "expression_list" {
                children(right)
            } else {
                vec![right]
            };
            if lefts.len() != rights.len() {
                return lefts.into_iter().map(|n| (n, None)).collect();
            }
            lefts
                .into_iter()
                .zip(rights.into_iter().map(Some))
                .collect()
        }
        _ => vec![],
    }
}
