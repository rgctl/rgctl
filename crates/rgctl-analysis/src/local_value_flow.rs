//! Local value transfers over persisted native operands and reaching definitions.
//! No source parsing, statement-text matching, or interprocedural target guessing.
use crate::cfg::ControlFlowGraph;
use crate::dataflow::compute_reaching_definitions;
use crate::operands::*;
use crate::pdg::{ProgramDependenceGraph, deterministic_pdg_node_id};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

#[cfg(test)]
#[path = "local_value_flow_tests.rs"]
mod tests;

/// Meaning of a local value-transfer edge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum LocalTransferKind {
    /// A reaching formal/local RHS supplies this exact read occurrence.
    LocalRead,
    /// Parentheses preserve their inner value.
    Parenthesized,
    /// Either selected branch may supply the conditional result.
    ConditionalAlternative,
    /// Reference cast/assertion preserves its operand on the successful path.
    AssertedValue,
}

/// Function-local expression transfer; qualify IDs with the enclosing callable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct LocalValueTransfer {
    /// Reaching definition's value or inner expression.
    pub from: ExpressionId,
    /// Reading or parenthesized expression.
    pub to: ExpressionId,
    /// Transfer semantics.
    pub kind: LocalTransferKind,
}

/// Why an empty or partial local result must not certify absence of flow.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum LocalFlowDiagnostic {
    /// Lowering was unavailable or diagnosed unsupported subforms.
    PartialOperands,
    /// Invalid expression, slot or CFG position in supplied records.
    InvalidReference,
    /// Supplied PDG does not match the canonical CFG statements.
    IncompatiblePdg,
    /// A read has no known reaching local definition (including external names).
    UnboundRead(ExpressionId),
    /// A reaching definition has no modeled initializer/RHS value.
    UnmodeledWrite(ExpressionId),
    /// One expression was lowered into multiple CFG positions; retain alternatives.
    AmbiguousOccurrence(ExpressionId),
    /// Multiple local writes in one statement need finer evaluation-order facts.
    IntraStatementWrites(ExpressionId),
    /// A query budget was exhausted; captured edges are only a prefix.
    LimitReached,
}

/// Bounds on one function-local query, separate from a larger solver's worklist.
#[derive(Debug, Clone, Copy)]
pub struct LocalFlowLimits {
    /// Maximum CFG statements before building reaching definitions.
    pub max_statements: usize,
    /// Maximum native local definitions before dataflow construction.
    pub max_definitions: usize,
    /// Maximum emitted distinct transfers.
    pub max_transfers: usize,
    /// Maximum inspected candidate definitions across reads.
    pub max_candidate_checks: usize,
}
impl Default for LocalFlowLimits {
    fn default() -> Self {
        Self {
            max_statements: 50_000,
            max_definitions: 50_000,
            max_transfers: 100_000,
            max_candidate_checks: 1_000_000,
        }
    }
}

/// Derived, query-local value transfers. Empty edges are not a coverage verdict.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct LocalValueFlow {
    /// Directed transfers between native expression IDs.
    pub transfers: Vec<LocalValueTransfer>,
    /// Explicit partial/unknown/budget evidence.
    pub diagnostics: Vec<LocalFlowDiagnostic>,
}

