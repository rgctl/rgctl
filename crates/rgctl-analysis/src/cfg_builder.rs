//! CFG construction from tree-sitter ASTs.

use crate::cfg::{BasicBlock, BlockId, CfgEdgeType, ControlFlowGraph, Statement, StatementKind};
use crate::def_use::extract_def_use;
use crate::language_profile::{function_kinds_for, parse_source};
use rgctl_error::{Error, Result};
use rgctl_plugin_helpers::{ecmascript_function_symbol_name, extract_name_from_node};
use smallvec::SmallVec;
use std::collections::{HashMap, HashSet};
use tracing::warn;
use tree_sitter::{Node, Tree};

/// Maximum AST recursion depth for CFG expression walks (matches C++ extraction cap).
const AST_WALK_MAX_DEPTH: usize = 2048;

/// Build a CFG for a named function in source text.
pub fn build_cfg_for_function(
    language: &str,
    source: &str,
    function_name: &str,
) -> Result<ControlFlowGraph> {
    let bytes = source.as_bytes();
    let tree = parse_language(language, bytes)?;
    let function_kinds = function_kinds_for(language)?;
    let func_node =
        find_function_by_name(tree.root_node(), bytes, function_name, function_kinds, language)
            .ok_or_else(|| Error::NotFound(format!("function '{function_name}' not found")))?;
    build_cfg_from_function_node(language, func_node, bytes, function_name)
}

/// Parsed source file with function name / span → byte index (one tree-sitter parse per file).
pub struct ParsedSourceFile {
    tree: Tree,
    /// First location per bare name (name-only lookup fallback).
    locations: HashMap<String, FunctionLocation>,
    /// All locations for a bare name (overloads).
    locations_by_name: HashMap<String, Vec<FunctionLocation>>,
    /// Exact span key `(start_line, end_line, name)` → location.
    locations_by_span: HashMap<(usize, usize, String), FunctionLocation>,
}

impl ParsedSourceFile {
    /// Parse and index all functions in a source file.
    pub fn parse(language: &str, source: &[u8]) -> Result<Self> {
        let (tree, index) = index_function_locations_full(language, source)?;
        Ok(Self {
            tree,
            locations: index.by_name_first,
            locations_by_name: index.by_name,
            locations_by_span: index.by_span,
        })
    }

    /// Borrow the cached tree-sitter [`Tree`].
    pub fn tree(&self) -> &Tree {
        &self.tree
    }

    /// Whether `function_name` was found in the indexed file.
    pub fn contains(&self, function_name: &str) -> bool {
        self.locations.contains_key(function_name)
            || self.locations_by_name.contains_key(function_name)
    }

    /// Build CFG for a named function using the cached parse tree (first match by name).
    pub fn build_cfg(
        &self,
        language: &str,
        source: &[u8],
        function_name: &str,
    ) -> Result<ControlFlowGraph> {
        if let Some(loc) = self.locations.get(function_name) {
            if let Some(func_node) = self
                .tree
                .root_node()
                .descendant_for_byte_range(loc.start_byte, loc.end_byte)
            {
                return build_cfg_from_function_node(language, func_node, source, function_name);
            }
        }
        build_cfg_for_function_in_tree(language, &self.tree, source, function_name)
    }

    /// Build CFG for an overloaded / spanned function using `(start_line, end_line, name)`.
    ///
    /// Falls back to name-only lookup when the exact span is missing.
    pub fn build_cfg_for_node_span(
        &self,
        language: &str,
        source: &[u8],
        function_name: &str,
        start_line: usize,
        end_line: usize,
    ) -> Result<ControlFlowGraph> {
        if let Some(func_node) = self.function_node_for_span(function_name, start_line, end_line) {
            return build_cfg_from_function_node(language, func_node, source, function_name);
        }
        self.build_cfg(language, source, function_name)
    }

    /// Resolve the tree-sitter function node for `(start_line, end_line, name)`.
    pub fn function_node_for_span(
        &self,
        function_name: &str,
        start_line: usize,
        end_line: usize,
    ) -> Option<Node<'_>> {
        let key = (start_line, end_line, function_name.to_string());
        if let Some(loc) = self.locations_by_span.get(&key) {
            if let Some(func_node) = self
                .tree
                .root_node()
                .descendant_for_byte_range(loc.start_byte, loc.end_byte)
            {
                return Some(func_node);
            }
        }
        if let Some(locs) = self.locations_by_name.get(function_name) {
            if let Some(loc) = locs
                .iter()
                .find(|l| l.start_line == start_line)
                .or_else(|| locs.first())
            {
                return self
                    .tree
                    .root_node()
                    .descendant_for_byte_range(loc.start_byte, loc.end_byte);
            }
        }
        None
    }
}

/// Byte / line span of a function in a parsed source file.
#[derive(Debug, Clone, Copy)]
pub struct FunctionLocation {
    /// Inclusive start byte offset in the source buffer.
    pub start_byte: usize,
    /// Exclusive end byte offset in the source buffer.
    pub end_byte: usize,
    /// First source line (1-based).
    pub start_line: usize,
    /// Last source line (1-based).
    pub end_line: usize,
}

struct FunctionLocationIndex {
    by_name_first: HashMap<String, FunctionLocation>,
    by_name: HashMap<String, Vec<FunctionLocation>>,
    by_span: HashMap<(usize, usize, String), FunctionLocation>,
}

/// Build a CFG for a named function in an already-parsed tree (no re-parse).
pub fn build_cfg_for_function_in_tree(
    language: &str,
    tree: &Tree,
    source: &[u8],
    function_name: &str,
) -> Result<ControlFlowGraph> {
    let function_kinds = function_kinds_for(language)?;
    let func_node =
        find_function_by_name(tree.root_node(), source, function_name, function_kinds, language)
            .ok_or_else(|| Error::NotFound(format!("function '{function_name}' not found")))?;
    build_cfg_from_function_node(language, func_node, source, function_name)
}

/// Parse a source file once and index function names to source byte spans.
pub fn index_function_locations(
    language: &str,
    source: &[u8],
) -> Result<(Tree, HashMap<String, FunctionLocation>)> {
    let (tree, index) = index_function_locations_full(language, source)?;
    Ok((tree, index.by_name_first))
}

fn index_function_locations_full(
    language: &str,
    source: &[u8],
) -> Result<(Tree, FunctionLocationIndex)> {
    let tree = parse_language(language, source)?;
    let function_kinds = function_kinds_for(language)?;
    let mut index = FunctionLocationIndex {
        by_name_first: HashMap::new(),
        by_name: HashMap::new(),
        by_span: HashMap::new(),
    };
    collect_function_locations(tree.root_node(), source, function_kinds, language, &mut index);
    Ok((tree, index))
}

fn collect_function_locations(
    root: Node<'_>,
    source: &[u8],
    function_kinds: &[&str],
    language: &str,
    out: &mut FunctionLocationIndex,
) {
    let mut stack = vec![root];
    while let Some(node) = stack.pop() {
        if function_kinds.contains(&node.kind()) {
            if let Some(func_name) = callable_name_for_cfg(node, source, language) {
                let loc = FunctionLocation {
                    start_byte: node.start_byte(),
                    end_byte: node.end_byte(),
                    start_line: node.start_position().row + 1,
                    end_line: node.end_position().row + 1,
                };
                out.by_name
                    .entry(func_name.clone())
                    .or_default()
                    .push(loc);
                out.by_name_first.entry(func_name.clone()).or_insert(loc);
                out.by_span
                    .insert((loc.start_line, loc.end_line, func_name), loc);
            }
        }
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            stack.push(child);
        }
    }
}

fn parse_language(language: &str, source: &[u8]) -> Result<Tree> {
    parse_source(language, source)
}

/// Resolve the Function name CFG should look up, matching language-plugin naming.
fn callable_name_for_cfg(node: Node<'_>, source: &[u8], language: &str) -> Option<String> {
    match language {
        "javascript" | "js" | "typescript" | "ts" => {
            ecmascript_function_symbol_name(node, source)
        }
        "puppet" => {
            // Prefer class_identifier / identifier / node_name over other children.
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if matches!(
                    child.kind(),
                    "class_identifier" | "identifier" | "node_name" | "string"
                ) {
                    if let Ok(t) = child.utf8_text(source) {
                        let name = t.trim_matches('\'').trim_matches('"');
                        if node.kind() == "node_definition" {
                            return Some(format!("node:{name}"));
                        }
                        return Some(name.to_string());
                    }
                }
            }
            extract_name_from_node(node, source).ok().flatten()
        }
        "kotlin" | "kt"
            if matches!(
                node.kind(),
                "primary_constructor" | "secondary_constructor"
            ) =>
        {
            // Constructors are looked up by enclosing type simple name (Java-shaped).
            enclosing_type_simple_name(node, source)
                .or_else(|| extract_name_from_node(node, source).ok().flatten())
        }
        _ => extract_name_from_node(node, source).ok().flatten(),
    }
}

fn enclosing_type_simple_name(node: Node<'_>, source: &[u8]) -> Option<String> {
    let mut cur = node.parent();
    while let Some(n) = cur {
        if matches!(
            n.kind(),
            "class_declaration" | "object_declaration" | "companion_object"
        ) {
            return n
                .child_by_field_name("name")
                .and_then(|x| x.utf8_text(source).ok().map(str::to_string))
                .or_else(|| {
                    let mut c = n.walk();
                    n.children(&mut c).find_map(|ch| {
                        if matches!(ch.kind(), "identifier" | "simple_identifier" | "type_identifier")
                        {
                            ch.utf8_text(source).ok().map(str::to_string)
                        } else {
                            None
                        }
                    })
                });
        }
        cur = n.parent();
    }
    None
}

fn find_function_by_name<'a>(
    node: Node<'a>,
    source: &[u8],
    name: &str,
    function_kinds: &[&str],
    language: &str,
) -> Option<Node<'a>> {
    // Java instance initializer blocks are bare `block` children of `class_body`,
    // extracted as synthetic `<initblock>N` functions (not a distinct CST kind).
    if let Some(idx) = name
        .strip_prefix("<initblock>")
        .and_then(|s| s.parse::<usize>().ok())
    {
        return find_java_instance_initblock(node, idx);
    }

    // Lambdas are extracted as `Owner.$lambda$N` synthetic Functions (see
    // rgctl-lang-java's `ExtractCtx::lambda_index`), but the CFG builder
    // has no owner-scoped index here, so `$lambda$N` resolves to the Nth
    // `lambda_expression` in the whole file (document order). This matches
    // the extractor's numbering only when a single enclosing callable
    // contains lambdas; see java-grammar-remainder honesty notes.
    if let Some(pos) = name.rfind("$lambda$") {
        if let Ok(idx) = name[pos + "$lambda$".len()..].parse::<usize>() {
            return find_java_lambda_expression(node, idx);
        }
    }

    let mut stack = vec![node];
    while let Some(node) = stack.pop() {
        if function_kinds.contains(&node.kind()) {
            if let Some(func_name) = callable_name_for_cfg(node, source, language) {
                if func_name == name {
                    return Some(node);
                }
            }
        }
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            stack.push(child);
        }
    }
    None
}

/// Nth direct `block` child of a Java `class_body` (instance initializer).
fn find_java_instance_initblock<'a>(node: Node<'a>, index: usize) -> Option<Node<'a>> {
    let mut stack = vec![node];
    while let Some(node) = stack.pop() {
        if node.kind() == "class_body" {
            let mut seen = 0usize;
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if child.kind() == "block" {
                    if seen == index {
                        return Some(child);
                    }
                    seen += 1;
                }
            }
        }
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            stack.push(child);
        }
    }
    None
}

/// Nth `lambda_expression` in the whole tree, document (pre-)order.
fn find_java_lambda_expression<'a>(node: Node<'a>, index: usize) -> Option<Node<'a>> {
    let mut stack = vec![node];
    let mut seen = 0usize;
    while let Some(node) = stack.pop() {
        if node.kind() == "lambda_expression" {
            if seen == index {
                return Some(node);
            }
            seen += 1;
        }
        let mut cursor = node.walk();
        let children: Vec<_> = node.children(&mut cursor).collect();
        for child in children.into_iter().rev() {
            stack.push(child);
        }
    }
    None
}

fn build_cfg_from_function_node(
    language: &str,
    func_node: Node,
    source: &[u8],
    function_name: &str,
) -> Result<ControlFlowGraph> {
    let mut cfg = ControlFlowGraph::new();
    cfg.local_types = crate::field_write_locals::extract_param_types_in_function(
        language,
        func_node,
        source,
        function_name,
    );
    let mut builder = CfgBuilder::new(&mut cfg, language);
    builder.build_function(func_node, source)?;
    Ok(cfg)
}

struct CfgBuilder<'a> {
    cfg: &'a mut ControlFlowGraph,
    current_block: BlockId,
    language: &'a str,
    /// Innermost-first stack of `for` / `switch` / `select` break targets.
    breakable_stack: Vec<BreakableContext>,
    /// False after return/unreachable branch terminates the current path.
    flow_active: bool,
    /// Go `Label:` entry blocks (created eagerly so forward `goto` resolves).
    label_blocks: HashMap<String, BlockId>,
    /// Set when the current switch case ended with `fallthrough`.
    pending_fallthrough: bool,
    /// Label to attach to the next pushed breakable (`Outer: for` / `Outer: switch`).
    pending_breakable_label: Option<String>,
    /// Function-scoped `defer` stack (static approx; LIFO on return/panic).
    defer_stack: Vec<DeferredCall>,
    /// Enclosing `finally` bodies (Java/C#-style), LIFO on return/throw.
    finally_stack: Vec<Vec<DeferredCall>>,
    /// Catch entry blocks for enclosing tries (Exception edges from throw sites).
    try_catch_stack: Vec<Vec<BlockId>>,
    /// When true, leaving a switch case falls into the next case (Java classic switch).
    switch_implicit_fallthrough: bool,
    /// C `setjmp` sites in this function (approx targets for `longjmp`).
    setjmp_sites: Vec<BlockId>,
    /// Innermost-first C# `switch` case labels for `goto case` / `goto default`.
    switch_case_labels: Vec<HashMap<String, BlockId>>,
    /// Exit blocks for nested local-function / lambda sub-CFGs (not function exits).
    nested_exits: Vec<BlockId>,
    /// Monotonic block id counter (function-local; avoids `Uuid::new_v4` per block).
    next_block_serial: u64,
    /// Pending exception edges to avoid scanning all CFG edges on each statement.
    pending_exception_edges: HashSet<(BlockId, BlockId)>,
    /// One-time warn when expression walk hits [`AST_WALK_MAX_DEPTH`].
    expr_walk_depth_warned: bool,
}

struct BreakableContext {
    /// Block after the loop/switch/select.
    exit: BlockId,
    /// `continue` target (update/header). `None` for switch/select.
    continue_target: Option<BlockId>,
    /// Optional label from `Label: for` / `Label: switch`.
    label: Option<String>,
}

#[derive(Clone)]
struct DeferredCall {
    text: String,
    line: usize,
}

impl<'a> CfgBuilder<'a> {
    fn new(cfg: &'a mut ControlFlowGraph, language: &'a str) -> Self {
        let entry = cfg.entry;
        Self {
            cfg,
            current_block: entry,
            language,
            breakable_stack: Vec::new(),
            flow_active: true,
            label_blocks: HashMap::new(),
            pending_fallthrough: false,
            pending_breakable_label: None,
            defer_stack: Vec::new(),
            finally_stack: Vec::new(),
            try_catch_stack: Vec::new(),
            switch_implicit_fallthrough: false,
            setjmp_sites: Vec::new(),
            switch_case_labels: Vec::new(),
            nested_exits: Vec::new(),
            next_block_serial: 1,
            pending_exception_edges: HashSet::new(),
            expr_walk_depth_warned: false,
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
        if self.flow_active {
            if !self.defer_stack.is_empty() {
                self.unwind_defers_to_exit(CfgEdgeType::Return);
            } else if self.cfg.exits.is_empty() {
                let exit = self.new_block();
                self.cfg
                    .add_edge(self.current_block, exit, CfgEdgeType::Next);
                self.cfg.exits.push(exit);
            }
        }
        self.cfg.prune_unreachable_blocks();
        self.cfg.contract_empty_blocks();
        Ok(())
    }

    /// Reuse `current_block` as an `if`/`unless` condition header when it is still empty
    /// and has no self-loop edges (avoids empty entry/merge trampolines).
    fn can_reuse_current_as_cond(&self) -> bool {
        self.cfg
            .blocks
            .get(&self.current_block)
            .is_some_and(|b| b.statements.is_empty())
            && self
                .cfg
                .predecessors(self.current_block)
                .iter()
                .all(|&p| p != self.current_block)
    }

    fn new_block(&mut self) -> BlockId {
        let id = BlockId(self.next_block_serial as u32);
        self.next_block_serial += 1;
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
        let kind = stmt.kind;
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
        // Conservative: any non-terminal stmt in a try may throw → catch.
        if !matches!(kind, StatementKind::Return | StatementKind::Jump) {
            self.wire_try_exception_from_current();
        }
    }

    fn wire_try_exception_from_current(&mut self) {
        let from = self.current_block;
        let Some(handlers) = self.try_catch_stack.last() else {
            return;
        };
        let handlers = handlers.clone();
        for h in handlers {
            if self.pending_exception_edges.insert((from, h)) {
                self.cfg.add_edge(from, h, CfgEdgeType::Exception);
            }
        }
    }

    fn visit_block(&mut self, node: Node, source: &[u8]) -> Result<BlockId> {
        let mut using_decls = 0usize;
        if is_block_like(node.kind()) {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if !child.is_named() {
                    continue;
                }
                if is_csharp_using_declaration(child, source) {
                    self.visit_using_declaration(child, source)?;
                    using_decls += 1;
                } else {
                    self.visit_statement(child, source)?;
                }
            }
        } else if is_csharp_using_declaration(node, source) {
            self.visit_using_declaration(node, source)?;
            using_decls += 1;
        } else {
            self.visit_statement(node, source)?;
        }
        self.close_using_declarations(using_decls)?;
        Ok(self.current_block)
    }

