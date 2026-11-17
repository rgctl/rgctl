//! Dispatch [`Command`] to JSON values.

use crate::command::{Command, CommandRegistry, CpgOp};
use crate::error::Result;
use crate::session::Session;
use crate::status::{self, PipelineStatus};
use serde_json::Value;

/// Execute a command against `session`. Unreadiness returns pipeline status JSON (`Ok`).
pub fn execute(session: &mut Session, command: Command) -> Result<Value> {
    execute_with_registry(session, command, &CommandRegistry::default())
}

/// Execute with a custom registry (disabled CPG ops fail as unknown).
pub fn execute_with_registry(
    session: &mut Session,
    command: Command,
    registry: &CommandRegistry,
) -> Result<Value> {
    match command {
        Command::Status => Ok(status_json(session)),
        Command::Query(args) => {
            crate::query::validate_query_args(&args)?;
            if let Some(status) = unreadiness_graph(session) {
                return Ok(status);
            }
            let repo = session.repo().to_path_buf();
            let graph = session.load_graph()?;
            crate::query::run_query(graph, &repo, &args)
        }
        Command::Search(args) => {
            if args.text.trim().is_empty() {
                return Err(crate::error::ServiceError::InvalidParams(
                    "`text` must not be empty".into(),
                ));
            }
            if !crate::search::semantic_ready(session.repo()) {
                return Ok(status_json(session));
            }
            if let Some(status) = unreadiness_graph(session) {
                return Ok(status);
            }
            let repo = session.repo().to_path_buf();
            let graph = session.load_graph()?;
            crate::search::run_search(graph, &repo, &args)
        }
        Command::Impact(args) => {
            if args.symbol.trim().is_empty() {
                return Err(crate::error::ServiceError::InvalidParams(
                    "`symbol` is required".into(),
                ));
            }
            if let Some(status) = unreadiness_graph(session) {
                return Ok(status);
            }
            let repo = session.repo().to_path_buf();
            let graph = session.load_graph()?;
            crate::impact::run_impact(graph, &repo, &args)
        }
        Command::Metrics(args) => {
            if !args.pagerank && !args.betweenness && !args.communities {
                return Err(crate::error::ServiceError::InvalidParams(
                    "at least one of pagerank, betweenness, communities is required".into(),
                ));
            }
            if let Some(status) = unreadiness_graph(session) {
                return Ok(status);
            }
            let snap_path = session.snapshot_path();
            if snap_path.is_file() {
                let store = rgctl_graph::SnapshotNodeStore::open(&snap_path)
                    .map_err(crate::error::ServiceError::from)?;
                return crate::metrics::run_metrics_from_store(&store, &args);
            }
            let graph = session.load_graph()?;
            crate::metrics::run_metrics(graph, &args)
        }
        Command::Cpg(args) => {
            if !registry.cpg_enabled(args.op) {
                return Err(crate::error::ServiceError::unknown_op(
                    args.op.as_str(),
                    &registry.cpg_ops,
                ));
            }
            if args.op != CpgOp::Status {
                if needs_cfg(args.op) && !crate::cpg::cfg_ready(session.repo()) {
                    return Ok(status_json(session));
                }
                if needs_graph(args.op) {
                    if let Some(status) = unreadiness_graph(session) {
                        return Ok(status);
                    }
                }
            }
            let repo = session.repo().to_path_buf();
            let graph = if needs_graph(args.op) && args.op != CpgOp::Status {
                Some(session.load_graph()?)
            } else {
                None
            };
            crate::cpg::run_cpg(graph, &repo, &args)
        }
        Command::Check(args) => {
            if let Some(status) = unreadiness_graph(session) {
                return Ok(status);
            }
            let repo = session.repo().to_path_buf();
            let graph = session.load_graph()?;
            crate::check::run_check(graph, &repo, &args)
        }
    }
}

fn needs_cfg(op: CpgOp) -> bool {
    matches!(
        op,
        CpgOp::Mutations | CpgOp::Flows | CpgOp::Slice | CpgOp::Inspect | CpgOp::Pdg | CpgOp::Ast
    )
}

fn needs_graph(op: CpgOp) -> bool {
    matches!(
        op,
        CpgOp::Function | CpgOp::Calls | CpgOp::Inspect | CpgOp::Pdg
    )
}

fn unreadiness_graph(session: &Session) -> Option<Value> {
    if session.graph_ready() {
        None
    } else {
        Some(status_json(session))
    }
}

fn status_json(session: &Session) -> Value {
    let mut status: PipelineStatus = status::read_status(session.repo());
    status::refresh_ready_flags(&mut status, session.repo());
    if !status.dashboard_ready {
        status.message = Some("Dashboard is being prepared".into());
    }
    serde_json::to_value(&status).unwrap_or_else(|_| {
        serde_json::json!({
            "schema_version": 1,
            "command": "pipeline_status",
            "message": "status unavailable",
        })
    })
}

/// Pipeline status document for this session (always succeeds).
#[must_use]
pub fn pipeline_status_value(session: &Session) -> Value {
    status_json(session)
}
