//! Link native expression occurrences to scoped CFG positions while lowering.
use super::*;
use crate::operands::*;

#[derive(Default)]
pub(super) struct OperandAnchors {
    names: HashMap<String, OperandNameId>,
    values: HashMap<(u32, u32), ExpressionId>,
    reads: Vec<(OperandSpan, ExpressionId, OperandNameId)>,
    slots: HashMap<String, LocalSlotId>,
}

fn checked_index(value: usize) -> Result<u32> {
    u32::try_from(value).map_err(|_| Error::ParseError {
        file: "source".into(),
        line: 0,
        message: "local operand index exceeds u32".into(),
    })
}

impl OperandAnchors {
    pub(super) fn new(facts: &FunctionOperands) -> Self {
        let mut result = Self::default();
        for (i, name) in facts.names.iter().enumerate() {
            result.names.insert(name.clone(), OperandNameId(i as u32));
        }
        let selectors: HashSet<_> = if facts.language == OperandLanguage::Java {
            facts.calls.iter().map(|call| call.callee).collect()
        } else {
            HashSet::new()
        };
        for (i, expression) in facts.expressions.iter().enumerate() {
            let id = ExpressionId(i as u32);
            if let Some(span) = expression.span {
                result.values.insert((span.start, span.end), id);
                if let ExpressionKind::Name(name) = expression.kind
                    && !selectors.contains(&id)
                {
                    result.reads.push((span, id, name));
                }
            }
        }
        result
            .reads
            .sort_by_key(|(span, id, _)| (span.start, span.end, id.0));
        result
    }
}

impl CfgBuilder<'_> {
    fn operand_statement(&self) -> Result<StatementRef> {
        Ok(StatementRef {
            block: self.current_block,
            index: checked_index(self.cfg.blocks[&self.current_block].statements.len())?,
        })
    }

    fn operand_slot(&mut self, scoped: &str) -> Result<LocalSlotId> {
        if let Some(&slot) = self.operand_anchors.slots.get(scoped) {
            return Ok(slot);
        }
        let name = if let Some(&name) = self.operand_anchors.names.get(scoped) {
            name
        } else {
            let id = OperandNameId(checked_index(self.cfg.operands.names.len())?);
            self.cfg.operands.names.push(scoped.into());
            self.operand_anchors.names.insert(scoped.into(), id);
            id
        };
        let slot = LocalSlotId(checked_index(self.cfg.operands.local_slots.len())?);
        self.cfg.operands.local_slots.push(name);
        self.operand_anchors.slots.insert(scoped.into(), slot);
        Ok(slot)
    }

    pub(super) fn capture_formal_write(&mut self, scoped: &str, value: ExpressionId) -> Result<()> {
        let slot = self.operand_slot(scoped)?;
        let statement = self.operand_statement()?;
        self.cfg.operands.local_writes.push(LocalWriteFact {
            slot,
            value: Some(value),
            statement,
        });
        Ok(())
    }

    pub(super) fn capture_local_reads(&mut self, node: Node<'_>, used: &[String]) -> Result<()> {
        if self.cfg.operands.status != OperandStatus::Captured {
            return Ok(());
        }
        let start = checked_index(node.start_byte())?;
        let end = checked_index(node.end_byte())?;
        let first = self
            .operand_anchors
            .reads
            .partition_point(|(span, _, _)| span.start < start);
        let last = self
            .operand_anchors
            .reads
            .partition_point(|(span, _, _)| span.start < end);
        let selected: Vec<_> = self.operand_anchors.reads[first..last]
            .iter()
            .filter(|(span, _, name)| {
                span.end <= end
                    && self
                        .cfg
                        .operands
                        .name(*name)
                        .is_some_and(|n| used.iter().any(|v| v == n))
            })
            .map(|(_, value, name)| {
                (
                    *value,
                    self.resolve_local(self.cfg.operands.name(*name).unwrap_or("")),
                )
            })
            .collect();
        let statement = self.operand_statement()?;
        for (value, scoped) in selected {
            let slot = self.operand_slot(&scoped)?;
            self.cfg.operands.local_uses.push(LocalUseFact {
                value,
                slot,
                statement,
            });
        }
        Ok(())
    }

    pub(super) fn capture_local_writes(
        &mut self,
        node: Node<'_>,
        source: &[u8],
        defined: &[DefVar],
    ) -> Result<()> {
        if self.cfg.operands.status != OperandStatus::Captured {
            return Ok(());
        }
        let candidates = crate::operands::local_write_sources(node);
        let statement = self.operand_statement()?;
        for definition in defined {
            let DefVar::Local(scoped) = definition else {
                continue;
            };
            let mut matches = candidates.iter().filter(|(target, _)| {
                target.kind() == "identifier"
                    && target
                        .utf8_text(source)
                        .ok()
                        .is_some_and(|raw| self.resolve_local(raw) == *scoped)
            });
            let candidate = matches.next();
            let value = if matches.next().is_none() {
                candidate.and_then(|(_, rhs)| *rhs).and_then(|rhs| {
                    let key = (
                        u32::try_from(rhs.start_byte()).ok()?,
                        u32::try_from(rhs.end_byte()).ok()?,
                    );
                    self.operand_anchors.values.get(&key).copied()
                })
            } else {
                None
            };
            let slot = self.operand_slot(scoped)?;
            self.cfg.operands.local_writes.push(LocalWriteFact {
                slot,
                value,
                statement,
            });
            if value.is_none() {
                self.cfg.operands.diagnostics.push(OperandDiagnostic {
                    span: OperandSpan {
                        start: checked_index(node.start_byte())?,
                        end: checked_index(node.end_byte())?,
                    },
                    kind: OperandDiagnosticKind::LocalWriteShape,
                });
            }
        }
        Ok(())
    }
}
