//! `rgctl cpg` — hybrid CPG façade (L_repo ⟷ L_proc).

use super::args::{InspectLayer, OutputFormat, PdgEdgeLayer, SliceDirection, SliceView};
use super::context::CliContext;
use super::inspect::{self, InspectArgs, suggest_function_names};
use super::markup::markup_context_unsupported;
use super::slice::{self, SliceArgs, normalize_function_selector};
use anyhow::Result;
use rgctl_analysis::{
    AstSkeletonArchive, CpgExportFormat, CpgExportScope, CpgFlowsArgs, MutationQuery,
    SliceDirection as AnalysisSliceDirection, cpg_calls, cpg_flows, cpg_function, cpg_mutations,
    cpg_status, export_cpg, find_function_nodes, parse_fqn_symbol,
};
use rgctl_error::SymbolCandidate;
use rgctl_graph::backend::MemoryBackend;
use rgctl_graph::schema::Node;
use std::path::Path;

pub enum CpgAction {
    Status,
    Function {
        symbol: String,
        file: Option<String>,
        class: Option<String>,
        line: Option<usize>,
    },
    Calls {
        symbol: String,
        file: Option<String>,
        class: Option<String>,
        line: Option<usize>,
    },
    Mutations {
        type_name: String,
        exclude_ctors: bool,
        member: Option<String>,
        include_unresolved: bool,
    },
    Flows {
        file: String,
        line: usize,
        variable: String,
        function: String,
        language: Option<String>,
        direction: SliceDirection,
        with_alias: bool,
    },
    Ast {
        symbol: String,
        file: Option<String>,
        class: Option<String>,
        line: Option<usize>,
    },
    Export {
        format: String,
        output: String,
        path_contains: Option<String>,
        include_l_proc: bool,
        include_field_writes: bool,
    },
    Pdg {
        symbol: String,
        file: Option<String>,
        class: Option<String>,
        line: Option<usize>,
        edge_layer: PdgEdgeLayer,
        def_use: bool,
    },
    Slice {
        file: String,
        line: usize,
        variable: String,
        function: Option<String>,
        language: Option<String>,
        direction: SliceDirection,
        taint: bool,
        view: SliceView,
    },
}

fn cpg_action_to_service(action: &CpgAction) -> rgctl_service::CpgArgs {
    use rgctl_service::{CpgArgs, CpgOp};
    let dir = |d: &SliceDirection| match d {
        SliceDirection::Forward => Some("forward".into()),
        SliceDirection::Backward => Some("backward".into()),
    };
    match action {
        CpgAction::Status => CpgArgs {
            op: CpgOp::Status,
            symbol: None,
            type_name: None,
            file: None,
            line: None,
            variable: None,
            function: None,
            exclude_ctors: false,
            member: None,
            include_unresolved: false,
            direction: None,
            with_alias: false,
        },
        CpgAction::Function { symbol, .. } => CpgArgs {
            op: CpgOp::Function,
            symbol: Some(symbol.clone()),
            type_name: None,
            file: None,
            line: None,
            variable: None,
            function: None,
            exclude_ctors: false,
            member: None,
            include_unresolved: false,
            direction: None,
            with_alias: false,
        },
        CpgAction::Calls { symbol, .. } => CpgArgs {
            op: CpgOp::Calls,
            symbol: Some(symbol.clone()),
            type_name: None,
            file: None,
            line: None,
            variable: None,
            function: None,
            exclude_ctors: false,
            member: None,
            include_unresolved: false,
            direction: None,
            with_alias: false,
        },
        CpgAction::Mutations {
            type_name,
            exclude_ctors,
            member,
            include_unresolved,
        } => CpgArgs {
            op: CpgOp::Mutations,
            symbol: None,
            type_name: Some(type_name.clone()),
            file: None,
            line: None,
            variable: None,
            function: None,
            exclude_ctors: *exclude_ctors,
            member: member.clone(),
            include_unresolved: *include_unresolved,
            direction: None,
            with_alias: false,
        },
        CpgAction::Flows {
            file,
            line,
            variable,
            function,
            direction,
            with_alias,
            ..
        } => CpgArgs {
            op: CpgOp::Flows,
            symbol: None,
            type_name: None,
            file: Some(file.clone()),
            line: Some(*line),
            variable: Some(variable.clone()),
            function: Some(function.clone()),
            exclude_ctors: false,
            member: None,
            include_unresolved: false,
            direction: dir(direction),
            with_alias: *with_alias,
        },
        CpgAction::Ast { symbol, .. } => CpgArgs {
            op: CpgOp::Ast,
            symbol: Some(symbol.clone()),
            type_name: None,
            file: None,
            line: None,
            variable: None,
            function: None,
            exclude_ctors: false,
            member: None,
            include_unresolved: false,
            direction: None,
            with_alias: false,
        },
        CpgAction::Pdg { symbol, .. } => CpgArgs {
            op: CpgOp::Pdg,
            symbol: Some(symbol.clone()),
            type_name: None,
            file: None,
            line: None,
            variable: None,
            function: None,
            exclude_ctors: false,
            member: None,
            include_unresolved: false,
            direction: None,
            with_alias: false,
        },
        CpgAction::Slice {
            file,
            line,
            variable,
            function,
            direction,
            ..
        } => CpgArgs {
            op: CpgOp::Slice,
            symbol: None,
            type_name: None,
            file: Some(file.clone()),
            line: Some(*line),
            variable: Some(variable.clone()),
            function: function.clone(),
            exclude_ctors: false,
            member: None,
            include_unresolved: false,
            direction: dir(direction),
            with_alias: false,
        },
        CpgAction::Export { .. } => unreachable!("export stays on CLI path"),
    }
}