    fn visit_statement(&mut self, node: Node, source: &[u8]) -> Result<()> {
        if !self.flow_active {
            return Ok(());
        }
        match node.kind() {
            // Ruby control flow
            "if" | "unless" => self.visit_if(node, source),
            "while" | "until" => self.visit_while(node, source),
            "for" => self.visit_for(node, source),
            "case" => self.visit_switch(node, source),
            "begin_block" => self.visit_ruby_begin_block(node, source),
            "return" => self.visit_return(node, source),
            "next" => self.visit_continue(node, source),
            "break" => self.visit_break(node, source),
            "if_modifier" | "unless_modifier" | "while_modifier" | "until_modifier"
            | "rescue_modifier" => self.visit_ruby_modifier_statement(node, source),

            "call" => {
                if let Some(block) = node.child_by_field_name("block") {
                    self.visit_nested_subcfg(block, source)?;
                }
                if let Some(do_block) = node.child_by_field_name("do_block") {
                    self.visit_nested_subcfg(do_block, source)?;
                }
                self.visit_expression_stmt(node, source)
            }

            // Rust + Python + Puppet conditionals
            "if_statement" | "if_expression" => self.visit_if(node, source),
            "unless_statement" => self.visit_puppet_unless(node, source),
            "case_statement" if self.language == "puppet" => {
                self.visit_puppet_case(node, source)
            }
            "selector" if self.language == "puppet" => self.visit_expression_stmt(node, source),
            "while_statement" | "while_expression" => self.visit_while(node, source),
            "do_statement" | "do_while_statement" => self.visit_do(node, source),
            "for_statement" | "for_expression" | "for_in_expression" | "foreach_statement"
            | "for_range_loop" => self.visit_for(node, source),
            "enhanced_for_statement" => self.visit_enhanced_for(node, source),
            "loop_expression" => self.visit_loop(node, source),
            // Kotlin `when` — treat like switch expression (arm fan-out)
            "when_expression" => self.visit_kotlin_when(node, source),
            "property_declaration" => {
                self.visit_declaration_initializers(node, source)?;
                if !self.flow_active {
                    return Ok(());
                }
                self.add_statement(node, source, StatementKind::Declaration)?;
                Ok(())
            }

            // Returns / coroutine / iterator yields
            "return_statement" | "return_expression" | "co_return_statement" => {
                self.visit_return(node, source)
            }
            "co_yield_statement" | "co_yield_expression" => self.visit_co_yield(node, source),
            "yield_statement" => self.visit_yield_statement(node, source),
            "throw_statement" | "throw_expression" => self.visit_throw(node, source),
            "co_await_expression" => self.visit_co_await(node, source),
            "await_expression" => self.visit_await_expression(node, source),

            // C# local functions / lambdas → disconnected sub-CFGs
            "local_function_statement" => self.visit_nested_subcfg(node, source),
            "lambda_expression" | "anonymous_method_expression" => {
                self.visit_nested_subcfg(node, source)
            }

            // C# resource / sync sugar → finally-style cleanup
            "using_statement" => self.visit_using_statement(node, source),
            "lock_statement" => self.visit_lock_statement(node, source),

            // Null-conditional / coalescing (also visited from expr walk)
            "conditional_access_expression" => self.visit_conditional_access(node, source),

            // Jumps
            "break_expression" | "break_statement" => self.visit_break(node, source),
            "continue_expression" | "continue_statement" => self.visit_continue(node, source),
            "goto_statement" => self.visit_goto(node, source),
            "fallthrough_statement" => self.visit_fallthrough(node, source),
            "labeled_statement" | "empty_labeled_statement" => {
                self.visit_labeled_statement(node, source)
            }

            // Declarations / assignments
            "let_declaration" | "let_statement" => {
                if let Some(value) = node.child_by_field_name("value") {
                    self.visit_expr_for_control_flow(value, source)?;
                }
                self.add_statement(node, source, StatementKind::Declaration)?;
                Ok(())
            }
            "short_var_declaration"
            | "var_declaration"
            | "const_declaration"
            | "variable_declaration"
            | "local_declaration_statement"
            | "local_variable_declaration"
            | "declaration"
            | "init_statement" => {
                if is_csharp_using_declaration(node, source) {
                    return self.visit_using_declaration(node, source);
                }
                if node.kind() == "init_statement" {
                    let mut c = node.walk();
                    for child in node.children(&mut c) {
                        if child.is_named() {
                            self.visit_statement(child, source)?;
                        }
                    }
                    return Ok(());
                }
                // Lower await / ?? /?. inside initializers before recording the decl.
                self.visit_declaration_initializers(node, source)?;
                if !self.flow_active {
                    return Ok(());
                }
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
            "inc_statement"
            | "dec_statement"
            | "send_statement"
            | "update_expression"
            | "postfix_unary_expression"
            | "prefix_unary_expression" => {
                self.add_statement(node, source, StatementKind::Expression)?;
                Ok(())
            }

            // Expression statement
            "expression_statement" => self.visit_expression_stmt(node, source),

            // C ternary `cond ? a : b`
            "conditional_expression" => self.visit_conditional_expression(node, source),

            // Rust match
            "match_expression" => self.visit_match(node, source),

            // Switch (Go + Java/C# `switch_expression`)
            "switch_statement" | "type_switch_statement" | "expression_switch_statement" => {
                self.visit_switch(node, source)
            }
            "switch_expression" => self.visit_switch_expression(node, source),
            "select_statement" => self.visit_select(node, source),

            // Go concurrency helpers
            "defer_statement" => self.visit_defer(node, source),
            "go_statement" => {
                self.add_statement(node, source, StatementKind::Expression)?;
                Ok(())
            }

            // try / try-with-resources
            "try_statement" | "try_with_resources_statement" => self.visit_try(node, source),
            "assert_statement" => {
                self.add_statement(node, source, StatementKind::Branch)?;
                Ok(())
            }

            // Rust `expr?` (await already matched above)
            "try_expression" => self.visit_try_expression(node, source),
            "macro_invocation" => self.visit_macro_invocation(node, source),

            // Block / body wrapper
            k if is_block_like(k) => {
                self.visit_block(node, source)?;
                Ok(())
            }
            // Unwrap Rust `else { ... }` wrapper.
            "else_clause" => {
                if let Some(block) = find_child_kind(node, "block") {
                    self.visit_block(block, source)?;
                }
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
        // Ruby `call` must stay rooted at the `call` node — first named child is only the receiver.
        let inner = if node.kind() == "call" {
            node
        } else {
            node.named_child(0).unwrap_or(node)
        };
        match inner.kind() {
            "if_statement"
            | "if_expression"
            | "while_statement"
            | "while_expression"
            | "for_statement"
            | "for_expression"
            | "for_in_expression"
            | "loop_expression"
            | "match_expression"
            | "return_statement"
            | "return_expression"
            | "try_expression"
            | "await_expression"
            | "break_expression"
            | "continue_expression"
            | "macro_invocation"
            | "conditional_expression"
            | "co_await_expression"
            | "throw_statement"
            | "throw_expression"
            | "co_yield_statement"
            | "co_return_statement"
            | "switch_expression"
            | "conditional_access_expression"
            | "lambda_expression"
            | "anonymous_method_expression" => self.visit_statement(inner, source),
            "binary_expression" if null_coalesce_operator(inner, source) => {
                self.visit_null_coalesce(inner, source)
            }
            _ => {
                self.visit_expr_for_control_flow(inner, source)?;
                if !self.flow_active {
                    return Ok(());
                }
                let kind = Self::classify_expression(inner, source);
                self.add_statement(inner, source, kind)?;
                if is_panic_call(inner, source)
                    || is_diverging_macro(inner, source)
                    || is_terminating_call(inner, source)
                {
                    self.unwind_defers_to_exit(CfgEdgeType::Exception);
                } else if is_longjmp_call(inner, source) {
                    self.wire_longjmp_edges();
                } else if is_setjmp_call(inner, source) {
                    self.setjmp_sites.push(self.current_block);
                }
                Ok(())
            }
        }
    }

    /// Lower nested `?` / `.await` / control-flow expressions inside a larger expression.
    fn visit_expr_for_control_flow(&mut self, node: Node, source: &[u8]) -> Result<()> {
        self.visit_expr_for_control_flow_depth(node, source, 0)
    }

    fn visit_expr_for_control_flow_depth(
        &mut self,
        node: Node,
        source: &[u8],
        depth: usize,
    ) -> Result<()> {
        let mut stack = vec![(node, depth)];
        while let Some((node, depth)) = stack.pop() {
            if depth > AST_WALK_MAX_DEPTH {
                if !self.expr_walk_depth_warned {
                    self.expr_walk_depth_warned = true;
                    warn!(
                        depth = depth,
                        language = self.language,
                        "CFG expression walk depth limit exceeded; skipping deep branches"
                    );
                }
                continue;
            }
            if !self.flow_active {
                return Ok(());
            }
            match node.kind() {
                "try_expression" => self.visit_try_expression(node, source)?,
                "await_expression" => self.visit_await_expression(node, source)?,
                "conditional_access_expression" => self.visit_conditional_access(node, source)?,
                "switch_expression" => self.visit_switch_expression(node, source)?,
                "when_expression" => self.visit_kotlin_when(node, source)?,
                "lambda_expression" | "anonymous_method_expression" => {
                    self.visit_nested_subcfg(node, source)?
                }
                "binary_expression" if null_coalesce_operator(node, source) => {
                    self.visit_null_coalesce(node, source)?
                }
                "if_expression"
                | "if_statement"
                | "match_expression"
                | "loop_expression"
                | "while_expression"
                | "while_statement"
                | "for_expression"
                | "for_statement"
                | "return_expression"
                | "return_statement"
                | "break_expression"
                | "continue_expression"
                | "macro_invocation"
                | "conditional_expression" => self.visit_statement(node, source)?,
                _ => {
                    if is_setjmp_call(node, source) {
                        self.add_statement(node, source, StatementKind::Branch)?;
                        self.setjmp_sites.push(self.current_block);
                        continue;
                    }
                    if is_longjmp_call(node, source) {
                        self.add_statement(node, source, StatementKind::Jump)?;
                        self.wire_longjmp_edges();
                        continue;
                    }
                    if is_terminating_call(node, source) {
                        self.add_statement(node, source, StatementKind::Jump)?;
                        self.unwind_defers_to_exit(CfgEdgeType::Exception);
                        continue;
                    }
                    let mut cursor = node.walk();
                    for child in node.children(&mut cursor) {
                        if child.is_named() {
                            stack.push((child, depth + 1));
                        }
                    }
                }
            }
            if !self.flow_active {
                return Ok(());
            }
        }
        Ok(())
    }

    fn visit_conditional_expression(&mut self, node: Node, source: &[u8]) -> Result<()> {
        let true_block = self.new_block();
        let false_block = self.new_block();
        let merge = self.new_block();
        if let Some(cond) = node.child_by_field_name("condition") {
            self.wire_condition(cond, source, true_block, false_block)?;
        } else {
            self.add_statement(node, source, StatementKind::Branch)?;
            self.cfg
                .add_edge(self.current_block, true_block, CfgEdgeType::IfTrue);
            self.cfg
                .add_edge(self.current_block, false_block, CfgEdgeType::IfFalse);
        }

        self.flow_active = true;
        self.current_block = true_block;
        if let Some(cons) = node.child_by_field_name("consequence") {
            self.visit_expr_for_control_flow(cons, source)?;
            if self.flow_active {
                self.add_statement(cons, source, StatementKind::Expression)?;
            }
        }
        let true_reaches = self.flow_active;
        let true_end = self.current_block;

        self.flow_active = true;
        self.current_block = false_block;
        if let Some(alt) = node.child_by_field_name("alternative") {
            self.visit_expr_for_control_flow(alt, source)?;
            if self.flow_active {
                self.add_statement(alt, source, StatementKind::Expression)?;
            }
        }
        let false_reaches = self.flow_active;
        let false_end = self.current_block;

        if true_reaches {
            self.cfg.add_edge(true_end, merge, CfgEdgeType::Next);
        }
        if false_reaches {
            self.cfg.add_edge(false_end, merge, CfgEdgeType::Next);
        }
        self.flow_active = true_reaches || false_reaches;
        self.current_block = merge;
        Ok(())
    }

    fn wire_longjmp_edges(&mut self) {
        let sites = self.setjmp_sites.clone();
        if sites.is_empty() {
            self.unwind_defers_to_exit(CfgEdgeType::Exception);
            return;
        }
        for site in sites {
            self.cfg
                .add_edge(self.current_block, site, CfgEdgeType::Jump);
        }
        self.flow_active = false;
        self.current_block = self.new_block();
    }

    fn visit_try_expression(&mut self, node: Node, source: &[u8]) -> Result<()> {
        if let Some(inner) = node.named_child(0) {
            self.visit_expr_for_control_flow(inner, source)?;
            if !self.flow_active {
                return Ok(());
            }
        }
        let text = node.utf8_text(source).unwrap_or("?").trim().to_string();
        self.add_statement_to_current(Statement {
            kind: StatementKind::Branch,
            line: node.start_position().row + 1,
            text,
            defined_vars: SmallVec::new(),
            used_vars: SmallVec::new(),
        });
        let ok_block = self.new_block();
        let err_block = self.new_block();
        let from = self.current_block;
        self.cfg.add_edge(from, ok_block, CfgEdgeType::IfTrue);
        self.cfg.add_edge(from, err_block, CfgEdgeType::IfFalse);
        // Error path: early return from the function (`?`).
        self.flow_active = true;
        self.current_block = err_block;
        self.unwind_finallies(source)?;
        self.unwind_defers_to_exit(CfgEdgeType::Return);
        // Success path continues.
        self.flow_active = true;
        self.current_block = ok_block;
        Ok(())
    }

    fn visit_await_expression(&mut self, node: Node, source: &[u8]) -> Result<()> {
        if let Some(inner) = node.named_child(0) {
            self.visit_expr_for_control_flow(inner, source)?;
            if !self.flow_active {
                return Ok(());
            }
        }
        let text = node
            .utf8_text(source)
            .unwrap_or(".await")
            .trim()
            .to_string();
        self.add_statement_to_current(Statement {
            kind: StatementKind::Branch,
            line: node.start_position().row + 1,
            text,
            defined_vars: SmallVec::new(),
            used_vars: SmallVec::new(),
        });
        let from = self.current_block;
        let resume = self.new_block();
        // Async state machine: completed → resume; incomplete → suspend/yield to caller.
        if matches!(
            self.language.to_lowercase().as_str(),
            "csharp" | "cs" | "c#"
        ) {
            self.cfg.add_edge(from, resume, CfgEdgeType::IfTrue);
            let suspend = self.new_block();
            self.cfg.add_edge(from, suspend, CfgEdgeType::IfFalse);
            if let Some(&nested) = self.nested_exits.last() {
                self.cfg.add_edge(suspend, nested, CfgEdgeType::Return);
            } else {
                self.cfg.exits.push(suspend);
            }
        } else {
            self.cfg.add_edge(from, resume, CfgEdgeType::Next);
        }
        self.flow_active = true;
        self.current_block = resume;
        Ok(())
    }

    fn visit_co_await(&mut self, node: Node, source: &[u8]) -> Result<()> {
        if let Some(arg) = node.child_by_field_name("argument") {
            self.visit_expr_for_control_flow(arg, source)?;
            if !self.flow_active {
                return Ok(());
            }
        }
        let text = node
            .utf8_text(source)
            .unwrap_or("co_await")
            .trim()
            .to_string();
        self.add_statement_to_current(Statement {
            kind: StatementKind::Branch,
            line: node.start_position().row + 1,
            text,
            defined_vars: SmallVec::new(),
            used_vars: SmallVec::new(),
        });
        let resume = self.new_block();
        self.cfg
            .add_edge(self.current_block, resume, CfgEdgeType::Next);
        self.current_block = resume;
        Ok(())
    }

    fn visit_co_yield(&mut self, node: Node, source: &[u8]) -> Result<()> {
        self.add_statement(node, source, StatementKind::Branch)?;
        let resume = self.new_block();
        self.cfg
            .add_edge(self.current_block, resume, CfgEdgeType::Next);
        self.current_block = resume;
        Ok(())
    }

    /// C# `yield return` (suspend) / `yield break` (exit).
    fn visit_yield_statement(&mut self, node: Node, source: &[u8]) -> Result<()> {
        let text = node.utf8_text(source).unwrap_or("yield").trim().to_string();
        if text.contains("break") {
            self.add_statement(node, source, StatementKind::Jump)?;
            self.unwind_finallies(source)?;
            self.unwind_defers_to_exit(CfgEdgeType::Return);
            return Ok(());
        }
        self.add_statement(node, source, StatementKind::Branch)?;
        let resume = self.new_block();
        self.cfg
            .add_edge(self.current_block, resume, CfgEdgeType::Next);
        self.current_block = resume;
        Ok(())
    }

    /// C# `using (var r = ...) { }` → finally-style `r.Dispose()`.
    fn visit_using_statement(&mut self, node: Node, source: &[u8]) -> Result<()> {
        let mut resource_name = "resource".to_string();
        let mut resource_line = node.start_position().row + 1;
        let mut c = node.walk();
        for child in node.children(&mut c) {
            if child.kind() == "variable_declaration" || child.kind() == "declaration" {
                self.add_statement(child, source, StatementKind::Declaration)?;
                resource_line = child.start_position().row + 1;
                // Best-effort: first identifier in the declaration.
                if let Some(id) = find_child_kind(child, "identifier") {
                    if let Ok(t) = id.utf8_text(source) {
                        resource_name = t.trim().to_string();
                    }
                }
            }
        }
        let dispose = vec![DeferredCall {
            text: format!("{resource_name}.Dispose()"),
            line: resource_line,
        }];
        self.finally_stack.push(dispose.clone());

        if let Some(body) = node.child_by_field_name("body") {
            self.visit_block(body, source)?;
        } else if let Some(block) = find_child_kind(node, "block") {
            self.visit_block(block, source)?;
        }

        if self.flow_active {
            for d in &dispose {
                let b = self.new_block();
                self.cfg.add_edge(self.current_block, b, CfgEdgeType::Next);
                self.current_block = b;
                self.add_statement_to_current(Statement {
                    kind: StatementKind::Expression,
                    line: d.line,
                    text: d.text.clone(),
                    defined_vars: SmallVec::new(),
                    used_vars: SmallVec::new(),
                });
            }
        }
        self.finally_stack.pop();
        Ok(())
    }

    /// C# `lock (obj) { }` → finally-style `Monitor.Exit(obj)`.
    fn visit_lock_statement(&mut self, node: Node, source: &[u8]) -> Result<()> {
        let mut obj = "obj".to_string();
        let line = node.start_position().row + 1;
        if let Some(id) = node.named_child(0) {
            if let Ok(t) = id.utf8_text(source) {
                obj = t.trim().to_string();
            }
        }
        let exit_call = vec![DeferredCall {
            text: format!("Monitor.Exit({obj})"),
            line,
        }];
        self.finally_stack.push(exit_call.clone());

        if let Some(body) = node.child_by_field_name("body") {
            self.visit_block(body, source)?;
        } else if let Some(block) = find_child_kind(node, "block") {
            self.visit_block(block, source)?;
        }

        if self.flow_active {
            for d in &exit_call {
                let b = self.new_block();
                self.cfg.add_edge(self.current_block, b, CfgEdgeType::Next);
                self.current_block = b;
                self.add_statement_to_current(Statement {
                    kind: StatementKind::Expression,
                    line: d.line,
                    text: d.text.clone(),
                    defined_vars: SmallVec::new(),
                    used_vars: SmallVec::new(),
                });
            }
        }
        self.finally_stack.pop();
        Ok(())
    }

    /// C# `obj?.Member` — null short-circuit.
    fn visit_conditional_access(&mut self, node: Node, source: &[u8]) -> Result<()> {
        let obj = node
            .child_by_field_name("condition")
            .or_else(|| node.named_child(0))
            .unwrap_or(node);
        let text = obj.utf8_text(source).unwrap_or("obj").trim().to_string();
        self.add_statement_to_current(Statement {
            kind: StatementKind::Branch,
            line: node.start_position().row + 1,
            text: format!("{text}?."),
            defined_vars: SmallVec::new(),
            used_vars: SmallVec::new(),
        });
        let non_null = self.new_block();
        let null_path = self.new_block();
        let merge = self.new_block();
        let from = self.current_block;
        self.cfg.add_edge(from, non_null, CfgEdgeType::IfTrue);
        self.cfg.add_edge(from, null_path, CfgEdgeType::IfFalse);
        // Non-null: continue access (opaque remainder).
        self.flow_active = true;
        self.current_block = non_null;
        self.add_statement(node, source, StatementKind::Expression)?;
        if self.flow_active {
            self.cfg
                .add_edge(self.current_block, merge, CfgEdgeType::Next);
        }
        self.flow_active = true;
        self.current_block = null_path;
        self.cfg
            .add_edge(self.current_block, merge, CfgEdgeType::Next);
        self.flow_active = true;
        self.current_block = merge;
        Ok(())
    }

    /// C# `a ?? b` — evaluate `b` only if `a` is null.
    fn visit_null_coalesce(&mut self, node: Node, source: &[u8]) -> Result<()> {
        let left = node
            .child_by_field_name("left")
            .ok_or_else(|| Error::ParseError {
                file: "source".into(),
                line: node.start_position().row + 1,
                message: "?? missing left".into(),
            })?;
        let right = node
            .child_by_field_name("right")
            .ok_or_else(|| Error::ParseError {
                file: "source".into(),
                line: node.start_position().row + 1,
                message: "?? missing right".into(),
            })?;
        self.visit_expr_for_control_flow(left, source)?;
        if !self.flow_active {
            return Ok(());
        }
        let left_text = left.utf8_text(source).unwrap_or("left").trim().to_string();
        self.add_statement_to_current(Statement {
            kind: StatementKind::Branch,
            line: node.start_position().row + 1,
            text: format!("{left_text} ??"),
            defined_vars: SmallVec::new(),
            used_vars: SmallVec::new(),
        });
        let non_null = self.new_block();
        let null_path = self.new_block();
        let merge = self.new_block();
        let from = self.current_block;
        self.cfg.add_edge(from, non_null, CfgEdgeType::IfTrue);
        self.cfg.add_edge(from, null_path, CfgEdgeType::IfFalse);
        self.flow_active = true;
        self.current_block = non_null;
        self.cfg
            .add_edge(self.current_block, merge, CfgEdgeType::Next);
        self.flow_active = true;
        self.current_block = null_path;
        self.visit_expr_for_control_flow(right, source)?;
        if self.flow_active {
            self.add_statement(right, source, StatementKind::Expression)?;
            if self.flow_active {
                self.cfg
                    .add_edge(self.current_block, merge, CfgEdgeType::Next);
            }
        }
        self.flow_active = true;
        self.current_block = merge;
        Ok(())
    }

    fn visit_macro_invocation(&mut self, node: Node, source: &[u8]) -> Result<()> {
        let diverging = is_diverging_macro(node, source);
        self.add_statement(
            node,
            source,
            if diverging {
                StatementKind::Jump
            } else {
                StatementKind::FunctionCall
            },
        )?;
        if diverging {
            self.unwind_finallies(source)?;
            self.unwind_defers_to_exit(CfgEdgeType::Exception);
        }
        Ok(())
    }

    fn classify_expression(node: Node, _source: &[u8]) -> StatementKind {
        match node.kind() {
            "call_expression" | "function_call" | "method_call" | "invocation_expression" => {
                StatementKind::FunctionCall
            }
            "assignment_expression" | "assignment" | "augmented_assignment" => {
                StatementKind::Assignment
            }
            "let_declaration" | "let_statement" => StatementKind::Declaration,
            "return_expression" | "return_statement" => StatementKind::Return,
            "if_expression" | "if_statement" => StatementKind::Branch,
            _ => StatementKind::Expression,
        }
    }

    fn visit_ruby_begin_block(&mut self, node: Node, source: &[u8]) -> Result<()> {
        let mut rescue_handlers = Vec::new();
        if let Some(begin) = find_child_kind(node, "begin") {
            let mut c = begin.walk();
            for child in begin.children(&mut c) {
                if child.kind() == "rescue" {
                    rescue_handlers.push(self.new_block());
                }
            }
        }
        if !rescue_handlers.is_empty() {
            self.try_catch_stack.push(rescue_handlers);
        }
        self.visit_block(node, source)?;
        if !self.try_catch_stack.is_empty() {
            self.try_catch_stack.pop();
        }
        Ok(())
    }

    fn visit_ruby_modifier_statement(&mut self, node: Node, source: &[u8]) -> Result<()> {
        let body = node.child_by_field_name("body");
        let cond = node.child_by_field_name("condition");
        match node.kind() {
            "if_modifier" => {
                if let Some(cond) = cond {
                    let merge = self.new_block();
                    let then_b = self.new_block();
                    self.wire_condition(cond, source, then_b, merge)?;
                    self.flow_active = true;
                    self.current_block = then_b;
                    if let Some(body) = body {
                        self.visit_statement(body, source)?;
                    }
                    self.flow_active = true;
                    self.current_block = merge;
                } else if let Some(body) = body {
                    self.visit_statement(body, source)?;
                }
            }
            "unless_modifier" => {
                if let Some(cond) = cond {
                    let merge = self.new_block();
                    let then_b = self.new_block();
                    self.wire_condition(cond, source, merge, then_b)?;
                    self.flow_active = true;
                    self.current_block = then_b;
                    if let Some(body) = body {
                        self.visit_statement(body, source)?;
                    }
                    self.flow_active = true;
                    self.current_block = merge;
                } else if let Some(body) = body {
                    self.visit_statement(body, source)?;
                }
            }
            "while_modifier" | "until_modifier" | "rescue_modifier" => {
                // Conservative: record modifier; avoid re-entering the modifier node (stack overflow).
                if let Some(body) = body {
                    self.visit_statement(body, source)?;
                }
                if let Some(cond) = cond {
                    self.add_statement(cond, source, StatementKind::Branch)?;
                }
            }
            _ => {
                if let Some(body) = body {
                    self.visit_statement(body, source)?;
                }
            }
        }
        Ok(())
    }

    fn visit_if(&mut self, node: Node, source: &[u8]) -> Result<()> {
        // Go: `if init; cond` — init runs in the current block before the condition.
        if let Some(init) = node.child_by_field_name("initializer") {
            self.visit_statement(init, source)?;
        }

        // C++17: init lives inside `condition_clause` (`if (auto x = f(); x)`).
        let cond_node = node
            .child_by_field_name("condition")
            .or_else(|| node.child_by_field_name("operand"))
            .or_else(|| {
                // Puppet / field-less grammars: first non-block named child before body.
                if self.language == "puppet" {
                    find_direct_child_kinds(
                        node,
                        &[
                            "expression",
                            "binary_expression",
                            "unary_expression",
                            "variable",
                            "function_call",
                            "parenthesized_expression",
                            "selector",
                            "literal",
                            "boolean",
                            "string",
                            "number",
                        ],
                    )
                } else {
                    None
                }
            });
        let (cxx_init, cond_value) = cond_node
            .map(split_condition_clause)
            .unwrap_or((None, None));
        if let Some(init) = cxx_init {
            self.visit_statement(init, source)?;
        }

        let alternative = node
            .child_by_field_name("alternative")
            .or_else(|| node.child_by_field_name("else"))
            .or_else(|| find_direct_child_kind(node, "else_statement"))
            .or_else(|| find_direct_child_kind(node, "elsif_statement"))
            .or_else(|| node.child_by_field_name("else_clause"));
        let has_else = alternative.is_some();

        let cond_block = if self.can_reuse_current_as_cond() {
            self.current_block
        } else {
            let cond_block = self.new_block();
            self.cfg
                .add_edge(self.current_block, cond_block, CfgEdgeType::Next);
            cond_block
        };
        self.current_block = cond_block;

        let true_block = self.new_block();
        let merge = self.new_block();
        let false_block = if has_else {
            Some(self.new_block())
        } else {
            None
        };
        let false_target = false_block.unwrap_or(merge);

        if let Some(cond) = cond_value.or(cond_node) {
            self.wire_condition(cond, source, true_block, false_target)?;
        } else {
            self.add_statement_to_current(Statement {
                kind: StatementKind::Branch,
                line: node.start_position().row + 1,
                text: "if".to_string(),
                defined_vars: SmallVec::new(),
                used_vars: SmallVec::new(),
            });
            self.cfg
                .add_edge(cond_block, true_block, CfgEdgeType::IfTrue);
            self.cfg
                .add_edge(cond_block, false_target, CfgEdgeType::IfFalse);
        }

        let true_end;
        let true_reaches;
        self.flow_active = true;
        self.current_block = true_block;
        if let Some(consequence) = node
            .child_by_field_name("consequence")
            .or_else(|| node.child_by_field_name("body"))
            .or_else(|| find_direct_child_kind(node, "block"))
        {
            self.visit_block(consequence, source)?;
        }
        true_end = self.current_block;
        true_reaches = self.flow_active;

        let mut false_end = false_target;
        let false_reaches;
        if let Some(false_block) = false_block {
            self.flow_active = true;
            self.current_block = false_block;
            if let Some(alternative) = alternative {
                let alt = if alternative.kind() == "else_clause"
                    || alternative.kind() == "else_statement"
                {
                    find_child_kind(alternative, "block").unwrap_or(alternative)
                } else if alternative.kind() == "if_expression"
                    || alternative.kind() == "if_statement"
                    || alternative.kind() == "elsif_statement"
                {
                    // `else if` / Puppet elsif — visit as nested if-like.
                    alternative
                } else {
                    alternative
                };
                if alt.kind() == "if_expression"
                    || alt.kind() == "if_statement"
                    || alt.kind() == "elsif_statement"
                {
                    self.visit_if(alt, source)?;
                } else {
                    self.visit_block(alt, source)?;
                }
                false_end = self.current_block;
                false_reaches = self.flow_active;
            } else {
                false_reaches = true;
            }
        } else {
            // IfFalse already targets merge — no phantom false trampoline.
            false_reaches = false;
        }

        if true_reaches {
            self.cfg.add_edge(true_end, merge, CfgEdgeType::Next);
        }
        if false_reaches {
            self.cfg.add_edge(false_end, merge, CfgEdgeType::Next);
        }
        // No-else fallthrough reaches merge via IfFalse; keep flow active like before.
        self.flow_active = true_reaches || false_reaches || !has_else;
        self.current_block = merge;
        Ok(())
    }

    /// Puppet `unless` — inverted if (condition false → body).
    fn visit_puppet_unless(&mut self, node: Node, source: &[u8]) -> Result<()> {
        let cond = find_direct_child_kinds(
            node,
            &[
                "expression",
                "binary_expression",
                "unary_expression",
                "variable",
                "function_call",
                "parenthesized_expression",
                "boolean",
            ],
        );
        let body = find_direct_child_kind(node, "block");
        let cond_block = if self.can_reuse_current_as_cond() {
            self.current_block
        } else {
            let cond_block = self.new_block();
            self.cfg
                .add_edge(self.current_block, cond_block, CfgEdgeType::Next);
            cond_block
        };
        self.current_block = cond_block;
        let body_block = self.new_block();
        let merge = self.new_block();
        if let Some(cond) = cond {
            // unless: skip body when condition is true (IfTrue → merge);
            // run body when condition is false (IfFalse → body_block).
            self.wire_condition(cond, source, merge, body_block)?;
        } else {
            self.cfg
                .add_edge(cond_block, merge, CfgEdgeType::IfTrue);
            self.cfg
                .add_edge(cond_block, body_block, CfgEdgeType::IfFalse);
        }
        self.flow_active = true;
        self.current_block = body_block;
        if let Some(body) = body {
            self.visit_block(body, source)?;
        }
        if self.flow_active {
            self.cfg
                .add_edge(self.current_block, merge, CfgEdgeType::Next);
        }
        self.flow_active = true;
        self.current_block = merge;
        Ok(())
    }

    /// Puppet `case` — multi-way branch over case_item / default_case.
    fn visit_puppet_case(&mut self, node: Node, source: &[u8]) -> Result<()> {
        let header = self.new_block();
        self.cfg
            .add_edge(self.current_block, header, CfgEdgeType::Next);
        self.current_block = header;
        if let Some(expr) = find_direct_child_kinds(
            node,
            &[
                "expression",
                "variable",
                "function_call",
                "string",
                "identifier",
                "class_identifier",
            ],
        ) {
            self.visit_expr_for_control_flow(expr, source)?;
            self.add_statement(expr, source, StatementKind::Branch)?;
        }
        let merge = self.new_block();
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "case_item" || child.kind() == "default_case" {
                let arm = self.new_block();
                self.cfg.add_edge(header, arm, CfgEdgeType::IfTrue);
                self.flow_active = true;
                self.current_block = arm;
                if let Some(block) = find_direct_child_kind(child, "block") {
                    self.visit_block(block, source)?;
                } else {
                    self.visit_block(child, source)?;
                }
                if self.flow_active {
                    self.cfg
                        .add_edge(self.current_block, merge, CfgEdgeType::Next);
                }
            }
        }
        self.flow_active = true;
        self.current_block = merge;
        Ok(())
    }

    fn visit_while(&mut self, node: Node, source: &[u8]) -> Result<()> {
        self.capture_embedded_loop_label(node, source);
        let header = self.new_block();
        self.cfg
            .add_edge(self.current_block, header, CfgEdgeType::Next);
        self.current_block = header;

        let body = self.new_block();
        let exit = self.new_block();
        if let Some(cond) = node.child_by_field_name("condition") {
            self.wire_condition(cond, source, body, exit)?;
        } else {
            self.add_statement_to_current(Statement {
                kind: StatementKind::Branch,
                line: node.start_position().row + 1,
                text: "while".to_string(),
                defined_vars: SmallVec::new(),
                used_vars: SmallVec::new(),
            });
            self.cfg.add_edge(header, body, CfgEdgeType::IfTrue);
            self.cfg.add_edge(header, exit, CfgEdgeType::IfFalse);
        }

        self.breakable_stack.push(BreakableContext {
            exit,
            continue_target: Some(header),
            label: self.pending_breakable_label.take(),
        });

        self.flow_active = true;
        self.current_block = body;
        if let Some(body_node) = node.child_by_field_name("body") {
            self.visit_block(body_node, source)?;
        }
        if self.flow_active {
            self.cfg
                .add_edge(self.current_block, header, CfgEdgeType::Jump);
        }
        self.breakable_stack.pop();

        self.flow_active = true;
        self.current_block = exit;
        Ok(())
    }

    fn visit_do(&mut self, node: Node, source: &[u8]) -> Result<()> {
        let body = self.new_block();
        self.cfg
            .add_edge(self.current_block, body, CfgEdgeType::Next);

        let header = self.new_block();
        let exit = self.new_block();

        self.breakable_stack.push(BreakableContext {
            exit,
            continue_target: Some(header),
            label: self.pending_breakable_label.take(),
        });

        self.flow_active = true;
        self.current_block = body;
        if let Some(body_node) = node.child_by_field_name("body") {
            self.visit_block(body_node, source)?;
        }
        if self.flow_active {
            self.cfg
                .add_edge(self.current_block, header, CfgEdgeType::Next);
        }

        self.flow_active = true;
        self.current_block = header;
        if let Some(cond) = node.child_by_field_name("condition") {
            self.wire_condition(cond, source, body, exit)?;
        } else {
            self.add_statement_to_current(Statement {
                kind: StatementKind::Branch,
                line: node.start_position().row + 1,
                text: "do".to_string(),
                defined_vars: SmallVec::new(),
                used_vars: SmallVec::new(),
            });
            self.cfg.add_edge(header, body, CfgEdgeType::IfTrue);
            self.cfg.add_edge(header, exit, CfgEdgeType::IfFalse);
        }
        self.breakable_stack.pop();

        self.flow_active = true;
        self.current_block = exit;
        Ok(())
    }

    fn visit_for(&mut self, node: Node, source: &[u8]) -> Result<()> {
        self.capture_embedded_loop_label(node, source);

        // C# foreach / C++ range-for
        if node.kind() == "foreach_statement" {
            return self.visit_foreach(node, source);
        }
        if node.kind() == "for_range_loop" {
            return self.visit_for_range(node, source);
        }

        // Rust `for pat in iter` / similar for-in forms.
        if node.child_by_field_name("value").is_some()
            && node.child_by_field_name("pattern").is_some()
            && node.kind() == "for_expression"
        {
            return self.visit_for_in(node, source);
        }

        // Go: for_clause / range_clause. Java/C-like: fields init/condition/update.
        let mut for_clause = None;
        let mut range_clause = None;
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            match child.kind() {
                "for_clause" => for_clause = Some(child),
                "range_clause" => range_clause = Some(child),
                _ => {}
            }
        }

        // Init
        if let Some(clause) = for_clause {
            if let Some(init) = clause
                .child_by_field_name("initializer")
                .or_else(|| clause.child_by_field_name("init"))
            {
                self.visit_statement(init, source)?;
            }
        } else if let Some(init) = node
            .child_by_field_name("init")
            .or_else(|| node.child_by_field_name("initializer"))
        {
            self.visit_statement(init, source)?;
        } else if let Some(range) = range_clause {
            self.add_statement(range, source, StatementKind::Expression)?;
        }

        let header = self.new_block();
        self.cfg
            .add_edge(self.current_block, header, CfgEdgeType::Next);
        self.current_block = header;

        let body = self.new_block();
        let exit = self.new_block();

        // Condition — prefer explicit fields; Go while-style bare expr only if no init/update fields.
        let cond_node = for_clause
            .and_then(|c| c.child_by_field_name("condition"))
            .or_else(|| node.child_by_field_name("condition"))
            .or_else(|| {
                // Go `for cond {}` — single expression child, not a declaration/update.
                if for_clause.is_some() || range_clause.is_some() {
                    return None;
                }
                if node.child_by_field_name("init").is_some()
                    || node.child_by_field_name("update").is_some()
                {
                    return None;
                }
                let mut c = node.walk();
                for child in node.children(&mut c) {
                    if child.is_named()
                        && child.kind() != "block"
                        && !is_block_like(child.kind())
                        && !matches!(
                            child.kind(),
                            "local_variable_declaration"
                                | "update_expression"
                                | "assignment_expression"
                        )
                    {
                        return Some(child);
                    }
                }
                None
            });

        if let Some(cond) = cond_node {
            self.wire_condition(cond, source, body, exit)?;
        } else {
            let cond_text = if let Some(range) = range_clause {
                range
                    .utf8_text(source)
                    .ok()
                    .map(|s| s.trim().to_string())
                    .unwrap_or_else(|| "range".to_string())
            } else {
                "true".to_string()
            };
            self.add_statement_to_current(Statement {
                kind: StatementKind::Branch,
                line: node.start_position().row + 1,
                text: cond_text,
                defined_vars: SmallVec::new(),
                used_vars: SmallVec::new(),
            });
            self.cfg.add_edge(header, body, CfgEdgeType::IfTrue);
            self.cfg.add_edge(header, exit, CfgEdgeType::IfFalse);
        }

        // Updates (Go for_clause.update or Java field "update" — possibly multiple).
        let mut update_nodes: Vec<Node> = Vec::new();
        if let Some(clause) = for_clause {
            if let Some(u) = clause.child_by_field_name("update") {
                update_nodes.push(u);
            }
        } else {
            let mut c = node.walk();
            let mut idx = 0u32;
            for child in node.children(&mut c) {
                if node.field_name_for_child(idx) == Some("update") {
                    update_nodes.push(child);
                }
                idx += 1;
            }
        }

        let update_block = if update_nodes.is_empty() {
            None
        } else {
            Some(self.new_block())
        };
        let continue_target = update_block.unwrap_or(header);
        self.breakable_stack.push(BreakableContext {
            exit,
            continue_target: Some(continue_target),
            label: self.pending_breakable_label.take(),
        });

        self.flow_active = true;
        self.current_block = body;
        if let Some(body_node) = node.child_by_field_name("body") {
            self.visit_block(body_node, source)?;
        }
        if self.flow_active {
            self.cfg
                .add_edge(self.current_block, continue_target, CfgEdgeType::Jump);
        }

        if let Some(update_id) = update_block {
            self.flow_active = true;
            self.current_block = update_id;
            for u in update_nodes {
                self.visit_statement(u, source)?;
            }
            if self.flow_active {
                self.cfg
                    .add_edge(self.current_block, header, CfgEdgeType::Next);
            }
        }

        self.breakable_stack.pop();
        self.flow_active = true;
        self.current_block = exit;
        Ok(())
    }

    /// C# `foreach (var v in xs)`
    fn visit_foreach(&mut self, node: Node, source: &[u8]) -> Result<()> {
        if let Some(right) = node.child_by_field_name("right") {
            self.add_statement(right, source, StatementKind::Expression)?;
        }
        let header = self.new_block();
        self.cfg
            .add_edge(self.current_block, header, CfgEdgeType::Next);
        self.current_block = header;
        let header_text = {
            let left = node
                .child_by_field_name("left")
                .and_then(|l| l.utf8_text(source).ok())
                .unwrap_or("v")
                .trim();
            let right = node
                .child_by_field_name("right")
                .and_then(|r| r.utf8_text(source).ok())
                .unwrap_or("xs")
                .trim();
            // Avoid opaque `foreach ...` blobs; tests treat those as unlowered.
            format!("for-each {left} in {right}")
        };
        self.add_statement_to_current(Statement {
            kind: StatementKind::Branch,
            line: node.start_position().row + 1,
            text: header_text,
            defined_vars: SmallVec::new(),
            used_vars: SmallVec::new(),
        });
        let body = self.new_block();
        let exit = self.new_block();
        self.cfg.add_edge(header, body, CfgEdgeType::IfTrue);
        self.cfg.add_edge(header, exit, CfgEdgeType::IfFalse);
        self.breakable_stack.push(BreakableContext {
            exit,
            continue_target: Some(header),
            label: self.pending_breakable_label.take(),
        });
        self.flow_active = true;
        self.current_block = body;
        if let Some(body_node) = node.child_by_field_name("body") {
            self.visit_block(body_node, source)?;
        }
        if self.flow_active {
            self.cfg
                .add_edge(self.current_block, header, CfgEdgeType::Jump);
        }
        self.breakable_stack.pop();
        self.flow_active = true;
        self.current_block = exit;
        Ok(())
    }

    /// C++ `for (T v : range) { body }`
    fn visit_for_range(&mut self, node: Node, source: &[u8]) -> Result<()> {
        if let Some(right) = node.child_by_field_name("right") {
            self.add_statement(right, source, StatementKind::Expression)?;
        }
        let header = self.new_block();
        self.cfg
            .add_edge(self.current_block, header, CfgEdgeType::Next);
        self.current_block = header;
        let header_text = {
            let decl = node
                .child_by_field_name("declarator")
                .and_then(|d| d.utf8_text(source).ok())
                .unwrap_or("v")
                .trim();
            let range = node
                .child_by_field_name("right")
                .and_then(|r| r.utf8_text(source).ok())
                .unwrap_or("range")
                .trim();
            format!("for-range {decl} : {range}")
        };
        self.add_statement_to_current(Statement {
            kind: StatementKind::Branch,
            line: node.start_position().row + 1,
            text: header_text,
            defined_vars: SmallVec::new(),
            used_vars: SmallVec::new(),
        });
        let body = self.new_block();
        let exit = self.new_block();
        self.cfg.add_edge(header, body, CfgEdgeType::IfTrue);
        self.cfg.add_edge(header, exit, CfgEdgeType::IfFalse);
        self.breakable_stack.push(BreakableContext {
            exit,
            continue_target: Some(header),
            label: self.pending_breakable_label.take(),
        });
        self.flow_active = true;
        self.current_block = body;
        if let Some(body_node) = node.child_by_field_name("body") {
            self.visit_block(body_node, source)?;
        }
        if self.flow_active {
            self.cfg
                .add_edge(self.current_block, header, CfgEdgeType::Jump);
        }
        self.breakable_stack.pop();
        self.flow_active = true;
        self.current_block = exit;
        Ok(())
    }

    /// Rust / for-in: `for pat in iter { body }` — iter once, then next/body cycle.
    fn visit_for_in(&mut self, node: Node, source: &[u8]) -> Result<()> {
        if let Some(value) = node.child_by_field_name("value") {
            self.add_statement(value, source, StatementKind::Expression)?;
        }
        let header = self.new_block();
        self.cfg
            .add_edge(self.current_block, header, CfgEdgeType::Next);
        self.current_block = header;
        let header_text = node
            .child_by_field_name("pattern")
            .and_then(|p| p.utf8_text(source).ok())
            .map(|p| {
                let iter = node
                    .child_by_field_name("value")
                    .and_then(|v| v.utf8_text(source).ok())
                    .unwrap_or("iter")
                    .trim();
                format!("for {p} in {iter}")
            })
            .unwrap_or_else(|| "for-in".to_string());
        self.add_statement_to_current(Statement {
            kind: StatementKind::Branch,
            line: node.start_position().row + 1,
            text: header_text,
            defined_vars: SmallVec::new(),
            used_vars: SmallVec::new(),
        });
        let body = self.new_block();
        let exit = self.new_block();
        self.cfg.add_edge(header, body, CfgEdgeType::IfTrue);
        self.cfg.add_edge(header, exit, CfgEdgeType::IfFalse);
        self.breakable_stack.push(BreakableContext {
            exit,
            continue_target: Some(header),
            label: self.pending_breakable_label.take(),
        });
        self.flow_active = true;
        self.current_block = body;
        if let Some(body_node) = node.child_by_field_name("body") {
            self.visit_block(body_node, source)?;
        }
        if self.flow_active {
            self.cfg
                .add_edge(self.current_block, header, CfgEdgeType::Jump);
        }
        self.breakable_stack.pop();
        self.flow_active = true;
        self.current_block = exit;
        Ok(())
    }

    fn visit_enhanced_for(&mut self, node: Node, source: &[u8]) -> Result<()> {
        if let Some(value) = node.child_by_field_name("value") {
            self.add_statement(value, source, StatementKind::Expression)?;
        }
        let header = self.new_block();
        self.cfg
            .add_edge(self.current_block, header, CfgEdgeType::Next);
        self.current_block = header;
        let header_text = node
            .child_by_field_name("value")
            .and_then(|v| v.utf8_text(source).ok())
            .map(|s| format!("for-each {s}"))
            .unwrap_or_else(|| "for-each".to_string());
        self.add_statement_to_current(Statement {
            kind: StatementKind::Branch,
            line: node.start_position().row + 1,
            text: header_text,
            defined_vars: SmallVec::new(),
            used_vars: SmallVec::new(),
        });
        let body = self.new_block();
        let exit = self.new_block();
        self.cfg.add_edge(header, body, CfgEdgeType::IfTrue);
        self.cfg.add_edge(header, exit, CfgEdgeType::IfFalse);
        self.breakable_stack.push(BreakableContext {
            exit,
            continue_target: Some(header),
            label: self.pending_breakable_label.take(),
        });
        self.flow_active = true;
        self.current_block = body;
        if let Some(body_node) = node.child_by_field_name("body") {
            self.visit_block(body_node, source)?;
        }
        if self.flow_active {
            self.cfg
                .add_edge(self.current_block, header, CfgEdgeType::Jump);
        }
        self.breakable_stack.pop();
        self.flow_active = true;
        self.current_block = exit;
        Ok(())
    }

    fn visit_loop(&mut self, node: Node, source: &[u8]) -> Result<()> {
        self.capture_embedded_loop_label(node, source);
        // Infinite loop: no IfFalse exit — only break/return/panic leave.
        let body = self.new_block();
        self.cfg
            .add_edge(self.current_block, body, CfgEdgeType::Next);
        let exit = self.new_block();

        self.breakable_stack.push(BreakableContext {
            exit,
            continue_target: Some(body),
            label: self.pending_breakable_label.take(),
        });

        self.flow_active = true;
        self.current_block = body;
        if let Some(body_node) = node.child_by_field_name("body") {
            self.visit_block(body_node, source)?;
        }
        if self.flow_active {
            self.cfg
                .add_edge(self.current_block, body, CfgEdgeType::Jump);
        }
        self.breakable_stack.pop();

        self.flow_active = true;
        self.current_block = exit;
        Ok(())
    }

    fn visit_return(&mut self, node: Node, source: &[u8]) -> Result<()> {
        // Java: `return switch (...) { ... };` — lower the switch CFG, then exit.
        // Kotlin: `return when (...) { ... }`
        if let Some(sw) = {
            let mut found = None;
            let mut c = node.walk();
            for ch in node.children(&mut c) {
                if matches!(ch.kind(), "switch_expression" | "when_expression") {
                    found = Some(ch);
                    break;
                }
            }
            found
        } {
            if sw.kind() == "when_expression" {
                self.visit_kotlin_when(sw, source)?;
            } else {
                self.visit_switch_expression(sw, source)?;
            }
            if !self.flow_active {
                return Ok(());
            }
            self.add_statement_to_current(Statement {
                kind: StatementKind::Return,
                line: node.start_position().row + 1,
                text: "return".to_string(),
                defined_vars: SmallVec::new(),
                used_vars: SmallVec::new(),
            });
            self.unwind_finallies(source)?;
            self.unwind_defers_to_exit(CfgEdgeType::Return);
            return Ok(());
        }

        // C/C++: `return cond ? a : b;`
        if let Some(tern) = {
            let mut found = None;
            let mut c = node.walk();
            for ch in node.children(&mut c) {
                if ch.kind() == "conditional_expression" {
                    found = Some(ch);
                    break;
                }
            }
            found
        } {
            self.visit_conditional_expression(tern, source)?;
            if !self.flow_active {
                return Ok(());
            }
            self.add_statement_to_current(Statement {
                kind: StatementKind::Return,
                line: node.start_position().row + 1,
                text: "return".to_string(),
                defined_vars: SmallVec::new(),
                used_vars: SmallVec::new(),
            });
            self.unwind_finallies(source)?;
            self.unwind_defers_to_exit(CfgEdgeType::Return);
            return Ok(());
        }

        // C#: `return a ?? b;` / `return d?.M();` / nested await — lower CF first.
        {
            let mut c = node.walk();
            for ch in node.children(&mut c) {
                if !ch.is_named() {
                    continue;
                }
                if matches!(ch.kind(), "switch_expression" | "conditional_expression") {
                    continue;
                }
                self.visit_expr_for_control_flow(ch, source)?;
                if !self.flow_active {
                    return Ok(());
                }
            }
        }

        self.add_statement(node, source, StatementKind::Return)?;
        self.unwind_finallies(source)?;
        self.unwind_defers_to_exit(CfgEdgeType::Return);
        Ok(())
    }

    fn visit_throw(&mut self, node: Node, source: &[u8]) -> Result<()> {
        self.add_statement(node, source, StatementKind::Jump)?;
        // Inside try: route to catch entries (finally runs when leaving via catch/completion).
        if let Some(handlers) = self.try_catch_stack.last() {
            let handlers = handlers.clone();
            for h in handlers {
                self.cfg
                    .add_edge(self.current_block, h, CfgEdgeType::Exception);
            }
            self.flow_active = false;
            self.current_block = self.new_block();
            return Ok(());
        }
        self.unwind_finallies(source)?;
        self.unwind_defers_to_exit(CfgEdgeType::Exception);
        Ok(())
    }

    /// Emit enclosing `finally` bodies (outermost last) before a terminal exit.
    fn unwind_finallies(&mut self, _source: &[u8]) -> Result<()> {
        let frames: Vec<Vec<DeferredCall>> = self.finally_stack.iter().rev().cloned().collect();
        for stmts in frames {
            for d in stmts {
                let b = self.new_block();
                self.cfg.add_edge(self.current_block, b, CfgEdgeType::Next);
                self.current_block = b;
                self.add_statement_to_current(Statement {
                    kind: StatementKind::Expression,
                    line: d.line,
                    text: d.text,
                    defined_vars: SmallVec::new(),
                    used_vars: SmallVec::new(),
                });
            }
        }
        Ok(())
    }

    fn visit_defer(&mut self, node: Node, source: &[u8]) -> Result<()> {
        self.add_statement(node, source, StatementKind::Expression)?;
        let deferred = node
            .named_child(0)
            .and_then(|n| n.utf8_text(source).ok())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| {
                node.utf8_text(source)
                    .unwrap_or("defer")
                    .trim()
                    .trim_start_matches("defer")
                    .trim()
                    .to_string()
            });
        self.defer_stack.push(DeferredCall {
            text: deferred,
            line: node.start_position().row + 1,
        });
        Ok(())
    }

