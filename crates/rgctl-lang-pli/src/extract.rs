//! Line/statement PL/I extractor (no tree-sitter).

use rgctl_plugin_api::{
    ComplexityMetrics, ExtractAllResult, LanguageCapabilities, LanguagePlugin, Relation,
    RelationType, Result, SourceLocation, Symbol, SymbolType,
};
use std::path::Path;

/// PL/I structural language plugin.
pub struct PliPlugin;

impl LanguagePlugin for PliPlugin {
    fn language_id(&self) -> &str {
        "pli"
    }

    fn file_extensions(&self) -> Vec<&str> {
        vec!["pli", "pl1", "inc"]
    }

    fn grammar(&self) -> Option<tree_sitter::Language> {
        None
    }

    fn capabilities(&self) -> LanguageCapabilities {
        LanguageCapabilities {
            extracts_functions: true,
            extracts_types: true,
            extracts_modules: true,
            extracts_relations: true,
            calculates_complexity: false,
            extracts_documentation: false,
            supports_incremental: false,
        }
    }

    fn extract_symbols(&self, file_path: &Path, source: &[u8]) -> Result<Vec<Symbol>> {
        Ok(self.extract_all(file_path, source)?.symbols)
    }

    fn extract_relations(
        &self,
        file_path: &Path,
        source: &[u8],
        _symbols: &[Symbol],
    ) -> Result<Vec<Relation>> {
        Ok(self.extract_all(file_path, source)?.relations)
    }

    fn extract_all(&self, file_path: &Path, source: &[u8]) -> Result<ExtractAllResult> {
        Ok(extract(file_path, source))
    }

    fn calculate_complexity(
        &self,
        _symbol: &Symbol,
        _source: &[u8],
    ) -> Result<Option<ComplexityMetrics>> {
        Ok(None)
    }
}

struct OpenProc {
    name: String,
    qualified: String,
    symbol_index: usize,
    start_line: usize,
    /// Count of nested `DO` / `SELECT` / `BEGIN` blocks inside this procedure.
    open_blocks: usize,
}

