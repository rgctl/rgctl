//! Type inference engine
//!
//! Task 4.1.1: Infer types for dynamically typed languages from usage patterns.

use regex::Regex;
use std::collections::HashMap;

/// Inferred type for a variable or parameter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InferredType {
    /// Unknown type
    Unknown,
    /// Integer numeric type
    Integer,
    /// Floating point numeric type
    Float,
    /// Numeric (int or float)
    Numeric,
    /// String type
    String,
    /// Boolean type
    Boolean,
    /// Collection/list type
    List,
    /// Object/dict type
    Object,
}

impl InferredType {
    /// Whether this type is numeric.
    pub fn is_numeric(&self) -> bool {
        matches!(self, Self::Integer | Self::Float | Self::Numeric)
    }
}

/// A type inference result with confidence.
#[derive(Debug, Clone, PartialEq)]
pub struct TypeInference {
    /// Inferred type
    pub inferred: InferredType,
    /// Confidence 0.0-1.0
    pub confidence: f64,
}

/// Type inferencer for dynamically typed languages.
pub struct TypeInferencer;

impl TypeInferencer {
    /// Create a new type inferencer.
    pub fn new() -> Self {
        Self
    }

    /// Infer parameter/variable types from Python source.
    pub fn infer_python(&self, source: &str) -> HashMap<String, TypeInference> {
        let mut types: HashMap<String, TypeInference> = HashMap::new();

        if let Ok(re) = Regex::new(r"def\s+\w+\s*\(([^)]*)\)") {
            if let Some(cap) = re.captures(source) {
                for param in cap[1].split(',') {
                    let name = param.trim().split(':').next().unwrap_or("").trim();
                    if !name.is_empty() && name != "self" {
                        types.insert(
                            name.to_string(),
                            TypeInference {
                                inferred: InferredType::Unknown,
                                confidence: 0.3,
                            },
                        );
                    }
                }
            }
        }

        for (name, inference) in types.iter_mut() {
            if source.contains(&format!("{name} +")) || source.contains(&format!("+ {name}")) {
                *inference = TypeInference {
                    inferred: InferredType::Numeric,
                    confidence: 0.8,
                };
            } else if source.contains(&format!("{name}.upper("))
                || source.contains(&format!("{name}.lower("))
            {
                *inference = TypeInference {
                    inferred: InferredType::String,
                    confidence: 0.85,
                };
            } else if source.contains(&format!("if {name}:")) {
                inference.confidence = inference.confidence.max(0.5);
            }
        }

        types
    }

    /// Infer types from JavaScript/TypeScript source.
    pub fn infer_javascript(&self, source: &str) -> HashMap<String, TypeInference> {
        let mut types = HashMap::new();

        if let Ok(re) = Regex::new(r"function\s+\w+\s*\(([^)]*)\)") {
            if let Some(cap) = re.captures(source) {
                for param in cap[1].split(',') {
                    let name = param.trim();
                    if !name.is_empty() {
                        types.insert(
                            name.to_string(),
                            TypeInference {
                                inferred: InferredType::Unknown,
                                confidence: 0.3,
                            },
                        );
                    }
                }
            }
        }

        for (name, inference) in types.iter_mut() {
            if source.contains(&format!("{name} +")) || source.contains(&format!("{name} *")) {
                *inference = TypeInference {
                    inferred: InferredType::Numeric,
                    confidence: 0.75,
                };
            } else if source.contains(&format!("{name}.length")) {
                *inference = TypeInference {
                    inferred: InferredType::String,
                    confidence: 0.7,
                };
            }
        }

        types
    }

    /// Map a language-specific type name to a normalized IDL type.
    pub fn map_to_idl_type(lang_type: &str) -> &'static str {
        match lang_type.to_lowercase().as_str() {
            "i32" | "i64" | "int" | "integer" | "usize" => "int64",
            "f32" | "f64" | "float" | "double" => "double",
            "string" | "str" => "string",
            "bool" | "boolean" => "bool",
            _ => "string",
        }
    }
}

impl Default for TypeInferencer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_python_type_inference() {
        let source = r#"
def calculate(x, y):
    result = x + y
    return result * 2
"#;
        let inferencer = TypeInferencer::new();
        let types = inferencer.infer_python(source);
        assert!(types["x"].inferred.is_numeric());
        assert!(types["y"].inferred.is_numeric());
    }

    #[test]
    fn test_type_mapping() {
        assert_eq!(TypeInferencer::map_to_idl_type("i32"), "int64");
        assert_eq!(TypeInferencer::map_to_idl_type("f64"), "double");
    }
}
