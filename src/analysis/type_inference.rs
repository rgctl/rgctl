//! Pattern-based type inference for dynamic languages (Phase 13.3).

use crate::analysis::cfg::ControlFlowGraph;
use crate::analysis::pdg::{PdgNodeId, ProgramDependenceGraph};
use std::collections::HashMap;

/// Inferred type for a variable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InferredType {
    /// Integer literal / usage.
    Int,
    /// Floating-point.
    Float,
    /// String.
    String,
    /// Boolean.
    Bool,
    /// None / null.
    None,
    /// Homogeneous list.
    List(Box<InferredType>),
    /// Key-value map.
    Dict(Box<InferredType>, Box<InferredType>),
    /// Tuple of element types.
    Tuple(Vec<InferredType>),
    /// Callable with params and return.
    Function {
        /// Parameter types.
        params: Vec<InferredType>,
        /// Return type.
        return_type: Box<InferredType>,
    },
    /// Unknown / unconstrained.
    Unknown,
    /// Union of alternatives.
    Union(Vec<InferredType>),
}

/// Inferred type for one variable at one program point.
#[derive(Debug, Clone)]
pub struct VariableType {
    /// Variable name.
    pub variable: String,
    /// Inferred type.
    pub inferred_type: InferredType,
    /// Confidence 0.0–1.0.
    pub confidence: f64,
    /// PDG node where type was inferred.
    pub node_id: PdgNodeId,
}

/// Type inference engine over a function PDG.
#[derive(Debug)]
pub struct TypeInferenceEngine<'a> {
    pdg: &'a ProgramDependenceGraph,
    _cfg: &'a ControlFlowGraph,
    language: &'a str,
    types: HashMap<PdgNodeId, HashMap<String, InferredType>>,
}

impl<'a> TypeInferenceEngine<'a> {
    /// Create an engine for the given language tag (`python`, `javascript`, `ruby`, …).
    pub fn new(
        pdg: &'a ProgramDependenceGraph,
        cfg: &'a ControlFlowGraph,
        language: &'a str,
    ) -> Self {
        let _ = cfg;
        Self {
            pdg,
            _cfg: cfg,
            language,
            types: HashMap::new(),
        }
    }

    /// Infer types for all variables in the PDG.
    pub fn infer(&mut self) -> Vec<VariableType> {
        match self.language {
            "python" | "py" => self.infer_python(),
            "javascript" | "js" | "typescript" | "ts" => self.infer_javascript(),
            "ruby" | "rb" => self.infer_ruby(),
            _ => Vec::new(),
        }
    }

    /// Lookup inferred type for `variable` at `node_id`.
    pub fn get_type(&self, node_id: PdgNodeId, variable: &str) -> Option<&InferredType> {
        self.types.get(&node_id)?.get(variable)
    }

    /// Merged best type for a variable across all nodes.
    pub fn variable_type(&self, variable: &str) -> Option<InferredType> {
        let mut found = None;
        for node_types in self.types.values() {
            if let Some(typ) = node_types.get(variable) {
                found = Some(merge_types(found.take(), typ.clone()));
            }
        }
        found
    }

