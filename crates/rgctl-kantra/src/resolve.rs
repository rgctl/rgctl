//! Map Kantra violations to graph node UUIDs.

use crate::engine::EvalNode;
use crate::findings::KantraViolation;
use std::collections::HashMap;
use uuid::Uuid;

/// Index for resolving violations to graph nodes.
#[derive(Debug, Default)]
pub struct ViolationResolver {
    by_file_line: HashMap<(String, usize), Uuid>,
    by_file_symbol: HashMap<(String, String), Uuid>,
    by_file: HashMap<String, Uuid>,
    by_import_name: HashMap<String, Vec<Uuid>>,
}

impl ViolationResolver {
    /// Build lookup tables from evaluation graph nodes (with stable ids).
    pub fn from_eval_nodes(nodes: &[EvalNode]) -> Self {
        let mut resolver = Self::default();
        for node in nodes {
            let Some(id) = node.id else {
                continue;
            };
            let file = node
                .file_path
                .as_deref()
                .map(normalize_path)
                .unwrap_or_default();
            if file.is_empty() {
                continue;
            }
            if node.node_type == "File" {
                resolver.by_file.entry(file.clone()).or_insert(id);
            }
            if let Some(line) = node.start_line {
                resolver
                    .by_file_line
                    .entry((file.clone(), line))
                    .or_insert(id);
            }
            let sym = node
                .qualified_name
                .as_deref()
                .unwrap_or(&node.name)
                .to_string();
            resolver
                .by_file_symbol
                .entry((file.clone(), sym.clone()))
                .or_insert(id);
            resolver
                .by_file_symbol
                .entry((file.clone(), node.name.clone()))
                .or_insert(id);
            if node.node_type == "Import" {
                resolver
                    .by_import_name
                    .entry(node.name.clone())
                    .or_default()
                    .push(id);
            }
        }
        resolver
    }

    /// Resolve a violation to a graph node id when possible.
    pub fn resolve(&self, violation: &KantraViolation) -> Option<Uuid> {
        let file = normalize_path(&violation.file);
        if let Some(sym) = violation.symbol.as_deref().filter(|s| !s.is_empty()) {
            if let Some(id) = self.by_file_symbol.get(&(file.clone(), sym.to_string())) {
                return Some(*id);
            }
            if violation.matched_by.contains("referenced")
                && let Some(ids) = self.by_import_name.get(sym)
            {
                return ids.first().copied();
            }
        }
        if let Some(id) = self.by_file_line.get(&(file.clone(), violation.line)) {
            return Some(*id);
        }
        if violation.matched_by.contains("filecontent") || violation.matched_by == "builtin.file" {
            return self.by_file.get(&file).copied();
        }
        self.by_file.get(&file).copied()
    }

    /// Attach `enrichment.node_id` for all violations.
    pub fn attach_node_ids(&self, violations: &mut [KantraViolation]) {
        for v in violations.iter_mut() {
            if v.enrichment
                .as_ref()
                .and_then(|e| e.node_id.as_ref())
                .is_some()
            {
                continue;
            }
            let node_id = self.resolve(v).map(|id| id.to_string());
            v.enrichment.get_or_insert_with(Default::default).node_id = node_id;
        }
    }
}

fn normalize_path(path: &str) -> String {
    path.replace('\\', "/")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::EvalNode;

    fn node(id: Uuid, ty: &str, file: &str, line: usize, name: &str) -> EvalNode {
        EvalNode {
            id: Some(id),
            node_type: ty.to_string(),
            name: name.to_string(),
            qualified_name: Some(name.to_string()),
            file_path: Some(file.to_string()),
            start_line: Some(line),
            labels: Vec::new(),
        }
    }

    #[test]
    fn resolves_import_by_symbol() {
        let import_id = Uuid::new_v4();
        let resolver = ViolationResolver::from_eval_nodes(&[node(
            import_id,
            "Import",
            "src/Foo.java",
            3,
            "javax.servlet.http.HttpServlet",
        )]);
        let violation = KantraViolation {
            rule_id: "r1".into(),
            category: None,
            file: "src/Foo.java".into(),
            line: 3,
            message: None,
            matched_by: "java.referenced".into(),
            symbol: Some("javax.servlet.http.HttpServlet".into()),
            targets: Vec::new(),
            sources: Vec::new(),
            effort: None,
            enrichment: None,
        };
        assert_eq!(resolver.resolve(&violation), Some(import_id));
    }

    #[test]
    fn resolves_file_node_for_filecontent() {
        let file_id = Uuid::new_v4();
        let resolver =
            ViolationResolver::from_eval_nodes(&[node(file_id, "File", "pom.xml", 1, "pom.xml")]);
        let violation = KantraViolation {
            rule_id: "r1".into(),
            category: None,
            file: "pom.xml".into(),
            line: 10,
            message: None,
            matched_by: "builtin.filecontent".into(),
            symbol: None,
            targets: Vec::new(),
            sources: Vec::new(),
            effort: None,
            enrichment: None,
        };
        assert_eq!(resolver.resolve(&violation), Some(file_id));
    }
}
