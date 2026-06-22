//! Domain pattern detection from the code graph
//!
//! Task 4.2.1: Auto-detect project-specific naming and label patterns.

use rbuilder_graph::backend::MemoryBackend;
use rbuilder_graph::schema::Node;
use regex::Regex;
use std::collections::HashMap;

/// A detected label usage pattern.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LabelPattern {
    /// Label value
    pub label: String,
    /// Number of nodes with this label
    pub count: usize,
    /// Human-readable description
    pub description: String,
}

/// A detected naming suffix/prefix pattern.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NamingPattern {
    /// Suffix (e.g. "Service")
    pub suffix: String,
    /// Number of matching nodes
    pub count: usize,
    /// Example node names
    pub examples: Vec<String>,
}

/// A detected architecture pattern (layers, modules, etc.).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArchitecturePattern {
    /// Pattern type (e.g. "layered", "module", "mvc")
    pub pattern_type: String,
    /// Pattern name or identifier
    pub name: String,
    /// Number of nodes participating in this pattern
    pub node_count: usize,
    /// Human-readable description
    pub description: String,
}

/// Domain vocabulary learned from the graph.
#[derive(Debug, Clone, Default)]
pub struct DomainContext {
    /// Label patterns sorted by frequency
    pub label_patterns: Vec<LabelPattern>,
    /// Naming suffix patterns
    pub naming_patterns: Vec<NamingPattern>,
    /// Architecture patterns (layers, modules, etc.)
    pub architecture_patterns: Vec<ArchitecturePattern>,
    /// Maps natural language terms to graph labels
    pub term_to_label: HashMap<String, String>,
    /// Maps natural language terms to node types
    pub term_to_node_type: HashMap<String, String>,
}

/// Detects domain-specific patterns in a code graph.
pub struct PatternDetector {
    min_label_frequency: usize,
    min_naming_frequency: usize,
}

impl Default for PatternDetector {
    fn default() -> Self {
        Self {
            min_label_frequency: 2,
            min_naming_frequency: 3,
        }
    }
}

impl PatternDetector {
    /// Create a new pattern detector.
    pub fn new() -> Self {
        Self::default()
    }

    /// Detect all domain patterns and build context.
    pub fn analyze(&self, backend: &MemoryBackend) -> rbuilder_error::Result<DomainContext> {
        let nodes = backend.all_nodes()?;
        Ok(DomainContext {
            label_patterns: self.detect_label_patterns(&nodes),
            naming_patterns: self.detect_naming_patterns(&nodes),
            architecture_patterns: self.detect_architecture_patterns(&nodes),
            term_to_label: self.build_term_label_map(&nodes),
            term_to_node_type: self.build_term_type_map(&nodes),
        })
    }

    /// Detect frequently used labels.
    pub fn detect_label_patterns(&self, nodes: &[Node]) -> Vec<LabelPattern> {
        let mut counts: HashMap<String, usize> = HashMap::new();
        for node in nodes {
            for label in &node.labels {
                *counts.entry(label.clone()).or_default() += 1;
            }
        }

        let mut patterns: Vec<LabelPattern> = counts
            .into_iter()
            .filter(|(_, count)| *count >= self.min_label_frequency)
            .map(|(label, count)| LabelPattern {
                description: format!("{count} nodes labeled '{label}'"),
                label,
                count,
            })
            .collect();

        patterns.sort_by_key(|b| std::cmp::Reverse(b.count));
        patterns
    }

    /// Detect common naming suffixes like *Service, *Controller.
    pub fn detect_naming_patterns(&self, nodes: &[Node]) -> Vec<NamingPattern> {
        let suffixes = [
            "Service",
            "Controller",
            "Repository",
            "Handler",
            "Manager",
            "Client",
            "Provider",
        ];
        let mut patterns = Vec::new();

        for suffix in suffixes {
            let matches: Vec<String> = nodes
                .iter()
                .filter(|n| n.name.ends_with(suffix))
                .map(|n| n.name.clone())
                .collect();
            if matches.len() >= self.min_naming_frequency {
                patterns.push(NamingPattern {
                    suffix: suffix.to_string(),
                    count: matches.len(),
                    examples: matches.into_iter().take(5).collect(),
                });
            }
        }

        patterns.sort_by_key(|b| std::cmp::Reverse(b.count));
        patterns
    }

    fn build_term_label_map(&self, nodes: &[Node]) -> HashMap<String, String> {
        let mut map = HashMap::new();
        for node in nodes {
            for label in &node.labels {
                if let Some(term) = label.split(':').next_back() {
                    map.insert(term.to_lowercase(), label.clone());
                }
            }
        }
        // Common domain mappings
        if map.values().any(|l| l.contains("service")) {
            map.insert("services".to_string(), "soa:service".to_string());
            map.insert("service".to_string(), "soa:service".to_string());
        }
        map
    }

    fn build_term_type_map(&self, nodes: &[Node]) -> HashMap<String, String> {
        let mut map = HashMap::new();
        let re = Regex::new(r"(?i)(component|service|controller|repository)$").unwrap();
        for node in nodes {
            if let Some(cap) = re.captures(&node.name) {
                map.insert(cap[1].to_lowercase(), format!("{:?}", node.node_type));
            }
        }
        map
    }

