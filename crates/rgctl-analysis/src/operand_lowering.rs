//! Lower operand roles from the same native tree used to build a function CFG.

use super::*;
use crate::language_profile::{canonical_language_id, function_kinds_for};
use rgctl_error::{Error, Result};
use std::collections::{HashMap, HashSet};
use tree_sitter::Node;

const EXPRESSION_DEPTH_LIMIT: usize = 64;

fn checked_operand_index(value: usize) -> Result<u32> {
    u32::try_from(value).map_err(|_| Error::ParseError {
        file: "source".into(),
        line: 0,
        message: "procedural operand index exceeds u32".into(),
    })
}

fn native_operand_span(node: Node<'_>) -> Result<OperandSpan> {
    Ok(OperandSpan {
        start: checked_operand_index(node.start_byte())?,
        end: checked_operand_index(node.end_byte())?,
    })
}

fn native_operand_children(node: Node<'_>) -> smallvec::SmallVec<[Node<'_>; 8]> {
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
    let canonical = canonical_language_id(language).unwrap_or("");
    let language = match canonical {
        "java" => OperandLanguage::Java,
        "javascript" => OperandLanguage::JavaScript,
        "typescript" | "tsx" => OperandLanguage::TypeScript,
        "python" => OperandLanguage::Python,
        "go" => OperandLanguage::Go,
        "rust" => OperandLanguage::Rust,
        "c" => OperandLanguage::C,
        "cpp" => OperandLanguage::Cpp,
        "csharp" => OperandLanguage::CSharp,
        "groovy" => OperandLanguage::Groovy,
        "kotlin" => OperandLanguage::Kotlin,
        "php" => OperandLanguage::Php,
        "puppet" => OperandLanguage::Puppet,
        "ruby" => OperandLanguage::Ruby,
        "swift" => OperandLanguage::Swift,
        "scala" => OperandLanguage::Scala,
        "dart" => OperandLanguage::Dart,
        "elixir" => OperandLanguage::Elixir,
        "lua" => OperandLanguage::Lua,
        "bash" => OperandLanguage::Bash,

        _ => {
            return Ok(FunctionOperands {
                status: OperandStatus::UnsupportedLanguage,
                span: Some(native_operand_span(function)?),
                source_line: Some(checked_operand_index(function.start_position().row + 1)?),
                ..Default::default()
            });
        }
    };
    let mut lowerer = Lowerer {
        facts: FunctionOperands {
            language,
            status: OperandStatus::Captured,
            span: Some(native_operand_span(function)?),
            source_line: Some(checked_operand_index(function.start_position().row + 1)?),
            ..Default::default()
        },
        source,
        names: HashMap::new(),
        expressions: HashMap::new(),
        field_write_nodes: HashSet::new(),
        nested_kinds: function_kinds_for(canonical)?,
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
        if language == OperandLanguage::Go
            && matches!(
                node.kind(),
                "short_var_declaration" | "assignment_statement"
            )
            && let (Some(left), Some(right)) = (
                node.child_by_field_name("left"),
                node.child_by_field_name("right"),
            )
            && left.named_child_count() > 1
            && right.named_child_count() == 1
            && let Some(rhs) = right.named_child(0)
            && matches!(rhs.kind(), "call_expression" | "type_assertion_expression")
        {
            let source = lowerer.expression(rhs, 0)?;
            for (position, target) in native_operand_children(left).into_iter().enumerate() {
                if target.kind() != "identifier" {
                    continue;
                }
                let value = ExpressionId(checked_operand_index(lowerer.facts.expressions.len())?);
                let inputs = lowerer.inputs(&[source])?;
                lowerer.facts.expressions.push(ExpressionFact {
                    span: Some(native_operand_span(target)?),
                    kind: ExpressionKind::Projection {
                        position: checked_operand_index(position)?,
                    },
                    inputs,
                });
                lowerer.facts.assignment_values.push(AssignmentValueFact {
                    target: native_operand_span(target)?,
                    value,
                });
            }
        }
        for (target, value) in local_write_sources(node) {
            if let Some(value) = value {
                let value = lowerer.expression(value, 0)?;
                if let Some((receiver, member)) = member_parts(target)
                    && lowerer.field_write_nodes.insert(target.id())
                {
                    let receiver = lowerer.expression(receiver, 0)?;
                    let member = lowerer.node_name(member)?;
                    lowerer.facts.field_writes.push(FieldWriteFact {
                        receiver,
                        member,
                        value,
                    });
                } else if let Some(container) = element_base(target)
                    && lowerer.field_write_nodes.insert(target.id())
                {
                    let container = lowerer.expression(container, 0)?;
                    lowerer
                        .facts
                        .element_writes
                        .push(ElementWriteFact { container, value });
                }
            }
        }
        if let Some(condition) = node.child_by_field_name("condition") {
            lowerer.expression(condition, 0)?;
        }
        match node.kind() {
            "invocation_expression"
            | "function_call_expression"
            | "member_call_expression"
            | "scoped_call_expression"
            | "function_call"
            | "command"
            | "call_expression"
            | "call"
            | "method_invocation"
            | "object_creation_expression"
            | "new_expression" => {
                lowerer.expression(node, 0)?;
            }
            "return_statement" | "return_expression" | "return" => {
                lowerer.return_site(node, false)?
            }
            "control_transfer_statement" if node.child_by_field_name("result").is_some() => {
                lowerer.return_site(node, false)?
            }
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
        && let Some(tail) = native_operand_children(body).last().copied()
        && !matches!(
            tail.kind(),
            "let_declaration" | "expression_statement" | "return_expression" | "item"
        )
        && !lowerer.is_callable_boundary(tail)
        && !tail.kind().ends_with("_item")
    {
        lowerer.return_site(tail, true)?;
    }
    if matches!(
        language,
        OperandLanguage::Ruby | OperandLanguage::Elixir | OperandLanguage::Puppet
    ) {
        if let Some(tail) = native_operand_children(body).last().copied()
            && !matches!(
                tail.kind(),
                "return" | "return_statement" | "return_expression"
            )
            && !lowerer.is_callable_boundary(tail)
        {
            lowerer.return_site(tail, true)?;
        }
    }
    Ok(lowerer.facts)
}

struct Lowerer<'a> {
    facts: FunctionOperands,
    source: &'a [u8],
    names: HashMap<&'a str, OperandNameId>,
    expressions: HashMap<usize, ExpressionId>,
    field_write_nodes: HashSet<usize>,
    nested_kinds: &'static [&'static str],
}