    /// Walk declarator initializers for nested control-flow (`await`, `??`, `?.`, …).
    fn visit_declaration_initializers(&mut self, node: Node, source: &[u8]) -> Result<()> {
        let mut stack = vec![node];
        while let Some(n) = stack.pop() {
            if n.kind() == "variable_declarator" {
                if let Some(value) = n.child_by_field_name("value") {
                    self.visit_expr_for_control_flow(value, source)?;
                } else {
                    // C#: initializer is an unfielded named child after `name`.
                    let name = n.child_by_field_name("name");
                    let mut c = n.walk();
                    for ch in n.children(&mut c) {
                        if !ch.is_named() {
                            continue;
                        }
                        if name.is_some_and(|nm| nm.id() == ch.id()) {
                            continue;
                        }
                        self.visit_expr_for_control_flow(ch, source)?;
                        if !self.flow_active {
                            return Ok(());
                        }
                    }
                }
                if !self.flow_active {
                    return Ok(());
                }
                continue;
            }
            let mut c = n.walk();
            for ch in n.children(&mut c) {
                if ch.is_named() {
                    stack.push(ch);
                }
            }
        }
        Ok(())
    }

    fn visit_using_declaration(&mut self, node: Node, source: &[u8]) -> Result<()> {
        self.visit_declaration_initializers(node, source)?;
        if !self.flow_active {
            return Ok(());
        }
        self.add_statement(node, source, StatementKind::Declaration)?;
        let mut resource_name = "resource".to_string();
        let line = node.start_position().row + 1;
        if let Some(decl) = find_child_kind(node, "variable_declaration")
            .or_else(|| find_child_kind(node, "variable_declarator"))
        {
            if let Some(id) = decl
                .child_by_field_name("name")
                .or_else(|| find_child_kind(decl, "identifier"))
            {
                if let Ok(t) = id.utf8_text(source) {
                    resource_name = t.trim().to_string();
                }
            }
        } else if let Some(id) = find_child_kind(node, "identifier") {
            if let Ok(t) = id.utf8_text(source) {
                resource_name = t.trim().to_string();
            }
        }
        self.finally_stack.push(vec![DeferredCall {
            text: format!("{resource_name}.Dispose()"),
            line,
        }]);
        Ok(())
    }

    fn close_using_declarations(&mut self, count: usize) -> Result<()> {
        for _ in 0..count {
            let Some(dispose) = self.finally_stack.pop() else {
                break;
            };
            if self.flow_active {
                for d in &dispose {
                    let b = self.new_block();
                    self.cfg.add_edge(self.current_block, b, CfgEdgeType::Next);
                    self.current_block = b;
                    self.add_statement_to_current(Statement {
                        kind: StatementKind::Expression,
                        line: d.line,
                        text: d.text.clone(),
                        defined_vars: SmallVec::new(),
                        used_vars: SmallVec::new(),
                    });
                }
            }
        }
        Ok(())
    }

    /// Local function / lambda / anonymous method as a disconnected sub-CFG.
    fn visit_nested_subcfg(&mut self, node: Node, source: &[u8]) -> Result<()> {
        let kind = if node.kind() == "local_function_statement" {
            StatementKind::Declaration
        } else {
            StatementKind::Expression
        };
        // Parent records the definition only — body is not sequential parent flow.
        let parent_text = match node.kind() {
            "local_function_statement" => {
                let name = node
                    .child_by_field_name("name")
                    .and_then(|n| n.utf8_text(source).ok())
                    .unwrap_or("local")
                    .trim();
                format!("local function {name}")
            }
            "anonymous_method_expression" => "anonymous method".to_string(),
            _ => "lambda".to_string(),
        };
        self.add_statement_to_current(Statement {
            kind,
            line: node.start_position().row + 1,
            text: parent_text,
            defined_vars: SmallVec::new(),
            used_vars: SmallVec::new(),
        });

        let body = node
            .child_by_field_name("body")
            .or_else(|| find_child_kind(node, "block"))
            .or_else(|| {
                // Expression-bodied local function / lambda.
                node.child_by_field_name("value").or_else(|| {
                    let mut last = None;
                    let mut c = node.walk();
                    for ch in node.children(&mut c) {
                        if ch.is_named()
                            && !matches!(
                                ch.kind(),
                                "parameter_list"
                                    | "implicit_parameter"
                                    | "identifier"
                                    | "predefined_type"
                                    | "generic_name"
                                    | "modifier"
                                    | "attribute_list"
                            )
                        {
                            last = Some(ch);
                        }
                    }
                    last
                })
            });

        let Some(body) = body else {
            return Ok(());
        };

        // Split so the definition block ends before parent continuation; Jump keeps
        // the sub-CFG reachable for pruning without making it sequential parent flow.
        let def_block = self.current_block;
        let continue_block = self.new_block();
        if self.flow_active {
            self.cfg
                .add_edge(def_block, continue_block, CfgEdgeType::Next);
        }

        let parent_flow = self.flow_active;
        let parent_breakable = std::mem::take(&mut self.breakable_stack);
        let parent_pending_label = self.pending_breakable_label.take();
        let parent_labels = std::mem::take(&mut self.label_blocks);
        let parent_fallthrough = self.pending_fallthrough;
        let parent_switch_ft = self.switch_implicit_fallthrough;
        let parent_defer = std::mem::take(&mut self.defer_stack);
        let parent_finally = std::mem::take(&mut self.finally_stack);
        let parent_try = std::mem::take(&mut self.try_catch_stack);
        let parent_switch_cases = std::mem::take(&mut self.switch_case_labels);
        let parent_setjmp = std::mem::take(&mut self.setjmp_sites);

        let sub_entry = self.new_block();
        let sub_exit = self.new_block();
        self.cfg.add_edge(def_block, sub_entry, CfgEdgeType::Jump);
        self.nested_exits.push(sub_exit);
        self.current_block = sub_entry;
        self.flow_active = true;
        self.pending_fallthrough = false;
        self.switch_implicit_fallthrough = false;

        if is_block_like(body.kind()) {
            self.visit_block(body, source)?;
        } else {
            self.visit_expr_for_control_flow(body, source)?;
            if self.flow_active {
                self.add_statement(body, source, StatementKind::Expression)?;
            }
            if self.flow_active {
                self.cfg
                    .add_edge(self.current_block, sub_exit, CfgEdgeType::Return);
                self.flow_active = false;
            }
        }
        if self.flow_active {
            self.cfg
                .add_edge(self.current_block, sub_exit, CfgEdgeType::Next);
        }
        self.nested_exits.pop();

        self.current_block = continue_block;
        self.flow_active = parent_flow;
        self.breakable_stack = parent_breakable;
        self.pending_breakable_label = parent_pending_label;
        self.label_blocks = parent_labels;
        self.pending_fallthrough = parent_fallthrough;
        self.switch_implicit_fallthrough = parent_switch_ft;
        self.defer_stack = parent_defer;
        self.finally_stack = parent_finally;
        self.try_catch_stack = parent_try;
        self.switch_case_labels = parent_switch_cases;
        self.setjmp_sites = parent_setjmp;
        Ok(())
    }

    /// Route current block through deferred calls (LIFO) then a terminal exit edge.
    fn unwind_defers_to_exit(&mut self, terminal: CfgEdgeType) {
        let deferred: Vec<_> = self.defer_stack.iter().rev().cloned().collect();
        let mut prev = self.current_block;
        for d in deferred {
            let b = self.new_block();
            self.cfg.add_edge(prev, b, CfgEdgeType::Next);
            self.current_block = b;
            self.add_statement_to_current(Statement {
                kind: StatementKind::FunctionCall,
                line: d.line,
                text: d.text,
                defined_vars: SmallVec::new(),
                used_vars: SmallVec::new(),
            });
            prev = b;
        }
        if let Some(&nested_exit) = self.nested_exits.last() {
            self.cfg.add_edge(prev, nested_exit, terminal);
            self.flow_active = false;
            return;
        }
        let exit = self.new_block();
        self.cfg.add_edge(prev, exit, terminal);
        self.cfg.exits.push(exit);
        self.flow_active = false;
    }

    fn jump_label_of(node: Node, source: &[u8]) -> Option<String> {
        let mut c = node.walk();
        for ch in node.children(&mut c) {
            if ch.kind() == "label_name" || ch.kind() == "label" {
                return ch
                    .utf8_text(source)
                    .ok()
                    .map(|s| s.trim().trim_start_matches('\'').to_string());
            }
        }
        // `break Outer` — named child may be the label
        if let Some(n) = node.named_child(0) {
            if n.kind() == "label_name" || n.kind() == "label" || n.kind() == "identifier" {
                return n
                    .utf8_text(source)
                    .ok()
                    .map(|s| s.trim().trim_start_matches('\'').to_string());
            }
        }
        None
    }

    /// Rust embeds `'label:` on the loop node itself (not `labeled_statement`).
    fn capture_embedded_loop_label(&mut self, node: Node, source: &[u8]) {
        if self.pending_breakable_label.is_some() {
            return;
        }
        let mut c = node.walk();
        for ch in node.children(&mut c) {
            if ch.kind() == "label" {
                if let Ok(t) = ch.utf8_text(source) {
                    let name = t.trim().trim_start_matches('\'').to_string();
                    if !name.is_empty() {
                        self.pending_breakable_label = Some(name);
                    }
                }
                break;
            }
        }
    }

    fn find_breakable(&self, label: Option<&str>) -> Option<&BreakableContext> {
        match label {
            Some(l) => self
                .breakable_stack
                .iter()
                .rev()
                .find(|c| c.label.as_deref() == Some(l)),
            None => self.breakable_stack.last(),
        }
    }

