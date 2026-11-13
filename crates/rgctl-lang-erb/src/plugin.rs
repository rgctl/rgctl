//! ERB `LanguagePlugin` — symbols, relations, translation metadata.

use crate::single_pass::{erb_ast_handlers, extract_all_single_pass};
use regex::Regex;
use rgctl_plugin_api::{
    AstHandlerTable, ComplexityMetrics, Error, ExtractAllResult, LanguagePlugin, Relation,
    RelationType, Result, SourceLocation, Symbol, SymbolType, complexity_from_metadata,
    parse_with_pool,
};
use rgctl_plugin_helpers::record_symbol_spans;
use std::path::Path;
use std::sync::OnceLock;
use tree_sitter::{Node, Parser, Tree};

/// Classification of ERB block translation complexity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tier {
    /// Simple variable output or block close — regex substitution
    T1 = 1,
    /// Conditionals, iteration, scope lookups — structural rewrite
    T2 = 2,
    /// Method chains, complex conditionals — rules + Jinja2 filters
    T3 = 3,
    /// Lambdas, select/map, Ruby stdlib — LLM-assisted
    T4 = 4,
}

/// What kind of ERB block.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BlockKind {
    /// `<%= ... %>` — output expression
    Output,
    /// `<% ... %>` — code/control flow
    Code,
    /// `<%# ... %>` — comment
    Comment,
    /// `<%graphql ... %>` — GraphQL directive body (Property handler)
    Graphql,
}

// ── Compiled regexes (one-time init) ────────────────────────────────

macro_rules! re {
    ($name:ident, $pat:expr) => {
        fn $name() -> &'static Regex {
            static RE: OnceLock<Regex> = OnceLock::new();
            RE.get_or_init(|| Regex::new($pat).expect(concat!("bad regex: ", $pat)))
        }
    };
}

re!(re_simple_var, r"^@\w+$");
re!(
    re_higher_order,
    r"\.(select|map|reject|collect|sort_by|find|detect|flat_map)\s*\{"
);
re!(
    re_method_chain,
    r"\.(join|downcase|upcase|strip|chomp|first|last|length|size|split|include\?|nil\?|empty\?|to_s|to_i|to_f|capitalize|reverse|uniq|sort|flatten|compact)"
);
re!(re_cond, r"^(if|elsif|unless)\s+");
re!(re_each, r"\.each\s+do\s*\|");
re!(re_access, r"^\w+\[");
re!(re_at_var, r"@(\w+)");
re!(re_scope_ref, r"scope\['([^']+)'\]");
re!(re_each_capture, r"@(\w+)\.each\s+do\s*\|([^|]+)\|");
re!(re_hash_access, r"^(\w+)\['([^']+)'\]$");
re!(re_join_single, r"\.join\('([^']*)'\)");

fn re_facts_path() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r#"@facts(\[(?:'[^']*'|"[^"]*")\])+"#).expect("bad regex: facts_path")
    })
}

fn re_join_double() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r#"\.join\("([^"]*)"\)"#).expect("bad regex: join_double"))
}

// ── Plugin ──────────────────────────────────────────────────────────

/// Puppet ERB template language plugin.
pub struct ErbPlugin {
    _parser: Parser,
}

