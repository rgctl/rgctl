//! Line/statement COBOL extractor (no tree-sitter).

use rgctl_plugin_api::{
    ComplexityMetrics, ExtractAllResult, LanguageCapabilities, LanguagePlugin, Relation,
    RelationType, Result, SourceLocation, Symbol, SymbolType,
};
use std::path::Path;

/// COBOL structural language plugin.
pub struct CobolPlugin;

impl LanguagePlugin for CobolPlugin {
    fn language_id(&self) -> &str {
        "cobol"
    }

    fn file_extensions(&self) -> Vec<&str> {
        vec!["cbl", "cob", "cpy"]
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

fn extract(file_path: &Path, source: &[u8]) -> ExtractAllResult {
    let file = file_path.to_string_lossy().into_owned();
    let text = String::from_utf8_lossy(source);
    let lines: Vec<&str> = text.lines().collect();
    let n = lines.len();

    let mut symbols: Vec<Symbol> = Vec::with_capacity(n / 4 + 8);
    let mut relations: Vec<Relation> = Vec::with_capacity(n / 8 + 4);

    let mut program: Option<String> = None;
    let mut program_idx: Option<usize> = None;
    let mut in_procedure = false;
    let mut current_para: Option<String> = None;
    let mut current_para_idx: Option<usize> = None;
    let mut current_section_idx: Option<usize> = None;

    let mut upper_buf = String::with_capacity(128);
    let mut exec_acc: Option<(usize, String)> = None;

    for (idx, raw) in lines.iter().enumerate() {
        let line_no = idx + 1;
        let Some(code) = cobol_code_area(raw) else {
            continue;
        };

        upper_buf.clear();
        for ch in code.chars() {
            upper_buf.push(ch.to_ascii_uppercase());
        }
        let upper_trim = upper_buf.trim();

        // Handle multi-line EXEC CICS / EXEC SQL ... END-EXEC blocks.
        if let Some((exec_start, ref mut buf)) = exec_acc {
            buf.push(' ');
            buf.push_str(upper_trim);
            if upper_trim.contains("END-EXEC") {
                let caller = current_para
                    .as_deref()
                    .or(program.as_deref())
                    .unwrap_or("<program>");
                process_exec_block(buf, caller, &file, exec_start, &mut symbols, &mut relations);
                exec_acc = None;
            }
            continue;
        }
        if (upper_trim.starts_with("EXEC CICS")
            || upper_trim.contains(" EXEC CICS")
            || upper_trim.starts_with("EXEC SQL")
            || upper_trim.contains(" EXEC SQL"))
            && !upper_trim.contains("END-EXEC")
        {
            exec_acc = Some((line_no, upper_trim.to_string()));
            continue;
        }

        if let Some(name) = match_program_id(upper_trim) {
            let prev_end = idx.max(1);
            close_span(&mut symbols, current_para_idx.take(), prev_end);
            close_span(&mut symbols, current_section_idx.take(), prev_end);
            close_span(&mut symbols, program_idx.take(), prev_end);

            program = Some(name.clone());
            in_procedure = false;
            current_para = None;
            let sym_idx = symbols.len();
            symbols.push(make_symbol(
                name,
                SymbolType::Module,
                None,
                &file,
                line_no,
                n.max(line_no),
                serde_json::json!({ "language": "cobol", "kind": "program" }),
            ));
            program_idx = Some(sym_idx);
            continue;
        }

        if upper_trim.contains("PROCEDURE DIVISION") {
            in_procedure = true;
            continue;
        }
        if (upper_trim.contains("DATA DIVISION")
            || upper_trim.contains("ENVIRONMENT DIVISION")
            || upper_trim.contains("IDENTIFICATION DIVISION")
            || upper_trim.contains("WORKING-STORAGE SECTION")
            || upper_trim.contains("LINKAGE SECTION")
            || upper_trim.contains("FILE SECTION")
            || upper_trim.contains("LOCAL-STORAGE SECTION"))
            && !upper_trim.contains("PROCEDURE DIVISION")
            && upper_trim.contains("DIVISION")
            && !upper_trim.contains("PROCEDURE")
        {
            let prev_end = idx.max(1);
            close_span(&mut symbols, current_para_idx.take(), prev_end);
            close_span(&mut symbols, current_section_idx.take(), prev_end);
            in_procedure = false;
        }

        if let Some(copybook) = match_copy(upper_trim) {
            symbols.push(make_symbol(
                copybook.clone(),
                SymbolType::Import,
                Some(copybook.clone()),
                &file,
                line_no,
                line_no,
                serde_json::json!({
                    "language": "cobol",
                    "import_path": copybook,
                }),
            ));
            continue;
        }

        if let Some(rec) = match_01_level(upper_trim) {
            let qn = program.as_ref().map(|p| format!("{p}::{rec}"));
            symbols.push(make_symbol(
                rec,
                SymbolType::Class,
                qn,
                &file,
                line_no,
                line_no,
                serde_json::json!({ "language": "cobol", "kind": "01-level" }),
            ));
            continue;
        }

        if in_procedure {
            if let Some(sec) = match_section(upper_trim) {
                let prev_end = idx.max(1);
                close_span(&mut symbols, current_para_idx.take(), prev_end);
                close_span(&mut symbols, current_section_idx.take(), prev_end);

                let qn = qualify(program.as_deref(), &sec);
                current_para = Some(sec.clone());
                let sym_idx = symbols.len();
                symbols.push(make_symbol(
                    sec,
                    SymbolType::Function,
                    Some(qn),
                    &file,
                    line_no,
                    line_no,
                    serde_json::json!({ "language": "cobol", "kind": "section" }),
                ));
                current_section_idx = Some(sym_idx);
                continue;
            }
            if let Some(para) = match_paragraph(upper_trim) {
                let prev_end = idx.max(1);
                close_span(&mut symbols, current_para_idx.take(), prev_end);

                let qn = qualify(program.as_deref(), &para);
                current_para = Some(para.clone());
                let sym_idx = symbols.len();
                symbols.push(make_symbol(
                    para,
                    SymbolType::Function,
                    Some(qn),
                    &file,
                    line_no,
                    line_no,
                    serde_json::json!({ "language": "cobol", "kind": "paragraph" }),
                ));
                current_para_idx = Some(sym_idx);
                continue;
            }
        }

        let caller = current_para
            .as_deref()
            .or(program.as_deref())
            .unwrap_or("<program>");

        if let Some((target, thru)) = match_perform(upper_trim) {
            relations.push(make_call(
                caller,
                &target,
                &file,
                line_no,
                serde_json::json!({
                    "language": "cobol",
                    "kind": "perform",
                    "thru": thru,
                }),
            ));
        }

        if let Some((target, dynamic)) = match_call(upper_trim) {
            relations.push(make_call(
                caller,
                &target,
                &file,
                line_no,
                serde_json::json!({
                    "language": "cobol",
                    "kind": "call",
                    "dynamic": dynamic,
                }),
            ));
        }

        process_exec_block(
            upper_trim,
            caller,
            &file,
            line_no,
            &mut symbols,
            &mut relations,
        );
    }

    let final_line = n.max(1);
    close_span(&mut symbols, current_para_idx, final_line);
    close_span(&mut symbols, current_section_idx, final_line);
    close_span(&mut symbols, program_idx, final_line);

    ExtractAllResult::from_parts(symbols, relations)
}

fn process_exec_block(
    upper_trim: &str,
    caller: &str,
    file: &str,
    line_no: usize,
    symbols: &mut Vec<Symbol>,
    relations: &mut Vec<Relation>,
) {
    if let Some((target, kind)) = match_exec_cics(upper_trim) {
        relations.push(make_call(
            caller,
            &target,
            file,
            line_no,
            serde_json::json!({
                "language": "cobol",
                "kind": kind,
            }),
        ));
    }
    if let Some(sql_inc) = match_exec_sql_include(upper_trim) {
        symbols.push(make_symbol(
            sql_inc.clone(),
            SymbolType::Import,
            Some(sql_inc.clone()),
            file,
            line_no,
            line_no,
            serde_json::json!({
                "language": "cobol",
                "import_path": sql_inc,
                "kind": "exec-sql-include",
            }),
        ));
    }
}

/// Extract the active COBOL code slice from a raw line without allocating.
///
/// Handles both traditional fixed-format (cols 1–6 sequence numbers/spaces, col 7
/// indicator, cols 8–72 Area A/B) and modern free-format COBOL, while avoiding
/// UTF-8 byte-boundary panics and avoiding truncating 2/4-space indented free-format
/// level numbers (such as `"    01 CUST-REC."`).
fn cobol_code_area(line: &str) -> Option<&str> {
    let line = line.trim_end_matches(['\r', '\n']);
    if line.is_empty() {
        return None;
    }

    let trimmed = line.trim();
    if trimmed.is_empty() || trimmed.starts_with("*>") || trimmed.starts_with('*') {
        return None;
    }

    // Check if the first 7 bytes are valid ASCII fixed-format sequence + indicator columns.
    if line.len() >= 7 && line.is_char_boundary(6) && line.is_char_boundary(7) {
        let seq = &line[..6];
        let indicator = line.as_bytes()[6];
        let is_all_spaces = seq.bytes().all(|b| b == b' ');
        let is_6_digits = seq.bytes().all(|b| b.is_ascii_digit());
        if (is_all_spaces || is_6_digits)
            && matches!(indicator, b' ' | b'*' | b'/' | b'D' | b'd' | b'-')
        {
            if matches!(indicator, b'*' | b'/' | b'D' | b'd') {
                return None;
            }
            let end = char_floor_boundary(line, 72);
            let code = if end > 7 { &line[7..end] } else { "" };
            let t = code.trim();
            if t.is_empty() {
                return None;
            }
            return Some(t);
        }
    }

    Some(trimmed)
}

fn char_floor_boundary(s: &str, max_byte: usize) -> usize {
    let mut idx = max_byte.min(s.len());
    while idx > 0 && !s.is_char_boundary(idx) {
        idx -= 1;
    }
    idx
}

fn close_span(symbols: &mut [Symbol], sym_idx: Option<usize>, end_line: usize) {
    if let Some(idx) = sym_idx
        && let Some(sym) = symbols.get_mut(idx)
    {
        sym.location.end_line = end_line.max(sym.location.start_line);
    }
}

fn match_program_id(upper: &str) -> Option<String> {
    // PROGRAM-ID. NAME.  or PROGRAM-ID. NAME
    let rest = upper.strip_prefix("PROGRAM-ID")?;
    let rest = rest.trim_start_matches(['.', ' ', '\t']);
    let name = rest
        .split(|c: char| c == '.' || c.is_whitespace() || c == ',')
        .next()?
        .trim();
    if name.is_empty() {
        return None;
    }
    Some(name.to_string())
}

fn match_copy(upper: &str) -> Option<String> {
    let rest = upper.strip_prefix("COPY")?;
    if !rest.starts_with(|c: char| c.is_whitespace()) {
        return None;
    }
    let rest = rest.trim_start();
    let token = rest
        .split(|c: char| c == '.' || c.is_whitespace() || c == ',')
        .next()?
        .trim()
        .trim_matches(|c| c == '\'' || c == '"');
    if token.is_empty() || token == "OF" || token == "IN" {
        return None;
    }
    Some(token.to_string())
}

fn match_exec_sql_include(upper: &str) -> Option<String> {
    if !upper.contains("EXEC SQL") {
        return None;
    }
    let idx = upper.find("INCLUDE")?;
    let after = upper[idx + "INCLUDE".len()..].trim_start();
    let token = after
        .split(|c: char| c.is_whitespace() || c == '.' || c == ',')
        .next()?
        .trim()
        .trim_matches(|c| c == '\'' || c == '"');
    if token.is_empty() || token == "END-EXEC" {
        return None;
    }
    Some(token.to_string())
}

fn match_01_level(upper: &str) -> Option<String> {
    let rest = upper.strip_prefix("01")?;
    if !rest.starts_with(|c: char| c.is_whitespace()) {
        return None;
    }
    let name = rest
        .trim_start()
        .split(|c: char| c.is_whitespace() || c == '.' || c == ',')
        .next()?
        .trim();
    if name.is_empty() || name == "FILLER" {
        return None;
    }
    Some(name.to_string())
}

fn match_section(upper: &str) -> Option<String> {
    // NAME SECTION.
    let trimmed = upper.trim_end_matches('.').trim();
    let (name, rest) = trimmed.rsplit_once(char::is_whitespace)?;
    if !rest.eq_ignore_ascii_case("SECTION") {
        return None;
    }
    let name = name.trim();
    if name.is_empty() || is_reserved_header(name) {
        return None;
    }
    Some(name.to_string())
}

fn match_paragraph(upper: &str) -> Option<String> {
    // NAME.   (paragraph header — single token ending with period)
    let t = upper.trim();
    if !t.ends_with('.') {
        return None;
    }
    let name = t.trim_end_matches('.').trim();
    if name.is_empty() || name.contains(char::is_whitespace) {
        return None;
    }
    if is_reserved_paragraph(name) {
        return None;
    }
    // Must look like a COBOL word
    if !name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return None;
    }
    Some(name.to_string())
}

fn match_perform(upper: &str) -> Option<(String, Option<String>)> {
    let rest = upper.strip_prefix("PERFORM")?;
    if !rest.starts_with(|c: char| c.is_whitespace()) {
        return None;
    }
    let mut parts = rest.split_whitespace();
    let target = parts.next()?.trim_matches(|c| c == '.' || c == ',');
    if target.is_empty() || is_perform_adverb(target) {
        return None;
    }
    let mut thru = None;
    let mut iter = parts;
    while let Some(tok) = iter.next() {
        if tok == "THRU" || tok == "THROUGH" {
            if let Some(end) = iter.next() {
                thru = Some(end.trim_matches(|c| c == '.' || c == ',').to_string());
            }
            break;
        }
    }
    Some((target.to_string(), thru))
}

fn match_call(upper: &str) -> Option<(String, bool)> {
    let rest = upper.strip_prefix("CALL")?;
    if !rest.starts_with(|c: char| c.is_whitespace()) {
        return None;
    }
    let rest = rest.trim_start();
    if rest.is_empty() {
        return None;
    }
    if rest.starts_with('\'') || rest.starts_with('"') {
        let quote = rest.chars().next()?;
        let end = rest[1..].find(quote)?;
        let name = &rest[1..1 + end];
        if name.is_empty() {
            return None;
        }
        return Some((name.to_string(), false));
    }
    let name = rest
        .split(|c: char| c.is_whitespace() || c == '.' || c == ',')
        .next()?
        .trim();
    if name.is_empty() {
        return None;
    }
    // Identifier CALL is treated as dynamic (variable).
    Some((name.to_string(), true))
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
    // PROGRAM('NAME') or PROGRAM("NAME") or PROGRAM(NAME)
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

fn is_reserved_header(name: &str) -> bool {
    matches!(
        name,
        "IDENTIFICATION"
            | "ENVIRONMENT"
            | "DATA"
            | "PROCEDURE"
            | "WORKING-STORAGE"
            | "LINKAGE"
            | "FILE"
            | "LOCAL-STORAGE"
            | "CONFIGURATION"
            | "INPUT-OUTPUT"
            | "COMMUNICATION"
            | "REPORT"
            | "SCREEN"
    )
}

fn is_reserved_paragraph(name: &str) -> bool {
    is_reserved_header(name)
        || matches!(
            name,
            "EXIT"
                | "EJECT"
                | "SKIP1"
                | "SKIP2"
                | "SKIP3"
                | "STOP"
                | "GOBACK"
                | "CONTINUE"
                | "NEXT"
                | "END-IF"
                | "END-PERFORM"
                | "END-EVALUATE"
                | "END-READ"
                | "END-WRITE"
                | "END-EXEC"
                | "ELSE"
                | "WHEN"
        )
        || name.ends_with("-DIVISION")
        || name.ends_with("DIVISION")
}

fn is_perform_adverb(tok: &str) -> bool {
    matches!(
        tok,
        "VARYING" | "UNTIL" | "TIMES" | "WITH" | "TEST" | "BEFORE" | "AFTER" | "INLINE"
    )
}

fn qualify(program: Option<&str>, local: &str) -> String {
    match program {
        Some(p) => format!("{p}::{local}"),
        None => local.to_string(),
    }
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