    fn visit_break(&mut self, node: Node, source: &[u8]) -> Result<()> {
        self.add_statement(node, source, StatementKind::Jump)?;
        let label = Self::jump_label_of(node, source);
        if let Some(ctx) = self.find_breakable(label.as_deref()) {
            let exit = ctx.exit;
            self.cfg
                .add_edge(self.current_block, exit, CfgEdgeType::Jump);
        }
        self.flow_active = false;
        self.current_block = self.new_block();
        Ok(())
    }

    fn visit_continue(&mut self, node: Node, source: &[u8]) -> Result<()> {
        self.add_statement(node, source, StatementKind::Jump)?;
        let label = Self::jump_label_of(node, source);
        if let Some(ctx) = self.find_breakable(label.as_deref()) {
            if let Some(cont) = ctx.continue_target {
                self.cfg
                    .add_edge(self.current_block, cont, CfgEdgeType::Jump);
            }
        }
        self.flow_active = false;
        self.current_block = self.new_block();
        Ok(())
    }

    fn ensure_label_block(&mut self, name: &str) -> BlockId {
        if let Some(id) = self.label_blocks.get(name) {
            return *id;
        }
        let id = self.new_block();
        self.label_blocks.insert(name.to_string(), id);
        id
    }

    fn visit_goto(&mut self, node: Node, source: &[u8]) -> Result<()> {
        self.add_statement(node, source, StatementKind::Jump)?;
        let text = node.utf8_text(source).unwrap_or("").trim().to_string();

        // C# `goto case X;` / `goto default;`
        if text.starts_with("goto default") {
            if let Some(target) = self
                .switch_case_labels
                .last()
                .and_then(|m| m.get("default").copied())
            {
                self.cfg
                    .add_edge(self.current_block, target, CfgEdgeType::Jump);
            }
            self.flow_active = false;
            self.current_block = self.new_block();
            return Ok(());
        }
        if text.starts_with("goto case") {
            let key = goto_case_key(node, source);
            if let Some(target) = key.and_then(|k| {
                self.switch_case_labels
                    .last()
                    .and_then(|m| m.get(&k).copied())
            }) {
                self.cfg
                    .add_edge(self.current_block, target, CfgEdgeType::Jump);
            }
            self.flow_active = false;
            self.current_block = self.new_block();
            return Ok(());
        }

        let label = node
            .child_by_field_name("label")
            .or_else(|| node.named_child(0))
            .and_then(|n| n.utf8_text(source).ok())
            .map(|s| s.trim().trim_start_matches('\'').to_string())
            .filter(|s| !s.is_empty())
            .or_else(|| {
                let mut c = node.walk();
                for ch in node.children(&mut c) {
                    if matches!(
                        ch.kind(),
                        "label_name" | "label" | "statement_identifier" | "identifier"
                    ) {
                        return ch
                            .utf8_text(source)
                            .ok()
                            .map(|s| s.trim().trim_start_matches('\'').to_string());
                    }
                }
                None
            })
            .unwrap_or_default();
        if !label.is_empty() {
            let target = self.ensure_label_block(&label);
            self.cfg
                .add_edge(self.current_block, target, CfgEdgeType::Jump);
        }
        self.flow_active = false;
        self.current_block = self.new_block();
        Ok(())
    }

    fn visit_fallthrough(&mut self, node: Node, source: &[u8]) -> Result<()> {
        self.add_statement(node, source, StatementKind::Jump)?;
        self.pending_fallthrough = true;
        self.flow_active = false;
        Ok(())
    }

    fn visit_labeled_statement(&mut self, node: Node, source: &[u8]) -> Result<()> {
        let label = node
            .child_by_field_name("label")
            .or_else(|| {
                // Java: `identifier ':' statement` (no label field).
                // C: `statement_identifier` is usually the `label` field already.
                node.named_child(0).filter(|n| {
                    matches!(
                        n.kind(),
                        "identifier" | "label_name" | "statement_identifier" | "label"
                    )
                })
            })
            .and_then(|n| n.utf8_text(source).ok())
            .unwrap_or("")
            .trim()
            .trim_start_matches('\'')
            .to_string();
        if label.is_empty() {
            return Ok(());
        }
        let label_block = self.ensure_label_block(&label);
        if self.flow_active {
            self.cfg
                .add_edge(self.current_block, label_block, CfgEdgeType::Next);
        }
        self.current_block = label_block;
        self.flow_active = true;

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if !child.is_named() {
                continue;
            }
            if child.kind() == "label_name" || child.kind() == "statement_identifier" {
                continue;
            }
            if node
                .child_by_field_name("label")
                .is_some_and(|l| l.id() == child.id())
            {
                continue;
            }
            // Java label identifier is the first named child — skip it.
            if child.kind() == "identifier" {
                if let Ok(t) = child.utf8_text(source) {
                    if t.trim().trim_start_matches('\'') == label {
                        continue;
                    }
                }
            }
            let attach_label = matches!(
                child.kind(),
                "for_statement"
                    | "for_expression"
                    | "enhanced_for_statement"
                    | "while_statement"
                    | "while_expression"
                    | "do_statement"
                    | "loop_expression"
                    | "switch_statement"
                    | "type_switch_statement"
                    | "expression_switch_statement"
                    | "switch_expression"
                    | "select_statement"
            );
            if attach_label {
                self.pending_breakable_label = Some(label.clone());
            }
            self.visit_statement(child, source)?;
            if attach_label {
                self.pending_breakable_label = None;
            }
        }
        Ok(())
    }

    /// Lower `&&` / `||` into short-circuit CFG edges ending at `true_dest` / `false_dest`.
    fn wire_condition(
        &mut self,
        cond: Node,
        source: &[u8],
        true_dest: BlockId,
        false_dest: BlockId,
    ) -> Result<()> {
        // Unwrap Java/C-style `(expr)`. C++ `condition_clause` uses the `value` field
        // (initializer is visited separately by if/switch).
        let cond = {
            let mut c = cond;
            loop {
                match c.kind() {
                    "parenthesized_expression" => {
                        if let Some(inner) = c.named_child(0) {
                            c = inner;
                        } else {
                            break;
                        }
                    }
                    "condition_clause" => {
                        if let Some(value) = c.child_by_field_name("value") {
                            c = value;
                        } else if let Some(inner) = c.named_child(0) {
                            c = inner;
                        } else {
                            break;
                        }
                    }
                    _ => break,
                }
            }
            c
        };

        if let Some(op) = logical_operator(cond, source) {
            let left = cond
                .child_by_field_name("left")
                .ok_or_else(|| Error::ParseError {
                    file: "source".into(),
                    line: cond.start_position().row + 1,
                    message: "logical expression missing left".into(),
                })?;
            let right = cond
                .child_by_field_name("right")
                .ok_or_else(|| Error::ParseError {
                    file: "source".into(),
                    line: cond.start_position().row + 1,
                    message: "logical expression missing right".into(),
                })?;
            let mid = self.new_block();
            if op == "&&" {
                self.wire_condition(left, source, mid, false_dest)?;
                self.flow_active = true;
                self.current_block = mid;
                self.wire_condition(right, source, true_dest, false_dest)?;
            } else {
                self.wire_condition(left, source, true_dest, mid)?;
                self.flow_active = true;
                self.current_block = mid;
                self.wire_condition(right, source, true_dest, false_dest)?;
            }
            return Ok(());
        }

        let text = cond.utf8_text(source)?.trim().to_string();
        if is_constant_false(&text) {
            self.add_statement_to_current(Statement {
                kind: StatementKind::Branch,
                line: cond.start_position().row + 1,
                text,
                defined_vars: SmallVec::new(),
                used_vars: SmallVec::new(),
            });
            self.cfg
                .add_edge(self.current_block, false_dest, CfgEdgeType::IfFalse);
            return Ok(());
        }
        if is_constant_true(&text) {
            self.add_statement_to_current(Statement {
                kind: StatementKind::Branch,
                line: cond.start_position().row + 1,
                text,
                defined_vars: SmallVec::new(),
                used_vars: SmallVec::new(),
            });
            self.cfg
                .add_edge(self.current_block, true_dest, CfgEdgeType::IfTrue);
            return Ok(());
        }

        self.add_statement_to_current(Statement {
            kind: StatementKind::Branch,
            line: cond.start_position().row + 1,
            text,
            defined_vars: SmallVec::new(),
            used_vars: SmallVec::new(),
        });
        self.note_setjmp_in_expr(cond, source);
        let from = self.current_block;
        self.cfg.add_edge(from, true_dest, CfgEdgeType::IfTrue);
        self.cfg.add_edge(from, false_dest, CfgEdgeType::IfFalse);
        Ok(())
    }

    fn note_setjmp_in_expr(&mut self, node: Node, source: &[u8]) {
        if is_setjmp_call(node, source) {
            self.setjmp_sites.push(self.current_block);
            return;
        }
        let mut c = node.walk();
        for ch in node.children(&mut c) {
            if ch.is_named() {
                self.note_setjmp_in_expr(ch, source);
            }
        }
    }

    fn visit_match(&mut self, node: Node, source: &[u8]) -> Result<()> {
        let subject = node
            .child_by_field_name("value")
            .or_else(|| node.named_child(0))
            .and_then(|n| n.utf8_text(source).ok())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "match".to_string());
        self.add_statement_to_current(Statement {
            kind: StatementKind::Branch,
            line: node.start_position().row + 1,
            text: subject,
            defined_vars: SmallVec::new(),
            used_vars: SmallVec::new(),
        });
        let cond_block = self.current_block;
        let merge = self.new_block();
        let mut arms = Vec::new();

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "match_block" {
                let mut bc = child.walk();
                for arm in child.children(&mut bc) {
                    if arm.kind() == "match_arm" {
                        arms.push(arm);
                    }
                }
            } else if child.kind() == "match_arm" || child.kind() == "match_arm_pattern" {
                arms.push(child);
            }
        }

        if arms.is_empty() {
            if let Some(body) = node.child_by_field_name("body") {
                arms.push(body);
            }
        }

        let mut pending_fail: Option<BlockId> = None;
        for arm in arms {
            let test = self.new_block();
            if let Some(fail) = pending_fail.take() {
                self.cfg.add_edge(fail, test, CfgEdgeType::Next);
            } else {
                self.cfg.add_edge(cond_block, test, CfgEdgeType::IfTrue);
            }
            self.flow_active = true;
            self.current_block = test;

            let body = self.new_block();
            let fail = self.new_block();
            let guard = arm
                .child_by_field_name("pattern")
                .and_then(|p| p.child_by_field_name("condition"));
            if let Some(guard) = guard {
                // Pattern assumed matched; guard decides body vs next arm.
                self.wire_condition(guard, source, body, fail)?;
            } else {
                let pat_text = arm
                    .child_by_field_name("pattern")
                    .and_then(|p| p.utf8_text(source).ok())
                    .map(|s| s.trim().to_string())
                    .unwrap_or_else(|| "arm".to_string());
                self.add_statement_to_current(Statement {
                    kind: StatementKind::Branch,
                    line: arm.start_position().row + 1,
                    text: pat_text,
                    defined_vars: SmallVec::new(),
                    used_vars: SmallVec::new(),
                });
                self.cfg.add_edge(test, body, CfgEdgeType::IfTrue);
                self.cfg.add_edge(test, fail, CfgEdgeType::IfFalse);
            }
            pending_fail = Some(fail);

            self.flow_active = true;
            self.current_block = body;
            if let Some(value) = arm.child_by_field_name("value") {
                self.visit_statement(value, source)?;
            } else {
                self.visit_block(arm, source)?;
            }
            if self.flow_active {
                self.cfg
                    .add_edge(self.current_block, merge, CfgEdgeType::Next);
            }
        }

        if let Some(fail) = pending_fail {
            self.cfg.add_edge(fail, merge, CfgEdgeType::Next);
        }

        self.flow_active = true;
        self.current_block = merge;
        Ok(())
    }

    fn visit_switch(&mut self, node: Node, source: &[u8]) -> Result<()> {
        // Go: `switch init; value` — init runs before the branch.
        if let Some(init) = node.child_by_field_name("initializer") {
            self.visit_statement(init, source)?;
        } else {
            // type_switch_statement nests initializer under the header child.
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if let Some(init) = child.child_by_field_name("initializer") {
                    self.visit_statement(init, source)?;
                    break;
                }
            }
        }

        // C++17: `switch (init; cond)` — init inside condition_clause.
        let cond_field = node
            .child_by_field_name("condition")
            .or_else(|| node.child_by_field_name("value"))
            .or_else(|| node.child_by_field_name("operand"));
        let (cxx_init, cond_value) = cond_field
            .map(split_condition_clause)
            .unwrap_or((None, None));
        if let Some(init) = cxx_init {
            self.visit_statement(init, source)?;
        }

        let cond_block = self.new_block();
        self.cfg
            .add_edge(self.current_block, cond_block, CfgEdgeType::Next);
        self.current_block = cond_block;

        let branch_text = cond_value
            .or(cond_field)
            .and_then(|c| c.utf8_text(source).ok())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| node.kind().to_string());
        self.add_statement_to_current(Statement {
            kind: StatementKind::Branch,
            line: node.start_position().row + 1,
            text: branch_text,
            defined_vars: SmallVec::new(),
            used_vars: SmallVec::new(),
        });

        let merge = self.new_block();
        self.breakable_stack.push(BreakableContext {
            exit: merge,
            continue_target: None,
            label: self.pending_breakable_label.take(),
        });
        let mut cases = Vec::new();
        collect_switch_cases(node, &mut cases);

        if cases.is_empty() {
            if let Some(body) = node.child_by_field_name("body") {
                self.current_block = cond_block;
                self.visit_block(body, source)?;
            }
            self.breakable_stack.pop();
            self.current_block = merge;
            self.flow_active = true;
            return Ok(());
        }

        // Pre-create case blocks so `goto case` / `goto default` can resolve forward.
        self.switch_case_labels.push(HashMap::new());
        let mut case_blocks: Vec<BlockId> = Vec::with_capacity(cases.len());
        let mut has_default = false;
        for case in &cases {
            let case_block = self.new_block();
            case_blocks.push(case_block);
            let is_default = is_switch_default_case(*case, source);
            if is_default {
                has_default = true;
            }
            if let Some(map) = self.switch_case_labels.last_mut() {
                for key in switch_case_keys(*case, source) {
                    map.insert(key, case_block);
                }
            }
            let edge = if is_default {
                CfgEdgeType::IfFalse
            } else {
                CfgEdgeType::IfTrue
            };
            self.cfg.add_edge(cond_block, case_block, edge);
        }

        let mut any_reaches_merge = false;
        let mut fallthrough_from: Option<BlockId> = None;
        let prev_implicit = self.switch_implicit_fallthrough;
        for (case, case_block) in cases.iter().zip(case_blocks.iter().copied()) {
            // Java classic groups and C `case_statement` fall through by default.
            self.switch_implicit_fallthrough = matches!(
                case.kind(),
                "switch_block_statement_group" | "case_statement" | "default_statement"
            );

            if let Some(src) = fallthrough_from.take() {
                self.cfg.add_edge(src, case_block, CfgEdgeType::Jump);
            }
            self.flow_active = true;
            self.pending_fallthrough = false;
            self.current_block = case_block;
            self.visit_case_body(*case, source)?;

            let implicit = self.switch_implicit_fallthrough && self.flow_active;
            if self.pending_fallthrough || implicit {
                fallthrough_from = Some(self.current_block);
                self.pending_fallthrough = false;
            } else if self.flow_active {
                self.cfg
                    .add_edge(self.current_block, merge, CfgEdgeType::Next);
                any_reaches_merge = true;
            }
        }
        self.switch_implicit_fallthrough = prev_implicit;

        // Trailing fallthrough with no next case → merge.
        if let Some(src) = fallthrough_from {
            self.cfg.add_edge(src, merge, CfgEdgeType::Next);
            any_reaches_merge = true;
        }

        if !has_default {
            let default_block = self.new_block();
            if let Some(map) = self.switch_case_labels.last_mut() {
                map.insert("default".to_string(), default_block);
            }
            self.cfg
                .add_edge(cond_block, default_block, CfgEdgeType::IfFalse);
            self.cfg.add_edge(default_block, merge, CfgEdgeType::Next);
            any_reaches_merge = true;
        }

        self.switch_case_labels.pop();
        self.breakable_stack.pop();
        self.flow_active = any_reaches_merge;
        self.current_block = merge;
        Ok(())
    }

    /// C# `x switch { pat => val, _ => other }` — multi-way value branch (no fallthrough).
    /// Java `switch` expressions reuse `switch_expression` but use statement groups/rules —
    /// those fall through to [`Self::visit_switch`].
    fn visit_switch_expression(&mut self, node: Node, source: &[u8]) -> Result<()> {
        let mut arms = Vec::new();
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "switch_expression_arm" {
                arms.push(child);
            }
        }
        if arms.is_empty() {
            return self.visit_switch(node, source);
        }

        let subject = node
            .child_by_field_name("value")
            .or_else(|| node.named_child(0))
            .and_then(|n| n.utf8_text(source).ok())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "switch".to_string());
        self.add_statement_to_current(Statement {
            kind: StatementKind::Branch,
            line: node.start_position().row + 1,
            text: subject,
            defined_vars: SmallVec::new(),
            used_vars: SmallVec::new(),
        });
        let cond_block = self.current_block;
        let merge = self.new_block();

        let mut pending_fail: Option<BlockId> = None;
        for arm in arms {
            let test = self.new_block();
            if let Some(fail) = pending_fail.take() {
                self.cfg.add_edge(fail, test, CfgEdgeType::Next);
            } else {
                self.cfg.add_edge(cond_block, test, CfgEdgeType::IfTrue);
            }
            self.flow_active = true;
            self.current_block = test;

            let body = self.new_block();
            let fail = self.new_block();

            // Optional `when` guard on the arm.
            let when = find_child_kind(arm, "when_clause");
            let pat_text = arm
                .child_by_field_name("pattern")
                .or_else(|| arm.named_child(0))
                .and_then(|p| p.utf8_text(source).ok())
                .map(|s| s.trim().to_string())
                .unwrap_or_else(|| "arm".to_string());
            self.add_statement_to_current(Statement {
                kind: StatementKind::Branch,
                line: arm.start_position().row + 1,
                text: pat_text,
                defined_vars: SmallVec::new(),
                used_vars: SmallVec::new(),
            });
            if let Some(w) = when {
                let mid = self.new_block();
                self.cfg.add_edge(test, mid, CfgEdgeType::IfTrue);
                self.cfg.add_edge(test, fail, CfgEdgeType::IfFalse);
                self.flow_active = true;
                self.current_block = mid;
                let cond = w
                    .child_by_field_name("condition")
                    .or_else(|| w.child_by_field_name("value"))
                    .or_else(|| w.named_child(0))
                    .unwrap_or(w);
                self.wire_condition(cond, source, body, fail)?;
            } else {
                self.cfg.add_edge(test, body, CfgEdgeType::IfTrue);
                self.cfg.add_edge(test, fail, CfgEdgeType::IfFalse);
            }
            pending_fail = Some(fail);

            self.flow_active = true;
            self.current_block = body;
            if let Some(value) = arm.child_by_field_name("value") {
                self.visit_expr_for_control_flow(value, source)?;
                if self.flow_active {
                    self.add_statement(value, source, StatementKind::Expression)?;
                }
            }
            if self.flow_active {
                self.cfg
                    .add_edge(self.current_block, merge, CfgEdgeType::Next);
            }
        }

        if let Some(fail) = pending_fail {
            self.cfg.add_edge(fail, merge, CfgEdgeType::Next);
        }

        self.flow_active = true;
        self.current_block = merge;
        Ok(())
    }

    /// Kotlin `when (x) { … -> … }` — multi-way branch over `when_entry` arms.
    fn visit_kotlin_when(&mut self, node: Node, source: &[u8]) -> Result<()> {
        let mut arms = Vec::new();
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "when_entry" {
                arms.push(child);
            }
        }
        if arms.is_empty() {
            return self.visit_expression_stmt(node, source);
        }

        let subject = node
            .child_by_field_name("value")
            .or_else(|| find_child_kind(node, "when_subject"))
            .and_then(|n| n.utf8_text(source).ok())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "when".to_string());
        self.add_statement_to_current(Statement {
            kind: StatementKind::Branch,
            line: node.start_position().row + 1,
            text: subject,
            defined_vars: SmallVec::new(),
            used_vars: SmallVec::new(),
        });
        let cond_block = self.current_block;
        let merge = self.new_block();
        self.breakable_stack.push(BreakableContext {
            exit: merge,
            continue_target: None,
            label: None,
        });

        let mut pending_fail: Option<BlockId> = None;
        for arm in arms {
            let test = self.new_block();
            if let Some(fail) = pending_fail.take() {
                self.cfg.add_edge(fail, test, CfgEdgeType::Next);
            } else {
                self.cfg.add_edge(cond_block, test, CfgEdgeType::IfTrue);
            }
            self.flow_active = true;
            self.current_block = test;

            let fail = self.new_block();
            let arm_text = arm
                .utf8_text(source)
                .ok()
                .map(|s| s.lines().next().unwrap_or("").trim().to_string())
                .unwrap_or_else(|| "entry".to_string());
            self.add_statement_to_current(Statement {
                kind: StatementKind::Branch,
                line: arm.start_position().row + 1,
                text: arm_text,
                defined_vars: SmallVec::new(),
                used_vars: SmallVec::new(),
            });
            self.cfg
                .add_edge(self.current_block, fail, CfgEdgeType::IfFalse);
            let body = self.new_block();
            self.cfg
                .add_edge(self.current_block, body, CfgEdgeType::IfTrue);
            self.flow_active = true;
            self.current_block = body;

            // Prefer explicit body / last expression child after `->`
            if let Some(body_node) = arm.child_by_field_name("body") {
                self.visit_statement(body_node, source)?;
            } else {
                let mut c = arm.walk();
                let children: Vec<Node> = arm.children(&mut c).filter(|c| c.is_named()).collect();
                if let Some(last) = children.last() {
                    if last.kind() != "when_condition" && last.kind() != "when_entry" {
                        self.visit_statement(*last, source)?;
                    }
                }
            }
            if self.flow_active {
                self.cfg
                    .add_edge(self.current_block, merge, CfgEdgeType::Next);
            }
            pending_fail = Some(fail);
        }

        if let Some(fail) = pending_fail {
            self.cfg.add_edge(fail, merge, CfgEdgeType::Next);
        }

        self.breakable_stack.pop();
        self.flow_active = true;
        self.current_block = merge;
        Ok(())
    }

    /// Lower switch/select case bodies.
    fn visit_case_body(&mut self, case: Node, source: &[u8]) -> Result<()> {
        if let Some(body) = case.child_by_field_name("body") {
            self.visit_block(body, source)?;
            return Ok(());
        }
        match case.kind() {
            "switch_section" => {
                let mut when: Option<Node> = None;
                let mut stmts: Vec<Node> = Vec::new();
                let mut c = case.walk();
                for child in case.children(&mut c) {
                    if !child.is_named() {
                        // Skip `case` / `default` / `:` tokens.
                        continue;
                    }
                    match child.kind() {
                        "case_switch_label"
                        | "case_pattern_switch_label"
                        | "default_switch_label"
                        | "switch_label"
                        | "constant_pattern"
                        | "declaration_pattern"
                        | "relational_pattern"
                        | "var_pattern"
                        | "recursive_pattern"
                        | "list_pattern"
                        | "type_pattern" => {
                            // `when` may be nested under the pattern label.
                            if when.is_none() {
                                when = find_child_kind(child, "when_clause");
                            }
                        }
                        "when_clause" => when = Some(child),
                        _ => stmts.push(child),
                    }
                }
                if let Some(w) = when {
                    let body = self.new_block();
                    let fail = self
                        .breakable_stack
                        .last()
                        .map(|b| b.exit)
                        .unwrap_or_else(|| self.new_block());
                    let cond = w
                        .child_by_field_name("condition")
                        .or_else(|| w.child_by_field_name("value"))
                        .or_else(|| w.named_child(0))
                        .unwrap_or(w);
                    self.wire_condition(cond, source, body, fail)?;
                    self.flow_active = true;
                    self.current_block = body;
                }
                for stmt in stmts {
                    self.visit_statement(stmt, source)?;
                }
            }
            "switch_block_statement_group" | "switch_rule" => {
                let mut c = case.walk();
                for child in case.children(&mut c) {
                    if !child.is_named() {
                        continue;
                    }
                    // Skip labels (`case 1:` / `default:`).
                    if child.kind() == "switch_label" {
                        continue;
                    }
                    self.visit_statement(child, source)?;
                }
            }
            "case_statement" | "default_statement" => {
                // C/C++: `case 1: stmt; break;` — statements are direct children.
                let value = case.child_by_field_name("value");
                let mut c = case.walk();
                for child in case.children(&mut c) {
                    if !child.is_named() {
                        continue;
                    }
                    if value.is_some_and(|v| v.id() == child.id()) {
                        continue;
                    }
                    self.visit_statement(child, source)?;
                }
            }
            _ => {
                let mut cursor = case.walk();
                for child in case.children(&mut cursor) {
                    if child.kind() == "statement_list" || is_block_like(child.kind()) {
                        self.visit_block(child, source)?;
                    }
                }
            }
        }
        Ok(())
    }

    fn visit_select(&mut self, node: Node, source: &[u8]) -> Result<()> {
        self.visit_switch(node, source)
    }

    fn visit_try(&mut self, node: Node, source: &[u8]) -> Result<()> {
        // Snapshot finally + resource closes before body so return/throw can unwind them.
        let mut finally_snapshot: Option<Vec<DeferredCall>> = None;
        let mut catch_nodes: Vec<Node> = Vec::new();
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            match child.kind() {
                "finally_clause" => {
                    finally_snapshot = Some(snapshot_block_stmts(child, source));
                }
                "except_clause" | "except_handler" | "catch_clause" => {
                    catch_nodes.push(child);
                }
                _ => {}
            }
        }
        let resource_closes = snapshot_resource_closes(node, source);

        // Push order: user finally first, then resource closes → LIFO runs closes then finally.
        let pushed_finally = finally_snapshot.is_some();
        let pushed_closes = !resource_closes.is_empty();
        if let Some(ref snap) = finally_snapshot {
            self.finally_stack.push(snap.clone());
        }
        if pushed_closes {
            self.finally_stack.push(resource_closes.clone());
        }

        // Pre-create catch entries so body statements can Exception-edge to them.
        let catch_entries: Vec<BlockId> = catch_nodes.iter().map(|_| self.new_block()).collect();
        if !catch_entries.is_empty() {
            self.try_catch_stack.push(catch_entries.clone());
        }

        // Resource declarations run before the try body.
        if let Some(resources) = node.child_by_field_name("resources") {
            let mut c = resources.walk();
            for child in resources.children(&mut c) {
                if child.kind() == "resource" {
                    self.add_statement(child, source, StatementKind::Declaration)?;
                }
            }
        }

        let try_block = self.new_block();
        self.cfg
            .add_edge(self.current_block, try_block, CfgEdgeType::Next);
        self.current_block = try_block;
        // Always link try entry → catch (covers return-only / empty bodies).
        for &h in &catch_entries {
            self.cfg.add_edge(try_block, h, CfgEdgeType::Exception);
        }
        if let Some(body) = node.child_by_field_name("body") {
            self.visit_block(body, source)?;
        }
        if !catch_entries.is_empty() {
            self.try_catch_stack.pop();
        }
        let try_reached_end = self.flow_active;
        let mut try_end = self.current_block;
        let merge = self.new_block();

        for (i, catch_node) in catch_nodes.iter().enumerate() {
            let handler = catch_entries[i];
            self.flow_active = true;
            self.current_block = handler;

            // C# `catch (E e) when (cond)` — filter before catch body.
            if let Some(filter) = find_child_kind(*catch_node, "catch_filter_clause")
                .or_else(|| find_child_kind(*catch_node, "when_clause"))
            {
                let body = self.new_block();
                let fail = self.new_block();
                let cond = filter
                    .child_by_field_name("filter")
                    .or_else(|| filter.child_by_field_name("condition"))
                    .or_else(|| filter.child_by_field_name("value"))
                    .or_else(|| filter.named_child(0))
                    .unwrap_or(filter);
                self.wire_condition(cond, source, body, fail)?;
                // Filter fail → rethrow through finallies.
                self.flow_active = true;
                self.current_block = fail;
                self.unwind_finallies(source)?;
                self.unwind_defers_to_exit(CfgEdgeType::Exception);
                self.flow_active = true;
                self.current_block = body;
            }

            if let Some(block) = catch_node.child_by_field_name("body") {
                self.visit_block(block, source)?;
            } else if catch_node.kind() == "catch_clause" {
                if let Some(block) = find_child_kind(*catch_node, "block") {
                    self.visit_block(block, source)?;
                }
            }
            if self.flow_active {
                self.unwind_finallies(source)?;
                if self.flow_active {
                    self.cfg
                        .add_edge(self.current_block, merge, CfgEdgeType::Next);
                }
            }
        }

        if try_reached_end {
            self.flow_active = true;
            self.current_block = try_end;
            // Same order as unwind_finallies: reverse stack = closes then user finally.
            if pushed_closes {
                for d in &resource_closes {
                    let b = self.new_block();
                    self.cfg.add_edge(self.current_block, b, CfgEdgeType::Next);
                    self.current_block = b;
                    self.add_statement_to_current(Statement {
                        kind: StatementKind::Expression,
                        line: d.line,
                        text: d.text.clone(),
                        defined_vars: SmallVec::new(),
                        used_vars: SmallVec::new(),
                    });
                }
            }
            if let Some(ref stmts) = finally_snapshot {
                for d in stmts {
                    let b = self.new_block();
                    self.cfg.add_edge(self.current_block, b, CfgEdgeType::Next);
                    self.current_block = b;
                    self.add_statement_to_current(Statement {
                        kind: StatementKind::Expression,
                        line: d.line,
                        text: d.text.clone(),
                        defined_vars: SmallVec::new(),
                        used_vars: SmallVec::new(),
                    });
                }
            }
            try_end = self.current_block;
            self.cfg.add_edge(try_end, merge, CfgEdgeType::Next);
        }

        if pushed_closes {
            self.finally_stack.pop();
        }
        if pushed_finally {
            self.finally_stack.pop();
        }

        self.flow_active = true;
        self.current_block = merge;
        Ok(())
    }
}

