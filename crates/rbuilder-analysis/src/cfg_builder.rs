//! CFG construction from tree-sitter ASTs.

use crate::cfg::{BasicBlock, BlockId, CfgEdgeType, ControlFlowGraph, Statement, StatementKind};
use rbuilder_error::{Error, Result};
use rbuilder_plugin_helpers::extract_name_from_node;
use tree_sitter::{Node, Parser, Tree};
use uuid::Uuid;

/// Build a CFG for a named function in source text.
pub fn build_cfg_for_function(
    language: &str,
    source: &str,
    function_name: &str,
) -> Result<ControlFlowGraph> {
    let bytes = source.as_bytes();
    let tree = parse_language(language, bytes)?;
    let func_node = find_function_by_name(tree.root_node(), bytes, function_name)
        .ok_or_else(|| Error::NotFound(format!("function '{function_name}' not found")))?;
    build_cfg_from_function_node(language, func_node, bytes)
}

fn parse_language(language: &str, source: &[u8]) -> Result<Tree> {
    let mut parser = Parser::new();
    match language.to_lowercase().as_str() {
        "rust" | "rs" => {
            parser
                .set_language(&tree_sitter_rust::LANGUAGE.into())
                .map_err(|e| Error::PluginError(format!("Rust grammar: {e}")))?;
        }
        "python" | "py" => {
            parser
                .set_language(&tree_sitter_python::LANGUAGE.into())
                .map_err(|e| Error::PluginError(format!("Python grammar: {e}")))?;
        }
        other => return Err(Error::UnsupportedLanguage(other.to_string())),
    }

    parser.parse(source, None).ok_or_else(|| Error::ParseError {
        file: "source".into(),
        line: 0,
        message: "Failed to parse source".to_string(),
    })
}

fn find_function_by_name<'a>(node: Node<'a>, source: &[u8], name: &str) -> Option<Node<'a>> {
    let function_kinds = ["function_item", "function_definition"];
    if function_kinds.contains(&node.kind()) {
        if let Ok(Some(func_name)) = extract_name_from_node(node, source) {
            if func_name == name {
                return Some(node);
            }
        }
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if let Some(found) = find_function_by_name(child, source, name) {
            return Some(found);
        }
    }
    None
}

fn build_cfg_from_function_node(
    language: &str,
    func_node: Node,
    source: &[u8],
) -> Result<ControlFlowGraph> {
    let mut cfg = ControlFlowGraph::new();
    let mut builder = CfgBuilder::new(&mut cfg, language);
    builder.build_function(func_node, source)?;
    Ok(cfg)
}

struct CfgBuilder<'a> {
    cfg: &'a mut ControlFlowGraph,
    current_block: BlockId,
    language: &'a str,
    loop_stack: Vec<LoopContext>,
}

struct LoopContext {
    header: BlockId,
    exit: BlockId,
}

impl<'a> CfgBuilder<'a> {
    fn new(cfg: &'a mut ControlFlowGraph, language: &'a str) -> Self {
        let entry = cfg.entry;
        Self {
            cfg,
            current_block: entry,
            language,
            loop_stack: Vec::new(),
        }
    }

    fn build_function(&mut self, func_node: Node, source: &[u8]) -> Result<()> {
        let body =
            function_body_node(func_node, self.language).ok_or_else(|| Error::ParseError {
                file: "source".into(),
                line: func_node.start_position().row + 1,
                message: "Function has no body".to_string(),
            })?;
        self.visit_block(body, source)?;
        if self.cfg.exits.is_empty() {
            let exit = self.new_block();
            self.cfg
                .add_edge(self.current_block, exit, CfgEdgeType::Next);
            self.cfg.exits.push(exit);
        }
        Ok(())
    }

    fn new_block(&mut self) -> BlockId {
        let id = Uuid::new_v4();
        self.cfg.add_block(BasicBlock {
            id,
            statements: Vec::new(),
            start_line: 0,
            end_line: 0,
        });
        id
    }

    fn add_statement(&mut self, node: Node, source: &[u8], kind: StatementKind) -> Result<()> {
        let line = node.start_position().row + 1;
        let text = node.utf8_text(source)?.trim().to_string();
        let stmt = Statement { kind, line, text };
        self.add_statement_to_current(stmt);
        Ok(())
    }

