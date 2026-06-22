//! Function signature extraction
//!
//! Task 4.1.2: Extract language-agnostic function signatures.

use rbuilder_graph::schema::{Node, NodeType};
use rbuilder_plugin_api::{Parameter, Symbol};
use regex::Regex;
use serde::{Deserialize, Serialize};

/// A normalized function parameter.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Param {
    /// Parameter name
    pub name: String,
    /// Parameter type (language-specific or normalized)
    pub type_: String,
}

/// Language-agnostic function signature.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FunctionSignature {
    /// Function name
    pub name: String,
    /// Module or namespace
    pub module: Option<String>,
    /// Parameters
    pub params: Vec<Param>,
    /// Return type if known
    pub return_type: Option<String>,
    /// Source file path
    pub file_path: Option<String>,
}

/// Extract signatures from graph function nodes.
pub struct SignatureExtractor;

impl SignatureExtractor {
    /// Extract signature from a graph node.
    pub fn from_node(node: &Node) -> Option<FunctionSignature> {
        if node.node_type != NodeType::Function {
            return None;
        }

        let params = if !node.parameters.is_empty() {
            node.parameters
                .iter()
                .map(|p| Param {
                    name: p.name.clone(),
                    type_: p
                        .param_type
                        .clone()
                        .unwrap_or_else(|| "unknown".to_string()),
                })
                .collect()
        } else {
            node.get_property("parameters")
                .map(|p| parse_params_json(p))
                .unwrap_or_default()
        };

        Some(FunctionSignature {
            name: node.name.clone(),
            module: node.qualified_name.clone(),
            params,
            return_type: node.return_type_text().map(str::to_string),
            file_path: node.file_path.clone(),
        })
    }

    /// Extract signature from a language plugin Symbol.
    pub fn from_symbol(symbol: &Symbol) -> FunctionSignature {
        FunctionSignature {
            name: symbol.name.clone(),
            module: symbol.qualified_name.clone(),
            params: symbol
                .parameters
                .iter()
                .map(|p| Param {
                    name: p.name.clone(),
                    type_: p
                        .param_type
                        .clone()
                        .unwrap_or_else(|| "unknown".to_string()),
                })
                .collect(),
            return_type: symbol.return_type.clone(),
            file_path: Some(symbol.location.file.clone()),
        }
    }

    /// Parse a function signature from source text (Rust/Python style).
    pub fn from_source(source: &str) -> Option<FunctionSignature> {
        let rust_re = Regex::new(r"fn\s+(\w+)\s*\(([^)]*)\)\s*(?:->\s*([\w<>]+))?").ok()?;
        let py_re = Regex::new(r"def\s+(\w+)\s*\(([^)]*)\)\s*(?:->\s*([\w]+))?").ok()?;

        if let Some(cap) = rust_re.captures(source) {
            return Some(FunctionSignature {
                name: cap[1].to_string(),
                module: None,
                params: parse_param_list(&cap[2]),
                return_type: cap.get(3).map(|m| m.as_str().to_string()),
                file_path: None,
            });
        }

        if let Some(cap) = py_re.captures(source) {
            return Some(FunctionSignature {
                name: cap[1].to_string(),
                module: None,
                params: parse_param_list(&cap[2]),
                return_type: cap.get(3).map(|m| m.as_str().to_string()),
                file_path: None,
            });
        }

        None
    }

    /// Check if two signatures are semantically equivalent (same param count and return type family).
    pub fn signatures_equivalent(a: &FunctionSignature, b: &FunctionSignature) -> bool {
        a.params.len() == b.params.len()
            && normalize_type(a.return_type.as_deref()) == normalize_type(b.return_type.as_deref())
    }
}

fn parse_param_list(raw: &str) -> Vec<Param> {
    raw.split(',')
        .filter(|s| !s.trim().is_empty())
        .map(|part| {
            let part = part.trim();
            if let Some((name, ty)) = part.split_once(':') {
                Param {
                    name: name.trim().to_string(),
                    type_: ty.trim().to_string(),
                }
            } else {
                Param {
                    name: part.to_string(),
                    type_: "unknown".to_string(),
                }
            }
        })
        .collect()
}

fn parse_params_json(raw: &str) -> Vec<Param> {
    serde_json::from_str::<Vec<Parameter>>(raw)
        .map(|params| {
            params
                .into_iter()
                .map(|p| Param {
                    name: p.name,
                    type_: p.param_type.unwrap_or_else(|| "unknown".to_string()),
                })
                .collect()
        })
        .unwrap_or_default()
}

fn normalize_type(ty: Option<&str>) -> Option<String> {
    ty.map(|t| {
        let lower = t.to_lowercase();
        match lower.as_str() {
            "i32" | "i64" | "int" | "integer" => "int".to_string(),
            "f32" | "f64" | "float" | "double" => "float".to_string(),
            other => other.to_string(),
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_signature_extraction() {
        let rust_sig = SignatureExtractor::from_source("fn add(a: i32, b: i32) -> i32").unwrap();
        assert_eq!(rust_sig.params.len(), 2);
        assert_eq!(rust_sig.return_type, Some("i32".to_string()));

        let py_sig = SignatureExtractor::from_source("def add(a: int, b: int) -> int").unwrap();
        assert_eq!(py_sig.params.len(), 2);
        assert!(SignatureExtractor::signatures_equivalent(
            &rust_sig, &py_sig
        ));
    }
}