impl ErbPlugin {
    pub fn new() -> Result<Self> {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_embedded_template::LANGUAGE.into())
            .map_err(|e| Error::PluginError(format!("Failed to set ERB grammar: {e}")))?;
        Ok(Self { _parser: parser })
    }

    pub(crate) fn parse(&self, file_path: &Path, source: &[u8]) -> Result<Tree> {
        parse_with_pool(
            "erb",
            tree_sitter_embedded_template::LANGUAGE.into(),
            source,
        )
        .map_err(|e| Error::ParseError {
            file: file_path.to_path_buf(),
            line: 0,
            message: e,
        })
    }

    fn loc(node: Node, file_path: &str) -> SourceLocation {
        SourceLocation {
            file: file_path.to_string(),
            start_line: node.start_position().row + 1,
            end_line: node.end_position().row + 1,
            start_column: node.start_position().column,
            end_column: node.end_position().column,
        }
    }

    /// Extract the code content from an ERB directive node.
    fn block_code(node: Node, source: &[u8]) -> Option<String> {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "code" {
                return child.utf8_text(source).ok().map(|s| s.trim().to_string());
            }
        }
        None
    }

    /// Extract comment body text (`comment` child under `comment_directive`).
    fn block_comment(node: Node, source: &[u8]) -> Option<String> {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "comment" {
                return child.utf8_text(source).ok().map(|s| s.trim().to_string());
            }
        }
        None
    }

    /// Classify a block's translation complexity.
    fn classify_tier(code: &str, kind: BlockKind) -> Tier {
        if matches!(kind, BlockKind::Comment | BlockKind::Graphql) {
            return Tier::T1;
        }

        let code_trimmed = code.trim_end_matches("-%").trim_end_matches('%').trim();

        // T1: simple variable output
        if kind == BlockKind::Output && re_simple_var().is_match(code_trimmed) {
            return Tier::T1;
        }

        // T1: block terminators and else
        if matches!(code_trimmed, "end" | "else") {
            return Tier::T1;
        }

        // T4: select/map/reject/collect with blocks
        if re_higher_order().is_match(code_trimmed) {
            return Tier::T4;
        }

        // T4: require (Ruby stdlib)
        if code_trimmed.starts_with("require ") || code_trimmed.starts_with("require(") {
            return Tier::T4;
        }

        // T3: method chains (.join, .downcase, .split, etc.)
        if re_method_chain().is_match(code_trimmed) {
            return Tier::T3;
        }

        // T3: complex conditionals (&&, ||)
        if code_trimmed.contains("&&") || code_trimmed.contains("||") {
            return Tier::T3;
        }

        // T2: simple conditionals
        if re_cond().is_match(code_trimmed) {
            return Tier::T2;
        }

        // T2: iteration
        if re_each().is_match(code_trimmed) {
            return Tier::T2;
        }

        // T2: scope lookups
        if code_trimmed.contains("scope[") {
            return Tier::T2;
        }

        // T2: hash/array access
        if kind == BlockKind::Output && re_access().is_match(code_trimmed) {
            return Tier::T2;
        }

        // T2: fact access
        if code_trimmed.contains("@facts[") {
            return Tier::T2;
        }

        // Default: T2
        Tier::T2
    }

    /// Extract `@variable` references from Ruby code.
    fn extract_at_variables(code: &str) -> Vec<String> {
        let mut vars: Vec<String> = re_at_var()
            .captures_iter(code)
            .filter_map(|c| c.get(1).map(|m| m.as_str().to_string()))
            .collect();
        vars.sort();
        vars.dedup();
        vars
    }

    /// Extract `@facts[...]` access paths.
    fn extract_fact_paths(code: &str) -> Vec<String> {
        re_facts_path()
            .find_iter(code)
            .map(|m| m.as_str().to_string())
            .collect()
    }

    /// Extract `scope['class::param']` references.
    fn extract_scope_refs(code: &str) -> Vec<String> {
        re_scope_ref()
            .captures_iter(code)
            .filter_map(|c| c.get(1).map(|m| m.as_str().to_string()))
            .collect()
    }

    /// Suggest a Jinja2 translation pattern for T1–T3 blocks.
    fn jinja2_hint(code: &str, kind: BlockKind, tier: Tier) -> Option<String> {
        let trimmed = code.trim_end_matches("-%").trim_end_matches('%').trim();

        match (tier, kind) {
            // T1 output: @var → {{ var }}
            (Tier::T1, BlockKind::Output) => {
                let var = trimmed.trim_start_matches('@');
                Some(format!("{{{{ {var} }}}}"))
            }
            // T1 comment
            (_, BlockKind::Comment) => Some(format!("{{# {trimmed} #}}")),
            // T1 block terminators — caller needs block stack context
            (Tier::T1, BlockKind::Code) if trimmed == "end" => {
                Some("{%- endif/endfor --%}".to_string())
            }
            (Tier::T1, BlockKind::Code) if trimmed == "else" => Some("{% else %}".to_string()),
            // T2 conditionals
            (Tier::T2, BlockKind::Code) if trimmed.starts_with("if ") => {
                let expr = trimmed.strip_prefix("if ").unwrap_or(trimmed);
                let j2 = Self::ruby_expr_to_jinja2(expr);
                Some(format!("{{% if {j2} %}}"))
            }
            (Tier::T2, BlockKind::Code) if trimmed.starts_with("elsif ") => {
                let expr = trimmed.strip_prefix("elsif ").unwrap_or(trimmed);
                let j2 = Self::ruby_expr_to_jinja2(expr);
                Some(format!("{{% elif {j2} %}}"))
            }
            (Tier::T2, BlockKind::Code) if trimmed.starts_with("unless ") => {
                let expr = trimmed.strip_prefix("unless ").unwrap_or(trimmed);
                let j2 = Self::ruby_expr_to_jinja2(expr);
                Some(format!("{{% if not ({j2}) %}}"))
            }
            // T2 iteration: @arr.each do |item|
            (Tier::T2, BlockKind::Code) => {
                if let Some(caps) = re_each_capture().captures(trimmed) {
                    let collection = caps.get(1).map(|m| m.as_str()).unwrap_or("items");
                    let vars = caps.get(2).map(|m| m.as_str()).unwrap_or("item").trim();
                    if vars.contains(',') {
                        let parts: Vec<&str> = vars.split(',').map(|s| s.trim()).collect();
                        Some(format!(
                            "{{% for {}, {} in {}.items() %}}",
                            parts.first().unwrap_or(&"k"),
                            parts.get(1).unwrap_or(&"v"),
                            collection
                        ))
                    } else {
                        Some(format!("{{% for {vars} in {collection} %}}"))
                    }
                } else {
                    None
                }
            }
            // T2 scope lookups in output
            (Tier::T2, BlockKind::Output) if trimmed.contains("scope[") => {
                if let Some(caps) = re_scope_ref().captures(trimmed) {
                    let full_ref = caps.get(1).map(|m| m.as_str()).unwrap_or(trimmed);
                    let var_name = full_ref.rsplit("::").next().unwrap_or(full_ref);
                    Some(format!("{{{{ {var_name} }}}}"))
                } else {
                    None
                }
            }
            // T2 hash access in output: vhost['key'] → vhost.key
            (Tier::T2, BlockKind::Output) => {
                if let Some(caps) = re_hash_access().captures(trimmed) {
                    let obj = caps.get(1).map(|m| m.as_str()).unwrap_or("obj");
                    let key = caps.get(2).map(|m| m.as_str()).unwrap_or("key");
                    Some(format!("{{{{ {obj}.{key} }}}}"))
                } else {
                    None
                }
            }
            // T3 method chains in output
            (Tier::T3, BlockKind::Output) => {
                let j2 = Self::ruby_output_to_jinja2(trimmed);
                Some(format!("{{{{ {j2} }}}}"))
            }
            _ => None,
        }
    }

    /// Convert a Ruby expression (from conditions) to Jinja2.
    fn ruby_expr_to_jinja2(expr: &str) -> String {
        re_at_var().replace_all(expr, "$1").to_string()
    }

    /// Convert a Ruby output expression to Jinja2 with filters.
    fn ruby_output_to_jinja2(expr: &str) -> String {
        let mut j2 = re_at_var().replace_all(expr, "$1").to_string();
        j2 = re_join_single()
            .replace_all(&j2, " | join('$1')")
            .to_string();
        j2 = re_join_double()
            .replace_all(&j2, " | join('$1')")
            .to_string();
        j2 = j2.replace(".downcase", " | lower");
        j2 = j2.replace(".upcase", " | upper");
        j2 = j2.replace(".strip", " | trim");
        j2 = j2.replace(".chomp", " | trim");
        j2 = j2.replace(".capitalize", " | capitalize");
        j2 = j2.replace(".reverse", " | reverse");
        j2 = j2.replace(".length", " | length");
        j2 = j2.replace(".size", " | length");
        j2 = j2.replace(".to_s", " | string");
        j2 = j2.replace(".to_i", " | int");
        j2 = j2.replace(".to_f", " | float");
        j2 = j2.replace(".uniq", " | unique");
        j2 = j2.replace(".sort", " | sort");
        j2 = j2.replace(".flatten", " | flatten");
        j2 = j2.replace(".compact", " | reject('none')");
        j2 = j2.replace(".first", " | first");
        j2 = j2.replace(".last", " | last");
        j2
    }

    pub(crate) fn collect_block_at(
        &self,
        node: Node,
        source: &[u8],
        file_path: &str,
        ordinals: &mut [usize; 4],
        symbols: &mut Vec<Symbol>,
        relations: &mut Vec<Relation>,
    ) {
        let kind = match node.kind() {
            "output_directive" => Some(BlockKind::Output),
            "directive" => Some(BlockKind::Code),
            "comment_directive" => Some(BlockKind::Comment),
            "graphql_directive" => Some(BlockKind::Graphql),
            _ => None,
        };

        if let Some(kind) = kind {
            // comment_directive aliases code as `comment`; graphql/others use `code`.
            let code = Self::block_code(node, source).or_else(|| Self::block_comment(node, source));
            if let Some(code) = code {
                let tier = Self::classify_tier(&code, kind);
                let at_vars = if kind == BlockKind::Graphql {
                    Vec::new()
                } else {
                    Self::extract_at_variables(&code)
                };
                let fact_paths = if kind == BlockKind::Graphql {
                    Vec::new()
                } else {
                    Self::extract_fact_paths(&code)
                };
                let scope_refs = if kind == BlockKind::Graphql {
                    Vec::new()
                } else {
                    Self::extract_scope_refs(&code)
                };
                let jinja2 = if kind == BlockKind::Graphql {
                    None
                } else {
                    Self::jinja2_hint(&code, kind, tier)
                };

                let (block_type, slot) = match kind {
                    BlockKind::Output => ("output", 0),
                    BlockKind::Code => ("code", 1),
                    BlockKind::Comment => ("comment", 2),
                    BlockKind::Graphql => ("graphql", 3),
                };

                // O(1) ordinal within this block kind, not the source line. Prepending
                // trivia must not churn StableNodeKey (`erb:output#1`, not `@L{n}`).
                ordinals[slot] += 1;
                let ordinal = ordinals[slot];
                let sym_name = format!("erb:{block_type}#{ordinal}");

                let mut sym = Symbol {
                    name: sym_name.clone(),
                    symbol_type: SymbolType::Variable,
                    qualified_name: Some(sym_name.clone()),
                    location: Self::loc(node, file_path),
                    signature: Some(code.clone()),
                    return_type: None,
                    parameters: vec![],
                    fields: vec![],
                    modifiers: vec![],
                    documentation: None,
                    metadata: if kind == BlockKind::Graphql {
                        serde_json::json!({
                            "language": "erb",
                            "block_kind": block_type,
                            "translation_tier": tier as u8,
                            "graphql_body": code,
                            "handler": "Property",
                        })
                    } else {
                        serde_json::json!({
                            "language": "erb",
                            "block_kind": block_type,
                            "translation_tier": tier as u8,
                            "ruby_code": code,
                            "jinja2_hint": jinja2,
                            "variables": at_vars,
                            "fact_paths": fact_paths,
                            "scope_refs": scope_refs,
                        })
                    },
                    ..Default::default()
                };
                // Synthetic `erb:{kind}#{ordinal}` names — decl span only.
                record_symbol_spans(&mut sym, node, None);
                symbols.push(sym);

                let rel_from = sym_name;

                for var in &at_vars {
                    if var == "facts" {
                        continue;
                    }
                    relations.push(Relation {
                        from: rel_from.clone(),
                        to: format!("${var}"),
                        relation_type: RelationType::UsesVariable,
                        location: Self::loc(node, file_path),
                        metadata: serde_json::json!({
                            "language": "erb",
                            "erb_variable": format!("@{var}"),
                        }),
                        to_qualified_hint: Some(format!("${var}")),
                        to_type_hint: Some("puppetvariable".to_string()),
                        ..Default::default()
                    });
                }

                for fact in &fact_paths {
                    relations.push(Relation {
                        from: rel_from.clone(),
                        to: fact.clone(),
                        relation_type: RelationType::UsesFact,
                        location: Self::loc(node, file_path),
                        metadata: serde_json::json!({
                            "language": "erb",
                            "fact_path": fact,
                        }),
                        to_qualified_hint: None,
                        to_type_hint: Some("puppetfact".to_string()),
                        ..Default::default()
                    });
                }

                for scope_ref in &scope_refs {
                    relations.push(Relation {
                        from: rel_from.clone(),
                        to: scope_ref.clone(),
                        relation_type: RelationType::References,
                        location: Self::loc(node, file_path),
                        metadata: serde_json::json!({
                            "language": "erb",
                            "scope_ref": scope_ref,
                        }),
                        to_qualified_hint: Some(scope_ref.clone()),
                        to_type_hint: Some("puppetvariable".to_string()),
                        ..Default::default()
                    });
                }
            }
        }
    }

    fn walk_blocks(
        &self,
        node: Node,
        source: &[u8],
        file_path: &str,
        ordinals: &mut [usize; 4],
        symbols: &mut Vec<Symbol>,
        relations: &mut Vec<Relation>,
    ) {
        self.collect_block_at(node, source, file_path, ordinals, symbols, relations);
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.walk_blocks(child, source, file_path, ordinals, symbols, relations);
        }
    }
}