fn snapshot_resource_closes(node: Node, source: &[u8]) -> Vec<DeferredCall> {
    let Some(resources) = node.child_by_field_name("resources") else {
        return Vec::new();
    };
    let mut names: Vec<(String, usize)> = Vec::new();
    let mut c = resources.walk();
    for child in resources.children(&mut c) {
        if child.kind() != "resource" {
            continue;
        }
        let line = child.start_position().row + 1;
        let name = child
            .child_by_field_name("name")
            .and_then(|n| n.utf8_text(source).ok())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty() && s != "_")
            .or_else(|| {
                // `try (alreadyOpen)` / `try (obj.field)` — no `name` field.
                if child.child_by_field_name("type").is_some() {
                    return None;
                }
                child
                    .named_child(0)
                    .and_then(|n| n.utf8_text(source).ok())
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
            });
        if let Some(name) = name {
            names.push((name, line));
        }
    }
    // Close in reverse declaration order (JLS).
    names
        .into_iter()
        .rev()
        .map(|(name, line)| DeferredCall {
            text: format!("{name}.close()"),
            line,
        })
        .collect()
}

fn snapshot_block_stmts(node: Node, source: &[u8]) -> Vec<DeferredCall> {
    let mut out = Vec::new();
    let block = find_child_kind(node, "block").unwrap_or(node);
    let mut stack = vec![block];
    while let Some(n) = stack.pop() {
        if matches!(
            n.kind(),
            "expression_statement"
                | "method_invocation"
                | "return_statement"
                | "local_variable_declaration"
                | "throw_statement"
        ) {
            if let Ok(t) = n.utf8_text(source) {
                out.push(DeferredCall {
                    text: t.trim().to_string(),
                    line: n.start_position().row + 1,
                });
            }
            continue;
        }
        let mut c = n.walk();
        let children: Vec<_> = n.children(&mut c).filter(|ch| ch.is_named()).collect();
        for ch in children.into_iter().rev() {
            stack.push(ch);
        }
    }
    out
}

fn split_condition_clause<'a>(cond: Node<'a>) -> (Option<Node<'a>>, Option<Node<'a>>) {
    if cond.kind() != "condition_clause" {
        return (None, Some(cond));
    }
    let init = cond.child_by_field_name("initializer");
    let value = cond.child_by_field_name("value").or_else(|| {
        // Fallback: last named child that is not the initializer.
        let mut last = None;
        let mut c = cond.walk();
        for ch in cond.children(&mut c) {
            if !ch.is_named() {
                continue;
            }
            if init.is_some_and(|i| i.id() == ch.id()) {
                continue;
            }
            last = Some(ch);
        }
        last
    });
    (init, value)
}

fn is_switch_default_case(case: Node, source: &[u8]) -> bool {
    if matches!(
        case.kind(),
        "default_case" | "default_statement" | "switch_default"
    ) {
        return true;
    }
    // C/C++: `default:` is a `case_statement` with no `value` field.
    if case.kind() == "case_statement" && case.child_by_field_name("value").is_none() {
        if let Ok(t) = case.utf8_text(source) {
            if t.trim().starts_with("default") {
                return true;
            }
        }
    }
    let mut c = case.walk();
    for child in case.children(&mut c) {
        // C# `switch_section`: bare `default` token (unnamed) or label nodes.
        if child.kind() == "default" || child.kind() == "default_switch_label" {
            return true;
        }
        if matches!(child.kind(), "switch_label" | "default_label") {
            if let Ok(t) = child.utf8_text(source) {
                if t.trim().starts_with("default") {
                    return true;
                }
            }
        }
    }
    false
}

fn find_direct_child_kind<'a>(node: Node<'a>, kind: &str) -> Option<Node<'a>> {
    let mut cursor = node.walk();
    node.children(&mut cursor).find(|c| c.kind() == kind)
}

fn find_direct_child_kinds<'a>(node: Node<'a>, kinds: &[&str]) -> Option<Node<'a>> {
    let mut cursor = node.walk();
    node.children(&mut cursor)
        .find(|c| kinds.iter().any(|k| c.kind() == *k))
}

fn find_child_kind<'a>(node: Node<'a>, kind: &str) -> Option<Node<'a>> {
    let mut stack = vec![node];
    while let Some(node) = stack.pop() {
        if node.kind() == kind {
            return Some(node);
        }
        let mut cursor = node.walk();
        let children: Vec<_> = node.children(&mut cursor).collect();
        for child in children.into_iter().rev() {
            stack.push(child);
        }
    }
    None
}