fn action_needs_cli_symbol_resolve(action: &CpgAction) -> bool {
    match action {
        CpgAction::Function {
            file, class, line, ..
        }
        | CpgAction::Calls {
            file, class, line, ..
        }
        | CpgAction::Ast {
            file, class, line, ..
        }
        | CpgAction::Pdg {
            file, class, line, ..
        } => file.is_some() || class.is_some() || line.is_some(),
        _ => false,
    }
}

pub fn run(ctx: &CliContext, action: CpgAction) -> Result<()> {
    // Service JSON path has no --file/--class/--line filters; use CLI resolve when present.
    if ctx.format == OutputFormat::Json
        && !matches!(&action, CpgAction::Export { .. })
        && !action_needs_cli_symbol_resolve(&action)
    {
        let svc = cpg_action_to_service(&action);
        let mut session = rgctl_service::Session::new(&ctx.repo);
        let value =
            rgctl_service::execute(&mut session, rgctl_service::Command::Cpg(svc))?;
        return ctx.emit_json_value(&value);
    }
    match action {
        CpgAction::Status => run_status(ctx),
        CpgAction::Function {
            symbol,
            file,
            class,
            line,
        } => run_function(ctx, &symbol, file.as_deref(), class.as_deref(), line),
        CpgAction::Calls {
            symbol,
            file,
            class,
            line,
        } => run_calls(ctx, &symbol, file.as_deref(), class.as_deref(), line),
        CpgAction::Mutations {
            type_name,
            exclude_ctors,
            member,
            include_unresolved,
        } => run_mutations(ctx, type_name, exclude_ctors, member, include_unresolved),
        CpgAction::Flows {
            file,
            line,
            variable,
            function,
            language,
            direction,
            with_alias,
        } => run_flows(
            ctx, file, line, variable, function, language, direction, with_alias,
        ),
        CpgAction::Ast {
            symbol,
            file,
            class,
            line,
        } => run_ast(ctx, &symbol, file.as_deref(), class.as_deref(), line),
        CpgAction::Export {
            format,
            output,
            path_contains,
            include_l_proc,
            include_field_writes,
        } => run_export(
            ctx,
            &format,
            &output,
            path_contains,
            include_l_proc,
            include_field_writes,
        ),
        CpgAction::Pdg {
            symbol,
            file,
            class,
            line,
            edge_layer,
            def_use,
        } => inspect::run(
            ctx,
            InspectArgs {
                symbol,
                file,
                class,
                line,
                layer: InspectLayer::Pdg {
                    edge_layer,
                    def_use,
                },
            },
        ),
        CpgAction::Slice {
            file,
            line,
            variable,
            function,
            language,
            direction,
            taint,
            view,
        } => slice::run(
            ctx,
            SliceArgs {
                file,
                line,
                variable,
                function,
                language,
                direction,
                taint,
                view,
            },
        ),
    }
}