    fn add_statement_to_current(&mut self, stmt: Statement) {
        let block = self
            .cfg
            .blocks
            .get_mut(&self.current_block)
            .expect("current block");
        if block.start_line == 0 || stmt.line < block.start_line {
            block.start_line = stmt.line;
        }
        if stmt.line > block.end_line {
            block.end_line = stmt.line;
        }
        block.statements.push(stmt);
    }

    fn visit_block(&mut self, node: Node, source: &[u8]) -> Result<BlockId> {
        if is_block_like(node.kind()) {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if child.is_named() {
                    self.visit_statement(child, source)?;
                }
            }
        } else {
            self.visit_statement(node, source)?;
        }
        Ok(self.current_block)
    }

    fn visit_statement(&mut self, node: Node, source: &[u8]) -> Result<()> {
        match node.kind() {
            // Rust + Python conditionals
            "if_statement" | "if_expression" => self.visit_if(node, source),
            "while_statement" | "while_expression" => self.visit_while(node, source),
            "for_statement" | "for_expression" | "for_in_expression" => {
                self.visit_for(node, source)
            }
            "loop_expression" => self.visit_loop(node, source),

            // Returns
            "return_statement" | "return_expression" => self.visit_return(node, source),

            // Jumps
            "break_expression" | "break_statement" => self.visit_break(node, source),
            "continue_expression" | "continue_statement" => self.visit_continue(node, source),

            // Declarations / assignments
            "let_declaration" | "let_statement" => {
                self.add_statement(node, source, StatementKind::Declaration)?;
                Ok(())
            }
            "assignment"
            | "assignment_expression"
            | "augmented_assignment"
            | "augmented_assignment_expression"
            | "compound_assignment_expr" => {
                self.add_statement(node, source, StatementKind::Assignment)?;
                Ok(())
            }

            // Expression statement
            "expression_statement" => self.visit_expression_stmt(node, source),

            // Rust match
            "match_expression" => self.visit_match(node, source),

            // Python try/except (simplified)
            "try_statement" => self.visit_try(node, source),

            // Block / body wrapper
            k if is_block_like(k) => {
                self.visit_block(node, source)?;
                Ok(())
            }

            // Default: treat as expression
            _ if node.is_named() => {
                let kind = Self::classify_expression(node, source);
                self.add_statement(node, source, kind)?;
                Ok(())
            }
            _ => Ok(()),
        }
    }

    fn visit_expression_stmt(&mut self, node: Node, source: &[u8]) -> Result<()> {
        let inner = node.named_child(0).unwrap_or(node);
        match inner.kind() {
            "if_statement" | "if_expression" | "while_statement" | "while_expression"
            | "for_statement" | "for_expression" | "for_in_expression" | "loop_expression"
            | "match_expression" | "return_statement" | "return_expression" => {
                self.visit_statement(inner, source)
            }
            _ => {
                let kind = Self::classify_expression(inner, source);
                self.add_statement(inner, source, kind)
            }
        }
    }

    fn classify_expression(node: Node, _source: &[u8]) -> StatementKind {
        match node.kind() {
            "call_expression" | "function_call" | "method_call" => StatementKind::FunctionCall,
            "assignment_expression" | "assignment" | "augmented_assignment" => {
                StatementKind::Assignment
            }
            "let_declaration" | "let_statement" => StatementKind::Declaration,
            "return_expression" | "return_statement" => StatementKind::Return,
            "if_expression" | "if_statement" => StatementKind::Branch,
            _ => StatementKind::Expression,
        }
    }

    fn visit_if(&mut self, node: Node, source: &[u8]) -> Result<()> {
        self.add_statement(node, source, StatementKind::Branch)?;
        let cond_block = self.current_block;

        let true_block = self.new_block();
        self.cfg
            .add_edge(cond_block, true_block, CfgEdgeType::IfTrue);

        let false_block = self.new_block();
        self.cfg
            .add_edge(cond_block, false_block, CfgEdgeType::IfFalse);

        self.current_block = true_block;
        if let Some(consequence) = node.child_by_field_name("consequence") {
            self.visit_block(consequence, source)?;
        }
        let true_end = self.current_block;

        self.current_block = false_block;
        if let Some(alternative) = node.child_by_field_name("alternative") {
            self.visit_block(alternative, source)?;
        } else if let Some(else_clause) = node.child_by_field_name("else_clause") {
            self.visit_block(else_clause, source)?;
        }
        let false_end = self.current_block;

        let merge = self.new_block();
        self.cfg.add_edge(true_end, merge, CfgEdgeType::Next);
        self.cfg.add_edge(false_end, merge, CfgEdgeType::Next);
        self.current_block = merge;
        Ok(())
    }

    fn visit_while(&mut self, node: Node, source: &[u8]) -> Result<()> {
        let header = self.new_block();
        self.cfg
            .add_edge(self.current_block, header, CfgEdgeType::Next);
        self.add_statement_to_current(Statement {
            kind: StatementKind::Branch,
            line: node.start_position().row + 1,
            text: node
                .child_by_field_name("condition")
                .and_then(|c| c.utf8_text(source).ok())
                .unwrap_or("while")
                .trim()
                .to_string(),
        });

        let body = self.new_block();
        self.cfg.add_edge(header, body, CfgEdgeType::IfTrue);
        let exit = self.new_block();
        self.cfg.add_edge(header, exit, CfgEdgeType::IfFalse);

        self.loop_stack.push(LoopContext { header, exit });

        self.current_block = body;
        if let Some(body_node) = node.child_by_field_name("body") {
            self.visit_block(body_node, source)?;
        }
        self.cfg
            .add_edge(self.current_block, header, CfgEdgeType::Jump);
        self.loop_stack.pop();

        self.current_block = exit;
        Ok(())
    }

    fn visit_for(&mut self, node: Node, source: &[u8]) -> Result<()> {
        let header = self.new_block();
        self.cfg
            .add_edge(self.current_block, header, CfgEdgeType::Next);
        self.add_statement_to_current(Statement {
            kind: StatementKind::Branch,
            line: node.start_position().row + 1,
            text: "for".to_string(),
        });

        let body = self.new_block();
        self.cfg.add_edge(header, body, CfgEdgeType::IfTrue);
        let exit = self.new_block();
        self.cfg.add_edge(header, exit, CfgEdgeType::IfFalse);

        self.loop_stack.push(LoopContext { header, exit });

        self.current_block = body;
        if let Some(body_node) = node.child_by_field_name("body") {
            self.visit_block(body_node, source)?;
        }
        self.cfg
            .add_edge(self.current_block, header, CfgEdgeType::Jump);
        self.loop_stack.pop();

        self.current_block = exit;
        Ok(())
    }

    fn visit_loop(&mut self, node: Node, source: &[u8]) -> Result<()> {
        let header = self.new_block();
        self.cfg
            .add_edge(self.current_block, header, CfgEdgeType::Next);
        let body = self.new_block();
        self.cfg.add_edge(header, body, CfgEdgeType::IfTrue);
        let exit = self.new_block();
        self.cfg.add_edge(header, exit, CfgEdgeType::IfFalse);

        self.loop_stack.push(LoopContext { header, exit });

        self.current_block = body;
        if let Some(body_node) = node.child_by_field_name("body") {
            self.visit_block(body_node, source)?;
        }
        self.cfg
            .add_edge(self.current_block, header, CfgEdgeType::Jump);
        self.loop_stack.pop();

        self.current_block = exit;
        Ok(())
    }

    fn visit_return(&mut self, node: Node, source: &[u8]) -> Result<()> {
        self.add_statement(node, source, StatementKind::Return)?;
        let exit = self.new_block();
        self.cfg
            .add_edge(self.current_block, exit, CfgEdgeType::Return);
        self.cfg.exits.push(exit);
        self.current_block = self.new_block();
        Ok(())
    }

    fn visit_break(&mut self, node: Node, source: &[u8]) -> Result<()> {
        self.add_statement(node, source, StatementKind::Jump)?;
        if let Some(ctx) = self.loop_stack.last() {
            self.cfg
                .add_edge(self.current_block, ctx.exit, CfgEdgeType::Jump);
        }
        self.current_block = self.new_block();
        Ok(())
    }

    fn visit_continue(&mut self, node: Node, source: &[u8]) -> Result<()> {
        self.add_statement(node, source, StatementKind::Jump)?;
        if let Some(ctx) = self.loop_stack.last() {
            self.cfg
                .add_edge(self.current_block, ctx.header, CfgEdgeType::Jump);
        }
        self.current_block = self.new_block();
        Ok(())
    }

    fn visit_match(&mut self, node: Node, source: &[u8]) -> Result<()> {
        self.add_statement(node, source, StatementKind::Branch)?;
        let cond_block = self.current_block;
        let merge = self.new_block();
        let mut arms = Vec::new();

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "match_arm" || child.kind() == "match_arm_pattern" {
                arms.push(child);
            }
        }

        if arms.is_empty() {
            if let Some(body) = node.child_by_field_name("body") {
                arms.push(body);
            }
        }

        for arm in arms {
            let arm_block = self.new_block();
            self.cfg
                .add_edge(cond_block, arm_block, CfgEdgeType::IfTrue);
            self.current_block = arm_block;
            self.visit_block(arm, source)?;
            self.cfg
                .add_edge(self.current_block, merge, CfgEdgeType::Next);
        }

        let default_block = self.new_block();
        self.cfg
            .add_edge(cond_block, default_block, CfgEdgeType::IfFalse);
        self.cfg.add_edge(default_block, merge, CfgEdgeType::Next);

        self.current_block = merge;
        Ok(())
    }

    fn visit_try(&mut self, node: Node, source: &[u8]) -> Result<()> {
        let try_block = self.new_block();
        self.cfg
            .add_edge(self.current_block, try_block, CfgEdgeType::Next);
        self.current_block = try_block;
        if let Some(body) = node.child_by_field_name("body") {
            self.visit_block(body, source)?;
        }
        let try_end = self.current_block;
        let merge = self.new_block();

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "except_clause" || child.kind() == "except_handler" {
                let handler = self.new_block();
                self.cfg
                    .add_edge(try_block, handler, CfgEdgeType::Exception);
                self.current_block = handler;
                if let Some(block) = child.child_by_field_name("body") {
                    self.visit_block(block, source)?;
                }
                self.cfg
                    .add_edge(self.current_block, merge, CfgEdgeType::Next);
            }
        }

        self.cfg.add_edge(try_end, merge, CfgEdgeType::Next);
        self.current_block = merge;
        Ok(())
    }
}

