//! Java-specific syntax rules for the shared callable-identity pipeline.
//! Signatures remain source identities, not resolved JVM descriptors.
use rgctl_plugin_api::{Error, Result, Symbol};
use tree_sitter::Node;

fn enclosing_type(node: Node<'_>) -> Option<Node<'_>> {
    let mut current = node;
    while let Some(parent) = current.parent() {
        if matches!(
            parent.kind(),
            "class_declaration"
                | "interface_declaration"
                | "record_declaration"
                | "enum_declaration"
                | "annotation_type_declaration"
        ) {
            return Some(parent);
        }
        current = parent;
    }
    None
}

fn append_type_tokens(node: Node, source: &[u8], output: &mut String) -> Result<()> {
    let mut stack = Vec::with_capacity(16);
    stack.push(node);
    while let Some(token) = stack.pop() {
        if matches!(
            token.kind(),
            "annotation" | "marker_annotation" | "line_comment" | "block_comment"
        ) {
            continue;
        }
        if token.child_count() == 0 {
            output.push_str(token.utf8_text(source)?);
        } else {
            for index in (0..token.child_count()).rev() {
                if let Some(child) = token.child(index) {
                    stack.push(child);
                }
            }
        }
    }
    Ok(())
}

pub(super) fn discriminator(node: Node, source: &[u8]) -> Result<String> {
    let owner = if node.kind() == "compact_constructor_declaration" {
        enclosing_type(node).unwrap_or(node)
    } else {
        node
    };
    let Some(parameters) = owner.child_by_field_name("parameters") else {
        return Ok("()".into());
    };
    let mut signature = String::from("(");
    let mut cursor = parameters.walk();
    let mut first = true;
    for parameter in parameters.named_children(&mut cursor) {
        if !matches!(parameter.kind(), "formal_parameter" | "spread_parameter") {
            continue;
        }
        if !first {
            signature.push(',');
        }
        first = false;
        let mut pcursor = parameter.walk();
        let ty = parameter
            .child_by_field_name("type")
            .or_else(|| {
                parameter.named_children(&mut pcursor).find(|child| {
                    !matches!(
                        child.kind(),
                        "modifiers" | "variable_declarator" | "annotation" | "marker_annotation"
                    )
                })
            })
            .ok_or_else(|| Error::ParseError {
                file: "<java>".into(),
                line: parameter.start_position().row + 1,
                message: "Callable parameter has no type".into(),
            })?;
        append_type_tokens(ty, source, &mut signature)?;
        if let Some(dimensions) = parameter.child_by_field_name("dimensions") {
            append_type_tokens(dimensions, source, &mut signature)?;
        }
        if parameter.kind() == "spread_parameter" {
            signature.push_str("[]");
        }
    }
    signature.push(')');
    Ok(signature)
}

pub fn enclosing_discriminator(node: Node, source: &[u8]) -> Result<Option<String>> {
    let mut current = node;
    while let Some(parent) = current.parent() {
        if matches!(
            parent.kind(),
            "method_declaration" | "constructor_declaration" | "compact_constructor_declaration"
        ) {
            return discriminator(parent, source).map(Some);
        }
        current = parent;
    }
    Ok(None)
}

pub(super) fn argument_identity(
    call: Node,
    source: &[u8],
    from: &Symbol,
    mut local_type: impl FnMut(&str) -> Option<String>,
) -> Option<String> {
    let arguments = call.child_by_field_name("arguments")?;
    let mut cursor = arguments.walk();
    let mut types = Vec::new();
    for argument in arguments.named_children(&mut cursor) {
        let text = argument.utf8_text(source).ok()?;
        let ty = match argument.kind() {
            "line_comment" | "block_comment" => continue,
            "string_literal" => "String".to_string(),
            "character_literal" => "char".to_string(),
            "true" | "false" => "boolean".to_string(),
            "decimal_integer_literal"
            | "hex_integer_literal"
            | "octal_integer_literal"
            | "binary_integer_literal" => if text.ends_with(['l', 'L']) {
                "long"
            } else {
                "int"
            }
            .to_string(),
            "decimal_floating_point_literal" | "hex_floating_point_literal" => {
                if text.ends_with(['f', 'F']) {
                    "float"
                } else {
                    "double"
                }
                .to_string()
            }
            "identifier" => from
                .parameters
                .iter()
                .find(|p| p.name == text)
                .and_then(|p| p.param_type.clone())
                .or_else(|| local_type(text))?,
            "object_creation_expression" => argument
                .child_by_field_name("type")?
                .utf8_text(source)
                .ok()?
                .to_string(),
            _ => return None,
        };
        types.push(
            ty.chars()
                .filter(|c| !c.is_whitespace())
                .collect::<String>(),
        );
    }
    Some(format!("({})", types.join(",")))
}