fn run_status(ctx: &CliContext) -> Result<()> {
    let status = cpg_status(&ctx.repo)?;
    if ctx.format == OutputFormat::Json {
        return ctx.emit_json_value(&serde_json::to_value(&status)?);
    }
    if status.archive_present {
        println!(
            "CPG L_proc: ready ({} functions) at {}",
            status.function_count, status.archive_path
        );
        if let Some(d) = &status.graph_digest {
            println!("  graph_digest: {d}");
        }
    } else {
        println!(
            "CPG L_proc: missing (run `rgctl discover --with-cfg`)\n  expected: {}",
            status.archive_path
        );
    }
    if status.field_write_index_present {
        println!(
            "CPG field writes: {} indexed (cpg mutations)",
            status.field_write_count
        );
    } else {
        println!("CPG field writes: missing (run discover --with-cfg)");
    }
    if status.ast_skeleton_present {
        println!("CPG AST skeleton: {} functions", status.ast_skeleton_count);
    } else {
        println!("CPG AST skeleton: missing (optional: discover --with-ast-skeleton)");
    }
    println!("CPG L_repo: use `cpg function` / `cpg calls` against the graph snapshot");
    Ok(())
}

fn run_mutations(
    ctx: &CliContext,
    type_name: String,
    exclude_ctors: bool,
    member: Option<String>,
    include_unresolved: bool,
) -> Result<()> {
    let result = cpg_mutations(
        &ctx.repo,
        MutationQuery {
            type_name,
            exclude_ctors,
            member,
            include_unresolved,
        },
    )?;
    if ctx.format == OutputFormat::Json {
        return ctx.emit_json_value(&serde_json::to_value(&result)?);
    }
    println!(
        "Mutations of {}{} ({} hits):",
        result.type_name,
        if result.exclude_ctors {
            " [excl. ctors]"
        } else {
            ""
        },
        result.mutations.len()
    );
    for m in &result.mutations {
        println!("  {}:{}  {}", m.file, m.line, m.code.trim());
    }
    Ok(())
}

fn run_flows(
    ctx: &CliContext,
    file: String,
    line: usize,
    variable: String,
    function: String,
    language: Option<String>,
    direction: SliceDirection,
    with_alias: bool,
) -> Result<()> {
    if let Some(msg) = markup_context_unsupported("cpg flows", Path::new(&file)) {
        anyhow::bail!(msg);
    }
    let direction = match direction {
        SliceDirection::Forward => AnalysisSliceDirection::Forward,
        SliceDirection::Backward => AnalysisSliceDirection::Backward,
    };
    // Prefer Class::method / Class.method as-is; analysis also accepts bare trailing name.
    let function = normalize_function_selector(&function, None);
    let result = cpg_flows(CpgFlowsArgs {
        repo_root: ctx.repo.clone(),
        file,
        line,
        variable,
        function,
        language,
        direction,
        with_alias,
    })?;
    if ctx.format == OutputFormat::Json {
        return ctx.emit_json_value(&serde_json::to_value(&result)?);
    }
    println!(
        "CPG flows ({}) {} @ {}:{} — {} steps ({:.0}% reduction){}",
        result.direction,
        result.variable,
        result.file,
        result.line,
        result.steps.len(),
        result.reduction_percent,
        if with_alias { " [alias]" } else { "" }
    );
    for step in &result.steps {
        println!("  {}: {}", step.line, step.code.trim());
    }
    Ok(())
}