fn is_block_like(kind: &str) -> bool {
    matches!(
        kind,
        "block"
            | "statement_block"
            | "compound_statement"
            | "source_file"
            | "function_body"
            | "block_expression"
    )
}

fn function_body_node<'a>(func_node: Node<'a>, language: &str) -> Option<Node<'a>> {
    if let Some(body) = func_node.child_by_field_name("body") {
        return Some(body);
    }
    match language.to_lowercase().as_str() {
        "rust" | "rs" => func_node.child_by_field_name("block"),
        _ => {
            let mut cursor = func_node.walk();
            for child in func_node.children(&mut cursor) {
                if is_block_like(child.kind()) {
                    return Some(child);
                }
            }
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cfg::CfgEdgeType;

    #[test]
    fn test_rust_if_else_cfg() {
        let code = r#"
fn example(x: i32) -> i32 {
    if x > 0 {
        return x;
    } else {
        return -x;
    }
}
"#;
        let cfg = build_cfg_for_function("rust", code, "example").unwrap();
        assert!(cfg.blocks.len() >= 4);
        let if_true = cfg
            .edges
            .iter()
            .filter(|e| e.edge_type == CfgEdgeType::IfTrue)
            .count();
        assert!(if_true >= 1);
    }

    #[test]
    fn test_rust_loop_has_cycle() {
        let code = r#"
fn loop_example(n: i32) -> i32 {
    let mut sum = 0;
    for i in 0..n {
        sum += i;
    }
    sum
}
"#;
        let cfg = build_cfg_for_function("rust", code, "loop_example").unwrap();
        assert!(cfg.has_cycle());
    }

    #[test]
    fn test_python_if_cfg() {
        let code = r#"
def example(x):
    if x > 0:
        return x
    else:
        return -x
"#;
        let cfg = build_cfg_for_function("python", code, "example").unwrap();
        assert!(cfg.blocks.len() >= 4);
    }

    #[test]
    fn test_unsupported_language() {
        let result = build_cfg_for_function("brainfuck", "+++", "main");
        assert!(matches!(result, Err(Error::UnsupportedLanguage(_))));
    }

    #[test]
    fn test_function_not_found() {
        let code = "fn other() {}";
        let result = build_cfg_for_function("rust", code, "missing");
        assert!(matches!(result, Err(Error::NotFound(_))));
    }
}