fn extract(file_path: &Path, source: &[u8]) -> ExtractAllResult {
    let file = file_path.to_string_lossy().into_owned();
    let text = String::from_utf8_lossy(source);
    let lines: Vec<&str> = text.lines().collect();
    let n = lines.len();

    let mut symbols: Vec<Symbol> = Vec::with_capacity(n / 4 + 8);
    let mut relations: Vec<Relation> = Vec::with_capacity(n / 8 + 4);
    let mut proc_stack: Vec<OpenProc> = Vec::with_capacity(8);
    let mut package: Option<String> = None;
    let mut package_idx: Option<usize> = None;

    let mut in_block_comment = false;
    let mut code_buf = String::with_capacity(128);
    let mut upper_buf = String::with_capacity(128);

    for (idx, raw) in lines.iter().enumerate() {
        let line_no = idx + 1;
        strip_pli_comments(raw, &mut in_block_comment, &mut code_buf);
        let code = code_buf.trim();
        if code.is_empty() {
            continue;
        }

        upper_buf.clear();
        for ch in code.chars() {
            upper_buf.push(ch.to_ascii_uppercase());
        }
        let upper_trim = upper_buf.trim();

        if let Some(name) = match_package(upper_trim) {
            package = Some(name.clone());
            let sym_idx = symbols.len();
            symbols.push(make_symbol(
                name,
                SymbolType::Module,
                package.clone(),
                &file,
                line_no,
                n.max(line_no),
                serde_json::json!({ "language": "pli", "kind": "package" }),
            ));
            package_idx = Some(sym_idx);
            continue;
        }

        if let Some(member) = match_include(upper_trim) {
            symbols.push(make_symbol(
                member.clone(),
                SymbolType::Import,
                Some(member.clone()),
                &file,
                line_no,
                line_no,
                serde_json::json!({
                    "language": "pli",
                    "import_path": member,
                }),
            ));
            continue;
        }

        if let Some(struc) = match_dcl_1(upper_trim) {
            let qn = match proc_stack.last() {
                Some(p) => Some(format!("{}::{struc}", p.qualified)),
                None => package.as_ref().map(|p| format!("{p}::{struc}")),
            };
            symbols.push(make_symbol(
                struc,
                SymbolType::Class,
                qn,
                &file,
                line_no,
                line_no,
                serde_json::json!({ "language": "pli", "kind": "structure" }),
            ));
            continue;
        }

        if let Some(name) = match_proc_start(upper_trim) {
            let qualified = match proc_stack.last() {
                Some(outer) => format!("{}::{name}", outer.qualified),
                None => match package.as_deref() {
                    Some(pkg) => format!("{pkg}::{name}"),
                    None => name.clone(),
                },
            };
            let symbol_index = symbols.len();
            symbols.push(make_symbol(
                name.clone(),
                SymbolType::Function,
                Some(qualified.clone()),
                &file,
                line_no,
                line_no,
                serde_json::json!({ "language": "pli", "kind": "procedure" }),
            ));
            proc_stack.push(OpenProc {
                name,
                qualified,
                symbol_index,
                start_line: line_no,
                open_blocks: 0,
            });
            continue;
        }

        // Track DO / SELECT / BEGIN blocks so their matching `END;` does not pop `proc_stack`.
        if is_block_opener(upper_trim) {
            if let Some(top) = proc_stack.last_mut() {
                top.open_blocks += 1;
            }
            // If the block closes on the same line (`DO; ... END;`), balance it immediately.
            if (upper_trim.ends_with("END;") || upper_trim.contains(" END;"))
                && let Some(top) = proc_stack.last_mut()
            {
                top.open_blocks = top.open_blocks.saturating_sub(1);
            }
            continue;
        }

        if let Some(end_name) = match_end(upper_trim) {
            match &end_name {
                Some(name) if package.as_ref() == Some(name) && proc_stack.is_empty() => {
                    if let Some(pkg_i) = package_idx
                        && let Some(sym) = symbols.get_mut(pkg_i)
                    {
                        sym.location.end_line = line_no.max(sym.location.start_line);
                    }
                }
                Some(name) => {
                    // Only pop procs if `name` matches an open procedure on the stack;
                    // otherwise it's a labeled `DO`/`SELECT`/`BEGIN` block closing.
                    if proc_stack.iter().any(|p| p.name == *name) {
                        while let Some(open) = proc_stack.pop() {
                            let closing = open.name == *name;
                            if let Some(sym) = symbols.get_mut(open.symbol_index) {
                                sym.location.end_line = line_no.max(open.start_line);
                            }
                            if closing {
                                break;
                            }
                        }
                    } else if let Some(top) = proc_stack.last_mut()
                        && top.open_blocks > 0
                    {
                        top.open_blocks -= 1;
                    }
                }
                None => {
                    if let Some(top) = proc_stack.last_mut()
                        && top.open_blocks > 0
                    {
                        top.open_blocks -= 1;
                    } else if let Some(open) = proc_stack.pop()
                        && let Some(sym) = symbols.get_mut(open.symbol_index)
                    {
                        sym.location.end_line = line_no.max(open.start_line);
                    }
                }
            }
            continue;
        }

        let caller = proc_stack
            .last()
            .map(|p| p.name.as_str())
            .or(package.as_deref())
            .unwrap_or("<pli>");

        if let Some(target) = match_call(upper_trim) {
            relations.push(make_call(
                caller,
                &target,
                &file,
                line_no,
                serde_json::json!({ "language": "pli", "kind": "call" }),
            ));
        }

        if let Some(target) = match_fetch(upper_trim) {
            relations.push(make_call(
                caller,
                &target,
                &file,
                line_no,
                serde_json::json!({
                    "language": "pli",
                    "kind": "fetch",
                    "dynamic": "true",
                }),
            ));
        }

        if let Some((target, kind)) = match_exec_cics(upper_trim) {
            relations.push(make_call(
                caller,
                &target,
                &file,
                line_no,
                serde_json::json!({ "language": "pli", "kind": kind }),
            ));
        }
    }

    // Close any unclosed procedures at EOF.
    while let Some(open) = proc_stack.pop() {
        if let Some(sym) = symbols.get_mut(open.symbol_index) {
            sym.location.end_line = n.max(open.start_line);
        }
    }

    ExtractAllResult::from_parts(symbols, relations)
}