fn run_ast(
    ctx: &CliContext,
    symbol: &str,
    file: Option<&str>,
    class: Option<&str>,
    line: Option<usize>,
) -> Result<()> {
    let archive = AstSkeletonArchive::open_if_exists(&ctx.repo)?.ok_or_else(|| {
        anyhow::anyhow!(
            "AST skeleton archive missing (run `rgctl discover --with-ast-skeleton`)"
        )
    })?;
    let bare = bare_method_name(symbol);
    let mut matches: Vec<_> = archive
        .records
        .iter()
        .filter(|r| {
            r.function_name == symbol
                || r.function_name == bare
                || r.function_name.ends_with(symbol)
                || format!("{}.{}", r.file_path, r.function_name).contains(symbol)
        })
        .collect();
    if let Some(file) = file {
        let f = file.replace('\\', "/");
        matches.retain(|r| {
            let p = r.file_path.replace('\\', "/");
            p.contains(&f) || p.ends_with(&f)
        });
    }
    if let Some(class) = class {
        matches.retain(|r| r.function_name.contains(class) || r.file_path.contains(class));
    }
    if let Some(line) = line {
        matches.retain(|r| {
            r.nodes
                .iter()
                .any(|n| n.start_line <= line && line <= n.end_line)
                || r.nodes.first().map(|n| n.start_line) == Some(line)
        });
    }
    if matches.is_empty() {
        let msg = format!("no AST skeleton for '{symbol}'");
        if ctx.format == OutputFormat::Json {
            let _ = ctx.emit_json_error(
                "not_found",
                "NOT_FOUND",
                &msg,
                None,
                None,
                Some("rgctl discover --with-ast-skeleton"),
            );
        }
        anyhow::bail!("{msg}");
    }
    if matches.len() > 1 && (file.is_none() && class.is_none() && line.is_none()) {
        // Keep multi-record listing when no disambiguation was requested (historical).
    } else if matches.len() > 1 {
        let rows: Vec<SymbolCandidate> = matches
            .iter()
            .map(|r| SymbolCandidate {
                id: String::new(),
                name: r.function_name.clone(),
                qualified_name: None,
                node_type: "function".into(),
                file: Some(r.file_path.clone()),
                line: r.nodes.first().map(|n| n.start_line),
            })
            .collect();
        let recovery =
            format!("rgctl cpg ast {symbol} --file <PATH> --class <CLASS> --line <N>");
        if ctx.format == OutputFormat::Json {
            let _ = ctx.emit_json_error(
                "ambiguous_symbol",
                "AMBIGUOUS_SYMBOL",
                &format!("Ambiguous symbol '{symbol}': {} matches", matches.len()),
                Some(&rows),
                None,
                Some(&recovery),
            );
        }
        anyhow::bail!(
            "Symbol '{symbol}' is ambiguous ({} matches). Disambiguate with --file / --class / --line.",
            matches.len()
        );
    }
    if ctx.format == OutputFormat::Json {
        return ctx.emit_json_value(&serde_json::json!({
            "schema_version": 1,
            "records": matches,
        }));
    }
    for rec in matches {
        println!(
            "{} ({}) — {} nodes",
            rec.function_name,
            rec.file_path,
            rec.nodes.len()
        );
        for n in &rec.nodes {
            let indent = if n.parent.is_some() { "  " } else { "" };
            println!(
                "{indent}[{:?}] L{}-{} {}",
                n.kind, n.start_line, n.end_line, n.label
            );
        }
    }
    Ok(())
}

fn run_export(
    ctx: &CliContext,
    format: &str,
    output: &str,
    path_contains: Option<String>,
    include_l_proc: bool,
    include_field_writes: bool,
) -> Result<()> {
    let format = match format.to_ascii_lowercase().as_str() {
        "graphml" => CpgExportFormat::GraphMl,
        "graphson" | "json" => CpgExportFormat::GraphSon,
        other => anyhow::bail!("unsupported export format '{other}' (graphml|graphson)"),
    };
    let graph = ctx.load_graph()?;
    let content = export_cpg(
        graph.backend(),
        &ctx.repo,
        format,
        &CpgExportScope {
            path_contains,
            include_l_proc,
            include_field_writes,
        },
    )?;
    std::fs::write(output, content)?;
    if ctx.format != OutputFormat::Json {
        println!("Wrote CPG export to {output}");
    } else {
        ctx.emit_json_value(&serde_json::json!({
            "schema_version": 1,
            "output": output,
        }))?;
    }
    Ok(())
}

fn bare_method_name(symbol: &str) -> &str {
    if let Some(idx) = symbol.rfind("::") {
        &symbol[idx + 2..]
    } else if let Some(idx) = symbol.rfind('.') {
        &symbol[idx + 1..]
    } else {
        symbol
    }
}