    /// Detect architecture patterns (layers, modules, MVC, etc.).
    pub fn detect_architecture_patterns(&self, nodes: &[Node]) -> Vec<ArchitecturePattern> {
        let mut patterns = Vec::new();

        // Detect directory-based modules
        let mut dir_counts: HashMap<String, usize> = HashMap::new();
        for node in nodes {
            if let Some(file_path) = &node.file_path {
                if let Some(dir) = extract_top_level_dir(file_path) {
                    *dir_counts.entry(dir).or_default() += 1;
                }
            }
        }

        for (dir, count) in dir_counts.iter() {
            if *count >= 3 {
                patterns.push(ArchitecturePattern {
                    pattern_type: "module".to_string(),
                    name: dir.clone(),
                    node_count: *count,
                    description: format!("Module '{dir}' with {count} symbols"),
                });
            }
        }

        // Detect layered architecture patterns
        let layer_keywords = [
            ("controller", "presentation"),
            ("handler", "presentation"),
            ("service", "business"),
            ("manager", "business"),
            ("repository", "data"),
            ("dao", "data"),
            ("model", "data"),
        ];

        let mut layer_counts: HashMap<String, usize> = HashMap::new();
        for node in nodes {
            let name_lower = node.name.to_lowercase();
            for (keyword, layer) in &layer_keywords {
                if name_lower.contains(keyword) {
                    *layer_counts.entry(layer.to_string()).or_default() += 1;
                }
            }
        }

        for (layer, count) in layer_counts.iter() {
            if *count >= 2 {
                patterns.push(ArchitecturePattern {
                    pattern_type: "layer".to_string(),
                    name: layer.clone(),
                    node_count: *count,
                    description: format!("{layer} layer with {count} components"),
                });
            }
        }

        // Detect MVC pattern
        let has_controllers = nodes
            .iter()
            .any(|n| n.name.to_lowercase().contains("controller"));
        let has_models = nodes
            .iter()
            .any(|n| n.name.to_lowercase().contains("model"));
        let has_views = nodes.iter().any(|n| n.name.to_lowercase().contains("view"));

        if has_controllers && has_models {
            let count = nodes
                .iter()
                .filter(|n| {
                    let name_lower = n.name.to_lowercase();
                    name_lower.contains("controller")
                        || name_lower.contains("model")
                        || name_lower.contains("view")
                })
                .count();

            let description = if has_views {
                format!("MVC pattern detected with {count} components")
            } else {
                format!("MC pattern detected with {count} components")
            };

            patterns.push(ArchitecturePattern {
                pattern_type: "mvc".to_string(),
                name: "MVC".to_string(),
                node_count: count,
                description,
            });
        }

        patterns.sort_by_key(|b| std::cmp::Reverse(b.node_count));
        patterns
    }
}

/// Extract the top-level directory from a file path.
fn extract_top_level_dir(path: &str) -> Option<String> {
    let normalized = path.replace('\\', "/");
    let parts: Vec<&str> = normalized.split('/').collect();

    // Look for directories after common prefixes
    for (i, part) in parts.iter().enumerate() {
        if (*part == "src" || *part == "lib" || *part == "app")
            && i + 1 < parts.len()
            && !parts[i + 1].contains('.')
        {
            return Some(parts[i + 1].to_string());
        }
    }

    // Fallback: first directory that isn't a common top-level name
    parts
        .iter()
        .find(|p| !p.is_empty() && !matches!(**p, "." | ".." | "src" | "lib" | "tests" | "test"))
        .map(|s| s.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rbuilder_graph::backend::GraphBackend;
    use rbuilder_graph::schema::{Node, NodeType};

    fn graph_with_labels() -> MemoryBackend {
        let mut backend = MemoryBackend::new();
        for i in 0..5 {
            let mut node = Node::new(NodeType::Class, format!("Component{i}"));
            node.labels.push("react:component".to_string());
            backend.insert_node(node).unwrap();
        }
        for name in [
            "AuthService",
            "UserService",
            "OrderService",
            "PaymentService",
        ] {
            let node = Node::new(NodeType::Class, name.to_string());
            backend.insert_node(node).unwrap();
        }
        backend
    }

    #[test]
    fn test_label_pattern_detection() {
        let backend = graph_with_labels();
        let nodes = backend.all_nodes().unwrap();
        let detector = PatternDetector::new();
        let patterns = detector.detect_label_patterns(&nodes);
        assert!(patterns.iter().any(|p| p.label == "react:component"));
    }

    #[test]
    fn test_naming_pattern_detection() {
        let backend = graph_with_labels();
        let nodes = backend.all_nodes().unwrap();
        let detector = PatternDetector::new();
        let patterns = detector.detect_naming_patterns(&nodes);
        assert!(patterns.iter().any(|p| p.suffix == "Service"));
    }

    #[test]
    fn test_architecture_pattern_detection() {
        let mut backend = MemoryBackend::new();

        // Create nodes for a layered architecture
        for name in ["UserController", "OrderController"] {
            let mut node = Node::new(NodeType::Class, name.to_string());
            node.file_path = Some("src/controllers/user.rs".to_string());
            backend.insert_node(node).unwrap();
        }

        for name in ["UserService", "OrderService", "PaymentService"] {
            let mut node = Node::new(NodeType::Class, name.to_string());
            node.file_path = Some("src/services/user.rs".to_string());
            backend.insert_node(node).unwrap();
        }

        for name in ["UserRepository", "OrderRepository"] {
            let mut node = Node::new(NodeType::Class, name.to_string());
            node.file_path = Some("src/repositories/user.rs".to_string());
            backend.insert_node(node).unwrap();
        }

        let nodes = backend.all_nodes().unwrap();
        let detector = PatternDetector::new();
        let patterns = detector.detect_architecture_patterns(&nodes);

        // Should detect layers
        assert!(patterns
            .iter()
            .any(|p| p.pattern_type == "layer" && p.name == "presentation"));
        assert!(patterns
            .iter()
            .any(|p| p.pattern_type == "layer" && p.name == "business"));
        assert!(patterns
            .iter()
            .any(|p| p.pattern_type == "layer" && p.name == "data"));

        // Should detect modules
        assert!(patterns.iter().any(|p| p.pattern_type == "module"));
    }
}