impl LocalValueFlow {
    /// Reuse the selected CFG/PDG and existing reaching-definition analysis.
    pub fn build(
        cfg: &ControlFlowGraph,
        pdg: &ProgramDependenceGraph,
        limits: LocalFlowLimits,
    ) -> Self {
        let mut result = Self::default();
        let mut diagnostics = HashSet::new();
        let mut emit_diagnostic = |kind| {
            if diagnostics.insert(kind) {
                result.diagnostics.push(kind);
            }
        };
        if cfg.operands.status != OperandStatus::Captured || !cfg.operands.diagnostics.is_empty() {
            emit_diagnostic(LocalFlowDiagnostic::PartialOperands);
        }
        if !cfg.operands.valid_references() || !cfg.valid_local_operand_positions() {
            emit_diagnostic(LocalFlowDiagnostic::InvalidReference);
            return result;
        }
        let mut statements = 0usize;
        let mut definitions = 0usize;
        for block in cfg.blocks.values() {
            statements = statements.saturating_add(block.statements.len());
            definitions = definitions.saturating_add(
                block
                    .statements
                    .iter()
                    .map(|s| s.defined_vars.len())
                    .sum::<usize>(),
            );
            if statements > limits.max_statements || definitions > limits.max_definitions {
                emit_diagnostic(LocalFlowDiagnostic::LimitReached);
                return result;
            }
            for (index, statement) in block.statements.iter().enumerate() {
                let id =
                    deterministic_pdg_node_id(block.id, index, statement.line, &statement.text);
                if !pdg
                    .nodes
                    .get(&id)
                    .is_some_and(|n| n.block == block.id && n.statement == *statement)
                {
                    emit_diagnostic(LocalFlowDiagnostic::IncompatiblePdg);
                    return result;
                }
            }
        }
        let reaching = compute_reaching_definitions(cfg, pdg);
        let mut writes: HashMap<_, Vec<_>> = HashMap::new();
        let mut writes_per_statement: HashMap<_, usize> = HashMap::new();
        for write in &cfg.operands.local_writes {
            writes
                .entry((write.statement, write.slot))
                .or_default()
                .push(write.value);
            *writes_per_statement.entry(write.statement).or_default() += 1;
        }
        let mut occurrences: HashMap<_, HashSet<_>> = HashMap::new();
        for usage in &cfg.operands.local_uses {
            occurrences
                .entry(usage.value)
                .or_default()
                .insert(usage.statement);
        }
        let mut seen = HashSet::new();
        let mut checks = 0usize;
        for usage in &cfg.operands.local_uses {
            if occurrences[&usage.value].len() > 1 {
                emit_diagnostic(LocalFlowDiagnostic::AmbiguousOccurrence(usage.value));
            }
            let key = cfg
                .operands
                .operand_name(cfg.operands.local_slots[usage.slot.0 as usize]);
            let Some(var) = key.and_then(|key| reaching.catalog.var_id_of(key)) else {
                emit_diagnostic(LocalFlowDiagnostic::UnboundRead(usage.value));
                continue;
            };
            let mut local_position = None;
            let mut candidates = Vec::new();
            let mut incoming_candidates = Vec::new();
            let mut same_statement = Vec::new();
            let multi_write = writes_per_statement
                .get(&usage.statement)
                .copied()
                .unwrap_or(0)
                > 1;
            if multi_write {
                emit_diagnostic(LocalFlowDiagnostic::IntraStatementWrites(usage.value));
            }
            for &index in reaching.catalog.defs_for_var_id(var) {
                checks = checks.saturating_add(1);
                if checks > limits.max_candidate_checks {
                    emit_diagnostic(LocalFlowDiagnostic::LimitReached);
                    return result;
                }
                let def = &reaching.catalog.defs[index];
                if reaching
                    .in_set
                    .get(&usage.statement.block)
                    .is_some_and(|incoming| incoming.contains(index))
                {
                    incoming_candidates.push(index);
                }
                if def.block == usage.statement.block {
                    if def.statement_index < usage.statement.index as usize {
                        if local_position.is_none_or(|position| def.statement_index > position) {
                            local_position = Some(def.statement_index);
                            candidates.clear();
                        }
                        if local_position == Some(def.statement_index) {
                            candidates.push(index);
                        }
                    } else if multi_write && def.statement_index == usage.statement.index as usize {
                        same_statement.push(index);
                    }
                }
            }
            if local_position.is_none() {
                candidates.extend(incoming_candidates);
            }
            candidates.extend(same_statement);
            candidates.sort_by_key(|&index| {
                let def = &reaching.catalog.defs[index];
                (def.block.0, def.statement_index, def.pdg_node)
            });
            if candidates.is_empty() {
                emit_diagnostic(LocalFlowDiagnostic::UnboundRead(usage.value));
            }
            for index in candidates {
                let def = &reaching.catalog.defs[index];
                let position = StatementRef {
                    block: def.block,
                    index: def.statement_index as u32,
                };
                let Some(values) = writes.get(&(position, usage.slot)) else {
                    emit_diagnostic(LocalFlowDiagnostic::UnmodeledWrite(usage.value));
                    continue;
                };
                for &value in values {
                    let Some(from) = value else {
                        emit_diagnostic(LocalFlowDiagnostic::UnmodeledWrite(usage.value));
                        continue;
                    };
                    let edge = LocalValueTransfer {
                        from,
                        to: usage.value,
                        kind: LocalTransferKind::LocalRead,
                    };
                    if seen.insert(edge) {
                        if result.transfers.len() >= limits.max_transfers {
                            emit_diagnostic(LocalFlowDiagnostic::LimitReached);
                            return result;
                        }
                        result.transfers.push(edge);
                    }
                }
            }
        }
        for (index, expression) in cfg.operands.expressions.iter().enumerate() {
            let kind = match expression.kind {
                ExpressionKind::Parenthesized => LocalTransferKind::Parenthesized,
                ExpressionKind::Conditional { .. } => LocalTransferKind::ConditionalAlternative,
                ExpressionKind::AssertedValue => LocalTransferKind::AssertedValue,
                ExpressionKind::Projection { position: 0 }
                    if expression
                        .inputs
                        .slice(&cfg.operands.inputs)
                        .and_then(|s| s.first())
                        .is_some_and(|id| {
                            cfg.operands.expressions[id.0 as usize].kind
                                == ExpressionKind::AssertedValue
                        }) =>
                {
                    LocalTransferKind::AssertedValue
                }
                _ => continue,
            };
            for &from in expression.inputs.slice(&cfg.operands.inputs).unwrap_or(&[]) {
                let edge = LocalValueTransfer {
                    from,
                    to: ExpressionId(index as u32),
                    kind,
                };
                if seen.insert(edge) {
                    if result.transfers.len() >= limits.max_transfers {
                        emit_diagnostic(LocalFlowDiagnostic::LimitReached);
                        return result;
                    }
                    result.transfers.push(edge);
                }
            }
        }
        result
            .transfers
            .sort_by_key(|edge| (edge.to.0, edge.from.0, edge.kind as u8));
        result
    }
}