    fn infer_python(&mut self) -> Vec<VariableType> {
        let mut results = Vec::new();
        for (node_id, node) in &self.pdg.nodes {
            let text = &node.statement.text;
            let mut node_types = HashMap::new();

            for var in &node.defined_vars {
                if text.contains(&format!("{var} =")) || text.contains(&format!("{var}=")) {
                    if text.matches('"').count() >= 2 || text.matches('\'').count() >= 2 {
                        node_types.insert(var.clone(), InferredType::String);
                    } else if text.contains(" = []") || text.contains("= []") {
                        node_types.insert(
                            var.clone(),
                            InferredType::List(Box::new(InferredType::Unknown)),
                        );
                    } else if text.contains(" = {}") || text.contains("= {}") {
                        node_types.insert(
                            var.clone(),
                            InferredType::Dict(
                                Box::new(InferredType::Unknown),
                                Box::new(InferredType::Unknown),
                            ),
                        );
                    } else if text.contains('.') && text.chars().any(|c| c.is_ascii_digit()) {
                        node_types.insert(var.clone(), InferredType::Float);
                    } else if text.chars().any(|c| c.is_ascii_digit()) {
                        node_types.insert(var.clone(), InferredType::Int);
                    }
                }
            }

            for var in &node.used_vars {
                if text.contains(&format!("{var}.upper"))
                    || text.contains(&format!("{var}.lower"))
                    || text.contains(&format!("{var}.strip"))
                {
                    node_types.insert(var.clone(), InferredType::String);
                }
                if text.contains(&format!("{var}.append")) {
                    node_types.insert(
                        var.clone(),
                        InferredType::List(Box::new(InferredType::Unknown)),
                    );
                }
            }

            self.types.insert(*node_id, node_types.clone());
            for (var, typ) in node_types {
                let from_method = text.contains(&format!("{var}."))
                    && !text.contains(&format!("{var} ="));
                results.push(VariableType {
                    variable: var,
                    inferred_type: typ.clone(),
                    confidence: confidence_for(&typ, from_method),
                    node_id: *node_id,
                });
            }
        }
        results
    }

    fn infer_javascript(&mut self) -> Vec<VariableType> {
        let mut results = Vec::new();
        for (node_id, node) in &self.pdg.nodes {
            let text = &node.statement.text;
            let mut node_types = HashMap::new();

            if text.contains("const ")
                || text.contains("let ")
                || text.contains("var ")
            {
                for var in &node.defined_vars {
                    if text.contains('"') || text.contains('\'') || text.contains('`') {
                        node_types.insert(var.clone(), InferredType::String);
                    } else if text.contains("true") || text.contains("false") {
                        node_types.insert(var.clone(), InferredType::Bool);
                    } else if text.contains('[') && text.contains(']') {
                        node_types.insert(
                            var.clone(),
                            InferredType::List(Box::new(InferredType::Unknown)),
                        );
                    } else if text.contains('{') && text.contains('}') {
                        node_types.insert(
                            var.clone(),
                            InferredType::Dict(
                                Box::new(InferredType::Unknown),
                                Box::new(InferredType::Unknown),
                            ),
                        );
                    } else if text.chars().any(|c| c.is_ascii_digit()) {
                        if text.contains('.') {
                            node_types.insert(var.clone(), InferredType::Float);
                        } else {
                            node_types.insert(var.clone(), InferredType::Int);
                        }
                    }
                }
            }

            for var in &node.used_vars {
                if text.contains(&format!("{var}.push")) {
                    node_types.insert(
                        var.clone(),
                        InferredType::List(Box::new(InferredType::Unknown)),
                    );
                }
                if text.contains(&format!("{var}.toUpperCase"))
                    || text.contains(&format!("{var}.toLowerCase"))
                {
                    node_types.insert(var.clone(), InferredType::String);
                }
            }

            self.types.insert(*node_id, node_types.clone());
            for (var, typ) in node_types {
                let from_method = text.contains(&format!("{var}."))
                    && !text.contains(&format!("{var} ="));
                results.push(VariableType {
                    variable: var,
                    inferred_type: typ.clone(),
                    confidence: confidence_for(&typ, from_method),
                    node_id: *node_id,
                });
            }
        }
        results
    }