impl<'a> Lowerer<'a> {
    fn is_callable_boundary(&self, node: Node<'_>) -> bool {
        (self.nested_kinds.contains(&node.kind())
            && !(self.facts.language == OperandLanguage::Elixir
                && node.kind() == "call"
                && node.child_by_field_name("do_block").is_none()
                && !native_operand_children(node)
                    .iter()
                    .any(|n| n.kind() == "do_block")))
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

    fn intern_operand_name(&mut self, text: &'a str) -> Result<OperandNameId> {
        if let Some(&id) = self.names.get(text) {
            return Ok(id);
        }
        let id = OperandNameId(checked_operand_index(self.facts.names.len())?);
        self.facts.names.push(text.into());
        self.names.insert(text, id);
        Ok(id)
    }

    fn node_name(&mut self, node: Node<'_>) -> Result<OperandNameId> {
        self.intern_operand_name(
            node.utf8_text(self.source)
                .unwrap_or("")
                .trim_start_matches('$'),
        )
    }

    fn diagnostic(&mut self, node: Node<'_>, kind: OperandDiagnosticKind) -> Result<()> {
        self.facts.diagnostics.push(OperandDiagnostic {
            span: native_operand_span(node)?,
            kind,
        });
        Ok(())
    }

    fn inputs(&mut self, values: &[ExpressionId]) -> Result<OperandRange> {
        let range = OperandRange {
            start: checked_operand_index(self.facts.inputs.len())?,
            len: checked_operand_index(values.len())?,
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
        let id = FormalId(checked_operand_index(self.facts.formals.len())?);
        let name = self.intern_operand_name(name)?;
        let value = ExpressionId(checked_operand_index(self.facts.expressions.len())?);
        self.facts.expressions.push(ExpressionFact {
            span: node.map(native_operand_span).transpose()?,
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
        if matches!(
            language,
            OperandLanguage::Java | OperandLanguage::Groovy | OperandLanguage::CSharp
        ) {
            let is_static = native_operand_children(function)
                .into_iter()
                .filter(|n| matches!(n.kind(), "modifiers" | "modifier"))
                .any(|n| {
                    let mut cursor = n.walk();
                    n.kind() == "modifier" && n.utf8_text(self.source).ok() == Some("static")
                        || n.children(&mut cursor).any(|n| n.kind() == "static")
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
                    .map(native_operand_children)
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
        if self.facts.receiver_convention == ReceiverConvention::None {
            let parent = function.parent();
            let class_method = match language {
                OperandLanguage::Cpp => {
                    parent.is_some_and(|n| n.kind() == "field_declaration_list")
                }
                OperandLanguage::Php => function.kind() == "method_declaration",
                OperandLanguage::Kotlin | OperandLanguage::Swift => {
                    parent.is_some_and(|n| n.kind() == "class_body")
                }
                OperandLanguage::Scala => parent
                    .and_then(|n| n.parent())
                    .is_some_and(|n| n.kind() == "class_definition"),
                OperandLanguage::Ruby => parent
                    .and_then(|n| n.parent())
                    .is_some_and(|n| matches!(n.kind(), "class" | "module")),
                OperandLanguage::Dart => function.kind() == "method_declaration",
                OperandLanguage::Lua => function
                    .child_by_field_name("name")
                    .is_some_and(|n| n.kind() == "method_index_expression"),
                _ => false,
            };
            let mut cursor = function.walk();
            let is_static = function.children(&mut cursor).any(|n| {
                n.kind() == "static"
                    || matches!(n.kind(), "static_modifier" | "storage_class_specifier")
                        && n.utf8_text(self.source).ok() == Some("static")
            });
            if class_method && !is_static {
                self.facts.receiver_convention = ReceiverConvention::ImplicitThis;
                let name = if matches!(
                    language,
                    OperandLanguage::Ruby | OperandLanguage::Swift | OperandLanguage::Lua
                ) {
                    "self"
                } else {
                    "this"
                };
                self.formal(name, None, FormalKind::Receiver, None)?;
            }
        }
        if let Some(receiver) = function.child_by_field_name("receiver") {
            self.facts.receiver_convention = ReceiverConvention::Explicit;
            for param in native_operand_children(receiver) {
                self.parameter(param, FormalKind::Receiver)?;
            }
        }
        if language == OperandLanguage::Bash {
            let mut positions = std::collections::BTreeSet::new();
            let mut stack = vec![function];
            while let Some(node) = stack.pop() {
                if node.id() != function.id() && self.is_callable_boundary(node) {
                    continue;
                }
                if node.kind() == "simple_expansion"
                    && let Some(name) = node.named_child(0)
                    && let Ok(text) = name.utf8_text(self.source)
                    && let Ok(position) = text.parse::<u32>()
                    && position > 0
                {
                    positions.insert((position, text));
                }
                let mut cursor = node.walk();
                stack.extend(node.named_children(&mut cursor));
            }
            for (position, text) in positions {
                self.formal(text, None, FormalKind::Positional(position - 1), None)?;
            }
            // Shell results are exit statuses and streams, not ordinary return values.
            self.diagnostic(function, OperandDiagnosticKind::ReturnConvention)?;
            return Ok(());
        }
        let parameters = function
            .child_by_field_name("parameters")
            .or_else(|| function.child_by_field_name("parameter"))
            .or_else(|| {
                function
                    .child_by_field_name("declarator")
                    .and_then(|n| n.child_by_field_name("parameters"))
            })
            .or_else(|| {
                function
                    .child_by_field_name("signature")
                    .and_then(|n| n.child_by_field_name("parameters"))
            })
            .or_else(|| {
                native_operand_children(function)
                    .into_iter()
                    .find(|n| matches!(n.kind(), "function_value_parameters" | "parameter_list"))
            })
            .or_else(|| {
                if language == OperandLanguage::Elixir {
                    native_operand_children(function)
                        .into_iter()
                        .find(|n| n.kind() == "arguments")
                        .and_then(|n| n.named_child(0))
                        .and_then(|n| {
                            native_operand_children(n)
                                .into_iter()
                                .find(|n| n.kind() == "arguments")
                        })
                } else {
                    None
                }
            });
        let parameters = parameters.or_else(|| {
            function
                .child_by_field_name("signature")
                .and_then(|n| {
                    native_operand_children(n)
                        .into_iter()
                        .find(|n| n.kind() == "function_signature")
                })
                .and_then(|n| n.child_by_field_name("parameters"))
        });
        let direct_parameters;
        let nodes = if language == OperandLanguage::Swift {
            direct_parameters = native_operand_children(function)
                .into_iter()
                .filter(|n| n.kind() == "parameter")
                .collect::<smallvec::SmallVec<_>>();
            direct_parameters
        } else if let Some(parameters) = parameters {
            if parameters.kind() == "identifier" {
                smallvec::smallvec![parameters]
            } else {
                native_operand_children(parameters)
            }
        } else {
            return Ok(());
        };
        let mut position = 0u32;
        let mut keyword_only = false;
        let mut positional_start = self.facts.formals.len();
        for param in nodes {
            // Python wraps annotated *args/**kwargs in a typed_parameter.
            // Bind its splat pattern; the annotation is not a formal value.
            let param = if language == OperandLanguage::Python && param.kind() == "typed_parameter"
            {
                param
                    .named_child(0)
                    .filter(|n| {
                        matches!(n.kind(), "list_splat_pattern" | "dictionary_splat_pattern")
                    })
                    .unwrap_or(param)
            } else {
                param
            };
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
                    .checked_add(checked_operand_index(
                        (self.facts.formals.len() - before).max(1),
                    )?)
                    .ok_or_else(|| Error::SerdeError("formal index overflow".into()))?;
            }
            if language == OperandLanguage::Python && rest {
                keyword_only = true;
            }
        }
        if language == OperandLanguage::Go
            && function.child_by_field_name("result").is_some_and(|n| {
                native_operand_children(n)
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
        let names: smallvec::SmallVec<[Node<'_>; 2]> = param
            .children_by_field_name("name", &mut cursor)
            .filter(|n| native_name(n.kind()))
            .collect();
        let names = if !names.is_empty() {
            names
        } else {
            let pattern = param
                .child_by_field_name("pattern")
                .or_else(|| param.child_by_field_name("declarator"))
                .or_else(|| param.child_by_field_name("left"))
                .or_else(|| {
                    if native_name(param.kind()) {
                        Some(param)
                    } else {
                        None
                    }
                })
                .or_else(|| {
                    native_operand_children(param).into_iter().find(|n| {
                        matches!(
                            n.kind(),
                            "identifier"
                                | "simple_identifier"
                                | "variable_name"
                                | "variable"
                                | "self"
                                | "rest_pattern"
                                | "variable_declarator"
                        )
                    })
                });
            match pattern {
                Some(n) if matches!(n.kind(), "rest_pattern" | "variable_declarator") => n
                    .child_by_field_name("name")
                    .or_else(|| n.named_child(0))
                    .into_iter()
                    .collect(),
                Some(n) => smallvec::smallvec![n],
                None => smallvec::smallvec![],
            }
        };
        if names.is_empty() {
            self.diagnostic(param, OperandDiagnosticKind::ParameterPattern)?;
        }
        let default = param
            .child_by_field_name("value")
            .or_else(|| param.child_by_field_name("right"));
        for (offset, name) in names.into_iter().enumerate() {
            if !native_name(name.kind()) {
                self.diagnostic(name, OperandDiagnosticKind::ParameterPattern)?;
                continue;
            }
            let kind = match kind {
                FormalKind::Positional(position) => FormalKind::Positional(
                    position
                        .checked_add(checked_operand_index(offset)?)
                        .ok_or_else(|| Error::SerdeError("formal index overflow".into()))?,
                ),
                other => other,
            };
            let text = name
                .utf8_text(self.source)
                .unwrap_or("")
                .trim_start_matches('$');
            self.formal(text, Some(name), kind, default)?;
            // TypeScript constructor parameter properties perform a native
            // instance-member store even though the body contains no assignment.
            if self.facts.language == OperandLanguage::TypeScript
                && param
                    .children(&mut param.walk())
                    .any(|n| matches!(n.kind(), "accessibility_modifier" | "readonly"))
                && let Some(receiver) = self
                    .facts
                    .formals
                    .iter()
                    .find(|f| f.kind == FormalKind::Receiver)
                    .map(|f| f.value)
                && let Some(formal) = self.facts.formals.last()
            {
                self.facts.field_writes.push(FieldWriteFact {
                    receiver,
                    member: formal.name,
                    value: formal.value,
                });
            }
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
        let id = ExpressionId(checked_operand_index(self.facts.expressions.len())?);
        self.expressions.insert(node.id(), id);
        self.facts.expressions.push(ExpressionFact {
            span: Some(native_operand_span(node)?),
            kind: ExpressionKind::Unknown,
            inputs: OperandRange::default(),
        });
        if depth >= EXPRESSION_DEPTH_LIMIT {
            self.diagnostic(node, OperandDiagnosticKind::DepthLimit)?;
            return Ok(id);
        }
        let kind = node.kind();
        let mut inputs = smallvec::SmallVec::<[ExpressionId; 4]>::new();
        let expression_kind = if self.is_callable_boundary(node) {
            self.diagnostic(node, OperandDiagnosticKind::NestedCallable)?;
            ExpressionKind::Unknown
        } else if matches!(
            kind,
            "invocation_expression"
                | "function_call_expression"
                | "member_call_expression"
                | "scoped_call_expression"
                | "function_call"
                | "command"
                | "call_expression"
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
                | "this_expression"
                | "self_expression"
                | "simple_identifier"
                | "name"
                | "variable_name"
                | "variable"
                | "word"
        ) {
            ExpressionKind::Name(self.node_name(node)?)
        } else if let Some(base) = element_base(node) {
            inputs.push(self.expression(base, depth + 1)?);
            for child in native_operand_children(node)
                .into_iter()
                .filter(|n| n.id() != base.id())
            {
                inputs.push(self.expression(child, depth + 1)?);
            }
            ExpressionKind::Element
        } else if matches!(kind, "array" | "list" | "tuple" | "dictionary" | "set") {
            for child in native_operand_children(node) {
                let value = if matches!(child.kind(), "pair" | "key_value_pair") {
                    child.child_by_field_name("value")
                } else {
                    Some(child)
                };
                if let Some(value) = value {
                    inputs.push(self.expression(value, depth + 1)?);
                }
            }
            ExpressionKind::Collection
        } else if let Some((receiver, member)) = member_parts(node) {
            inputs.push(self.expression(receiver, depth + 1)?);
            ExpressionKind::Member(self.node_name(member)?)
        } else if let Some((condition, consequence, alternative)) =
            conditional_parts(node, self.facts.language)
        {
            let condition = self.expression(condition, depth + 1)?;
            inputs.push(self.expression(consequence, depth + 1)?);
            inputs.push(self.expression(alternative, depth + 1)?);
            ExpressionKind::Conditional { condition }
        } else if let Some(value) = asserted_value(node) {
            inputs.push(self.expression(value, depth + 1)?);
            ExpressionKind::AssertedValue
        } else if matches!(kind, "composite_literal" | "struct_expression") {
            let type_node = node
                .child_by_field_name("type")
                .or_else(|| node.child_by_field_name("name"));
            if let Some(type_node) = type_node {
                let type_value = self.expression(type_node, depth + 1)?;
                inputs.push(type_value);
                for child in native_operand_children(node) {
                    if child.id() != type_node.id() {
                        inputs.push(self.expression(child, depth + 1)?);
                    }
                }
                if self.facts.language == OperandLanguage::Go
                    && matches!(
                        type_node.kind(),
                        "type_identifier" | "qualified_type" | "generic_type" | "struct_type"
                    )
                    && let Some(body) = node.child_by_field_name("body")
                {
                    for field in native_operand_children(body)
                        .into_iter()
                        .filter(|n| n.kind() == "keyed_element")
                    {
                        let key = field
                            .child_by_field_name("key")
                            .and_then(|n| n.named_child(0));
                        let value = field
                            .child_by_field_name("value")
                            .and_then(|n| n.named_child(0));
                        if let (Some(key), Some(value)) = (key, value)
                            && matches!(key.kind(), "identifier" | "field_identifier")
                        {
                            let member = self.node_name(key)?;
                            let value = self.expression(value, depth + 1)?;
                            self.facts.field_writes.push(FieldWriteFact {
                                receiver: id,
                                member,
                                value,
                            });
                        }
                    }
                }
                if self.facts.language == OperandLanguage::Go
                    && matches!(
                        type_node.kind(),
                        "array_type" | "slice_type" | "implicit_length_array_type" | "map_type"
                    )
                {
                    inputs.clear();
                    if let Some(body) = node.child_by_field_name("body") {
                        for element in native_operand_children(body) {
                            let value = if element.kind() == "keyed_element" {
                                element.child_by_field_name("value")
                            } else {
                                Some(element)
                            };
                            if let Some(value) = value.and_then(|n| n.named_child(0)) {
                                inputs.push(self.expression(value, depth + 1)?);
                            }
                        }
                    }
                    ExpressionKind::Collection
                } else {
                    ExpressionKind::Construction(type_value)
                }
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
        ) && !native_operand_children(node).iter().any(|n| {
            n.kind().contains("interpolation")
                || matches!(
                    n.kind(),
                    "template_substitution" | "simple_expansion" | "expansion"
                )
        }) {
            ExpressionKind::Literal
        } else {
            for child in native_operand_children(node) {
                inputs.push(self.expression(child, depth + 1)?);
            }
            if kind == "parenthesized_expression"
                || (kind == "simple_expansion")
                || (kind == "string"
                    && native_operand_children(node).len() == 1
                    && native_operand_children(node)[0].kind() == "simple_expansion")
            {
                ExpressionKind::Parenthesized
            } else {
                ExpressionKind::Computation
            }
        };
        let inputs = self.inputs(&inputs)?;
        self.facts.expressions[id.0 as usize] = ExpressionFact {
            span: Some(native_operand_span(node)?),
            kind: expression_kind,
            inputs,
        };
        Ok(id)
    }

    fn call(&mut self, node: Node<'_>, result: ExpressionId, depth: usize) -> Result<CallSiteId> {
        let id = CallSiteId(checked_operand_index(self.facts.calls.len())?);
        let kind = if matches!(node.kind(), "new_expression" | "object_creation_expression") {
            InvocationKind::Construction
        } else {
            InvocationKind::Call
        };
        self.facts.calls.push(CallSiteFact {
            span: native_operand_span(node)?,
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
            .or_else(|| node.child_by_field_name("type"))
            .or_else(|| node.child_by_field_name("method"))
            .or_else(|| node.child_by_field_name("target"))
            .or_else(|| node.named_child(0));
        let callee_node = callee_node.map(|n| {
            if n.kind() == "command_name" {
                n.named_child(0).unwrap_or(n)
            } else {
                n
            }
        });
        let receiver_node = node
            .child_by_field_name("object")
            .or_else(|| node.child_by_field_name("receiver"))
            .or_else(|| {
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
        let mut arguments = smallvec::SmallVec::<[ArgumentFact; 4]>::new();
        let list = node
            .child_by_field_name("arguments")
            .or_else(|| {
                native_operand_children(node)
                    .into_iter()
                    .find(|n| matches!(n.kind(), "arguments" | "value_arguments"))
            })
            .or_else(|| {
                native_operand_children(node)
                    .into_iter()
                    .find(|n| n.kind() == "call_suffix")
                    .and_then(|n| {
                        native_operand_children(n)
                            .into_iter()
                            .find(|n| n.kind() == "value_arguments")
                    })
            });
        let actuals = if node.kind() == "command" {
            let mut cursor = node.walk();
            node.children_by_field_name("argument", &mut cursor)
                .collect()
        } else if self.facts.language == OperandLanguage::Puppet && node.kind() == "function_call" {
            native_operand_children(node).into_iter().skip(1).collect()
        } else {
            list.map(native_operand_children).unwrap_or_default()
        };
        {
            for argument in actuals {
                let argument = if matches!(argument.kind(), "argument" | "value_argument")
                    && argument.named_child_count() == 1
                {
                    argument.named_child(0).unwrap_or(argument)
                } else {
                    argument
                };
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
            start: checked_operand_index(self.facts.arguments.len())?,
            len: checked_operand_index(arguments.len())?,
        };
        self.facts.arguments.extend(arguments);
        self.facts.calls[id.0 as usize] = CallSiteFact {
            span: native_operand_span(node)?,
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
            smallvec::smallvec![node]
        } else {
            let nodes = native_operand_children(node);
            if nodes.len() == 1
                && nodes
                    .first()
                    .is_some_and(|n| matches!(n.kind(), "expression_list" | "argument_list"))
            {
                native_operand_children(nodes[0])
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
            span: native_operand_span(node)?,
            values,
            implicit,
        });
        Ok(())
    }
}

fn member_parts(node: Node<'_>) -> Option<(Node<'_>, Node<'_>)> {
    if !matches!(
        node.kind(),
        "member_expression"
            | "attribute"
            | "selector_expression"
            | "field_expression"
            | "member_access_expression"
            | "navigation_expression"
            | "method_index_expression"
            | "dot_index_expression"
    ) {
        return None;
    }
    let object = node
        .child_by_field_name("object")
        .or_else(|| node.child_by_field_name("operand"))
        .or_else(|| node.child_by_field_name("value"))
        .or_else(|| node.child_by_field_name("argument"))
        .or_else(|| node.child_by_field_name("expression"))
        .or_else(|| node.child_by_field_name("target"))
        .or_else(|| node.child_by_field_name("table"))
        .or_else(|| {
            let mut cursor = node.walk();
            node.children(&mut cursor)
                .find(|n| matches!(n.kind(), "this" | "self"))
        })
        .or_else(|| {
            if node.kind() == "navigation_expression" {
                node.named_child(0)
            } else {
                None
            }
        })?;
    let member = node
        .child_by_field_name("property")
        .or_else(|| node.child_by_field_name("attribute"))
        .or_else(|| node.child_by_field_name("field"))
        .or_else(|| node.child_by_field_name("name"))
        .or_else(|| node.child_by_field_name("method"))
        .or_else(|| {
            node.child_by_field_name("suffix")
                .and_then(|n| n.child_by_field_name("suffix").or(Some(n)))
        })
        .or_else(|| {
            if node.kind() == "navigation_expression" {
                node.named_child(1)
            } else {
                None
            }
        })?;
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
        "declaration" | "declaration_command" => native_operand_children(root)
            .into_iter()
            .filter(|n| matches!(n.kind(), "init_declarator" | "variable_assignment"))
            .map(|n| {
                (
                    n.child_by_field_name("declarator")
                        .or_else(|| n.child_by_field_name("name"))
                        .unwrap_or(n),
                    n.child_by_field_name("value"),
                )
            })
            .collect(),
        "property_declaration" | "val_definition" | "var_definition" => {
            let target = root
                .child_by_field_name("pattern")
                .or_else(|| root.child_by_field_name("name"))
                .or_else(|| {
                    native_operand_children(root)
                        .into_iter()
                        .find(|n| n.kind() == "variable_declaration")
                        .and_then(|n| n.named_child(0))
                });
            target
                .map(|n| {
                    vec![(
                        n.child_by_field_name("bound_identifier").unwrap_or(n),
                        root.child_by_field_name("value")
                            .or_else(|| initializer_after_equals(root)),
                    )]
                })
                .unwrap_or_default()
        }
        "let_declaration" | "let_statement" => root
            .child_by_field_name("pattern")
            .map(|target| vec![(target, root.child_by_field_name("value").map(scalar))])
            .unwrap_or_default(),
        "var_declaration" | "var_spec_list" => native_operand_children(root)
            .into_iter()
            .flat_map(local_write_sources)
            .collect(),
        "variable_declaration"
            if native_operand_children(root)
                .iter()
                .any(|n| n.kind() == "assignment_statement") =>
        {
            native_operand_children(root)
                .into_iter()
                .flat_map(local_write_sources)
                .collect()
        }
        "variable_declaration"
        | "lexical_declaration"
        | "local_variable_declaration"
        | "local_declaration_statement" => native_operand_children(root)
            .into_iter()
            .flat_map(local_write_sources)
            .collect(),
        "variable_declarator" | "initialized_variable_definition" | "var_spec" => root
            .child_by_field_name("name")
            .map(|target| {
                vec![(
                    target,
                    root.child_by_field_name("value")
                        .or_else(|| initializer_after_equals(root))
                        .map(scalar),
                )]
            })
            .unwrap_or_default(),
        "type_switch_statement" => root
            .child_by_field_name("alias")
            .and_then(|n| n.named_child(0))
            .map(|alias| vec![(alias, root.child_by_field_name("value"))])
            .unwrap_or_default(),
        "assignment"
        | "assignment_expression"
        | "assignment_statement"
        | "binary_operator"
        | "short_var_declaration" => {
            let Some(left) = root
                .child_by_field_name("left")
                .or_else(|| {
                    native_operand_children(root)
                        .into_iter()
                        .find(|n| n.kind() == "variable_list")
                })
                .or_else(|| {
                    if root.kind() == "assignment" {
                        root.named_child(0)
                    } else {
                        None
                    }
                })
            else {
                return vec![];
            };
            let Some(right) = root
                .child_by_field_name("right")
                .or_else(|| {
                    native_operand_children(root)
                        .into_iter()
                        .find(|n| n.kind() == "expression_list")
                })
                .or_else(|| {
                    if root.kind() == "assignment" {
                        root.named_child(1)
                    } else {
                        None
                    }
                })
            else {
                return vec![(left, None)];
            };
            let lefts = if matches!(left.kind(), "expression_list" | "variable_list") {
                native_operand_children(left)
            } else {
                smallvec::smallvec![left]
            };
            let rights = if right.kind() == "expression_list" {
                native_operand_children(right)
            } else {
                smallvec::smallvec![right]
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

// Only selected values flow through a conditional. Its condition remains a
// separate native operand, preserving control/data distinctions for consumers.
fn conditional_parts(
    node: Node<'_>,
    language: OperandLanguage,
) -> Option<(Node<'_>, Node<'_>, Node<'_>)> {
    if matches!(
        node.kind(),
        "ternary_expression" | "conditional_expression" | "conditional" | "if_expression"
    ) {
        if let (Some(condition), Some(yes), Some(no)) = (
            node.child_by_field_name("condition"),
            node.child_by_field_name("consequence")
                .or_else(|| node.child_by_field_name("body")),
            node.child_by_field_name("alternative"),
        ) {
            return Some((condition, branch_value(yes)?, branch_value(no)?));
        }
        if node.named_child_count() == 3 {
            return if language == OperandLanguage::Python {
                Some((
                    node.named_child(1)?,
                    node.named_child(0)?,
                    node.named_child(2)?,
                ))
            } else {
                Some((
                    node.named_child(0)?,
                    branch_value(node.named_child(1)?)?,
                    branch_value(node.named_child(2)?)?,
                ))
            };
        }
    }
    None
}

fn branch_value(mut node: Node<'_>) -> Option<Node<'_>> {
    for _ in 0..EXPRESSION_DEPTH_LIMIT {
        if !matches!(
            node.kind(),
            "block" | "statements" | "else_clause" | "else" | "control_structure_body"
        ) {
            return Some(node);
        }
        node = native_operand_children(node).last().copied()?;
    }
    None
}

fn asserted_value(node: Node<'_>) -> Option<Node<'_>> {
    match node.kind() {
        "cast_expression" => node.child_by_field_name("value"),
        "type_assertion_expression" => node.child_by_field_name("operand"),
        "as_expression" | "non_null_expression" => node.named_child(0),
        "type_assertion" => node.named_child(node.named_child_count().checked_sub(1)?),
        _ => None,
    }
}

fn native_name(kind: &str) -> bool {
    matches!(
        kind,
        "identifier" | "simple_identifier" | "variable_name" | "variable" | "self" | "word"
    )
}

// Some grammars (Kotlin/C#) leave initializers unnamed. Only an actual native
// `=` token qualifies a final child as an initializer; type/pattern children do not.
fn initializer_after_equals(node: Node<'_>) -> Option<Node<'_>> {
    let mut cursor = node.walk();
    if node.children(&mut cursor).any(|n| n.kind() == "=") {
        node.named_child(node.named_child_count().checked_sub(1)?)
    } else {
        None
    }
}

fn element_base(node: Node<'_>) -> Option<Node<'_>> {
    if !matches!(
        node.kind(),
        "subscript_expression" | "subscript" | "index_expression" | "array_access"
    ) {
        return None;
    }
    node.child_by_field_name("object")
        .or_else(|| node.child_by_field_name("value"))
        .or_else(|| node.child_by_field_name("operand"))
        .or_else(|| node.child_by_field_name("array"))
}