/// Strip single-line and multi-line `/* ... */` comments into `out` while preserving
/// code before and after inline comments (e.g. `/* comment */ CALL FOO;`).
fn strip_pli_comments(raw: &str, in_block_comment: &mut bool, out: &mut String) {
    out.clear();
    let mut rest = raw;
    while !rest.is_empty() {
        if *in_block_comment {
            if let Some(end_idx) = rest.find("*/") {
                *in_block_comment = false;
                out.push(' ');
                rest = &rest[end_idx + 2..];
            } else {
                break;
            }
        } else if let Some(start_idx) = rest.find("/*") {
            out.push_str(&rest[..start_idx]);
            *in_block_comment = true;
            rest = &rest[start_idx + 2..];
        } else {
            out.push_str(rest);
            break;
        }
    }
}

fn is_block_opener(upper: &str) -> bool {
    let body = if let Some((_, after_label)) = split_label(upper) {
        after_label.trim_start()
    } else {
        upper
    };
    for kw in ["DO", "SELECT", "BEGIN"] {
        if let Some(rest) = body.strip_prefix(kw)
            && (rest.is_empty()
                || rest.starts_with(|c: char| c.is_whitespace() || c == ';' || c == '('))
        {
            return true;
        }
    }
    false
}

fn match_package(upper: &str) -> Option<String> {
    // NAME: PACKAGE ...
    let (label, rest) = split_label(upper)?;
    let rest = rest.trim_start();
    if rest.starts_with("PACKAGE") {
        return Some(label);
    }
    None
}

fn match_proc_start(upper: &str) -> Option<String> {
    // NAME: PROC ...  or NAME: PROCEDURE ...
    let (label, rest) = split_label(upper)?;
    let rest = rest.trim_start();
    if rest.starts_with("PROCEDURE") || rest.starts_with("PROC") {
        // Avoid matching PROCESS
        if rest.starts_with("PROCEDURE")
            || rest == "PROC"
            || rest.starts_with("PROC ")
            || rest.starts_with("PROC;")
            || rest.starts_with("PROC(")
        {
            return Some(label);
        }
    }
    None
}

fn match_end(upper: &str) -> Option<Option<String>> {
    // END;  or END NAME;
    let rest = upper.strip_prefix("END")?;
    if !rest.is_empty() && !rest.starts_with(|c: char| c.is_whitespace() || c == ';') {
        return None;
    }
    let rest = rest.trim_start().trim_end_matches(';').trim();
    if rest.is_empty() {
        return Some(None);
    }
    let name = rest
        .split(|c: char| c.is_whitespace() || c == ';')
        .next()?
        .to_string();
    Some(Some(name))
}

fn match_include(upper: &str) -> Option<String> {
    // %INCLUDE MEMBER;  or % INCLUDE MEMBER;
    let rest = upper.strip_prefix('%')?.trim_start();
    let rest = rest.strip_prefix("INCLUDE")?;
    if !rest.starts_with(|c: char| c.is_whitespace()) {
        return None;
    }
    let token = rest
        .trim_start()
        .trim_end_matches(';')
        .split(|c: char| c.is_whitespace() || c == ';')
        .next()?
        .trim()
        .trim_matches(|c| c == '\'' || c == '"');
    if token.is_empty() {
        return None;
    }
    Some(token.to_string())
}

