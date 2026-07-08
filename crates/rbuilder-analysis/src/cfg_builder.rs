//! CFG construction from tree-sitter ASTs.

use crate::cfg::{BasicBlock, BlockId, CfgEdgeType, ControlFlowGraph, Statement, StatementKind};
use crate::def_use::extract_def_use;
use crate::language_profile::{function_kinds_for, parse_source};
use rbuilder_error::{Error, Result};
use rbuilder_plugin_helpers::extract_name_from_node;
use std::collections::HashSet;
use tree_sitter::{Node, Tree};
use uuid::Uuid;

/// Build a CFG for a named function in source text.
pub fn build_cfg_for_function(
    language: &str,
    source: &str,
    function_name: &str,
) -> Result<ControlFlowGraph> {
    let bytes = source.as_bytes();
    let tree = parse_language(language, bytes)?;
    let function_kinds = function_kinds_for(language)?;
    let func_node = find_function_by_name(tree.root_node(), bytes, function_name, function_kinds)
        .ok_or_else(|| Error::NotFound(format!("function '{function_name}' not found")))?;
    build_cfg_from_function_node(language, func_node, bytes)
}

fn parse_language(language: &str, source: &[u8]) -> Result<Tree> {
    parse_source(language, source)
}

fn find_function_by_name<'a>(
    node: Node<'a>,
    source: &[u8],
    name: &str,
    function_kinds: &[&str],
) -> Option<Node<'a>> {
    if function_kinds.contains(&node.kind()) {
        if let Ok(Some(func_name)) = extract_name_from_node(node, source) {
            if func_name == name {
                return Some(node);
            }
        }
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if let Some(found) = find_function_by_name(child, source, name, function_kinds) {
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
    /// False after return/unreachable branch terminates the current path.
    flow_active: bool,
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
            flow_active: true,
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
        if self.flow_active && self.cfg.exits.is_empty() {
            let exit = self.new_block();
            self.cfg
                .add_edge(self.current_block, exit, CfgEdgeType::Next);
            self.cfg.exits.push(exit);
        }
        self.cfg.prune_unreachable_blocks();
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
        let (defined_vars, used_vars) = extract_def_use(node, source);
        let stmt = Statement {
            kind,
            line,
            text,
            defined_vars,
            used_vars,
        };
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
        if !self.flow_active {
            return Ok(());
        }
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
            "short_var_declaration" | "var_declaration" | "const_declaration" => {
                self.add_statement(node, source, StatementKind::Declaration)?;
                Ok(())
            }
            "assignment"
            | "assignment_expression"
            | "assignment_statement"
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

            // Go / shared switch & select
            "switch_statement" | "type_switch_statement" | "expression_switch_statement" => {
                self.visit_switch(node, source)
            }
            "select_statement" => self.visit_select(node, source),

            // Go concurrency helpers (sequential approximation)
            "defer_statement" | "go_statement" => {
                self.add_statement(node, source, StatementKind::Expression)?;
                Ok(())
            }

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

        let cond_text = node
            .child_by_field_name("condition")
            .or_else(|| node.child_by_field_name("operand"))
            .and_then(|c| c.utf8_text(source).ok())
            .unwrap_or("")
            .trim()
            .to_string();

        let true_block = self.new_block();
        self.cfg
            .add_edge(cond_block, true_block, CfgEdgeType::IfTrue);

        let false_block = self.new_block();
        self.cfg
            .add_edge(cond_block, false_block, CfgEdgeType::IfFalse);

        let mut true_end = true_block;
        if !is_constant_false(&cond_text) {
            self.current_block = true_block;
            if let Some(consequence) = node
                .child_by_field_name("consequence")
                .or_else(|| node.child_by_field_name("body"))
            {
                self.visit_block(consequence, source)?;
            }
            true_end = self.current_block;
        }

        let mut false_end = false_block;
        self.current_block = false_block;
        if !is_constant_true(&cond_text) {
            if let Some(alternative) = node
                .child_by_field_name("alternative")
                .or_else(|| node.child_by_field_name("else"))
            {
                self.visit_block(alternative, source)?;
            } else if let Some(else_clause) = node.child_by_field_name("else_clause") {
                self.visit_block(else_clause, source)?;
            }
            false_end = self.current_block;
        }

        let merge = self.new_block();
        if !is_constant_false(&cond_text) {
            self.cfg.add_edge(true_end, merge, CfgEdgeType::Next);
        }
        if !is_constant_true(&cond_text) {
            self.cfg.add_edge(false_end, merge, CfgEdgeType::Next);
        }
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
            defined_vars: HashSet::new(),
            used_vars: HashSet::new(),
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
        if let Some(init) = node
            .child_by_field_name("initializer")
            .or_else(|| node.child_by_field_name("range"))
        {
            self.visit_statement(init, source)?;
        }

        let header = self.new_block();
        self.cfg
            .add_edge(self.current_block, header, CfgEdgeType::Next);
        self.add_statement_to_current(Statement {
            kind: StatementKind::Branch,
            line: node.start_position().row + 1,
            text: "for".to_string(),
            defined_vars: HashSet::new(),
            used_vars: HashSet::new(),
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
        self.flow_active = false;
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

    fn visit_switch(&mut self, node: Node, source: &[u8]) -> Result<()> {
        self.add_statement(node, source, StatementKind::Branch)?;
        let cond_block = self.current_block;
        let merge = self.new_block();
        let mut cases = Vec::new();
        collect_switch_cases(node, &mut cases);

        if cases.is_empty() {
            if let Some(body) = node.child_by_field_name("body") {
                self.current_block = cond_block;
                self.visit_block(body, source)?;
            }
            self.current_block = merge;
            return Ok(());
        }

        let mut has_default = false;
        for case in cases {
            let is_default = matches!(case.kind(), "default_case" | "default_statement");
            if is_default {
                has_default = true;
            }

            let case_block = self.new_block();
            let edge = if is_default {
                CfgEdgeType::IfFalse
            } else {
                CfgEdgeType::IfTrue
            };
            self.cfg.add_edge(cond_block, case_block, edge);
            self.current_block = case_block;

            if let Some(body) = case.child_by_field_name("body") {
                self.visit_block(body, source)?;
            } else {
                self.visit_block(case, source)?;
            }
            self.cfg
                .add_edge(self.current_block, merge, CfgEdgeType::Next);
        }

        if !has_default {
            let default_block = self.new_block();
            self.cfg
                .add_edge(cond_block, default_block, CfgEdgeType::IfFalse);
            self.cfg
                .add_edge(default_block, merge, CfgEdgeType::Next);
        }

        self.current_block = merge;
        Ok(())
    }

    fn visit_select(&mut self, node: Node, source: &[u8]) -> Result<()> {
        self.visit_switch(node, source)
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

fn collect_switch_cases<'a>(node: Node<'a>, cases: &mut Vec<Node<'a>>) {
    match node.kind() {
        "expression_case"
        | "type_case"
        | "case_clause"
        | "default_case"
        | "default_statement"
        | "communication_case" => cases.push(node),
        _ => {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if child.is_named() {
                    collect_switch_cases(child, cases);
                }
            }
        }
    }
}

fn is_block_like(kind: &str) -> bool {
    matches!(
        kind,
        "block"
            | "statement_block"
            | "statement_list"
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

fn is_constant_false(text: &str) -> bool {
    matches!(text, "false" | "False" | "0")
}

fn is_constant_true(text: &str) -> bool {
    matches!(text, "true" | "True" | "1")
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
    fn test_go_method_if_cfg() {
        let code = r#"
package handler

func (h *AuthHandler) Login(c *gin.Context) {
    if c == nil {
        return
    }
    return
}
"#;
        let cfg = build_cfg_for_function("go", code, "Login").unwrap();
        assert!(cfg.blocks.len() >= 4);
        let if_true = cfg
            .edges
            .iter()
            .filter(|e| e.edge_type == CfgEdgeType::IfTrue)
            .count();
        assert!(if_true >= 1);
    }

    #[test]
    fn test_go_for_loop_has_cycle() {
        let code = r#"
package demo

func Sum(n int) int {
    total := 0
    for i := 0; i < n; i++ {
        total += i
    }
    return total
}
"#;
        let cfg = build_cfg_for_function("go", code, "Sum").unwrap();
        assert!(cfg.has_cycle());
    }

    #[test]
    fn test_go_switch_cfg() {
        let code = r#"
package demo

func Pick(x int) int {
    switch x {
    case 1:
        return 10
    case 2:
        return 20
    default:
        return 0
    }
}
"#;
        let cfg = build_cfg_for_function("go", code, "Pick").unwrap();
        assert!(cfg.blocks.len() >= 5);
        let branches = cfg
            .edges
            .iter()
            .filter(|e| e.edge_type == CfgEdgeType::IfTrue)
            .count();
        assert!(branches >= 2);
    }

    #[test]
    fn test_go_select_cfg() {
        let code = r#"
package demo

func Wait(ch chan int, done chan struct{}) int {
    select {
    case v := <-ch:
        return v
    default:
        return -1
    }
}
"#;
        let cfg = build_cfg_for_function("go", code, "Wait").unwrap();
        assert!(cfg.blocks.len() >= 4);
    }

    #[test]
    fn test_go_range_for_cfg() {
        let code = r#"
package demo

func Keys(m map[string]int) int {
    n := 0
    for k := range m {
        n += len(k)
    }
    return n
}
"#;
        let cfg = build_cfg_for_function("go", code, "Keys").unwrap();
        assert!(cfg.has_cycle());
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
