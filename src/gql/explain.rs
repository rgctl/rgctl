//! Explain plan for GQL queries (Phase 12.4 / 12.5).

use serde::{Deserialize, Serialize};

/// One step in a query execution plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExplainStep {
    /// Step name (Match, Filter, Project, Limit, ...)
    pub operation: String,
    /// Human-readable detail
    pub detail: String,
    /// Rows entering this step
    pub rows_in: usize,
    /// Rows leaving this step
    pub rows_out: usize,
}

/// Full explain plan for a GQL query.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExplainPlan {
    /// Ordered execution steps
    pub steps: Vec<ExplainStep>,
}

impl ExplainPlan {
    /// Create an empty plan.
    pub fn new() -> Self {
        Self { steps: Vec::new() }
    }

    /// Append a step to the plan.
    pub fn push(&mut self, step: ExplainStep) {
        self.steps.push(step);
    }

    /// Total number of steps.
    pub fn len(&self) -> usize {
        self.steps.len()
    }

    /// Whether the plan has no steps.
    pub fn is_empty(&self) -> bool {
        self.steps.is_empty()
    }
}

impl Default for ExplainPlan {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_explain_plan_push() {
        let mut plan = ExplainPlan::new();
        plan.push(ExplainStep {
            operation: "Match".into(),
            detail: "MATCH (f:Function)".into(),
            rows_in: 0,
            rows_out: 3,
        });
        assert_eq!(plan.len(), 1);
    }
}
