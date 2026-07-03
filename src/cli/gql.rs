//! `rbuilder gql` — graph query language execution.

use super::args::OutputFormat;
use super::context::CliContext;
use super::gql_output::gql_result_to_json;
use anyhow::Result;

pub struct GqlArgs {
    pub query: String,
    pub explain: bool,
    pub macro_name: Option<String>,
}

pub fn run(ctx: &CliContext, args: GqlArgs) -> Result<()> {
    use crate::gql::{execute, execute_explain, execute_macro, QueryMacroRegistry};

    let graph = ctx.load_graph()?;
    let backend = graph.backend();
    let registry = QueryMacroRegistry::with_defaults();

    let result = if let Some(name) = args.macro_name {
        execute_macro(backend, &registry, &name)?
    } else if args.explain {
        execute_explain(backend, &args.query)?
    } else {
        execute(backend, &args.query)?
    };

    if ctx.format == OutputFormat::Json {
        return ctx.emit_json_value(&gql_result_to_json(&result, args.explain));
    }

    if args.explain {
        if let Some(plan) = result.plan {
            for step in &plan.steps {
                println!("{}: {}", step.operation, step.detail);
            }
            println!();
        }
    }

    for row in &result.rows {
        let names: Vec<_> = row.values().map(|binding| binding.name.clone()).collect();
        println!("{}", names.join(" -> "));
    }
    Ok(())
}
