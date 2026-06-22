//! Complexity metrics aggregation and classification
//!
//! Task 2.1.2: Cyclomatic/cognitive complexity analysis from graph properties.

use rbuilder_error::Result;
use rbuilder_graph::backend::MemoryBackend;
use rbuilder_graph::schema::{Node, NodeType};
use std::collections::HashMap;

/// Complexity level classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ComplexityLevel {
    /// Cyclomatic <= 5
    Low,
    /// Cyclomatic 6-10
    Medium,
    /// Cyclomatic 11-20
    High,
    /// Cyclomatic > 20
    Critical,
}

/// Per-function complexity record.
#[derive(Debug, Clone)]
pub struct FunctionComplexity {
    /// Function node
    pub node: Node,
    /// Cyclomatic complexity
    pub cyclomatic: usize,
    /// Cognitive complexity
    pub cognitive: usize,
    /// Lines of code
    pub loc: usize,
    /// Classification
    pub level: ComplexityLevel,
}

/// Aggregate complexity report.
#[derive(Debug, Clone, Default)]
pub struct ComplexityReport {
    /// Per-function complexity
    pub functions: Vec<FunctionComplexity>,
    /// Count by level
    pub by_level: HashMap<ComplexityLevel, usize>,
    /// Average cyclomatic complexity
    pub avg_cyclomatic: f64,
    /// Maximum cyclomatic complexity
    pub max_cyclomatic: usize,
}

/// Classify cyclomatic complexity into a level.
pub fn classify_complexity(cyclomatic: usize) -> ComplexityLevel {
    match cyclomatic {
        0..=5 => ComplexityLevel::Low,
        6..=10 => ComplexityLevel::Medium,
        11..=20 => ComplexityLevel::High,
        _ => ComplexityLevel::Critical,
    }
}

/// Analyze complexity from graph node properties.
pub struct ComplexityAnalyzer;

impl ComplexityAnalyzer {
    /// Generate a complexity report from the graph.
    pub fn analyze(backend: &MemoryBackend) -> Result<ComplexityReport> {
        let functions = backend.find_nodes_by_type(NodeType::Function)?;
        let mut report = ComplexityReport::default();
        let mut total_cyclomatic = 0usize;

        for node in functions {
            let cyclomatic = node
                .get_property("cyclomatic")
                .and_then(|v| v.parse().ok())
                .unwrap_or(1);
            let cognitive = node
                .get_property("cognitive")
                .and_then(|v| v.parse().ok())
                .unwrap_or(cyclomatic);
            let loc = node
                .get_property("loc")
                .and_then(|v| v.parse().ok())
                .unwrap_or(0);
            let level = classify_complexity(cyclomatic);

            *report.by_level.entry(level).or_default() += 1;
            total_cyclomatic += cyclomatic;
            report.max_cyclomatic = report.max_cyclomatic.max(cyclomatic);

            report.functions.push(FunctionComplexity {
                node,
                cyclomatic,
                cognitive,
                loc,
                level,
            });
        }

        report
            .functions
            .sort_by_key(|b| std::cmp::Reverse(b.cyclomatic));
        if !report.functions.is_empty() {
            report.avg_cyclomatic = total_cyclomatic as f64 / report.functions.len() as f64;
        }
        Ok(report)
    }

    /// Find functions above a complexity threshold.
    pub fn find_above_threshold(backend: &MemoryBackend, threshold: usize) -> Result<Vec<Node>> {
        let report = Self::analyze(backend)?;
        Ok(report
            .functions
            .into_iter()
            .filter(|f| f.cyclomatic > threshold)
            .map(|f| f.node)
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rbuilder_graph::backend::GraphBackend;

    #[test]
    fn test_complexity_classification() {
        assert_eq!(classify_complexity(3), ComplexityLevel::Low);
        assert_eq!(classify_complexity(8), ComplexityLevel::Medium);
        assert_eq!(classify_complexity(15), ComplexityLevel::High);
        assert_eq!(classify_complexity(25), ComplexityLevel::Critical);
    }

    #[test]
    fn test_complexity_report() {
        let mut backend = MemoryBackend::new();
        let mut node = Node::new(NodeType::Function, "complex".to_string());
        node.properties
            .insert("cyclomatic".to_string(), "15".to_string());
        node.properties
            .insert("cognitive".to_string(), "20".to_string());
        node.properties.insert("loc".to_string(), "100".to_string());
        backend.insert_node(node).unwrap();

        let report = ComplexityAnalyzer::analyze(&backend).unwrap();
        assert_eq!(report.functions.len(), 1);
        assert_eq!(report.functions[0].level, ComplexityLevel::High);
    }
}