    fn infer_ruby(&mut self) -> Vec<VariableType> {
        let mut results = Vec::new();
        for (node_id, node) in &self.pdg.nodes {
            let text = &node.statement.text;
            let mut node_types = HashMap::new();

            for var in &node.defined_vars {
                if text.contains(&format!("{var} =")) {
                    if text.matches('"').count() >= 2 || text.matches('\'').count() >= 2 {
                        node_types.insert(var.clone(), InferredType::String);
                    } else if text.contains('[') && text.contains(']') {
                        node_types.insert(
                            var.clone(),
                            InferredType::List(Box::new(InferredType::Unknown)),
                        );
                    } else if text.contains('{') && text.contains('}') {
                        node_types.insert(
                            var.clone(),
                            InferredType::Dict(
                                Box::new(InferredType::Unknown),
                                Box::new(InferredType::Unknown),
                            ),
                        );
                    } else if text.chars().any(|c| c.is_ascii_digit()) {
                        node_types.insert(var.clone(), InferredType::Int);
                    }
                }
            }

            for var in &node.used_vars {
                if text.contains(&format!("{var}.upcase"))
                    || text.contains(&format!("{var}.downcase"))
                {
                    node_types.insert(var.clone(), InferredType::String);
                }
                if text.contains(&format!("{var}<<")) {
                    node_types.insert(
                        var.clone(),
                        InferredType::List(Box::new(InferredType::Unknown)),
                    );
                }
            }

            self.types.insert(*node_id, node_types.clone());
            for (var, typ) in node_types {
                let from_method = text.contains(&format!("{var}."))
                    || text.contains(&format!("{var}<<"));
                results.push(VariableType {
                    variable: var,
                    inferred_type: typ.clone(),
                    confidence: confidence_for(&typ, from_method),
                    node_id: *node_id,
                });
            }
        }
        results
    }
}

/// Calibrated confidence based on inference method reliability.
pub fn confidence_for(typ: &InferredType, from_method_call: bool) -> f64 {
    if from_method_call {
        return match typ {
            InferredType::String => 0.86,
            InferredType::List(_) => 0.78,
            _ => 0.65,
        };
    }
    match typ {
        InferredType::Int | InferredType::Float | InferredType::Bool => 0.92,
        InferredType::String => 0.9,
        InferredType::List(_) => 0.76,
        InferredType::Dict(_, _) => 0.74,
        InferredType::None => 0.88,
        InferredType::Unknown => 0.4,
        InferredType::Union(_) => 0.55,
        InferredType::Tuple(_) | InferredType::Function { .. } => 0.6,
    }
}

fn merge_types(a: Option<InferredType>, b: InferredType) -> InferredType {
    match a {
        None => b,
        Some(InferredType::Unknown) => b,
        Some(a) if a == b => a,
        Some(a) => InferredType::Union(vec![a, b]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::cfg_builder::build_cfg_for_function;
    use crate::analysis::pdg::ProgramDependenceGraph;

    #[cfg(feature = "lang-python")]
    #[test]
    fn test_type_inference_python_literals() {
        let code = r#"
def example():
    x = 42
    y = "hello"
    z = 3.14
    items = []
"#;
        let cfg = build_cfg_for_function("python", code, "example").unwrap();
        let pdg = ProgramDependenceGraph::build(&cfg, code.as_bytes()).unwrap();
        let mut engine = TypeInferenceEngine::new(&pdg, &cfg, "python");
        let types = engine.infer();
        assert!(types.iter().any(|t| t.variable == "x" && t.inferred_type == InferredType::Int));
        assert!(
            types
                .iter()
                .any(|t| t.variable == "y" && t.inferred_type == InferredType::String)
        );
    }

    #[cfg(feature = "lang-python")]
    #[test]
    fn test_type_inference_method_calls() {
        let code = r#"
def process(data):
    upper = data.upper()
    items = []
    items.append("test")
"#;
        let cfg = build_cfg_for_function("python", code, "process").unwrap();
        let pdg = ProgramDependenceGraph::build(&cfg, code.as_bytes()).unwrap();
        let mut engine = TypeInferenceEngine::new(&pdg, &cfg, "python");
        let types = engine.infer();
        assert!(
            types
                .iter()
                .any(|t| t.variable == "data" && t.inferred_type == InferredType::String)
        );
        assert!(types.iter().any(|t| {
            t.variable == "items" && matches!(t.inferred_type, InferredType::List(_))
        }));
    }
}
