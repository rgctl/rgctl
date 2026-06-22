//! Staged / changed-file risk analysis for git hooks.

use crate::file_tracker::normalize_path_str;
use rbuilder_analysis::BlastRadiusAnalyzer;
use rbuilder_error::Result;
use rbuilder_graph::code_graph::CodeGraph;
use rbuilder_graph::schema::NodeType;
use rbuilder_project_config::project::RiskLevel;
use serde::Serialize;

/// Per-symbol blast-radius detail for a changed file.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ChangeDetail {
    /// Changed file (repo-relative).
    pub file: String,
    /// Symbol at risk.
    pub symbol: String,
    /// Blast radius score (0–100).
    pub blast_radius_score: f64,
    /// Number of direct callers.
    pub direct_callers: usize,
    /// Transitive impact zone size.
    pub impact_zone_size: usize,
}

/// Aggregate summary for change detection.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ChangeSummary {
    /// Files analyzed.
    pub files_analyzed: usize,
    /// Symbols evaluated.
    pub symbols_analyzed: usize,
    /// Highest blast-radius score observed.
    pub max_score: f64,
    /// Overall risk level.
    pub risk_level: RiskLevel,
}

/// Full change-detection result (JSON for hooks / MCP).
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ChangeDetectionResult {
    /// Input files.
    pub files: Vec<String>,
    /// Overall risk level.
    pub risk_level: RiskLevel,
    /// Highest score across symbols.
    pub max_score: f64,
    /// Per-symbol details.
    pub details: Vec<ChangeDetail>,
    /// Summary block.
    pub summary: ChangeSummary,
}

/// Detects blast-radius risk for a set of changed files.
pub struct ChangeDetector {
    blast_radius_threshold: usize,
}

impl ChangeDetector {
    /// Create a detector with default thresholds.
    pub fn new() -> Self {
        Self {
            blast_radius_threshold: 50,
        }
    }

    /// Override blast-radius impact zone threshold from config.
    pub fn with_blast_radius_threshold(mut self, threshold: usize) -> Self {
        self.blast_radius_threshold = threshold;
        self
    }

    /// Analyze changed files against the indexed graph.
    pub fn detect(&self, graph: &CodeGraph, files: &[String]) -> Result<ChangeDetectionResult> {
        let backend = graph.backend();
        let analyzer = BlastRadiusAnalyzer::new(backend);
        let mut details = Vec::new();
        let mut max_score = 0.0f64;

        let normalized: Vec<String> = files.iter().map(|f| normalize_path_str(f)).collect();

        let nodes = backend.all_nodes()?;
        for file in &normalized {
            for node in &nodes {
                if node.node_type != NodeType::Function {
                    continue;
                }
                let node_file = node
                    .file_path
                    .as_deref()
                    .map(normalize_path_str)
                    .unwrap_or_default();
                if node_file != *file {
                    continue;
                }
                if let Ok(report) = analyzer.analyze(&node.name) {
                    max_score = max_score.max(report.score);
                    details.push(ChangeDetail {
                        file: file.clone(),
                        symbol: node.name.clone(),
                        blast_radius_score: report.score,
                        direct_callers: report.direct_callers.len(),
                        impact_zone_size: report.impact_zone.len(),
                    });
                }
            }
        }

        details.sort_by(|a, b| {
            b.blast_radius_score
                .partial_cmp(&a.blast_radius_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        let risk_level = classify_risk(max_score, &details, self.blast_radius_threshold);
        let summary = ChangeSummary {
            files_analyzed: normalized.len(),
            symbols_analyzed: details.len(),
            max_score,
            risk_level,
        };

        Ok(ChangeDetectionResult {
            files: normalized,
            risk_level,
            max_score,
            details,
            summary,
        })
    }
}

impl Default for ChangeDetector {
    fn default() -> Self {
        Self::new()
    }
}

fn classify_risk(max_score: f64, details: &[ChangeDetail], blast_threshold: usize) -> RiskLevel {
    let large_impact = details
        .iter()
        .any(|d| d.impact_zone_size >= blast_threshold);
    if max_score >= 80.0 || large_impact {
        RiskLevel::Critical
    } else if max_score >= 60.0 {
        RiskLevel::High
    } else if max_score >= 40.0 {
        RiskLevel::Medium
    } else {
        RiskLevel::Low
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rbuilder_graph::backend::GraphBackend;
    use rbuilder_graph::schema::{Edge, EdgeType, Node};

    fn chain_graph() -> CodeGraph {
        let mut graph = CodeGraph::new();
        let backend = graph.backend_mut();
        let a = Node::new(NodeType::Function, "a".into()).with_file_path("src/lib.rs".into());
        let b = Node::new(NodeType::Function, "b".into()).with_file_path("src/lib.rs".into());
        let c = Node::new(NodeType::Function, "c".into()).with_file_path("src/lib.rs".into());
        let id_a = a.id;
        let id_b = b.id;
        let id_c = c.id;
        backend.insert_node(a).unwrap();
        backend.insert_node(b).unwrap();
        backend.insert_node(c).unwrap();
        backend
            .insert_edge(Edge::new(id_a, id_b, EdgeType::Calls))
            .unwrap();
        backend
            .insert_edge(Edge::new(id_b, id_c, EdgeType::Calls))
            .unwrap();
        graph
    }

    #[test]
    fn test_detect_changes_chain() {
        let graph = chain_graph();
        let detector = ChangeDetector::new();
        let result = detector
            .detect(&graph, &["src/lib.rs".to_string()])
            .unwrap();
        assert!(!result.details.is_empty());
        assert!(result.max_score > 0.0);
        assert!(result.details.iter().any(|d| d.symbol == "c"));
    }

    #[test]
    fn test_risk_classification_critical() {
        assert_eq!(classify_risk(85.0, &[], 50), RiskLevel::Critical);
        assert_eq!(
            classify_risk(
                10.0,
                &[ChangeDetail {
                    file: "f.rs".into(),
                    symbol: "x".into(),
                    blast_radius_score: 10.0,
                    direct_callers: 0,
                    impact_zone_size: 55,
                }],
                50
            ),
            RiskLevel::Critical
        );
    }

    #[test]
    fn test_risk_classification_medium_and_high() {
        assert_eq!(classify_risk(65.0, &[], 50), RiskLevel::High);
        assert_eq!(classify_risk(45.0, &[], 50), RiskLevel::Medium);
        assert_eq!(classify_risk(10.0, &[], 50), RiskLevel::Low);
    }

    #[test]
    fn test_detect_changes_json_roundtrip() {
        let graph = chain_graph();
        let result = ChangeDetector::new()
            .detect(&graph, &["src/lib.rs".to_string()])
            .unwrap();
        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("risk_level"));
        assert!(json.contains("max_score"));
    }
}