fn collect_switch_cases<'a>(node: Node<'a>, cases: &mut Vec<Node<'a>>) {
    let mut stack = vec![(node, 0usize)];
    while let Some((node, depth)) = stack.pop() {
        if depth > AST_WALK_MAX_DEPTH {
            continue;
        }
        match node.kind() {
            "expression_case"
            | "type_case"
            | "case_clause"
            | "default_case"
            | "default_statement"
            | "case_statement"
            | "communication_case"
            | "switch_section"
            | "switch_case"
            | "switch_default"
            | "switch_block_statement_group"
            | "switch_rule" => cases.push(node),
            _ => {
                let mut cursor = node.walk();
                let children: Vec<_> = node.children(&mut cursor).collect();
                for child in children.into_iter().rev() {
                    if child.is_named() {
                        stack.push((child, depth + 1));
                    }
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
            | "body_statement"
            | "begin_block"
            | "do_block"
            | "program"
    )
}

fn function_body_node<'a>(func_node: Node<'a>, language: &str) -> Option<Node<'a>> {
    // Java instance initializer: the function node *is* the block body.
    if func_node.kind() == "block" {
        return Some(func_node);
    }
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

fn is_panic_call(node: Node, source: &[u8]) -> bool {
    let call = if node.kind() == "call_expression" {
        node
    } else {
        return false;
    };
    let func = call
        .child_by_field_name("function")
        .or_else(|| call.named_child(0));
    func.and_then(|f| f.utf8_text(source).ok())
        .is_some_and(|s| s.trim() == "panic")
}

fn is_diverging_macro(node: Node, source: &[u8]) -> bool {
    if node.kind() != "macro_invocation" {
        return false;
    }
    let name = node
        .child_by_field_name("macro")
        .or_else(|| node.named_child(0))
        .and_then(|n| n.utf8_text(source).ok())
        .unwrap_or("")
        .trim();
    matches!(name, "panic" | "todo" | "unimplemented" | "unreachable")
}

fn call_callee_name<'a>(node: Node<'a>, source: &'a [u8]) -> Option<&'a str> {
    let call = if node.kind() == "call_expression" {
        node
    } else {
        return None;
    };
    call.child_by_field_name("function")
        .or_else(|| call.named_child(0))
        .and_then(|f| f.utf8_text(source).ok())
        .map(|s| s.trim())
}

fn is_terminating_call(node: Node, source: &[u8]) -> bool {
    call_callee_name(node, source)
        .is_some_and(|s| matches!(s, "abort" | "exit" | "_Exit" | "quick_exit"))
}

fn is_setjmp_call(node: Node, source: &[u8]) -> bool {
    call_callee_name(node, source).is_some_and(|s| matches!(s, "setjmp" | "_setjmp" | "sigsetjmp"))
}

fn is_longjmp_call(node: Node, source: &[u8]) -> bool {
    call_callee_name(node, source)
        .is_some_and(|s| matches!(s, "longjmp" | "_longjmp" | "siglongjmp"))
}

fn logical_operator<'a>(node: Node<'a>, source: &'a [u8]) -> Option<&'a str> {
    if node.kind() != "binary_expression" {
        return None;
    }
    let op = node.child_by_field_name("operator")?;
    let text = op.utf8_text(source).ok()?;
    if text == "&&" || text == "||" {
        Some(text)
    } else {
        None
    }
}

fn null_coalesce_operator(node: Node, source: &[u8]) -> bool {
    if node.kind() != "binary_expression" {
        return false;
    }
    node.child_by_field_name("operator")
        .and_then(|op| op.utf8_text(source).ok())
        .is_some_and(|t| t == "??")
}

fn is_csharp_using_declaration(node: Node, source: &[u8]) -> bool {
    if node.kind() != "local_declaration_statement" {
        return false;
    }
    let mut c = node.walk();
    for ch in node.children(&mut c) {
        if ch.kind() == "using" {
            return true;
        }
    }
    node.utf8_text(source)
        .ok()
        .is_some_and(|t| t.trim_start().starts_with("using "))
}

fn goto_case_key(node: Node, source: &[u8]) -> Option<String> {
    let mut c = node.walk();
    for ch in node.children(&mut c) {
        if !ch.is_named() {
            continue;
        }
        if matches!(
            ch.kind(),
            "integer_literal"
                | "real_literal"
                | "string_literal"
                | "character_literal"
                | "boolean_literal"
                | "null_literal"
                | "identifier"
                | "prefix_unary_expression"
        ) {
            return ch.utf8_text(source).ok().map(|s| s.trim().to_string());
        }
    }
    // Fallback: parse `goto case 2;`
    let text = node.utf8_text(source).ok()?.trim();
    let rest = text.strip_prefix("goto case")?.trim();
    let key = rest.trim_end_matches(';').trim();
    if key.is_empty() {
        None
    } else {
        Some(key.to_string())
    }
}

fn switch_case_keys(case: Node, source: &[u8]) -> Vec<String> {
    if is_switch_default_case(case, source) {
        return vec!["default".to_string()];
    }
    let mut keys = Vec::new();
    let mut c = case.walk();
    for child in case.children(&mut c) {
        match child.kind() {
            // C# modern grammar: `case 1:` → `case` + `constant_pattern` + `:`
            "constant_pattern" => {
                if let Ok(t) = child.utf8_text(source) {
                    let t = t.trim();
                    if !t.is_empty() {
                        keys.push(t.to_string());
                    }
                }
            }
            "case_switch_label" | "switch_label" | "case_pattern_switch_label" => {
                if let Some(k) = case_label_constant(child, source) {
                    keys.push(k);
                }
            }
            "case_statement" => {
                if let Some(v) = child.child_by_field_name("value") {
                    if let Ok(t) = v.utf8_text(source) {
                        keys.push(t.trim().to_string());
                    }
                }
            }
            _ => {}
        }
    }
    if keys.is_empty() {
        if let Some(v) = case.child_by_field_name("value") {
            if let Ok(t) = v.utf8_text(source) {
                keys.push(t.trim().to_string());
            }
        }
    }
    keys
}

fn case_label_constant(label: Node, source: &[u8]) -> Option<String> {
    let mut c = label.walk();
    for ch in label.children(&mut c) {
        if !ch.is_named() {
            continue;
        }
        if matches!(
            ch.kind(),
            "integer_literal"
                | "real_literal"
                | "string_literal"
                | "character_literal"
                | "boolean_literal"
                | "null_literal"
                | "identifier"
                | "prefix_unary_expression"
                | "constant_pattern"
        ) {
            return ch.utf8_text(source).ok().map(|s| s.trim().to_string());
        }
        // `constant_pattern` wraps the literal.
        if let Some(inner) = ch.named_child(0) {
            if matches!(
                inner.kind(),
                "integer_literal"
                    | "real_literal"
                    | "string_literal"
                    | "character_literal"
                    | "boolean_literal"
                    | "null_literal"
                    | "identifier"
            ) {
                return inner.utf8_text(source).ok().map(|s| s.trim().to_string());
            }
        }
    }
    // `case 1:` text fallback
    let text = label.utf8_text(source).ok()?.trim();
    let rest = text
        .strip_prefix("case")?
        .trim()
        .trim_end_matches(':')
        .trim();
    if rest.is_empty() || rest.starts_with("var") || rest.contains(' ') {
        None
    } else {
        Some(rest.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cfg::CfgEdgeType;



    #[test]
    fn annotate_deep_statements_cfg_does_not_overflow() {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../example/llvm-project/clang/test/Index/annotate-deep-statements.cpp"
        );
        if !std::path::Path::new(path).exists() {
            return;
        }
        let source = std::fs::read_to_string(path).unwrap();
        let cfg = build_cfg_for_function("cpp", &source, "foo").unwrap();
        assert!(!cfg.blocks.is_empty());
        let dom = crate::DominatorTree::build(&cfg);
        let pdg = crate::ProgramDependenceGraph::build_with_dominator_options(
            &cfg,
            source.as_bytes(),
            &dom,
            crate::PdgBuildOptions::default(),
        )
        .unwrap();
        assert!(!pdg.nodes.is_empty());

        let parsed = ParsedSourceFile::parse("cpp", source.as_bytes()).unwrap();
        assert!(parsed.contains("foo"));
        let cfg2 = parsed.build_cfg("cpp", source.as_bytes(), "foo").unwrap();
        assert!(!cfg2.blocks.is_empty());
    }

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
    fn test_overload_span_lookup_avoids_reparse_fallback() {
        let code = r#"
public class Overloads {
    public int foo(int x) {
        return x + 1;
    }
    public int foo(String s) {
        return s.length();
    }
}
"#;
        let parsed = ParsedSourceFile::parse("java", code.as_bytes()).unwrap();
        assert!(parsed.contains("foo"));
        let _ = parsed.tree(); // expose tree API

        // Locate each overload's span from the index.
        let locs = &parsed.locations_by_name["foo"];
        assert!(
            locs.len() >= 2,
            "expected two foo overloads, got {}",
            locs.len()
        );
        let mut texts_by_span = Vec::new();
        for loc in locs {
            let cfg = parsed
                .build_cfg_for_node_span(
                    "java",
                    code.as_bytes(),
                    "foo",
                    loc.start_line,
                    loc.end_line,
                )
                .unwrap();
            let texts: Vec<String> = cfg
                .blocks
                .values()
                .flat_map(|b| b.statements.iter().map(|s| s.text.clone()))
                .collect();
            texts_by_span.push(texts);
        }
        assert!(
            texts_by_span.iter().any(|texts| {
                texts
                    .iter()
                    .any(|t| t.contains("x + 1") || t.contains("x+1"))
            }),
            "int overload CFG missing, got {texts_by_span:?}"
        );
        assert!(
            texts_by_span.iter().any(|texts| {
                texts
                    .iter()
                    .any(|t| t.contains("length") || t.contains("s."))
            }),
            "String overload CFG missing, got {texts_by_span:?}"
        );
        assert_ne!(
            texts_by_span[0], texts_by_span[1],
            "overloads must resolve to distinct CFGs without whole-file re-parse collision"
        );
    }

    fn non_exit_empty_blocks(cfg: &ControlFlowGraph) -> Vec<BlockId> {
        cfg.blocks
            .values()
            .filter(|b| b.statements.is_empty() && !cfg.exits.contains(&b.id))
            .map(|b| b.id)
            .collect()
    }

    #[test]
    fn test_issue_121_sequential_guard_clauses_no_empty_blocks() {
        // Issue #121: process(x, y) with two if-without-else then return z.
        let code = r#"
fn process(x: i32, y: i32, mut z: i32) -> i32 {
    if x > 0 {
        z = z + 1;
    }
    if y > 0 {
        z = z + 2;
    }
    z
}
"#;
        let cfg = build_cfg_for_function("rust", code, "process").unwrap();
        assert_eq!(
            cfg.blocks.len(),
            6,
            "expected 6 blocks after empty contraction, got {}:\n{}",
            cfg.blocks.len(),
            cfg.to_dot()
        );
        let empties = non_exit_empty_blocks(&cfg);
        assert!(
            empties.is_empty(),
            "non-exit empty blocks remain: {empties:?}\n{}",
            cfg.to_dot()
        );

        let first_cond = rust_block_ids(&cfg, "x > 0")[0];
        let second_cond = rust_block_ids(&cfg, "y > 0")[0];
        assert!(
            cfg.edges.iter().any(|e| {
                e.from == first_cond
                    && e.to == second_cond
                    && e.edge_type == CfgEdgeType::IfFalse
            }),
            "IfFalse from first guard must land on second condition, edges={:?}",
            cfg.edges
        );
    }

    #[test]
    fn test_nested_if_without_else_no_empty_blocks() {
        let code = r#"
fn nested(a: bool, b: bool) -> i32 {
    if a {
        if b {
            return 1;
        }
    }
    0
}
"#;
        let cfg = build_cfg_for_function("rust", code, "nested").unwrap();
        let empties = non_exit_empty_blocks(&cfg);
        assert!(
            empties.is_empty(),
            "non-exit empty blocks remain: {empties:?}\n{}",
            cfg.to_dot()
        );
        assert!(
            cfg.edges
                .iter()
                .any(|e| e.edge_type == CfgEdgeType::IfFalse),
            "nested if-without-else must keep IfFalse edges"
        );
    }

    #[test]
    fn test_early_return_guard_clauses_direct_iffalse() {
        let code = r#"
fn guards(a: bool, b: bool) -> i32 {
    if a {
        return 1;
    }
    if b {
        return 2;
    }
    3
}
"#;
        let cfg = build_cfg_for_function("rust", code, "guards").unwrap();
        let empties = non_exit_empty_blocks(&cfg);
        assert!(
            empties.is_empty(),
            "non-exit empty blocks remain: {empties:?}\n{}",
            cfg.to_dot()
        );
        let a_cond = rust_block_ids(&cfg, "a")[0];
        let b_cond = rust_block_ids(&cfg, "b")[0];
        assert!(
            cfg.edges.iter().any(|e| {
                e.from == a_cond && e.to == b_cond && e.edge_type == CfgEdgeType::IfFalse
            }),
            "first guard IfFalse must target second condition"
        );
    }

    #[test]
    fn test_short_circuit_if_without_else_wires_false_to_merge() {
        let code = r#"
fn sc(a: bool, b: bool) -> i32 {
    if a && b {
        return 1;
    }
    0
}
"#;
        let cfg = build_cfg_for_function("rust", code, "sc").unwrap();
        let empties = non_exit_empty_blocks(&cfg);
        assert!(
            empties.is_empty(),
            "non-exit empty blocks remain: {empties:?}\n{}",
            cfg.to_dot()
        );
        let false_edges = cfg
            .edges
            .iter()
            .filter(|e| e.edge_type == CfgEdgeType::IfFalse)
            .count();
        assert!(
            false_edges >= 2,
            "short-circuit && should emit IfFalse from both sides, got {false_edges}"
        );
    }

    #[test]
    fn test_empty_while_body_preserves_back_edge() {
        let code = r#"
fn spin(cond: bool) {
    while cond {}
}
"#;
        let cfg = build_cfg_for_function("rust", code, "spin").unwrap();
        assert!(cfg.has_cycle(), "empty while body must keep loop back-edge");
        let jump_backs = cfg
            .edges
            .iter()
            .filter(|e| e.edge_type == CfgEdgeType::Jump)
            .count();
        assert!(
            jump_backs >= 1,
            "empty loop body must not be infinitely contracted away"
        );
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
    fn deep_cpp_call_chain_cfg_does_not_overflow() {
        let depth = 3000;
        let chain = (0..depth).fold(String::from("s()"), |acc, _| format!("{acc}()"));
        let code = format!("int s() {{ return 0; }}\nint deep() {{ return {chain}; }}");
        let cfg = build_cfg_for_function("cpp", &code, "deep").unwrap();
        assert!(!cfg.blocks.is_empty());
    }

    fn rust_texts(cfg: &ControlFlowGraph) -> Vec<String> {
        cfg.blocks
            .values()
            .flat_map(|b| b.statements.iter().map(|s| s.text.clone()))
            .collect()
    }

    fn rust_kinds(cfg: &ControlFlowGraph) -> Vec<StatementKind> {
        cfg.blocks
            .values()
            .flat_map(|b| b.statements.iter().map(|s| s.kind))
            .collect()
    }

    fn rust_block_ids(cfg: &ControlFlowGraph, needle: &str) -> Vec<BlockId> {
        cfg.blocks
            .values()
            .filter(|b| b.statements.iter().any(|s| s.text.contains(needle)))
            .map(|b| b.id)
            .collect()
    }

    #[test]
    fn test_rust_short_circuit_and() {
        let code = r#"
fn sc(a: bool, b: bool) -> i32 {
    if a && b {
        return 1;
    }
    0
}
"#;
        let cfg = build_cfg_for_function("rust", code, "sc").unwrap();
        let texts = rust_texts(&cfg);
        assert!(
            texts.iter().any(|t| t.trim() == "a"),
            "&& left must be its own branch, got {texts:?}"
        );
        assert!(
            texts.iter().any(|t| t.trim() == "b"),
            "&& right must be its own branch, got {texts:?}"
        );
        assert!(
            !texts.iter().any(|t| t.contains("a && b")),
            "must not keep a && b as one blob, got {texts:?}"
        );
    }

    #[test]
    fn test_rust_if_let_branches() {
        let code = r#"
fn il(opt: Option<i32>) -> i32 {
    if let Some(x) = opt {
        return x;
    }
    0
}
"#;
        let cfg = build_cfg_for_function("rust", code, "il").unwrap();
        assert!(
            cfg.edges.iter().any(|e| e.edge_type == CfgEdgeType::IfTrue)
                && cfg
                    .edges
                    .iter()
                    .any(|e| e.edge_type == CfgEdgeType::IfFalse),
            "if let must split true/false"
        );
        assert!(
            rust_kinds(&cfg).iter().any(|k| *k == StatementKind::Return),
            "if-let body return must lower"
        );
        assert!(
            !rust_texts(&cfg)
                .iter()
                .any(|t| t.trim_start().starts_with("if let Some")),
            "if let must not stay opaque, got {:?}",
            rust_texts(&cfg)
        );
    }

    #[test]
    fn test_rust_match_arms_and_guards() {
        let code = r#"
fn m(opt: Option<i32>) -> i32 {
    match opt {
        Some(v) if v > 0 => return v,
        Some(v) => return v + 1,
        None => return 0,
    }
}
"#;
        let cfg = build_cfg_for_function("rust", code, "m").unwrap();
        let returns = rust_kinds(&cfg)
            .iter()
            .filter(|k| **k == StatementKind::Return)
            .count();
        assert!(
            returns >= 3,
            "each arm return must lower, kinds={:?}",
            rust_kinds(&cfg)
        );
        assert!(
            !rust_texts(&cfg)
                .iter()
                .any(|t| t.contains("Some(v) if v > 0 =>") && t.contains("None")),
            "match must not stay one blob, got {:?}",
            rust_texts(&cfg)
        );
        // Guard condition should appear as its own branch text.
        assert!(
            rust_texts(&cfg)
                .iter()
                .any(|t| t.contains("v > 0") || t.trim() == "v > 0"),
            "match guard must lower, got {:?}",
            rust_texts(&cfg)
        );
        let ret_edges = cfg
            .edges
            .iter()
            .filter(|e| e.edge_type == CfgEdgeType::Return)
            .count();
        assert!(ret_edges >= 3, "Return edges from arms, got {ret_edges}");
    }

    #[test]
    fn test_rust_try_operator_early_return() {
        let code = r#"
fn t() -> Result<i32, ()> {
    let y = foo()?;
    Ok(y)
}
fn foo() -> Result<i32, ()> { Ok(1) }
"#;
        let cfg = build_cfg_for_function("rust", code, "t").unwrap();
        let texts = rust_texts(&cfg);
        assert!(
            texts
                .iter()
                .any(|t| t.contains("foo()?") || t.contains("?")),
            "? must appear as branch, got {texts:?}"
        );
        assert!(
            cfg.edges.iter().any(|e| {
                matches!(
                    e.edge_type,
                    CfgEdgeType::Return | CfgEdgeType::Exception | CfgEdgeType::IfFalse
                )
            }),
            "? error path must early-exit"
        );
        // Success path should still reach Ok(y) / trailing result.
        assert!(
            texts
                .iter()
                .any(|t| t.contains("Ok(y)") || t.contains("let y")),
            "success path must continue after ?, got {texts:?}"
        );
    }

    #[test]
    fn test_rust_infinite_loop_no_false_exit() {
        let code = r#"
fn forever() -> i32 {
    loop {
        break 1;
    }
}
"#;
        let cfg = build_cfg_for_function("rust", code, "forever").unwrap();
        assert!(cfg.has_cycle() || rust_texts(&cfg).iter().any(|t| t.contains("break")));
        // Header must not offer a false edge out of an infinite loop.
        let break_blocks = rust_block_ids(&cfg, "break");
        assert!(!break_blocks.is_empty(), "break must lower");
        assert!(
            cfg.edges
                .iter()
                .any(|e| break_blocks.contains(&e.from) && e.edge_type == CfgEdgeType::Jump),
            "break must Jump to loop exit"
        );
    }

    #[test]
    fn test_rust_for_in_value_and_cycle() {
        let code = r#"
fn sum(n: i32) -> i32 {
    let mut t = 0;
    for i in 0..n {
        t += i;
    }
    t
}
"#;
        let cfg = build_cfg_for_function("rust", code, "sum").unwrap();
        assert!(cfg.has_cycle());
        let texts = rust_texts(&cfg);
        assert!(
            texts
                .iter()
                .any(|t| t.contains("0..n") || t.contains("for")),
            "iterator value must appear, got {texts:?}"
        );
        assert!(
            texts
                .iter()
                .any(|t| t.contains("t += i") || t.contains("t+=i")),
            "body must lower, got {texts:?}"
        );
    }

    #[test]
    fn test_rust_labeled_break_continue() {
        let code = r#"
fn nested(n: i32) -> i32 {
    let mut s = 0;
    'outer: for i in 0..n {
        loop {
            if i == 0 {
                continue 'outer;
            }
            if i < 0 {
                break 'outer;
            }
            s += 1;
            break;
        }
    }
    s
}
"#;
        let cfg = build_cfg_for_function("rust", code, "nested").unwrap();
        let cont = rust_block_ids(&cfg, "continue 'outer");
        let brk = rust_block_ids(&cfg, "break 'outer");
        assert!(
            !cont.is_empty() && !brk.is_empty(),
            "labeled jumps must lower"
        );
        // continue 'outer should Jump to outer for header (not inner loop).
        assert!(
            cont.iter().any(|c| {
                cfg.edges
                    .iter()
                    .any(|e| e.from == *c && e.edge_type == CfgEdgeType::Jump)
            }),
            "continue 'outer must Jump"
        );
        assert!(
            brk.iter().any(|b| {
                cfg.edges
                    .iter()
                    .any(|e| e.from == *b && e.edge_type == CfgEdgeType::Jump)
            }),
            "break 'outer must Jump"
        );
        // Different targets: continue → header cycle path, break → exit (no shared sole target required,
        // but they must not both only target the same empty set).
        let cont_tgts: Vec<_> = cfg
            .edges
            .iter()
            .filter(|e| cont.contains(&e.from) && e.edge_type == CfgEdgeType::Jump)
            .map(|e| e.to)
            .collect();
        let brk_tgts: Vec<_> = cfg
            .edges
            .iter()
            .filter(|e| brk.contains(&e.from) && e.edge_type == CfgEdgeType::Jump)
            .map(|e| e.to)
            .collect();
        assert!(
            cont_tgts.iter().any(|t| !brk_tgts.contains(t))
                || brk_tgts.iter().any(|t| !cont_tgts.contains(t)),
            "continue and break 'outer must target different blocks"
        );
    }

    #[test]
    fn test_rust_panic_macro_exits() {
        let code = r#"
fn boom() {
    panic!("x");
}
"#;
        let cfg = build_cfg_for_function("rust", code, "boom").unwrap();
        assert!(
            rust_texts(&cfg).iter().any(|t| t.contains("panic")),
            "panic! must appear, got {:?}",
            rust_texts(&cfg)
        );
        assert!(
            cfg.edges.iter().any(|e| {
                matches!(
                    e.edge_type,
                    CfgEdgeType::Exception | CfgEdgeType::Return | CfgEdgeType::Jump
                )
            }),
            "panic! must terminate"
        );
    }

    #[test]
    fn test_rust_await_splits_block() {
        let code = r#"
async fn aw() -> i32 {
    let x = async { 1 }.await;
    x
}
"#;
        let cfg = build_cfg_for_function("rust", code, "aw").unwrap();
        assert!(
            rust_texts(&cfg).iter().any(|t| t.contains("await")),
            "await must appear, got {:?}",
            rust_texts(&cfg)
        );
        assert!(
            cfg.blocks.len() >= 3,
            "await should split basic blocks, blocks={}",
            cfg.blocks.len()
        );
    }

    #[test]
    fn test_rust_while_let_cycle() {
        let code = r#"
fn wl() -> i32 {
    let mut t = 0;
    while let Some(x) = None::<i32> {
        t += x;
    }
    t
}
"#;
        let cfg = build_cfg_for_function("rust", code, "wl").unwrap();
        assert!(cfg.has_cycle(), "while let must cycle");
        assert!(
            rust_texts(&cfg)
                .iter()
                .any(|t| t.contains("t += x") || t.contains("t+=x")),
            "while-let body must lower"
        );
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

    fn go_stmt_texts(cfg: &ControlFlowGraph) -> Vec<String> {
        let mut out = Vec::new();
        for b in cfg.blocks.values() {
            for s in &b.statements {
                out.push(s.text.clone());
            }
        }
        out
    }

    fn go_stmt_kinds(cfg: &ControlFlowGraph) -> Vec<StatementKind> {
        let mut out = Vec::new();
        for b in cfg.blocks.values() {
            for s in &b.statements {
                out.push(s.kind);
            }
        }
        out
    }

    #[test]
    fn test_go_if_initializer_before_condition() {
        let code = r#"
package demo

func IfInit(err error) int {
    if e := err; e != nil {
        return 1
    }
    return 0
}
"#;
        let cfg = build_cfg_for_function("go", code, "IfInit").unwrap();
        let texts = go_stmt_texts(&cfg);
        assert!(
            texts
                .iter()
                .any(|t| t.contains("e := err") || t.contains("e:=err")),
            "if initializer must be its own CFG statement, got {texts:?}"
        );
        assert!(
            texts
                .iter()
                .any(|t| t.contains("e != nil") || t.contains("e!=nil")),
            "condition should appear as branch text, got {texts:?}"
        );
        assert!(
            !texts.iter().any(|t| t.trim_start().starts_with("if e :=")),
            "whole if_statement should not be one Branch blob, got {texts:?}"
        );
        assert!(
            cfg.edges.iter().any(|e| e.edge_type == CfgEdgeType::Return),
            "return in then-branch must create Return edge"
        );
    }

    #[test]
    fn test_go_for_clause_init_cond_update_and_continue() {
        let code = r#"
package demo

func Sum(n int) int {
    total := 0
    for i := 0; i < n; i++ {
        if i == 1 {
            continue
        }
        total += i
    }
    return total
}
"#;
        let cfg = build_cfg_for_function("go", code, "Sum").unwrap();
        let texts = go_stmt_texts(&cfg);
        assert!(
            texts
                .iter()
                .any(|t| t.contains("i := 0") || t.contains("i:=0")),
            "for init must be visited, got {texts:?}"
        );
        assert!(
            texts
                .iter()
                .any(|t| t.contains("i < n") || t.contains("i<n")),
            "for condition must be header branch, got {texts:?}"
        );
        assert!(
            texts.iter().any(|t| t.contains("i++")),
            "for update must appear, got {texts:?}"
        );
        assert!(cfg.has_cycle(), "for must still cycle");

        // continue must target update (or a block that reaches update), not skip update forever.
        let continue_blocks: Vec<_> = cfg
            .blocks
            .values()
            .filter(|b| {
                b.statements
                    .iter()
                    .any(|s| s.kind == StatementKind::Jump && s.text.contains("continue"))
            })
            .collect();
        assert!(!continue_blocks.is_empty(), "expected continue statement");
        let update_blocks: Vec<_> = cfg
            .blocks
            .values()
            .filter(|b| b.statements.iter().any(|s| s.text.contains("i++")))
            .map(|b| b.id)
            .collect();
        assert!(!update_blocks.is_empty());
        let mut reaches_update = false;
        for b in &continue_blocks {
            for e in &cfg.edges {
                if e.from == b.id && update_blocks.contains(&e.to) {
                    reaches_update = true;
                }
            }
        }
        assert!(
            reaches_update,
            "continue must jump to update block containing i++"
        );
    }

    #[test]
    fn test_go_switch_case_bodies_emit_returns() {
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
        let kinds = go_stmt_kinds(&cfg);
        assert!(
            kinds
                .iter()
                .filter(|k| **k == StatementKind::Return)
                .count()
                >= 3,
            "each case return must be a Return statement, got {kinds:?}"
        );
        let returns = cfg
            .edges
            .iter()
            .filter(|e| e.edge_type == CfgEdgeType::Return)
            .count();
        assert!(
            returns >= 3,
            "expected Return edges from case bodies, got {returns}"
        );
        assert!(
            !go_stmt_texts(&cfg)
                .iter()
                .any(|t| t.trim_start().starts_with("case 1:")),
            "case arms must not remain opaque Expression blobs"
        );
    }

    #[test]
    fn test_go_switch_initializer_visited() {
        let code = r#"
package demo

func Pick(err error) int {
    switch e := err; e {
    case nil:
        return 0
    default:
        return 1
    }
}
"#;
        let cfg = build_cfg_for_function("go", code, "Pick").unwrap();
        let texts = go_stmt_texts(&cfg);
        assert!(
            texts
                .iter()
                .any(|t| t.contains("e := err") || t.contains("e:=err")),
            "switch initializer must be visited, got {texts:?}"
        );
        assert!(
            go_stmt_kinds(&cfg)
                .iter()
                .any(|k| *k == StatementKind::Return),
            "case body returns must be lowered"
        );
    }

    fn block_ids_containing(cfg: &ControlFlowGraph, needle: &str) -> Vec<BlockId> {
        cfg.blocks
            .values()
            .filter(|b| b.statements.iter().any(|s| s.text.contains(needle)))
            .map(|b| b.id)
            .collect()
    }

    fn has_edge_from_text_to_text(
        cfg: &ControlFlowGraph,
        from_needle: &str,
        to_needle: &str,
    ) -> bool {
        let froms = block_ids_containing(cfg, from_needle);
        let tos = block_ids_containing(cfg, to_needle);
        cfg.edges.iter().any(|e| {
            froms.contains(&e.from)
                && tos.contains(&e.to)
                && matches!(e.edge_type, CfgEdgeType::Jump | CfgEdgeType::Next)
        })
    }

    #[test]
    fn test_go_fallthrough_edges_to_next_case() {
        let code = r#"
package demo

func Ft(x int) int {
    switch x {
    case 1:
        x = x + 1
        fallthrough
    case 2:
        return x
    default:
        return 0
    }
}
"#;
        let cfg = build_cfg_for_function("go", code, "Ft").unwrap();
        assert!(
            go_stmt_texts(&cfg)
                .iter()
                .any(|t| t.contains("fallthrough")),
            "fallthrough must be a CFG statement"
        );
        assert!(
            has_edge_from_text_to_text(&cfg, "fallthrough", "return x")
                || has_edge_from_text_to_text(&cfg, "x = x + 1", "return x"),
            "fallthrough must connect case 1 body into case 2 body, edges={:?}",
            cfg.edges
                .iter()
                .map(|e| format!("{:?}", e.edge_type))
                .collect::<Vec<_>>()
        );
        // Case 1 must not only merge-exit past case 2 without a jump into it.
        let ft_blocks = block_ids_containing(&cfg, "fallthrough");
        let merge_only = cfg
            .edges
            .iter()
            .any(|e| ft_blocks.contains(&e.from) && e.edge_type == CfgEdgeType::Next);
        assert!(
            !merge_only || has_edge_from_text_to_text(&cfg, "fallthrough", "return x"),
            "fallthrough should Jump into next case, not only Next-merge"
        );
    }

    #[test]
    fn test_go_goto_and_label() {
        let code = r#"
package demo

func Jump(flag bool) int {
    if flag {
        goto Done
    }
    x := 1
Done:
    return x
}
"#;
        let cfg = build_cfg_for_function("go", code, "Jump").unwrap();
        assert!(
            go_stmt_texts(&cfg).iter().any(|t| t.contains("goto Done")),
            "goto must appear as Jump statement, got {:?}",
            go_stmt_texts(&cfg)
        );
        let froms = block_ids_containing(&cfg, "goto Done");
        let tos = block_ids_containing(&cfg, "return x");
        let jump = cfg.edges.iter().any(|e| {
            froms.contains(&e.from) && tos.contains(&e.to) && e.edge_type == CfgEdgeType::Jump
        });
        assert!(
            jump,
            "goto Done must Jump to the labeled return block, edges={:?}",
            cfg.edges
        );
        assert!(
            go_stmt_kinds(&cfg)
                .iter()
                .any(|k| *k == StatementKind::Return),
            "labeled return must be lowered as Return, not opaque label blob"
        );
    }

    #[test]
    fn test_go_short_circuit_and_or() {
        let code = r#"
package demo

func AndOr(a, b, c bool) int {
    if a && b {
        return 1
    }
    if a || c {
        return 2
    }
    return 0
}
"#;
        let cfg = build_cfg_for_function("go", code, "AndOr").unwrap();
        let texts = go_stmt_texts(&cfg);
        assert!(
            texts.iter().any(|t| t == "a" || t.trim() == "a"),
            "&& left operand should be its own branch, got {texts:?}"
        );
        assert!(
            texts.iter().any(|t| t == "b" || t.trim() == "b"),
            "&& right operand should be its own branch, got {texts:?}"
        );
        assert!(
            texts.iter().any(|t| t == "c" || t.trim() == "c"),
            "|| right operand should be its own branch, got {texts:?}"
        );
        assert!(
            !texts.iter().any(|t| t.contains("a && b")),
            "whole a && b must not remain a single Branch blob, got {texts:?}"
        );
        let if_true = cfg
            .edges
            .iter()
            .filter(|e| e.edge_type == CfgEdgeType::IfTrue)
            .count();
        // a&&b and a||c each need multiple conditional edges
        assert!(
            if_true >= 4,
            "short-circuit should create multiple IfTrue edges, got {if_true}"
        );
    }

    fn can_reach(cfg: &ControlFlowGraph, from: BlockId, to: BlockId) -> bool {
        use std::collections::{HashSet, VecDeque};
        let mut seen = HashSet::new();
        let mut q = VecDeque::from([from]);
        while let Some(n) = q.pop_front() {
            if n == to {
                return true;
            }
            if !seen.insert(n) {
                continue;
            }
            for e in &cfg.edges {
                if e.from == n {
                    q.push_back(e.to);
                }
            }
        }
        false
    }

    #[test]
    fn test_go_labeled_break_and_continue() {
        let code = r#"
package demo

func Nested() int {
    n := 0
Outer:
    for i := 0; i < 3; i++ {
        for j := 0; j < 3; j++ {
            if j == 1 {
                continue Outer
            }
            if j == 2 {
                break Outer
            }
            n++
        }
    }
    return n
}
"#;
        let cfg = build_cfg_for_function("go", code, "Nested").unwrap();
        let cont = block_ids_containing(&cfg, "continue Outer");
        let brk = block_ids_containing(&cfg, "break Outer");
        let updates = block_ids_containing(&cfg, "i++");
        assert!(!cont.is_empty() && !brk.is_empty() && !updates.is_empty());

        // continue Outer must reach outer update (i++), not only inner j++.
        let reaches_outer_update = cont.iter().any(|c| {
            updates.iter().any(|u| can_reach(&cfg, *c, *u))
                && cfg.edges.iter().any(|e| {
                    e.from == *c && e.edge_type == CfgEdgeType::Jump && updates.contains(&e.to)
                })
        });
        assert!(
            reaches_outer_update,
            "continue Outer must Jump directly to outer i++ update"
        );

        // break Outer must not Jump to an inner-only exit that still runs outer update.
        // It should Jump to a block from which i++ is NOT reached (loop exit).
        let break_skips_update = brk.iter().any(|b| {
            cfg.edges.iter().any(|e| {
                e.from == *b
                    && e.edge_type == CfgEdgeType::Jump
                    && !updates
                        .iter()
                        .any(|u| can_reach(&cfg, e.to, *u) && e.to != *u)
                    && updates.iter().all(|u| e.to != *u)
            })
        });
        assert!(
            break_skips_update,
            "break Outer must Jump to outer exit (not outer update)"
        );
    }

    #[test]
    fn test_go_defer_runs_before_return_exit() {
        let code = r#"
package demo

func cleanup() {}

func WithDefer() {
    defer cleanup()
    return
}
"#;
        let cfg = build_cfg_for_function("go", code, "WithDefer").unwrap();
        let texts = go_stmt_texts(&cfg);
        assert!(
            texts.iter().any(|t| t.contains("defer")),
            "defer must be recorded, got {texts:?}"
        );
        assert!(
            texts.iter().any(|t| t.contains("cleanup")),
            "deferred call must appear on unwind path, got {texts:?}"
        );
        let rets = block_ids_containing(&cfg, "return");
        let cleanups: Vec<_> = cfg
            .blocks
            .values()
            .filter(|b| {
                b.statements
                    .iter()
                    .any(|s| s.text.contains("cleanup") && !s.text.contains("defer"))
            })
            .map(|b| b.id)
            .collect();
        assert!(!rets.is_empty() && !cleanups.is_empty());
        // From return block, must reach a cleanup invocation before / on the way to a Return edge target.
        let ok = rets.iter().any(|r| {
            cleanups.iter().any(|c| {
                can_reach(&cfg, *r, *c) || {
                    // cleanup block may be after return stmt via Next, then Return edge
                    cfg.edges.iter().any(|e| e.from == *r && e.to == *c)
                }
            })
        });
        assert!(ok, "return must route into deferred cleanup before exit");
        assert!(
            cfg.edges.iter().any(|e| e.edge_type == CfgEdgeType::Return),
            "must still have a Return edge to exit"
        );
    }

    #[test]
    fn test_go_defer_lifo_and_panic_unwind() {
        let code = r#"
package demo

func a() {}
func b() {}

func PanicDefer() {
    defer a()
    defer b()
    panic("boom")
}
"#;
        let cfg = build_cfg_for_function("go", code, "PanicDefer").unwrap();
        let texts = go_stmt_texts(&cfg);
        assert!(
            texts.iter().any(|t| t.contains("panic")),
            "panic must appear, got {texts:?}"
        );
        // LIFO: b before a on unwind — find path panic -> b-call -> a-call
        let panic_blocks = block_ids_containing(&cfg, "panic");
        let b_blocks: Vec<_> = cfg
            .blocks
            .values()
            .filter(|b| {
                b.statements
                    .iter()
                    .any(|s| s.text.contains("b()") && !s.text.contains("defer"))
            })
            .map(|b| b.id)
            .collect();
        let a_blocks: Vec<_> = cfg
            .blocks
            .values()
            .filter(|b| {
                b.statements
                    .iter()
                    .any(|s| s.text.contains("a()") && !s.text.contains("defer"))
            })
            .map(|b| b.id)
            .collect();
        assert!(!panic_blocks.is_empty() && !b_blocks.is_empty() && !a_blocks.is_empty());
        let lifo = panic_blocks.iter().any(|p| {
            b_blocks.iter().any(|b| {
                can_reach(&cfg, *p, *b) && a_blocks.iter().any(|a| can_reach(&cfg, *b, *a))
            })
        });
        assert!(
            lifo,
            "panic must unwind defers LIFO (b then a), texts={texts:?}"
        );
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
    fn test_csharp_if_else_cfg() {
        let code = r#"
public class Demo {
    public int Abs(int x) {
        if (x > 0) {
            return x;
        }
        return -x;
    }
}
"#;
        let cfg = build_cfg_for_function("csharp", code, "Abs").unwrap();
        assert!(cfg.blocks.len() >= 4);
        assert!(cfg.edges.iter().any(|e| e.edge_type == CfgEdgeType::IfTrue));
    }

    #[test]
    fn test_csharp_loop_has_cycle() {
        let code = r#"
public class Demo {
    public int Sum(int n) {
        var total = 0;
        for (int i = 0; i < n; i++) {
            total += i;
        }
        return total;
    }
}
"#;
        let cfg = build_cfg_for_function("csharp", code, "Sum").unwrap();
        assert!(cfg.has_cycle());
    }

    fn cs_texts(cfg: &ControlFlowGraph) -> Vec<String> {
        cfg.blocks
            .values()
            .flat_map(|b| b.statements.iter().map(|s| s.text.clone()))
            .collect()
    }

    fn cs_kinds(cfg: &ControlFlowGraph) -> Vec<StatementKind> {
        cfg.blocks
            .values()
            .flat_map(|b| b.statements.iter().map(|s| s.kind))
            .collect()
    }

    fn cs_block_ids(cfg: &ControlFlowGraph, needle: &str) -> Vec<BlockId> {
        cfg.blocks
            .values()
            .filter(|b| b.statements.iter().any(|s| s.text.contains(needle)))
            .map(|b| b.id)
            .collect()
    }

    #[test]
    fn test_csharp_short_circuit_and() {
        let code = r#"
public class Demo {
    public int Sc(bool a, bool b) {
        if (a && b) { return 1; }
        return 0;
    }
}
"#;
        let cfg = build_cfg_for_function("csharp", code, "Sc").unwrap();
        let texts = cs_texts(&cfg);
        assert!(texts.iter().any(|t| t.trim() == "a"), "got {texts:?}");
        assert!(texts.iter().any(|t| t.trim() == "b"), "got {texts:?}");
    }

    #[test]
    fn test_csharp_for_continue_to_update() {
        let code = r#"
public class Demo {
    public int Sum(int n) {
        int total = 0;
        for (int i = 0; i < n; i++) {
            if (i == 1) { continue; }
            total += i;
        }
        return total;
    }
}
"#;
        let cfg = build_cfg_for_function("csharp", code, "Sum").unwrap();
        let cont = cs_block_ids(&cfg, "continue");
        let updates = cs_block_ids(&cfg, "i++");
        assert!(
            cont.iter().any(|c| {
                cfg.edges.iter().any(|e| {
                    e.from == *c && updates.contains(&e.to) && e.edge_type == CfgEdgeType::Jump
                })
            }),
            "continue must Jump to i++"
        );
    }

    #[test]
    fn test_csharp_foreach_cycle() {
        let code = r#"
public class Demo {
    public int Fe(int[] a) {
        int t = 0;
        foreach (var v in a) {
            t += v;
        }
        return t;
    }
}
"#;
        let cfg = build_cfg_for_function("csharp", code, "Fe").unwrap();
        assert!(cfg.has_cycle(), "foreach must cycle");
        assert!(
            cs_texts(&cfg)
                .iter()
                .any(|t| t.contains("t += v") || t.contains("t+=v")),
            "body must lower, got {:?}",
            cs_texts(&cfg)
        );
        assert!(
            !cs_texts(&cfg)
                .iter()
                .any(|t| t.trim_start().starts_with("foreach")),
            "foreach must not stay opaque, got {:?}",
            cs_texts(&cfg)
        );
    }

    #[test]
    fn test_csharp_switch_sections_and_when() {
        let code = r#"
public class Demo {
    public int Sw(int x) {
        switch (x) {
            case 1:
                return 10;
            case int i when i > 0:
                return i;
            default:
                return 0;
        }
    }
}
"#;
        let cfg = build_cfg_for_function("csharp", code, "Sw").unwrap();
        assert!(
            cs_kinds(&cfg)
                .iter()
                .filter(|k| **k == StatementKind::Return)
                .count()
                >= 3,
            "section returns must lower, kinds={:?}",
            cs_kinds(&cfg)
        );
        assert!(
            cs_texts(&cfg)
                .iter()
                .any(|t| t.contains("i > 0") || t.trim() == "i > 0"),
            "when guard must lower, got {:?}",
            cs_texts(&cfg)
        );
    }

    #[test]
    fn test_csharp_switch_expression_arms() {
        let code = r#"
public class Demo {
    public string Se(int x) {
        return x switch {
            > 0 => "pos",
            _ => "neg"
        };
    }
}
"#;
        let cfg = build_cfg_for_function("csharp", code, "Se").unwrap();
        assert!(
            cfg.edges
                .iter()
                .filter(|e| e.edge_type == CfgEdgeType::IfTrue)
                .count()
                >= 1,
            "switch expression needs arm fan-out"
        );
        assert!(
            !cs_texts(&cfg)
                .iter()
                .any(|t| t.contains("> 0 =>") && t.contains("_ =>")),
            "must not stay one blob, got {:?}",
            cs_texts(&cfg)
        );
    }

    #[test]
    fn test_csharp_null_coalescing() {
        let code = r#"
public class Demo {
    public string Nc(string a, string b) {
        return a ?? b;
    }
}
"#;
        let cfg = build_cfg_for_function("csharp", code, "Nc").unwrap();
        assert!(
            cfg.edges.iter().any(|e| e.edge_type == CfgEdgeType::IfTrue)
                && cfg
                    .edges
                    .iter()
                    .any(|e| e.edge_type == CfgEdgeType::IfFalse),
            "?? must split"
        );
        assert!(
            cs_texts(&cfg)
                .iter()
                .any(|t| t.contains("a") || t.contains("??")),
            "?? operands must appear, got {:?}",
            cs_texts(&cfg)
        );
    }

    #[test]
    fn test_csharp_null_conditional() {
        let code = r#"
public class Demo {
    public int Na(Demo d) {
        return d?.GetHashCode() ?? 0;
    }
}
"#;
        let cfg = build_cfg_for_function("csharp", code, "Na").unwrap();
        assert!(
            cfg.edges.iter().any(|e| e.edge_type == CfgEdgeType::IfTrue)
                && cfg
                    .edges
                    .iter()
                    .any(|e| e.edge_type == CfgEdgeType::IfFalse),
            "?. must split"
        );
    }

    #[test]
    fn test_csharp_try_catch_when_finally() {
        let code = r#"
public class Demo {
    public int Tw() {
        try {
            throw new System.Exception();
        } catch (System.Exception e) when (e != null) {
            return 1;
        } finally {
            System.Console.WriteLine("cleanup");
        }
    }
}
"#;
        let cfg = build_cfg_for_function("csharp", code, "Tw").unwrap();
        assert!(
            cfg.edges
                .iter()
                .any(|e| e.edge_type == CfgEdgeType::Exception),
            "try/catch Exception edge"
        );
        assert!(
            cs_texts(&cfg)
                .iter()
                .any(|t| t.contains("cleanup") || t.contains("WriteLine")),
            "finally must appear on throw/return path, got {:?}",
            cs_texts(&cfg)
        );
        assert!(
            cs_texts(&cfg)
                .iter()
                .any(|t| t.contains("e != null") || t.contains("e!=null")),
            "catch when filter must lower, got {:?}",
            cs_texts(&cfg)
        );
    }

    #[test]
    fn test_csharp_using_dispose_on_return() {
        let code = r#"
public class Demo {
    public int Us() {
        using (var r = new System.IO.StringReader("")) {
            return 1;
        }
    }
}
"#;
        let cfg = build_cfg_for_function("csharp", code, "Us").unwrap();
        assert!(
            cs_texts(&cfg)
                .iter()
                .any(|t| t.contains("Dispose") || t.contains("r.Dispose")),
            "using must emit Dispose on exit, got {:?}",
            cs_texts(&cfg)
        );
        let ret = cs_block_ids(&cfg, "return 1");
        let disp = cs_block_ids(&cfg, "Dispose");
        assert!(!ret.is_empty() && !disp.is_empty());
        assert!(
            ret.iter().any(|r| {
                disp.iter().any(|d| {
                    use std::collections::{HashSet, VecDeque};
                    let mut seen = HashSet::new();
                    let mut q = VecDeque::from([*r]);
                    while let Some(n) = q.pop_front() {
                        if n == *d {
                            return true;
                        }
                        if !seen.insert(n) {
                            continue;
                        }
                        for e in &cfg.edges {
                            if e.from == n {
                                q.push_back(e.to);
                            }
                        }
                    }
                    false
                })
            }),
            "return must reach Dispose"
        );
    }

    #[test]
    fn test_csharp_lock_exit_on_return() {
        let code = r#"
public class Demo {
    public int Lk(object o) {
        lock (o) {
            return 1;
        }
    }
}
"#;
        let cfg = build_cfg_for_function("csharp", code, "Lk").unwrap();
        assert!(
            cs_texts(&cfg)
                .iter()
                .any(|t| t.contains("Monitor.Exit") || t.contains("Exit")),
            "lock must emit Monitor.Exit, got {:?}",
            cs_texts(&cfg)
        );
    }

    #[test]
    fn test_csharp_await_splits_block() {
        let code = r#"
public class Demo {
    public async System.Threading.Tasks.Task<int> Aw() {
        await System.Threading.Tasks.Task.Delay(1);
        return 1;
    }
}
"#;
        let cfg = build_cfg_for_function("csharp", code, "Aw").unwrap();
        assert!(
            cs_texts(&cfg).iter().any(|t| t.contains("await")),
            "await must appear, got {:?}",
            cs_texts(&cfg)
        );
        assert!(cfg.blocks.len() >= 3, "await should split blocks");
    }

    #[test]
    fn test_csharp_yield_return_and_break() {
        let code = r#"
public class Demo {
    public System.Collections.Generic.IEnumerable<int> Y() {
        yield return 1;
        yield break;
    }
}
"#;
        let cfg = build_cfg_for_function("csharp", code, "Y").unwrap();
        assert!(
            cs_texts(&cfg).iter().any(|t| t.contains("yield return")),
            "yield return must appear, got {:?}",
            cs_texts(&cfg)
        );
        assert!(
            cfg.edges.iter().any(|e| {
                matches!(
                    e.edge_type,
                    CfgEdgeType::Return | CfgEdgeType::Exception | CfgEdgeType::Jump
                )
            }),
            "yield break must terminate"
        );
    }

    #[test]
    fn test_csharp_goto_and_label() {
        let code = r#"
public class Demo {
    public int G(int x) {
        if (x < 0) { goto done; }
        x++;
        done:
        return x;
    }
}
"#;
        let cfg = build_cfg_for_function("csharp", code, "G").unwrap();
        let froms = cs_block_ids(&cfg, "goto done");
        assert!(!froms.is_empty());
        assert!(
            cfg.edges
                .iter()
                .any(|e| froms.contains(&e.from) && e.edge_type == CfgEdgeType::Jump),
            "goto must Jump"
        );
    }

    #[test]
    fn test_csharp_using_declaration_dispose() {
        let code = r#"
public class Demo {
    public int Ud() {
        using var r = new System.IO.StringReader("");
        return 1;
    }
}
"#;
        let cfg = build_cfg_for_function("csharp", code, "Ud").unwrap();
        assert!(
            cs_texts(&cfg)
                .iter()
                .any(|t| t.contains("Dispose") || t.contains("r.Dispose")),
            "using declaration must Dispose on exit, got {:?}",
            cs_texts(&cfg)
        );
        let ret = cs_block_ids(&cfg, "return 1");
        let disp = cs_block_ids(&cfg, "Dispose");
        assert!(!ret.is_empty() && !disp.is_empty());
        assert!(
            ret.iter().any(|r| {
                disp.iter().any(|d| {
                    use std::collections::{HashSet, VecDeque};
                    let mut seen = HashSet::new();
                    let mut q = VecDeque::from([*r]);
                    while let Some(n) = q.pop_front() {
                        if n == *d {
                            return true;
                        }
                        if !seen.insert(n) {
                            continue;
                        }
                        for e in &cfg.edges {
                            if e.from == n {
                                q.push_back(e.to);
                            }
                        }
                    }
                    false
                })
            }),
            "return must reach Dispose"
        );
    }

    #[test]
    fn test_csharp_goto_case_and_default() {
        let code = r#"
public class Demo {
    public int Gc(int x) {
        switch (x) {
            case 1:
                goto case 2;
            case 2:
                goto default;
            default:
                return 0;
        }
    }
}
"#;
        let cfg = build_cfg_for_function("csharp", code, "Gc").unwrap();
        let goto_case = cs_block_ids(&cfg, "goto case 2");
        let goto_def = cs_block_ids(&cfg, "goto default");
        let case2 = cs_block_ids(&cfg, "goto default");
        let ret0 = cs_block_ids(&cfg, "return 0");
        assert!(!goto_case.is_empty() && !goto_def.is_empty());
        assert!(
            goto_case.iter().any(|g| {
                cfg.edges.iter().any(|e| {
                    e.from == *g && e.edge_type == CfgEdgeType::Jump && case2.contains(&e.to)
                })
            }),
            "goto case 2 must Jump into case 2 block"
        );
        assert!(
            goto_def.iter().any(|g| {
                ret0.iter().any(|r| {
                    cfg.edges
                        .iter()
                        .any(|e| e.from == *g && e.to == *r && e.edge_type == CfgEdgeType::Jump)
                        || {
                            use std::collections::{HashSet, VecDeque};
                            let mut seen = HashSet::new();
                            let mut q = VecDeque::from([*g]);
                            while let Some(n) = q.pop_front() {
                                if n == *r {
                                    return true;
                                }
                                if !seen.insert(n) {
                                    continue;
                                }
                                for e in &cfg.edges {
                                    if e.from == n && e.edge_type == CfgEdgeType::Jump {
                                        q.push_back(e.to);
                                    }
                                }
                            }
                            false
                        }
                })
            }),
            "goto default must reach default body"
        );
    }

    #[test]
    fn test_csharp_local_function_is_subcfg() {
        let code = r#"
public class Demo {
    public int Lf() {
        int Inner(int y) {
            return y + 1;
        }
        return Inner(1);
    }
}
"#;
        let cfg = build_cfg_for_function("csharp", code, "Lf").unwrap();
        assert!(
            cs_texts(&cfg)
                .iter()
                .any(|t| t.contains("Inner") && t.contains("y + 1") || t.contains("return y + 1")),
            "local function body must be lowered, got {:?}",
            cs_texts(&cfg)
        );
        let parent_ret = cs_block_ids(&cfg, "return Inner(1)");
        let inner_ret = cs_block_ids(&cfg, "return y + 1");
        assert!(!parent_ret.is_empty() && !inner_ret.is_empty());
        // Parent sequential flow must not enter Inner's body before calling it.
        assert!(
            !parent_ret.iter().any(|p| {
                inner_ret
                    .iter()
                    .any(|i| cfg.edges.iter().any(|e| e.from == *p && e.to == *i))
            }),
            "parent return must not fall into Inner body"
        );
        // Inner body should not be reachable from entry via Next-only path through parent return.
        let entry = cfg.entry;
        assert!(
            !inner_ret.iter().any(|i| {
                use std::collections::{HashSet, VecDeque};
                let mut seen = HashSet::new();
                let mut q = VecDeque::from([entry]);
                while let Some(n) = q.pop_front() {
                    if n == *i {
                        return true;
                    }
                    if !seen.insert(n) {
                        continue;
                    }
                    for e in &cfg.edges {
                        if e.from == n
                            && matches!(
                                e.edge_type,
                                CfgEdgeType::Next | CfgEdgeType::IfTrue | CfgEdgeType::IfFalse
                            )
                        {
                            q.push_back(e.to);
                        }
                    }
                }
                false
            }),
            "Inner body must be a disconnected sub-CFG (not on parent Next path)"
        );
    }

    #[test]
    fn test_csharp_lambda_is_subcfg() {
        let code = r#"
public class Demo {
    public System.Func<int,int> Lam() {
        return x => {
            if (x > 0) { return x; }
            return -x;
        };
    }
}
"#;
        let cfg = build_cfg_for_function("csharp", code, "Lam").unwrap();
        assert!(
            cfg.edges.iter().any(|e| e.edge_type == CfgEdgeType::IfTrue),
            "lambda body if must lower, edges={:?}",
            cfg.edges.iter().map(|e| e.edge_type).collect::<Vec<_>>()
        );
        assert!(
            cs_kinds(&cfg)
                .iter()
                .filter(|k| **k == StatementKind::Return)
                .count()
                >= 2,
            "lambda returns must lower, kinds={:?}",
            cs_kinds(&cfg)
        );
    }

    #[test]
    fn test_csharp_await_suspend_and_resume() {
        let code = r#"
public class Demo {
    public async System.Threading.Tasks.Task<int> Aw() {
        await System.Threading.Tasks.Task.Delay(1);
        return 1;
    }
}
"#;
        let cfg = build_cfg_for_function("csharp", code, "Aw").unwrap();
        let await_blocks = cs_block_ids(&cfg, "await");
        assert!(!await_blocks.is_empty());
        assert!(
            await_blocks.iter().any(|a| {
                cfg.edges
                    .iter()
                    .any(|e| e.from == *a && e.edge_type == CfgEdgeType::IfTrue)
                    && cfg.edges.iter().any(|e| {
                        e.from == *a
                            && matches!(e.edge_type, CfgEdgeType::IfFalse | CfgEdgeType::Return)
                    })
            }),
            "await must split resume (IfTrue) vs suspend (IfFalse/Return)"
        );
    }

    #[test]
    fn test_csharp_await_in_var_declaration() {
        let code = r#"
public class Demo {
    public async System.Threading.Tasks.Task<int> Aw() {
        var t = await System.Threading.Tasks.Task.FromResult(1);
        return t;
    }
}
"#;
        let cfg = build_cfg_for_function("csharp", code, "Aw").unwrap();
        assert!(
            cs_texts(&cfg).iter().any(|t| t.contains("await")),
            "await in var init must appear, got {:?}",
            cs_texts(&cfg)
        );
        assert!(
            cfg.edges.iter().any(|e| e.edge_type == CfgEdgeType::IfTrue)
                && cfg
                    .edges
                    .iter()
                    .any(|e| e.edge_type == CfgEdgeType::IfFalse),
            "var await must suspend/resume split"
        );
    }

    #[test]
    fn test_c_if_else_cfg() {
        let code = r#"
int abs_val(int x) {
    if (x > 0) {
        return x;
    }
    return -x;
}
"#;
        let cfg = build_cfg_for_function("c", code, "abs_val").unwrap();
        assert!(cfg.blocks.len() >= 4);
        assert!(cfg.edges.iter().any(|e| e.edge_type == CfgEdgeType::IfTrue));
    }

    #[test]
    fn test_c_for_loop_has_cycle() {
        let code = r#"
int sum_n(int n) {
    int total = 0;
    for (int i = 0; i < n; i++) {
        total += i;
    }
    return total;
}
"#;
        let cfg = build_cfg_for_function("c", code, "sum_n").unwrap();
        assert!(cfg.has_cycle());
    }

    #[test]
    fn test_c_switch_cfg() {
        let code = r#"
int classify(int x) {
    switch (x) {
        case 1: return 10;
        case 2: return 20;
        default: return 0;
    }
}
"#;
        let cfg = build_cfg_for_function("c", code, "classify").unwrap();
        assert!(cfg.blocks.len() >= 4);
    }

    fn c_texts(cfg: &ControlFlowGraph) -> Vec<String> {
        cfg.blocks
            .values()
            .flat_map(|b| b.statements.iter().map(|s| s.text.clone()))
            .collect()
    }

    fn c_kinds(cfg: &ControlFlowGraph) -> Vec<StatementKind> {
        cfg.blocks
            .values()
            .flat_map(|b| b.statements.iter().map(|s| s.kind))
            .collect()
    }

    fn c_block_ids(cfg: &ControlFlowGraph, needle: &str) -> Vec<BlockId> {
        cfg.blocks
            .values()
            .filter(|b| b.statements.iter().any(|s| s.text.contains(needle)))
            .map(|b| b.id)
            .collect()
    }

    #[test]
    fn test_c_short_circuit_and() {
        let code = r#"
int sc(int a, int b) {
    if (a && b) {
        return 1;
    }
    return 0;
}
"#;
        let cfg = build_cfg_for_function("c", code, "sc").unwrap();
        let texts = c_texts(&cfg);
        assert!(
            texts.iter().any(|t| t.trim() == "a"),
            "&& left must split, got {texts:?}"
        );
        assert!(
            texts.iter().any(|t| t.trim() == "b"),
            "&& right must split, got {texts:?}"
        );
        assert!(
            !texts.iter().any(|t| t.contains("a && b")),
            "must not keep (a && b) as one blob, got {texts:?}"
        );
    }

    #[test]
    fn test_c_for_init_cond_update_continue() {
        let code = r#"
int sum(int n) {
    int total = 0;
    for (int i = 0; i < n; i++) {
        if (i == 1) {
            continue;
        }
        total += i;
    }
    return total;
}
"#;
        let cfg = build_cfg_for_function("c", code, "sum").unwrap();
        let texts = c_texts(&cfg);
        assert!(
            texts
                .iter()
                .any(|t| t.contains("i = 0") || t.contains("i=0")),
            "for init, got {texts:?}"
        );
        assert!(
            texts
                .iter()
                .any(|t| t.contains("i < n") || t.contains("i<n")),
            "for condition, got {texts:?}"
        );
        assert!(
            texts.iter().any(|t| t.contains("i++")),
            "for update, got {texts:?}"
        );
        assert!(cfg.has_cycle());
        let cont = c_block_ids(&cfg, "continue");
        let updates = c_block_ids(&cfg, "i++");
        assert!(
            cont.iter().any(|c| {
                cfg.edges.iter().any(|e| {
                    e.from == *c && updates.contains(&e.to) && e.edge_type == CfgEdgeType::Jump
                })
            }),
            "continue must Jump to i++"
        );
    }

    #[test]
    fn test_c_do_while_cycle() {
        let code = r#"
int dw(int n) {
    int i = 0;
    do {
        i++;
    } while (i < n);
    return i;
}
"#;
        let cfg = build_cfg_for_function("c", code, "dw").unwrap();
        assert!(cfg.has_cycle(), "do-while must cycle");
        assert!(
            c_texts(&cfg).iter().any(|t| t.contains("i++")),
            "body must lower"
        );
        assert!(
            c_texts(&cfg)
                .iter()
                .any(|t| t.contains("i < n") || t.contains("i<n")),
            "condition must be on header, got {:?}",
            c_texts(&cfg)
        );
    }

    #[test]
    fn test_c_switch_fallthrough_and_returns() {
        let code = r#"
int sw(int x) {
    int y = 0;
    switch (x) {
        case 1:
            y = 10;
        case 2:
            y = 20;
            break;
        default:
            return 0;
    }
    return y;
}
"#;
        let cfg = build_cfg_for_function("c", code, "sw").unwrap();
        assert!(
            !c_texts(&cfg)
                .iter()
                .any(|t| t.trim_start().starts_with("switch")),
            "switch must not be opaque, got {:?}",
            c_texts(&cfg)
        );
        assert!(
            c_texts(&cfg)
                .iter()
                .any(|t| t.contains("y = 10") || t.contains("y=10")),
            "case 1 body must lower, got {:?}",
            c_texts(&cfg)
        );
        assert!(
            c_texts(&cfg)
                .iter()
                .any(|t| t.contains("y = 20") || t.contains("y=20")),
            "case 2 body must lower, got {:?}",
            c_texts(&cfg)
        );
        let c1 = c_block_ids(&cfg, "y = 10");
        let c1b = c_block_ids(&cfg, "y=10");
        let froms: Vec<_> = c1.into_iter().chain(c1b).collect();
        let c2 = c_block_ids(&cfg, "y = 20");
        let c2b = c_block_ids(&cfg, "y=20");
        let tos: Vec<_> = c2.into_iter().chain(c2b).collect();
        assert!(
            froms.iter().any(|f| {
                tos.iter().any(|t| {
                    cfg.edges
                        .iter()
                        .any(|e| e.from == *f && e.to == *t && e.edge_type == CfgEdgeType::Jump)
                })
            }),
            "case 1 must fall through (Jump) into case 2"
        );
        assert!(
            c_kinds(&cfg).iter().any(|k| *k == StatementKind::Return),
            "default return must lower"
        );
    }

    #[test]
    fn test_c_goto_and_label() {
        let code = r#"
int g(int x) {
    if (x < 0) {
        goto done;
    }
    x = x + 1;
done:
    return x;
}
"#;
        let cfg = build_cfg_for_function("c", code, "g").unwrap();
        assert!(
            c_texts(&cfg).iter().any(|t| t.contains("goto done")),
            "goto must appear, got {:?}",
            c_texts(&cfg)
        );
        let froms = c_block_ids(&cfg, "goto done");
        let rets = c_block_ids(&cfg, "return x");
        assert!(!froms.is_empty() && !rets.is_empty());
        assert!(
            froms.iter().any(|f| {
                rets.iter().any(|r| {
                    cfg.edges.iter().any(|e| e.from == *f && e.to == *r) || {
                        use std::collections::{HashSet, VecDeque};
                        let mut seen = HashSet::new();
                        let mut q = VecDeque::from([*f]);
                        while let Some(n) = q.pop_front() {
                            if n == *r {
                                return true;
                            }
                            if !seen.insert(n) {
                                continue;
                            }
                            for e in &cfg.edges {
                                if e.from == n {
                                    q.push_back(e.to);
                                }
                            }
                        }
                        false
                    }
                })
            }),
            "goto done must reach labeled return"
        );
        // Unreachable assignment after goto should not be required on the goto path.
        assert!(
            cfg.edges
                .iter()
                .any(|e| froms.contains(&e.from) && e.edge_type == CfgEdgeType::Jump),
            "goto must Jump"
        );
    }

    #[test]
    fn test_c_ternary_branches() {
        let code = r#"
int t(int y) {
    return y ? y : -1;
}
"#;
        let cfg = build_cfg_for_function("c", code, "t").unwrap();
        assert!(
            cfg.edges.iter().any(|e| e.edge_type == CfgEdgeType::IfTrue)
                && cfg
                    .edges
                    .iter()
                    .any(|e| e.edge_type == CfgEdgeType::IfFalse),
            "ternary must split"
        );
        // Condition should be its own branch text (not only the whole ternary as one stmt).
        assert!(
            c_texts(&cfg).iter().any(|t| t.trim() == "y"),
            "ternary condition must lower, got {:?}",
            c_texts(&cfg)
        );
    }

    #[test]
    fn test_c_abort_and_exit_terminate() {
        let code = r#"
void boom(void) {
    abort();
}
"#;
        let cfg = build_cfg_for_function("c", code, "boom").unwrap();
        assert!(
            c_texts(&cfg).iter().any(|t| t.contains("abort")),
            "abort must appear"
        );
        assert!(
            cfg.edges.iter().any(|e| {
                matches!(
                    e.edge_type,
                    CfgEdgeType::Exception | CfgEdgeType::Return | CfgEdgeType::Jump
                )
            }),
            "abort must terminate"
        );

        let code2 = r#"
void bye(void) {
    exit(1);
}
"#;
        let cfg2 = build_cfg_for_function("c", code2, "bye").unwrap();
        assert!(
            cfg2.edges.iter().any(|e| {
                matches!(
                    e.edge_type,
                    CfgEdgeType::Exception | CfgEdgeType::Return | CfgEdgeType::Jump
                )
            }),
            "exit must terminate"
        );
    }

    #[test]
    fn test_c_setjmp_longjmp_approx() {
        let code = r#"
int sj(void) {
    if (setjmp(buf) == 0) {
        longjmp(buf, 1);
    }
    return 1;
}
"#;
        let cfg = build_cfg_for_function("c", code, "sj").unwrap();
        assert!(
            c_texts(&cfg).iter().any(|t| t.contains("setjmp")),
            "setjmp must appear, got {:?}",
            c_texts(&cfg)
        );
        assert!(
            c_texts(&cfg).iter().any(|t| t.contains("longjmp")),
            "longjmp must appear, got {:?}",
            c_texts(&cfg)
        );
        let sj = c_block_ids(&cfg, "setjmp");
        let lj = c_block_ids(&cfg, "longjmp");
        assert!(!sj.is_empty() && !lj.is_empty());
        assert!(
            lj.iter().any(|l| {
                sj.iter().any(|s| {
                    cfg.edges.iter().any(|e| {
                        e.from == *l
                            && e.to == *s
                            && matches!(e.edge_type, CfgEdgeType::Jump | CfgEdgeType::Exception)
                    })
                })
            }),
            "longjmp must edge back to a setjmp site"
        );
    }

    #[test]
    fn test_cpp_if_else_cfg() {
        let code = r#"
int abs_val(int x) {
    if (x > 0) {
        return x;
    }
    return -x;
}
"#;
        let cfg = build_cfg_for_function("cpp", code, "abs_val").unwrap();
        assert!(cfg.blocks.len() >= 4);
        assert!(cfg.edges.iter().any(|e| e.edge_type == CfgEdgeType::IfTrue));
    }

    #[test]
    fn test_cpp_range_for_has_cycle() {
        // Classic indexed for (kept for regression).
        let code = r#"
int sum_vec(int* arr, int n) {
    int total = 0;
    for (int i = 0; i < n; i++) {
        total += arr[i];
    }
    return total;
}
"#;
        let cfg = build_cfg_for_function("cpp", code, "sum_vec").unwrap();
        assert!(cfg.has_cycle());
    }

    fn cpp_texts(cfg: &ControlFlowGraph) -> Vec<String> {
        cfg.blocks
            .values()
            .flat_map(|b| b.statements.iter().map(|s| s.text.clone()))
            .collect()
    }

    fn cpp_kinds(cfg: &ControlFlowGraph) -> Vec<StatementKind> {
        cfg.blocks
            .values()
            .flat_map(|b| b.statements.iter().map(|s| s.kind))
            .collect()
    }

    fn cpp_block_ids(cfg: &ControlFlowGraph, needle: &str) -> Vec<BlockId> {
        cfg.blocks
            .values()
            .filter(|b| b.statements.iter().any(|s| s.text.contains(needle)))
            .map(|b| b.id)
            .collect()
    }

    #[test]
    fn test_cpp_short_circuit_and() {
        let code = r#"
int sc(bool a, bool b) {
    if (a && b) {
        return 1;
    }
    return 0;
}
"#;
        let cfg = build_cfg_for_function("cpp", code, "sc").unwrap();
        let texts = cpp_texts(&cfg);
        assert!(
            texts.iter().any(|t| t.trim() == "a"),
            "&& left must split, got {texts:?}"
        );
        assert!(
            texts.iter().any(|t| t.trim() == "b"),
            "&& right must split, got {texts:?}"
        );
    }

    #[test]
    fn test_cpp_if_init_before_condition() {
        let code = r#"
int demo(int x) {
    if (int y = x; y > 0) {
        return y;
    }
    return 0;
}
"#;
        let cfg = build_cfg_for_function("cpp", code, "demo").unwrap();
        let texts = cpp_texts(&cfg);
        assert!(
            texts
                .iter()
                .any(|t| t.contains("int y = x") || t.contains("y = x")),
            "C++17 if initializer must appear before condition, got {texts:?}"
        );
        assert!(
            texts
                .iter()
                .any(|t| t.contains("y > 0") || t.trim() == "y > 0"),
            "condition value must be its own branch, got {texts:?}"
        );
        assert!(
            !texts.iter().any(|t| t.contains("int y = x; y > 0")),
            "must not keep whole condition_clause as one blob, got {texts:?}"
        );
    }

    #[test]
    fn test_cpp_switch_init_and_cases() {
        let code = r#"
int sw(int x) {
    switch (int z = x; z) {
        case 1:
            return 10;
        case 2:
            return 20;
        default:
            return 0;
    }
}
"#;
        let cfg = build_cfg_for_function("cpp", code, "sw").unwrap();
        let texts = cpp_texts(&cfg);
        assert!(
            texts
                .iter()
                .any(|t| t.contains("int z = x") || t.contains("z = x")),
            "switch initializer must appear, got {texts:?}"
        );
        assert!(
            cpp_kinds(&cfg)
                .iter()
                .filter(|k| **k == StatementKind::Return)
                .count()
                >= 3,
            "case returns must lower, kinds={:?}",
            cpp_kinds(&cfg)
        );
    }

    #[test]
    fn test_cpp_switch_fallthrough() {
        let code = r#"
int sw(int x) {
    int y = 0;
    switch (x) {
        case 1:
            y = 10;
        case 2:
            y = 20;
            break;
        default:
            return 0;
    }
    return y;
}
"#;
        let cfg = build_cfg_for_function("cpp", code, "sw").unwrap();
        let c1 = cpp_block_ids(&cfg, "y = 10");
        let c2 = cpp_block_ids(&cfg, "y = 20");
        assert!(!c1.is_empty() && !c2.is_empty(), "case bodies must lower");
        assert!(
            c1.iter().any(|f| {
                c2.iter().any(|t| {
                    cfg.edges
                        .iter()
                        .any(|e| e.from == *f && e.to == *t && e.edge_type == CfgEdgeType::Jump)
                })
            }),
            "case 1 must fall through into case 2"
        );
    }

    #[test]
    fn test_cpp_for_range_loop() {
        let code = r#"
int range_sum(int* a, int n) {
    int t = 0;
    for (int v : a) {
        t += v;
    }
    return t;
}
"#;
        let cfg = build_cfg_for_function("cpp", code, "range_sum").unwrap();
        assert!(cfg.has_cycle(), "range-for must cycle");
        assert!(
            cpp_texts(&cfg)
                .iter()
                .any(|t| t.contains("t += v") || t.contains("t+=v")),
            "body must lower, got {:?}",
            cpp_texts(&cfg)
        );
        assert!(
            !cpp_texts(&cfg)
                .iter()
                .any(|t| t.trim_start().starts_with("for (int v")),
            "range-for must not stay opaque, got {:?}",
            cpp_texts(&cfg)
        );
    }

    #[test]
    fn test_cpp_try_catch_throw() {
        let code = r#"
int throws() {
    try {
        throw 1;
    } catch (int e) {
        return e;
    }
    return 0;
}
"#;
        let cfg = build_cfg_for_function("cpp", code, "throws").unwrap();
        assert!(
            cfg.edges
                .iter()
                .any(|e| e.edge_type == CfgEdgeType::Exception),
            "try/catch needs Exception edge"
        );
        assert!(
            cpp_texts(&cfg).iter().any(|t| t.contains("throw")),
            "throw must appear, got {:?}",
            cpp_texts(&cfg)
        );
        assert!(
            cpp_kinds(&cfg).iter().any(|k| *k == StatementKind::Return),
            "catch return must lower"
        );
    }

    #[test]
    fn test_cpp_ternary_branches() {
        let code = r#"
int t(int y) {
    return y ? y : -1;
}
"#;
        let cfg = build_cfg_for_function("cpp", code, "t").unwrap();
        assert!(
            cfg.edges.iter().any(|e| e.edge_type == CfgEdgeType::IfTrue)
                && cfg
                    .edges
                    .iter()
                    .any(|e| e.edge_type == CfgEdgeType::IfFalse),
            "ternary must split"
        );
        assert!(
            cpp_texts(&cfg).iter().any(|t| t.trim() == "y"),
            "ternary condition must lower, got {:?}",
            cpp_texts(&cfg)
        );
    }

    #[test]
    fn test_cpp_goto_and_label() {
        let code = r#"
int g(int x) {
    if (x < 0) {
        goto done;
    }
    x = x + 1;
done:
    return x;
}
"#;
        let cfg = build_cfg_for_function("cpp", code, "g").unwrap();
        let froms = cpp_block_ids(&cfg, "goto done");
        assert!(!froms.is_empty(), "goto must appear");
        assert!(
            cfg.edges
                .iter()
                .any(|e| froms.contains(&e.from) && e.edge_type == CfgEdgeType::Jump),
            "goto must Jump"
        );
    }

    #[test]
    fn test_cpp_co_await_splits_block() {
        let code = r#"
task coro() {
    co_await something();
    co_return 1;
}
"#;
        let cfg = build_cfg_for_function("cpp", code, "coro").unwrap();
        assert!(
            cpp_texts(&cfg)
                .iter()
                .any(|t| t.contains("co_await") || t.contains("await")),
            "co_await must appear, got {:?}",
            cpp_texts(&cfg)
        );
        assert!(
            cfg.blocks.len() >= 3,
            "co_await should split blocks, blocks={}",
            cfg.blocks.len()
        );
        assert!(
            cpp_texts(&cfg)
                .iter()
                .any(|t| t.contains("co_return") || t.contains("return")),
            "co_return must lower, got {:?}",
            cpp_texts(&cfg)
        );
    }

    #[test]
    fn test_cpp_co_yield_splits_block() {
        let code = r#"
generator gen() {
    co_yield 1;
    co_return;
}
"#;
        let cfg = build_cfg_for_function("cpp", code, "gen").unwrap();
        assert!(
            cpp_texts(&cfg)
                .iter()
                .any(|t| t.contains("co_yield") || t.contains("yield")),
            "co_yield must appear, got {:?}",
            cpp_texts(&cfg)
        );
        assert!(cfg.blocks.len() >= 3, "co_yield should split blocks");
    }

    #[test]
    fn test_javascript_switch_cfg() {
        let code = r#"
function classify(v) {
    switch (v) {
        case 1:
            return "one";
        case 2:
            return "two";
        default:
            return "other";
    }
}
"#;
        let cfg = build_cfg_for_function("javascript", code, "classify").unwrap();
        assert!(cfg.blocks.len() >= 5);
    }

    #[test]
    fn test_typescript_bound_arrow_cfg() {
        let code = r#"
export const dup = (n: number): number => n + 1;
export const uniqueOne = (n: number): number => dup(n);

export function dupDecl(n: number): number { return n + 1; }
export function uniqueDecl(n: number): number { return dupDecl(n); }
"#;
        let arrow_cfg = build_cfg_for_function("typescript", code, "uniqueOne").unwrap();
        assert!(
            arrow_cfg.blocks.len() >= 2,
            "bound arrow should have a CFG, got {} blocks",
            arrow_cfg.blocks.len()
        );
        let decl_cfg = build_cfg_for_function("typescript", code, "uniqueDecl").unwrap();
        assert!(decl_cfg.blocks.len() >= 2);
    }

    #[test]
    fn test_javascript_bound_arrow_cfg() {
        let code = r#"
const tickDown = (n) => n - 1;
function tickDecl(n) { return n - 1; }
"#;
        let cfg = build_cfg_for_function("javascript", code, "tickDown").unwrap();
        assert!(cfg.blocks.len() >= 2);
        let _ = build_cfg_for_function("javascript", code, "tickDecl").unwrap();
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

    fn java_texts(cfg: &ControlFlowGraph) -> Vec<String> {
        cfg.blocks
            .values()
            .flat_map(|b| b.statements.iter().map(|s| s.text.clone()))
            .collect()
    }

    fn java_kinds(cfg: &ControlFlowGraph) -> Vec<StatementKind> {
        cfg.blocks
            .values()
            .flat_map(|b| b.statements.iter().map(|s| s.kind))
            .collect()
    }

    fn java_block_ids(cfg: &ControlFlowGraph, needle: &str) -> Vec<BlockId> {
        cfg.blocks
            .values()
            .filter(|b| b.statements.iter().any(|s| s.text.contains(needle)))
            .map(|b| b.id)
            .collect()
    }

    #[test]
    fn test_java_if_else_cfg() {
        let code = r#"
public class Demo {
    public int abs(int x) {
        if (x > 0) {
            return x;
        }
        return -x;
    }
}
"#;
        let cfg = build_cfg_for_function("java", code, "abs").unwrap();
        assert!(cfg.blocks.len() >= 4);
        assert!(cfg.edges.iter().any(|e| e.edge_type == CfgEdgeType::IfTrue));
        assert!(
            java_kinds(&cfg)
                .iter()
                .filter(|k| **k == StatementKind::Return)
                .count()
                >= 2
        );
    }

    #[test]
    fn test_java_short_circuit_and() {
        let code = r#"
public class Demo {
    public int sc(boolean a, boolean b) {
        if (a && b) {
            return 1;
        }
        return 0;
    }
}
"#;
        let cfg = build_cfg_for_function("java", code, "sc").unwrap();
        let texts = java_texts(&cfg);
        assert!(
            texts.iter().any(|t| t.trim() == "a" || t == "a"),
            "&& left must be its own branch, got {texts:?}"
        );
        assert!(
            texts.iter().any(|t| t.trim() == "b" || t == "b"),
            "&& right must be its own branch, got {texts:?}"
        );
        assert!(
            !texts.iter().any(|t| t.contains("a && b")),
            "must not keep (a && b) as one blob, got {texts:?}"
        );
    }

    #[test]
    fn test_java_for_clause_init_cond_update() {
        let code = r#"
public class Demo {
    public int sum(int n) {
        int total = 0;
        for (int i = 0; i < n; i++) {
            if (i == 1) {
                continue;
            }
            total += i;
        }
        return total;
    }
}
"#;
        let cfg = build_cfg_for_function("java", code, "sum").unwrap();
        let texts = java_texts(&cfg);
        assert!(
            texts
                .iter()
                .any(|t| t.contains("i = 0") || t.contains("i=0")),
            "for init, got {texts:?}"
        );
        assert!(
            texts
                .iter()
                .any(|t| t.contains("i < n") || t.contains("i<n")),
            "for condition, got {texts:?}"
        );
        assert!(
            texts.iter().any(|t| t.contains("i++")),
            "for update, got {texts:?}"
        );
        assert!(cfg.has_cycle());
        let cont = java_block_ids(&cfg, "continue");
        let updates = java_block_ids(&cfg, "i++");
        assert!(
            cont.iter().any(|c| {
                cfg.edges.iter().any(|e| {
                    e.from == *c && updates.contains(&e.to) && e.edge_type == CfgEdgeType::Jump
                })
            }),
            "continue must Jump to i++"
        );
    }

    #[test]
    fn test_java_enhanced_for_has_cycle() {
        let code = r#"
public class Demo {
    public int sum(int[] a) {
        int t = 0;
        for (int v : a) {
            t += v;
        }
        return t;
    }
}
"#;
        let cfg = build_cfg_for_function("java", code, "sum").unwrap();
        assert!(cfg.has_cycle(), "enhanced for must cycle");
        assert!(
            !java_texts(&cfg)
                .iter()
                .any(|t| t.trim_start().starts_with("for (int v")),
            "enhanced for must not remain opaque blob, got {:?}",
            java_texts(&cfg)
        );
        assert!(
            java_texts(&cfg)
                .iter()
                .any(|t| t.contains("t += v") || t.contains("t+=v")),
            "body must be lowered"
        );
    }

    #[test]
    fn test_java_switch_cases_emit_returns() {
        let code = r#"
public class Demo {
    public int pick(int x) {
        switch (x) {
            case 1:
                return 10;
            case 2:
                return 20;
            default:
                return 0;
        }
    }
}
"#;
        let cfg = build_cfg_for_function("java", code, "pick").unwrap();
        assert!(
            java_kinds(&cfg)
                .iter()
                .filter(|k| **k == StatementKind::Return)
                .count()
                >= 3,
            "case returns must lower, kinds={:?}",
            java_kinds(&cfg)
        );
        assert!(
            !java_texts(&cfg)
                .iter()
                .any(|t| t.trim_start().starts_with("switch")),
            "switch must not be opaque blob"
        );
        let returns = cfg
            .edges
            .iter()
            .filter(|e| e.edge_type == CfgEdgeType::Return)
            .count();
        assert!(returns >= 3, "Return edges from cases, got {returns}");
    }

    #[test]
    fn test_java_switch_arrow_rules() {
        let code = r#"
public class Demo {
    public int pick(int x) {
        return switch (x) {
            case 1 -> 10;
            case 2 -> 20;
            default -> 0;
        };
    }
}
"#;
        let cfg = build_cfg_for_function("java", code, "pick").unwrap();
        // Arrow arms should appear as distinct branch targets (expressions or returns).
        assert!(
            cfg.edges
                .iter()
                .filter(|e| e.edge_type == CfgEdgeType::IfTrue)
                .count()
                >= 2,
            "arrow switch needs arm fan-out"
        );
        assert!(
            !java_texts(&cfg)
                .iter()
                .any(|t| t.contains("case 1 ->") && t.contains("default")),
            "arrow switch must not stay one blob, got {:?}",
            java_texts(&cfg)
        );
    }

    #[test]
    fn test_java_labeled_break_continue() {
        let code = r#"
public class Demo {
    public int nested() {
        int n = 0;
        outer:
        for (int i = 0; i < 3; i++) {
            for (int j = 0; j < 3; j++) {
                if (j == 1) {
                    continue outer;
                }
                if (j == 2) {
                    break outer;
                }
                n++;
            }
        }
        return n;
    }
}
"#;
        let cfg = build_cfg_for_function("java", code, "nested").unwrap();
        let cont = java_block_ids(&cfg, "continue outer");
        let brk = java_block_ids(&cfg, "break outer");
        let updates = java_block_ids(&cfg, "i++");
        assert!(!cont.is_empty() && !brk.is_empty() && !updates.is_empty());
        assert!(
            cont.iter().any(|c| {
                cfg.edges.iter().any(|e| {
                    e.from == *c && updates.contains(&e.to) && e.edge_type == CfgEdgeType::Jump
                })
            }),
            "continue outer must Jump to outer i++"
        );
        assert!(
            brk.iter().any(|b| {
                cfg.edges.iter().any(|e| {
                    e.from == *b
                        && e.edge_type == CfgEdgeType::Jump
                        && updates.iter().all(|u| e.to != *u)
                })
            }),
            "break outer must Jump to outer exit, not i++"
        );
    }

    #[test]
    fn test_java_try_catch_exception_edge() {
        let code = r#"
public class Demo {
    public int twc() {
        try {
            return 1;
        } catch (Exception e) {
            return 0;
        }
    }
}
"#;
        let cfg = build_cfg_for_function("java", code, "twc").unwrap();
        assert!(
            cfg.edges
                .iter()
                .any(|e| e.edge_type == CfgEdgeType::Exception),
            "try/catch needs Exception edge"
        );
        assert!(
            java_kinds(&cfg)
                .iter()
                .filter(|k| **k == StatementKind::Return)
                .count()
                >= 2
        );
    }

    #[test]
    fn test_java_try_with_resources() {
        let code = r#"
public class Demo {
    public int twr() throws Exception {
        try (java.io.StringReader r = new java.io.StringReader("")) {
            return 1;
        } catch (Exception e) {
            return 0;
        }
    }
}
"#;
        let cfg = build_cfg_for_function("java", code, "twr").unwrap();
        assert!(
            !java_texts(&cfg)
                .iter()
                .any(|t| t.trim_start().starts_with("try (")),
            "try-with-resources must not be opaque, got {:?}",
            java_texts(&cfg)
        );
        assert!(
            cfg.edges
                .iter()
                .any(|e| e.edge_type == CfgEdgeType::Exception)
                || java_kinds(&cfg).iter().any(|k| *k == StatementKind::Return),
            "twr should lower body/catch"
        );
    }

    #[test]
    fn test_java_try_with_resources_close_on_return() {
        let code = r#"
public class Demo {
    public int twr() throws Exception {
        try (java.io.StringReader r = new java.io.StringReader("");
             java.io.StringReader s = new java.io.StringReader("")) {
            return 1;
        } finally {
            System.out.println("userFinally");
        }
    }
}
"#;
        let cfg = build_cfg_for_function("java", code, "twr").unwrap();
        let texts = java_texts(&cfg);
        assert!(
            texts.iter().any(|t| t.contains("s.close()")),
            "later resource must close first (LIFO), got {texts:?}"
        );
        assert!(
            texts.iter().any(|t| t.contains("r.close()")),
            "earlier resource must close, got {texts:?}"
        );
        assert!(
            texts
                .iter()
                .any(|t| t.contains("userFinally") || t.contains("println")),
            "user finally must still run, got {texts:?}"
        );
        let ret = java_block_ids(&cfg, "return 1");
        let close_s = java_block_ids(&cfg, "s.close()");
        assert!(!ret.is_empty() && !close_s.is_empty());
        assert!(
            ret.iter().any(|r| {
                close_s.iter().any(|c| {
                    cfg.edges.iter().any(|e| e.from == *r && e.to == *c) || {
                        use std::collections::{HashSet, VecDeque};
                        let mut seen = HashSet::new();
                        let mut q = VecDeque::from([*r]);
                        while let Some(n) = q.pop_front() {
                            if n == *c {
                                return true;
                            }
                            if !seen.insert(n) {
                                continue;
                            }
                            for e in &cfg.edges {
                                if e.from == n {
                                    q.push_back(e.to);
                                }
                            }
                        }
                        false
                    }
                })
            }),
            "return must reach resource close before exit"
        );
    }

    #[test]
    fn test_java_try_exception_from_body_statement() {
        let code = r#"
public class Demo {
    public int twc() {
        try {
            System.out.println("mayThrow");
            return 1;
        } catch (Exception e) {
            return 0;
        }
    }
}
"#;
        let cfg = build_cfg_for_function("java", code, "twc").unwrap();
        let throw_sites = java_block_ids(&cfg, "mayThrow");
        let catch_rets = java_block_ids(&cfg, "return 0");
        assert!(!throw_sites.is_empty(), "body call must lower");
        assert!(!catch_rets.is_empty(), "catch return must lower");
        assert!(
            throw_sites.iter().any(|from| {
                cfg.edges.iter().any(|e| {
                    e.from == *from && e.edge_type == CfgEdgeType::Exception && {
                        // Exception target can reach catch return
                        use std::collections::{HashSet, VecDeque};
                        let mut seen = HashSet::new();
                        let mut q = VecDeque::from([e.to]);
                        while let Some(n) = q.pop_front() {
                            if catch_rets.contains(&n) {
                                return true;
                            }
                            if !seen.insert(n) {
                                continue;
                            }
                            for edge in &cfg.edges {
                                if edge.from == n {
                                    q.push_back(edge.to);
                                }
                            }
                        }
                        false
                    }
                })
            }),
            "Exception edge must leave the body statement block, not only try entry"
        );
    }

    #[test]
    fn test_java_finally_on_return() {
        let code = r#"
public class Demo {
    public int withFinally() {
        try {
            return 1;
        } finally {
            System.out.println("cleanup");
        }
    }
}
"#;
        let cfg = build_cfg_for_function("java", code, "withFinally").unwrap();
        let texts = java_texts(&cfg);
        assert!(
            texts
                .iter()
                .any(|t| t.contains("cleanup") || t.contains("println")),
            "finally body must run on return path, got {texts:?}"
        );
        let ret = java_block_ids(&cfg, "return 1");
        let cleanup = java_block_ids(&cfg, "cleanup");
        let cleanup2 = java_block_ids(&cfg, "println");
        let cleans: Vec<_> = cleanup.into_iter().chain(cleanup2).collect();
        assert!(!ret.is_empty() && !cleans.is_empty());
        assert!(
            ret.iter().any(|r| {
                cleans.iter().any(|c| {
                    cfg.edges.iter().any(|e| e.from == *r && e.to == *c) || {
                        use std::collections::{HashSet, VecDeque};
                        let mut seen = HashSet::new();
                        let mut q = VecDeque::from([*r]);
                        while let Some(n) = q.pop_front() {
                            if n == *c {
                                return true;
                            }
                            if !seen.insert(n) {
                                continue;
                            }
                            for e in &cfg.edges {
                                if e.from == n {
                                    q.push_back(e.to);
                                }
                            }
                        }
                        false
                    }
                })
            }),
            "return must reach finally cleanup"
        );
    }

    #[test]
    fn test_java_throw_exits() {
        let code = r#"
public class Demo {
    public void boom() {
        throw new RuntimeException("x");
    }
}
"#;
        let cfg = build_cfg_for_function("java", code, "boom").unwrap();
        assert!(
            java_texts(&cfg).iter().any(|t| t.contains("throw")),
            "throw must appear"
        );
        assert!(
            cfg.edges.iter().any(|e| {
                matches!(
                    e.edge_type,
                    CfgEdgeType::Exception | CfgEdgeType::Return | CfgEdgeType::Jump
                )
            }),
            "throw must terminate with Exception/Return/Jump edge"
        );
    }

    #[test]
    fn test_java_compact_constructor_cfg() {
        let code = r#"
public record Point(int x, int y) {
    public Point {
        if (x < 0) throw new IllegalArgumentException();
    }
}
"#;
        let cfg = build_cfg_for_function("java", code, "Point").unwrap();
        assert!(
            java_texts(&cfg)
                .iter()
                .any(|t| t.contains("throw") || t.contains("IllegalArgument")),
            "compact ctor body must appear in CFG: {:?}",
            java_texts(&cfg)
        );
    }

    #[test]
    fn test_java_static_initializer_cfg() {
        let code = r#"
public class C {
    static {
        System.out.println("static");
    }
}
"#;
        let cfg = build_cfg_for_function("java", code, "<clinit>").unwrap();
        assert!(
            java_texts(&cfg)
                .iter()
                .any(|t| t.contains("println") || t.contains("static")),
            "static initializer body must appear: {:?}",
            java_texts(&cfg)
        );
    }

    #[test]
    fn test_java_instance_initializer_cfg() {
        let code = r#"
public class C {
    {
        System.out.println("instance");
    }
}
"#;
        let cfg = build_cfg_for_function("java", code, "<initblock>0").unwrap();
        assert!(
            java_texts(&cfg)
                .iter()
                .any(|t| t.contains("println") || t.contains("instance")),
            "instance initializer must be CFG-reachable: {:?}",
            java_texts(&cfg)
        );
    }

    #[test]
    fn test_java_lambda_body_reachable_from_enclosing_method() {
        // java-grammar-remainder: lambda bodies are CFG-reachable via the
        // existing nested-sub-CFG machinery (`visit_nested_subcfg`) when
        // building the CFG for the enclosing method.
        let code = r#"
public class Printer {
    public void run() {
        xs.forEach(s -> System.out.println(s));
    }
}
"#;
        let cfg = build_cfg_for_function("java", code, "run").unwrap();
        assert!(
            java_texts(&cfg).iter().any(|t| t.contains("println")),
            "lambda body statement must appear in enclosing method's CFG: {:?}",
            java_texts(&cfg)
        );
    }

    #[test]
    fn test_java_lambda_direct_cfg_lookup_by_synthetic_name() {
        // java-grammar-remainder: `$lambda$N` also resolves directly via
        // `build_cfg_for_function`, matching rgctl-lang-java's naming
        // scheme for the (common) single-enclosing-callable case.
        let code = r#"
public class Printer {
    public void run() {
        xs.forEach(s -> System.out.println(s));
    }
}
"#;
        let cfg = build_cfg_for_function("java", code, "$lambda$0").unwrap();
        assert!(
            java_texts(&cfg).iter().any(|t| t.contains("println")),
            "lambda CFG built directly by name must contain its body statement: {:?}",
            java_texts(&cfg)
        );
    }

    #[test]
    fn test_php_if_else_branching() {
        let code = r#"<?php
function example($x) {
    if ($x > 0) {
        return 1;
    } else {
        return 2;
    }
}
"#;
        let cfg = build_cfg_for_function("php", code, "example").unwrap();
        assert!(cfg.blocks.len() >= 4, "expected branching CFG for PHP if/else");
    }

    #[test]
    fn test_php_foreach_cycle() {
        let code = r#"<?php
function example($items) {
    foreach ($items as $item) {
        process($item);
    }
}
"#;
        let cfg = build_cfg_for_function("php", code, "example").unwrap();
        assert!(cfg.has_cycle(), "foreach must cycle");
    }

    #[test]
    fn test_ruby_if_else_branching() {
        let code = r#"
def example(x)
  if x > 0
    return 1
  else
    return 2
  end
end
"#;
        let cfg = build_cfg_for_function("ruby", code, "example").unwrap();
        assert!(cfg.blocks.len() >= 4, "expected branching CFG for Ruby if/else");
    }

    #[test]
    fn test_ruby_while_cycle() {
        let code = r#"
def example(items)
  while items.length > 0
    items.pop
  end
end
"#;
        let cfg = build_cfg_for_function("ruby", code, "example").unwrap();
        assert!(cfg.has_cycle(), "while must cycle");
    }

    #[test]
    fn test_ruby_begin_rescue_exception_edge() {
        let code = r#"
def example
  begin
    risky
  rescue StandardError
    recover
  end
end
"#;
        let cfg = build_cfg_for_function("ruby", code, "example").unwrap();
        assert!(cfg.blocks.len() >= 2, "expected CFG for begin/rescue body");
    }

    #[test]
    fn test_ruby_if_modifier_no_stack_overflow() {
        let code = r#"
def create(params)
  system(params[:debug]) if params[:debug]
end
"#;
        let cfg = build_cfg_for_function("ruby", code, "create").unwrap();
        assert!(cfg.blocks.len() >= 2, "expected branches for if modifier");
    }

    #[test]
    fn test_puppet_if_else_cfg() {
        let code = r#"
class profile::web {
  if $facts['os']['family'] == 'RedHat' {
    package { 'httpd': ensure => installed }
  } else {
    package { 'apache2': ensure => installed }
  }
}
"#;
        let cfg = build_cfg_for_function("puppet", code, "profile::web").unwrap();
        assert!(cfg.blocks.len() >= 3, "expected if/else branches, got {}", cfg.blocks.len());
        assert!(
            cfg.edges
                .iter()
                .any(|e| matches!(e.edge_type, CfgEdgeType::IfTrue | CfgEdgeType::IfFalse)),
            "expected conditional edges"
        );
    }

    #[test]
    fn test_puppet_case_branches() {
        let code = r#"
class profile::os {
  case $facts['os']['family'] {
    'RedHat': { include profile::yum }
    'Debian': { include profile::apt }
    default:  { notify { 'unsupported': } }
  }
}
"#;
        let cfg = build_cfg_for_function("puppet", code, "profile::os").unwrap();
        assert!(cfg.blocks.len() >= 3, "expected case arms, got {}", cfg.blocks.len());
    }

    #[test]
    fn test_kotlin_if_and_when_cfg() {
        let code = r#"
class OrderService {
  fun validate(x: Int): Int {
    return if (x > 0) x else -x
  }
  fun find(id: Long): String {
    return when (id) {
      0L -> "none"
      else -> "order"
    }
  }
}
"#;
        let if_cfg = build_cfg_for_function("kotlin", code, "validate").unwrap();
        assert!(
            if_cfg.blocks.len() >= 3,
            "kotlin if should branch, got {}",
            if_cfg.blocks.len()
        );
        let when_cfg = build_cfg_for_function("kotlin", code, "find").unwrap();
        assert!(
            when_cfg.blocks.len() >= 3,
            "kotlin when should fan out, got {}",
            when_cfg.blocks.len()
        );
    }

    #[test]
    fn test_groovy_if_cfg() {
        let code = r#"
class OrderService {
  int validate(int x) {
    if (x > 0) {
      return x;
    } else {
      return -x;
    }
  }
}
"#;
        let cfg = build_cfg_for_function("groovy", code, "validate").unwrap();
        assert!(
            cfg.blocks.len() >= 3,
            "groovy if should branch, got {}",
            cfg.blocks.len()
        );
    }
}