fn node_candidate(n: &Node) -> SymbolCandidate {
    SymbolCandidate {
        id: n.id.to_string(),
        name: n.name.to_string(),
        qualified_name: n.qualified_name.as_ref().map(|s| s.to_string()),
        node_type: "function".into(),
        file: n.file_path.as_ref().map(|s| s.to_string()),
        line: n.start_line,
    }
}

/// Resolve a unique function id using --file/--class/--line; no silent first-suffix pick.
fn resolve_cpg_symbol_id(
    ctx: &CliContext,
    backend: &MemoryBackend,
    symbol: &str,
    file: Option<&str>,
    class: Option<&str>,
    line: Option<usize>,
) -> Result<String> {
    let parsed = parse_fqn_symbol(
        symbol,
        class.map(str::to_string),
        file.map(str::to_string),
    );
    let mut matches = find_function_nodes(backend, &parsed.target_name)?;
    if let Some(file) = parsed.file_filter.as_deref().or(file) {
        let f = file.replace('\\', "/");
        matches.retain(|n| {
            n.file_path.as_deref().is_some_and(|p| {
                let p = p.replace('\\', "/");
                p.contains(&f) || p.ends_with(&f)
            })
        });
    }
    if let Some(class) = parsed.class_filter.as_deref().or(class) {
        matches.retain(|n| {
            n.qualified_name
                .as_deref()
                .is_some_and(|q| q.contains(class))
                || n.name.contains(class)
        });
    }
    if let Some(line) = line {
        matches.retain(|n| n.start_line == Some(line));
    }
    match matches.len() {
        1 => Ok(matches[0].id.to_string()),
        0 => {
            let suggestions = suggest_function_names(backend, &parsed.target_name, 5);
            let msg = format!("no function matching '{symbol}'");
            if ctx.format == OutputFormat::Json {
                let _ = ctx.emit_json_error(
                    "not_found",
                    "NOT_FOUND",
                    &msg,
                    None,
                    Some(&suggestions),
                    Some("rgctl find <PATTERN> --type function"),
                );
            }
            anyhow::bail!("{msg}");
        }
        count => {
            let rows: Vec<SymbolCandidate> = matches.iter().map(node_candidate).collect();
            let recovery =
                format!("rgctl cpg function {symbol} --file <PATH> --class <CLASS> --line <N>");
            if ctx.format == OutputFormat::Json {
                let _ = ctx.emit_json_error(
                    "ambiguous_symbol",
                    "AMBIGUOUS_SYMBOL",
                    &format!("Ambiguous symbol '{symbol}': {count} matches"),
                    Some(&rows),
                    None,
                    Some(&recovery),
                );
            }
            anyhow::bail!(
                "Symbol '{symbol}' is ambiguous ({count} matches). Disambiguate with --file / --class / --line."
            );
        }
    }
}

fn run_function(
    ctx: &CliContext,
    symbol: &str,
    file: Option<&str>,
    class: Option<&str>,
    line: Option<usize>,
) -> Result<()> {
    let graph = ctx.load_graph()?;
    let backend = graph.backend();
    let resolved = if file.is_some() || class.is_some() || line.is_some() || symbol.contains("::")
    {
        resolve_cpg_symbol_id(ctx, backend, symbol, file, class, line)?
    } else {
        // Keep AmbiguousSymbol from analysis when bare name is unique or not.
        match cpg_function(backend, &ctx.repo, symbol) {
            Ok(info) => {
                if ctx.format == OutputFormat::Json {
                    return ctx.emit_json_value(&serde_json::to_value(&info)?);
                }
                print_function_info(&info);
                return Ok(());
            }
            Err(rgctl_error::Error::AmbiguousSymbol { name, count, .. }) => {
                let matches = find_function_nodes(backend, &name)?;
                let rows: Vec<SymbolCandidate> = matches.iter().map(node_candidate).collect();
                let recovery =
                    format!("rgctl cpg function {name} --file <PATH> --class <CLASS> --line <N>");
                if ctx.format == OutputFormat::Json {
                    let _ = ctx.emit_json_error(
                        "ambiguous_symbol",
                        "AMBIGUOUS_SYMBOL",
                        &format!("Ambiguous symbol '{name}': {count} matches"),
                        Some(&rows),
                        None,
                        Some(&recovery),
                    );
                }
                anyhow::bail!(
                    "Symbol '{name}' is ambiguous ({count} matches). Disambiguate with --file / --class / --line."
                );
            }
            Err(rgctl_error::Error::NotFound(msg)) => {
                let suggestions = suggest_function_names(backend, symbol, 5);
                if ctx.format == OutputFormat::Json {
                    let _ = ctx.emit_json_error(
                        "not_found",
                        "NOT_FOUND",
                        &msg,
                        None,
                        Some(&suggestions),
                        Some("rgctl find <PATTERN> --type function"),
                    );
                }
                anyhow::bail!("{msg}");
            }
            Err(e) => return Err(e.into()),
        }
    };
    let info = cpg_function(backend, &ctx.repo, &resolved)?;
    if ctx.format == OutputFormat::Json {
        return ctx.emit_json_value(&serde_json::to_value(&info)?);
    }
    print_function_info(&info);
    Ok(())
}