fn match_dcl_1(upper: &str) -> Option<String> {
    // DCL 1 NAME  or DECLARE 1 NAME
    let rest = if let Some(r) = upper.strip_prefix("DECLARE") {
        r
    } else {
        upper.strip_prefix("DCL")?
    };
    if !rest.starts_with(|c: char| c.is_whitespace()) {
        return None;
    }
    let rest = rest.trim_start();
    if !rest.starts_with('1') {
        return None;
    }
    let after = rest[1..].trim_start();
    let name = after
        .split(|c: char| c.is_whitespace() || c == ',' || c == ';' || c == '(')
        .next()?
        .trim();
    if name.is_empty() {
        return None;
    }
    Some(name.to_string())
}

fn match_call(upper: &str) -> Option<String> {
    let rest = upper.strip_prefix("CALL")?;
    if !rest.starts_with(|c: char| c.is_whitespace()) {
        return None;
    }
    let token = rest
        .trim_start()
        .split(|c: char| c == '(' || c.is_whitespace() || c == ';' || c == ',')
        .next()?
        .trim();
    if token.is_empty() {
        return None;
    }
    Some(token.to_string())
}

fn match_fetch(upper: &str) -> Option<String> {
    let rest = upper.strip_prefix("FETCH")?;
    if !rest.starts_with(|c: char| c.is_whitespace()) {
        return None;
    }
    let token = rest
        .trim_start()
        .trim_end_matches(';')
        .split(|c: char| c.is_whitespace() || c == ';' || c == ',')
        .next()?
        .trim();
    if token.is_empty() {
        return None;
    }
    Some(token.to_string())
}

fn match_exec_cics(upper: &str) -> Option<(String, &'static str)> {
    if !upper.contains("EXEC CICS") {
        return None;
    }
    let kind = if upper.contains(" XCTL ") || upper.contains(" XCTL(") {
        "exec-cics-xctl"
    } else if upper.contains(" LINK ") || upper.contains(" LINK(") {
        "exec-cics-link"
    } else {
        return None;
    };
    let prog_idx = upper.find("PROGRAM")?;
    let after = upper[prog_idx + "PROGRAM".len()..].trim_start();
    let after = after.strip_prefix('(')?.trim_start();
    let name = if after.starts_with('\'') || after.starts_with('"') {
        let quote = after.chars().next()?;
        let end = after[1..].find(quote)?;
        after[1..1 + end].to_string()
    } else {
        after
            .split(|c: char| c == ')' || c == ',' || c.is_whitespace())
            .next()?
            .to_string()
    };
    if name.is_empty() {
        return None;
    }
    Some((name, kind))
}

fn split_label(upper: &str) -> Option<(String, &str)> {
    let idx = upper.find(':')?;
    let label = upper[..idx].trim();
    if label.is_empty()
        || !label
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '#' || c == '@' || c == '$')
    {
        return None;
    }
    Some((label.to_string(), &upper[idx + 1..]))
}

fn make_symbol(
    name: String,
    symbol_type: SymbolType,
    qualified_name: Option<String>,
    file: &str,
    start_line: usize,
    end_line: usize,
    metadata: serde_json::Value,
) -> Symbol {
    Symbol {
        name,
        symbol_type,
        qualified_name,
        location: SourceLocation {
            file: file.to_string(),
            start_line,
            end_line,
            start_column: 0,
            end_column: 0,
        },
        signature: None,
        return_type: None,
        parameters: vec![],
        fields: vec![],
        modifiers: vec![],
        documentation: None,
        metadata,
        ..Default::default()
    }
}

fn make_call(
    from: &str,
    to: &str,
    file: &str,
    line: usize,
    metadata: serde_json::Value,
) -> Relation {
    Relation {
        from: from.to_string(),
        to: to.to_string(),
        relation_type: RelationType::Calls,
        location: SourceLocation {
            file: file.to_string(),
            start_line: line,
            end_line: line,
            start_column: 0,
            end_column: 0,
        },
        metadata,
        to_qualified_hint: None,
        to_type_hint: None,
        ..Default::default()
    }
}