impl LanguagePlugin for ErbPlugin {
    fn language_id(&self) -> &str {
        "erb"
    }

    fn file_extensions(&self) -> Vec<&str> {
        vec!["erb"]
    }

    fn grammar(&self) -> Option<tree_sitter::Language> {
        Some(tree_sitter_embedded_template::LANGUAGE.into())
    }

    fn extract_symbols(&self, file_path: &Path, source: &[u8]) -> Result<Vec<Symbol>> {
        let tree = self.parse(file_path, source)?;
        let path_str = file_path.to_string_lossy();
        let mut ordinals = [0usize; 4];
        let mut symbols = Vec::new();
        let mut _rels = Vec::new();
        self.walk_blocks(
            tree.root_node(),
            source,
            &path_str,
            &mut ordinals,
            &mut symbols,
            &mut _rels,
        );
        Ok(symbols)
    }

    fn extract_relations(
        &self,
        file_path: &Path,
        source: &[u8],
        _symbols: &[Symbol],
    ) -> Result<Vec<Relation>> {
        let tree = self.parse(file_path, source)?;
        let path_str = file_path.to_string_lossy();
        let mut ordinals = [0usize; 4];
        let mut _syms = Vec::new();
        let mut relations = Vec::new();
        self.walk_blocks(
            tree.root_node(),
            source,
            &path_str,
            &mut ordinals,
            &mut _syms,
            &mut relations,
        );
        Ok(relations)
    }