fn print_function_info(info: &rgctl_analysis::CpgFunctionInfo) {
    println!(
        "{} ({}){}",
        info.name,
        info.id,
        if info.is_constructor {
            " [constructor]"
        } else {
            ""
        }
    );
    if let Some(qn) = &info.qualified_name {
        println!("  qualified: {qn}");
    }
    if let Some(file) = &info.file_path {
        println!("  file: {}:{}", file, info.start_line.unwrap_or(0));
    }
    println!(
        "  L_proc: {}",
        if info.has_l_proc {
            "yes (CFG/PDG archived)"
        } else {
            "no — run discover --with-cfg"
        }
    );
}

fn run_calls(
    ctx: &CliContext,
    symbol: &str,
    file: Option<&str>,
    class: Option<&str>,
    line: Option<usize>,
) -> Result<()> {
    let graph = ctx.load_graph()?;
    let backend = graph.backend();
    let resolved = if file.is_some() || class.is_some() || line.is_some() {
        resolve_cpg_symbol_id(ctx, backend, symbol, file, class, line)?
    } else {
        match cpg_calls(backend, symbol) {
            Ok(info) => {
                if ctx.format == OutputFormat::Json {
                    return ctx.emit_json_value(&serde_json::to_value(&info)?);
                }
                print_calls_info(&info);
                return Ok(());
            }
            Err(rgctl_error::Error::AmbiguousSymbol { name, count, .. }) => {
                let matches = find_function_nodes(backend, &name)?;
                let rows: Vec<SymbolCandidate> = matches.iter().map(node_candidate).collect();
                let recovery =
                    format!("rgctl cpg calls {name} --file <PATH> --class <CLASS> --line <N>");
                if ctx.format == OutputFormat::Json {
                    let _ = ctx.emit_json_error(
                        "ambiguous_symbol",
                        "AMBIGUOUS_SYMBOL",
                        &format!("Ambiguous symbol '{name}': {count} matches"),
                        Some(&rows),
                        None,
                        Some(&recovery),
                    );
                }
                anyhow::bail!(
                    "Symbol '{name}' is ambiguous ({count} matches). Disambiguate with --file / --class / --line."
                );
            }
            Err(rgctl_error::Error::NotFound(msg)) => {
                let suggestions = suggest_function_names(backend, symbol, 5);
                if ctx.format == OutputFormat::Json {
                    let _ = ctx.emit_json_error(
                        "not_found",
                        "NOT_FOUND",
                        &msg,
                        None,
                        Some(&suggestions),
                        Some("rgctl find <PATTERN> --type function"),
                    );
                }
                anyhow::bail!("{msg}");
            }
            Err(e) => return Err(e.into()),
        }
    };
    let info = cpg_calls(backend, &resolved)?;
    if ctx.format == OutputFormat::Json {
        return ctx.emit_json_value(&serde_json::to_value(&info)?);
    }
    print_calls_info(&info);
    Ok(())
}

fn print_calls_info(info: &rgctl_analysis::CpgCallsInfo) {
    println!(
        "CALL neighborhood for {} ({}):",
        info.function_name, info.function_id
    );
    if info.edges.is_empty() {
        println!("  (no CALL edges)");
        return;
    }
    for e in &info.edges {
        println!("  [{}] {} ({})", e.direction, e.name, e.id);
    }
}
