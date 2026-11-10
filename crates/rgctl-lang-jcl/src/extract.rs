//! Card/statement JCL extractor (no tree-sitter).

use rgctl_plugin_api::{
    ComplexityMetrics, ExtractAllResult, LanguageCapabilities, LanguagePlugin, Relation,
    RelationType, Result, SourceLocation, Symbol, SymbolType,
};
use std::collections::{HashMap, HashSet};
use std::path::Path;

/// JCL structural language plugin.
pub struct JclPlugin;

impl LanguagePlugin for JclPlugin {
    fn language_id(&self) -> &str {
        "jcl"
    }

    fn file_extensions(&self) -> Vec<&str> {
        vec!["jcl", "proc", "cntl"]
    }

    fn grammar(&self) -> Option<tree_sitter::Language> {
        None
    }

    fn capabilities(&self) -> LanguageCapabilities {
        LanguageCapabilities {
            extracts_functions: true,
            extracts_types: false,
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

#[derive(Debug, Clone)]
struct Statement {
    start_line: usize,
    end_line: usize,
    name: String,
    operation: String,
    operands: String,
}

fn extract(file_path: &Path, source: &[u8]) -> ExtractAllResult {
    let file = file_path.to_string_lossy().into_owned();
    let text = String::from_utf8_lossy(source);
    let raw_lines: Vec<&str> = text.lines().collect();
    let statements = coalesce_statements(&raw_lines);

    let mut symbols: Vec<Symbol> = Vec::with_capacity(statements.len() + 8);
    let mut relations: Vec<Relation> = Vec::with_capacity(statements.len());
    let mut seen_datasets: HashSet<String> = HashSet::new();
    let mut dataset_writers: HashMap<String, String> = HashMap::new();

    let mut job_or_proc: Option<String> = None;
    let mut current_step: Option<String> = None;
    let mut current_step_is_tso: bool = false;

    for stmt in &statements {
        let op = stmt.operation.as_str();
        let operands_upper = stmt.operands.to_ascii_uppercase();

        match op {
            "JOB" => {
                let name = if stmt.name.is_empty() {
                    "JOB".to_string()
                } else {
                    stmt.name.clone()
                };
                job_or_proc = Some(name.clone());
                current_step = None;
                current_step_is_tso = false;
                symbols.push(make_symbol(
                    name,
                    SymbolType::Module,
                    job_or_proc.clone(),
                    &file,
                    stmt.start_line,
                    stmt.end_line,
                    serde_json::json!({ "language": "jcl", "kind": "job" }),
                ));
            }
            "PROC" => {
                let name = if stmt.name.is_empty() {
                    "PROC".to_string()
                } else {
                    stmt.name.clone()
                };
                job_or_proc = Some(name.clone());
                current_step = None;
                current_step_is_tso = false;
                symbols.push(make_symbol(
                    name.clone(),
                    SymbolType::Function,
                    Some(name),
                    &file,
                    stmt.start_line,
                    stmt.end_line,
                    serde_json::json!({ "language": "jcl", "kind": "proc" }),
                ));
            }
            "EXEC" => {
                let step = if stmt.name.is_empty() {
                    format!("STEP{}", stmt.start_line)
                } else {
                    stmt.name.clone()
                };
                let qn = match job_or_proc.as_deref() {
                    Some(j) => format!("{j}::{step}"),
                    None => step.clone(),
                };
                symbols.push(make_symbol(
                    step.clone(),
                    SymbolType::Function,
                    Some(qn),
                    &file,
                    stmt.start_line,
                    stmt.end_line,
                    serde_json::json!({ "language": "jcl", "kind": "step" }),
                ));
                current_step = Some(step.clone());

                if let Some(pgm) = operand_value(&operands_upper, "PGM") {
                    current_step_is_tso = is_tso_launcher(&pgm);
                    relations.push(make_rel(
                        &step,
                        &pgm,
                        RelationType::Calls,
                        &file,
                        stmt.start_line,
                        serde_json::json!({ "language": "jcl", "kind": "exec-pgm" }),
                    ));
                } else if let Some(proc) = operand_value(&operands_upper, "PROC") {
                    current_step_is_tso = false;
                    relations.push(make_rel(
                        &step,
                        &proc,
                        RelationType::Calls,
                        &file,
                        stmt.start_line,
                        serde_json::json!({ "language": "jcl", "kind": "exec-proc" }),
                    ));
                } else if let Some(implied) = first_positional_proc(&operands_upper) {
                    // EXEC procname — implied PROC=
                    current_step_is_tso = false;
                    relations.push(make_rel(
                        &step,
                        &implied,
                        RelationType::Calls,
                        &file,
                        stmt.start_line,
                        serde_json::json!({ "language": "jcl", "kind": "exec-proc" }),
                    ));
                } else {
                    current_step_is_tso = false;
                }
            }
            "INCLUDE" => {
                if let Some(mem) = operand_value(&operands_upper, "MEMBER") {
                    symbols.push(make_symbol(
                        mem.clone(),
                        SymbolType::Import,
                        Some(mem.clone()),
                        &file,
                        stmt.start_line,
                        stmt.end_line,
                        serde_json::json!({
                            "language": "jcl",
                            "import_path": mem,
                        }),
                    ));
                }
            }
            "DD" => {
                let ddname = stmt.name.to_ascii_uppercase();
                if is_system_dd(&ddname, &operands_upper) {
                    continue;
                }

                // Inline DD * / DD DATA stream: scan for RUN PROGRAM when TSO launcher step.
                if operands_upper.trim_start().starts_with('*')
                    || operand_value(&operands_upper, "DATA").is_some()
                    || operands_upper.split(',').any(|p| p.trim() == "*")
                {
                    if current_step_is_tso && let Some(step) = current_step.as_deref() {
                        for prog in scan_systsin_run_programs(&raw_lines, stmt.end_line) {
                            relations.push(make_rel(
                                step,
                                &prog,
                                RelationType::Calls,
                                &file,
                                stmt.end_line + 1,
                                serde_json::json!({
                                    "language": "jcl",
                                    "kind": "systsin-run-program",
                                }),
                            ));
                        }
                    }
                    continue;
                }

                if let Some(dsn) = operand_value(&operands_upper, "DSN")
                    .or_else(|| operand_value(&operands_upper, "DSNAME"))
                {
                    if dsn == "*" || dsn.eq_ignore_ascii_case("NULLFILE") {
                        continue;
                    }
                    let disp = parse_disp(&operands_upper);
                    if seen_datasets.insert(dsn.clone()) {
                        symbols.push(make_symbol(
                            dsn.clone(),
                            SymbolType::Variable,
                            Some(dsn.clone()),
                            &file,
                            stmt.start_line,
                            stmt.end_line,
                            serde_json::json!({
                                "language": "jcl",
                                "kind": "dataset",
                                "disp": disp,
                            }),
                        ));
                    }
                    if let Some(step) = current_step.as_deref() {
                        relations.push(make_rel(
                            step,
                            &dsn,
                            RelationType::References,
                            &file,
                            stmt.start_line,
                            serde_json::json!({
                                "language": "jcl",
                                "kind": "dd-dsn",
                                "disp": disp,
                                "ddname": ddname,
                            }),
                        ));
                        if disp == "write" {
                            dataset_writers.insert(dsn.clone(), step.to_string());
                        } else if let Some(writer_step) = dataset_writers.get(&dsn)
                            && writer_step != step
                        {
                            // Emit directed producer -> consumer pipeline edge so blast-radius
                            // and graph traversal can follow STEP1 (NEW/MOD) -> STEP2 (SHR/OLD).
                            relations.push(make_rel(
                                writer_step,
                                step,
                                RelationType::References,
                                &file,
                                stmt.start_line,
                                serde_json::json!({
                                    "language": "jcl",
                                    "kind": "dataset-pipeline",
                                    "dataset": dsn,
                                }),
                            ));
                        }
                    }
                }
            }
            _ => {}
        }
    }

    ExtractAllResult::from_parts(symbols, relations)
}

/// Merge continued JCL cards (`//   more,`) into logical statements.
fn coalesce_statements(lines: &[&str]) -> Vec<Statement> {
    let mut out: Vec<Statement> = Vec::with_capacity(lines.len() / 2 + 4);
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        let line_no = i + 1;

        if line.starts_with("//*") || line.trim_start().starts_with("/*") || !line.starts_with("//")
        {
            i += 1;
            continue;
        }

        let Some((name, op, mut operands)) = parse_jcl_card(line) else {
            i += 1;
            continue;
        };

        let start_line = line_no;
        let mut end_line = line_no;
        i += 1;

        // Continuations: prior operands end with comma; next card is `//` + blanks + operands.
        while operands.trim_end().ends_with(',') && i < lines.len() {
            let cont = lines[i];
            if cont.starts_with("//*") {
                i += 1;
                continue;
            }
            let Some(extra) = continuation_operands(cont) else {
                break;
            };
            if extra.is_empty() {
                i += 1;
                break;
            }
            operands.push_str(extra);
            end_line = i + 1;
            i += 1;
        }

        out.push(Statement {
            start_line,
            end_line,
            name,
            operation: op,
            operands,
        });
    }
    out
}

fn parse_jcl_card(line: &str) -> Option<(String, String, String)> {
    // //NAME OP OPERANDS   or  // OP OPERANDS  or // NAME OP
    if !line.starts_with("//") {
        return None;
    }
    let rest = &line[2..];
    // INCLUDE may appear as `// INCLUDE MEMBER=...` (blank name).
    let trimmed = rest.trim_start();
    if trimmed.is_empty() {
        return None;
    }

    // If line is `//NAMEOP...` without space after name — classic format:
    // name is cols 3-10 (8 chars), op starts after.
    // We use a whitespace-tolerant parse:
    // - If rest starts with whitespace, name is empty.
    // - Else name is first token, then op, then operands.
    // `split_whitespace` collapses runs of spaces (`//STEP1   EXEC …`).
    if rest.starts_with(|c: char| c.is_whitespace()) {
        let mut parts = trimmed.split_whitespace();
        let op = parts.next()?.to_ascii_uppercase();
        let operands = parts.collect::<Vec<_>>().join(" ");
        if op.is_empty() {
            return None;
        }
        return Some((String::new(), op, operands));
    }

    let mut parts = rest.split_whitespace();
    let name = parts.next()?.to_string();
    let op = parts.next()?.to_ascii_uppercase();
    let operands = parts.collect::<Vec<_>>().join(" ");
    if op.is_empty() {
        return None;
    }
    Some((name, op, operands))
}

fn continuation_operands(line: &str) -> Option<&str> {
    if !line.starts_with("//") {
        return None;
    }
    let rest = &line[2..];
    // Continuation cards have a blank name field (whitespace immediately after `//`).
    if !rest.starts_with(|c: char| c.is_whitespace()) {
        return None;
    }
    let trimmed = rest.trim_start();
    if trimmed.is_empty() {
        return Some("");
    }
    // New statements with a blank name (`// EXEC ...`) are not continuations.
    let first = trimmed
        .split(|c: char| c == '=' || c == ',' || c.is_whitespace())
        .next()
        .unwrap_or("");
    let upper = first.to_ascii_uppercase();
    if matches!(
        upper.as_str(),
        "JOB"
            | "EXEC"
            | "DD"
            | "PROC"
            | "PEND"
            | "INCLUDE"
            | "SET"
            | "IF"
            | "THEN"
            | "ELSE"
            | "ENDIF"
            | "JCLLIB"
            | "OUTPUT"
            | "CNTL"
            | "COMMAND"
    ) {
        return None;
    }
    // Continuations are operand fragments (must contain `=` or be a keyword-less list).
    if !trimmed.contains('=') && !trimmed.contains(',') {
        return None;
    }
    Some(trimmed)
}

fn operand_value(operands_upper: &str, key: &str) -> Option<String> {
    // Find KEY=VALUE at a token boundary (start of string or preceded by `,`, `(`, or whitespace).
    let pattern = format!("{key}=");
    let mut search_from = 0;
    let idx = loop {
        let rel = operands_upper[search_from..].find(&pattern)?;
        let abs = search_from + rel;
        if abs == 0 {
            break abs;
        }
        let prev = operands_upper.as_bytes()[abs - 1];
        if prev == b',' || prev == b'(' || prev.is_ascii_whitespace() {
            break abs;
        }
        search_from = abs + pattern.len();
    };
    let after = &operands_upper[idx + pattern.len()..];
    let value = if let Some(rest) = after.strip_prefix('\'') {
        let end = rest.find('\'')?;
        rest[..end].to_string()
    } else if let Some(rest) = after.strip_prefix('"') {
        let end = rest.find('"')?;
        rest[..end].to_string()
    } else if after.starts_with('(') {
        // Take through matching paren or until comma at depth 0 — for DISP=(NEW,CATLG) we
        // want the whole group when key is DISP; for DSN we rarely see parens.
        let mut depth = 0;
        let mut end = 0;
        for (i, c) in after.char_indices() {
            match c {
                '(' => depth += 1,
                ')' => {
                    depth -= 1;
                    if depth == 0 {
                        end = i + 1;
                        break;
                    }
                }
                _ => {}
            }
        }
        if end == 0 {
            return None;
        }
        after[..end].to_string()
    } else {
        after
            .split(',')
            .next()?
            .trim()
            .trim_end_matches(')')
            .to_string()
    };
    if value.is_empty() {
        return None;
    }
    Some(value)
}

fn first_positional_proc(operands_upper: &str) -> Option<String> {
    // EXEC BILLPROC,PARM=...  — first token before comma without '='.
    let first = operands_upper.split(',').next()?.trim();
    if first.is_empty() || first.contains('=') {
        return None;
    }
    Some(first.to_string())
}

fn is_tso_launcher(pgm: &str) -> bool {
    matches!(pgm, "IKJEFT01" | "IKJEFT1A" | "IKJEFT1B")
}

fn is_system_dd(ddname: &str, operands_upper: &str) -> bool {
    if matches!(
        ddname,
        "STEPLIB" | "JOBLIB" | "SYSPRINT" | "SYSDUMP" | "SYSUDUMP" | "SYSABEND"
    ) {
        return true;
    }
    if operands_upper.contains("DUMMY") || operand_value(operands_upper, "DUMMY").is_some() {
        return true;
    }
    // SYSOUT=*
    if let Some(v) = operand_value(operands_upper, "SYSOUT")
        && (v == "*" || v.starts_with('*'))
    {
        return true;
    }
    false
}

fn parse_disp(operands_upper: &str) -> &'static str {
    let Some(raw) = operand_value(operands_upper, "DISP") else {
        return "shr";
    };
    let inner = raw
        .trim()
        .trim_start_matches('(')
        .split(',')
        .next()
        .unwrap_or("SHR")
        .trim();
    match inner {
        "NEW" | "MOD" => "write",
        "OLD" | "SHR" => "read",
        other if other.starts_with("NEW") || other.starts_with("MOD") => "write",
        _ => "read",
    }
}

/// Scan lines after a `DD *` card until `/*` for `RUN PROGRAM(name)`.
fn scan_systsin_run_programs(lines: &[&str], dd_end_line: usize) -> Vec<String> {
    let mut out = Vec::new();
    let mut i = dd_end_line; // 1-based end of DD stmt → next line index = dd_end_line
    while i < lines.len() {
        let line = lines[i];
        if line.trim_start().starts_with("/*") {
            break;
        }
        if line.starts_with("//") && !line.starts_with("//*") {
            // Next JCL statement ends the instream (when not using /*).
            break;
        }
        let upper = line.to_ascii_uppercase();
        if let Some(prog) = match_run_program(&upper) {
            out.push(prog);
        }
        i += 1;
    }
    out
}

fn match_run_program(upper: &str) -> Option<String> {
    let idx = upper.find("RUN")?;
    let after_run = upper[idx + 3..].trim_start();
    let after = after_run.strip_prefix("PROGRAM")?;
    let after = after.trim_start().strip_prefix('(')?.trim_start();
    let name = if let Some(rest) = after.strip_prefix('\'') {
        let end = rest.find('\'')?;
        rest[..end].to_string()
    } else {
        after
            .split(|c: char| c == ')' || c == ',' || c.is_whitespace())
            .next()?
            .to_string()
    };
    if name.is_empty() {
        return None;
    }
    Some(name)
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

fn make_rel(
    from: &str,
    to: &str,
    relation_type: RelationType,
    file: &str,
    line: usize,
    metadata: serde_json::Value,
) -> Relation {
    Relation {
        from: from.to_string(),
        to: to.to_string(),
        relation_type,
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