    fn extract_all(&self, file_path: &Path, source: &[u8]) -> Result<ExtractAllResult> {
        extract_all_single_pass(self, file_path, source)
    }

    fn declared_ast_handlers(&self) -> Option<&AstHandlerTable> {
        Some(erb_ast_handlers())
    }

    fn calculate_complexity(
        &self,
        symbol: &Symbol,
        _source: &[u8],
    ) -> Result<Option<ComplexityMetrics>> {
        if let Some(metrics) = complexity_from_metadata(symbol) {
            return Ok(Some(metrics));
        }
        Ok(Some(ComplexityMetrics {
            cyclomatic: 1,
            cognitive: 0,
            loc: symbol
                .location
                .end_line
                .saturating_sub(symbol.location.start_line)
                + 1,
            parameters: 0,
            nesting_depth: 0,
            returns: 0,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plugin() -> ErbPlugin {
        ErbPlugin::new().expect("plugin")
    }

    #[test]
    fn extracts_simple_variables() {
        let src = b"<h1><%= @hostname %></h1>\n<p><%= @fqdn %></p>";
        let p = plugin();
        let path = Path::new("templates/index.html.erb");
        let result = p.extract_all(path, src).expect("extract_all");

        assert_eq!(result.symbols.len(), 2);
        assert!(
            result.symbols.iter().all(|s| {
                s.metadata.get("translation_tier").and_then(|v| v.as_u64()) == Some(1)
            })
        );

        let var_rels: Vec<_> = result
            .relations
            .iter()
            .filter(|r| r.relation_type == RelationType::UsesVariable)
            .collect();
        assert_eq!(var_rels.len(), 2);
        let targets: Vec<&str> = var_rels.iter().map(|r| r.to.as_str()).collect();
        assert!(targets.contains(&"$hostname"));
        assert!(targets.contains(&"$fqdn"));
    }

    #[test]
    fn extracts_facts_and_scope() {
        let src = b"<%= @facts['os']['family'] %>\n<%= scope['profile::nginx::port'] %>";
        let p = plugin();
        let path = Path::new("templates/config.erb");
        let result = p.extract_all(path, src).expect("extract_all");

        let fact_rels: Vec<_> = result
            .relations
            .iter()
            .filter(|r| r.relation_type == RelationType::UsesFact)
            .collect();
        assert_eq!(fact_rels.len(), 1);
        assert!(fact_rels[0].to.contains("@facts['os']['family']"));

        let ref_rels: Vec<_> = result
            .relations
            .iter()
            .filter(|r| r.relation_type == RelationType::References)
            .collect();
        assert_eq!(ref_rels.len(), 1);
        assert_eq!(ref_rels[0].to, "profile::nginx::port");
    }

    #[test]
    fn classifies_tiers_correctly() {
        assert_eq!(
            ErbPlugin::classify_tier("@hostname", BlockKind::Output) as u8,
            1
        );
        assert_eq!(ErbPlugin::classify_tier("end", BlockKind::Code) as u8, 1);
        assert_eq!(
            ErbPlugin::classify_tier("if @os == 'RedHat'", BlockKind::Code) as u8,
            2
        );
        assert_eq!(
            ErbPlugin::classify_tier("@arr.each do |item|", BlockKind::Code) as u8,
            2
        );
        assert_eq!(
            ErbPlugin::classify_tier("@aliases.join(' ')", BlockKind::Output) as u8,
            3
        );
        assert_eq!(
            ErbPlugin::classify_tier("@nets.select { |n| n.include?('/') }", BlockKind::Code) as u8,
            4
        );
    }

    #[test]
    fn jinja2_hints_for_simple_blocks() {
        let hint = ErbPlugin::jinja2_hint("@hostname", BlockKind::Output, Tier::T1);
        assert_eq!(hint, Some("{{ hostname }}".to_string()));

        let hint = ErbPlugin::jinja2_hint("else", BlockKind::Code, Tier::T1);
        assert_eq!(hint, Some("{% else %}".to_string()));
    }

    #[test]
    fn jinja2_hints_for_iteration() {
        let hint = ErbPlugin::jinja2_hint("@vhosts.each do |vhost|", BlockKind::Code, Tier::T2);
        assert_eq!(hint, Some("{% for vhost in vhosts %}".to_string()));

        let hint =
            ErbPlugin::jinja2_hint("@headers.each do |key, value|", BlockKind::Code, Tier::T2);
        assert_eq!(
            hint,
            Some("{% for key, value in headers.items() %}".to_string())
        );
    }

    #[test]
    fn jinja2_hints_for_method_chains() {
        let hint = ErbPlugin::jinja2_hint("@aliases.join(' ')", BlockKind::Output, Tier::T3);
        assert_eq!(hint, Some("{{ aliases | join(' ') }}".to_string()));

        let hint = ErbPlugin::jinja2_hint("@hostname.downcase", BlockKind::Output, Tier::T3);
        assert_eq!(hint, Some("{{ hostname | lower }}".to_string()));
    }

    #[test]
    fn registry_extensions() {
        let p = plugin();
        assert_eq!(p.language_id(), "erb");
        assert!(p.file_extensions().contains(&"erb"));
        assert!(p.grammar().is_some());
    }

    #[test]
    fn honesty_regex_gaps() {
        // @facts.dig(...) and @facts.get('os') are NOT captured by the bracket regex.
        let facts_dig = ErbPlugin::extract_fact_paths("@facts.dig('os', 'family')");
        assert!(
            facts_dig.is_empty(),
            "dig() should not match: {facts_dig:?}"
        );

        let facts_get = ErbPlugin::extract_fact_paths("@facts.get('os')");
        assert!(
            facts_get.is_empty(),
            "get() should not match: {facts_get:?}"
        );

        // Non-@ locals produce no variable references.
        let locals = ErbPlugin::extract_at_variables("item['name']");
        assert!(
            locals.is_empty(),
            "non-@ locals should not match: {locals:?}"
        );
    }

    #[test]
    fn epp_files_not_handled() {
        let p = plugin();
        assert!(!p.file_extensions().contains(&"epp"));
        assert!(!p.can_handle(Path::new("templates/config.epp")));
    }
}
