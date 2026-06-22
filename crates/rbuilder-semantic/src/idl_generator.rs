//! IDL generation
//!
//! Task 4.1.3: Generate Proto/Thrift/OpenAPI IDL from function signatures.

use crate::signature::{FunctionSignature, SignatureExtractor};
use crate::type_inference::TypeInferencer;
use handlebars::Handlebars;
use rbuilder_error::{Error, Result};
use rbuilder_graph::backend::MemoryBackend;
use rbuilder_graph::schema::NodeType;
use std::collections::HashMap;
use std::path::Path;

/// Supported IDL output formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdlFormat {
    /// Protocol Buffers proto3
    Proto,
    /// Apache Thrift
    Thrift,
    /// OpenAPI 3.0 YAML
    OpenApi,
}

impl IdlFormat {
    /// Parse format from CLI string.
    pub fn parse(s: &str) -> Result<Self> {
        match s.to_lowercase().as_str() {
            "proto" | "protobuf" => Ok(Self::Proto),
            "thrift" => Ok(Self::Thrift),
            "openapi" | "swagger" => Ok(Self::OpenApi),
            other => Err(Error::ConfigError(format!(
                "Unsupported IDL format: {other}"
            ))),
        }
    }

    /// Default file extension for this format.
    pub fn extension(&self) -> &'static str {
        match self {
            Self::Proto => "proto",
            Self::Thrift => "thrift",
            Self::OpenApi => "yaml",
        }
    }
}

/// IDL generator using Handlebars templates.
pub struct IdlGenerator {
    handlebars: Handlebars<'static>,
}

impl Default for IdlGenerator {
    fn default() -> Self {
        Self::new()
    }
}

impl IdlGenerator {
    /// Create a generator with built-in templates.
    pub fn new() -> Self {
        let mut handlebars = Handlebars::new();
        handlebars
            .register_template_string("proto", PROTO_TEMPLATE)
            .unwrap();
        handlebars
            .register_template_string("thrift", THRIFT_TEMPLATE)
            .unwrap();
        handlebars
            .register_template_string("openapi", OPENAPI_TEMPLATE)
            .unwrap();
        Self { handlebars }
    }

    /// Generate IDL for a single function signature.
    pub fn generate(&self, format: IdlFormat, signature: &FunctionSignature) -> Result<String> {
        let data = signature_template_data(signature);
        let template = match format {
            IdlFormat::Proto => "proto",
            IdlFormat::Thrift => "thrift",
            IdlFormat::OpenApi => "openapi",
        };
        self.handlebars
            .render(template, &data)
            .map_err(|e| Error::Other(format!("Template render error: {e}")))
    }

    /// Generate proto IDL for a function signature.
    pub fn generate_proto(&self, signature: &FunctionSignature) -> Result<String> {
        self.generate(IdlFormat::Proto, signature)
    }

    /// Extract function signatures from graph and generate module IDL.
    pub fn generate_module(
        &self,
        backend: &MemoryBackend,
        format: IdlFormat,
        module: &str,
    ) -> Result<String> {
        let functions = backend.find_nodes_by_type(NodeType::Function)?;
        let signatures: Vec<FunctionSignature> = functions
            .iter()
            .filter(|n| {
                module.is_empty()
                    || n.file_path
                        .as_deref()
                        .map(|p| p.contains(module))
                        .unwrap_or(false)
                    || n.name.contains(module)
            })
            .filter_map(SignatureExtractor::from_node)
            .collect();

        if signatures.is_empty() {
            return Err(Error::NotFound(format!(
                "No functions found for module '{module}'"
            )));
        }

        let mut output = format!("// Generated IDL for module: {module}\n\n");
        for sig in &signatures {
            output.push_str(&self.generate(format, sig)?);
            output.push('\n');
        }
        Ok(output)
    }

    /// Write module IDL to an output directory.
    pub fn write_module(
        &self,
        backend: &MemoryBackend,
        format: IdlFormat,
        module: &str,
        output_dir: &Path,
    ) -> Result<std::path::PathBuf> {
        std::fs::create_dir_all(output_dir)?;
        let content = self.generate_module(backend, format, module)?;
        let filename = if module.is_empty() {
            format!("service.{}", format.extension())
        } else {
            format!("{module}.{}", format.extension())
        };
        let path = output_dir.join(filename);
        std::fs::write(&path, content)?;
        Ok(path)
    }
}

fn signature_template_data(signature: &FunctionSignature) -> HashMap<String, serde_json::Value> {
    let service_name = to_pascal_case(&signature.name);
    let request_name = format!("{service_name}Request");
    let response_name = format!("{service_name}Response");

    let params: Vec<serde_json::Value> = signature
        .params
        .iter()
        .enumerate()
        .map(|(i, p)| {
            serde_json::json!({
                "name": p.name,
                "proto_type": map_proto_type(&p.type_),
                "thrift_type": map_thrift_type(&p.type_),
                "openapi_type": map_openapi_type(&p.type_),
                "index": i + 1,
            })
        })
        .collect();

    let return_proto = signature
        .return_type
        .as_deref()
        .map(map_proto_type)
        .unwrap_or_else(|| "string".to_string());

    let return_thrift = signature
        .return_type
        .as_deref()
        .map(map_thrift_type)
        .unwrap_or_else(|| "string".to_string());

    let return_openapi = signature
        .return_type
        .as_deref()
        .map(map_openapi_type)
        .unwrap_or_else(|| "string".to_string());

    let mut data = HashMap::new();
    data.insert("name".to_string(), serde_json::json!(signature.name));
    data.insert("service_name".to_string(), serde_json::json!(service_name));
    data.insert("request_name".to_string(), serde_json::json!(request_name));
    data.insert(
        "response_name".to_string(),
        serde_json::json!(response_name),
    );
    data.insert("params".to_string(), serde_json::json!(params));
    data.insert("return_type".to_string(), serde_json::json!(return_proto));
    data.insert("return_proto".to_string(), serde_json::json!(return_proto));
    data.insert(
        "return_thrift".to_string(),
        serde_json::json!(return_thrift),
    );
    data.insert(
        "return_openapi".to_string(),
        serde_json::json!(return_openapi),
    );
    data.insert(
        "has_return".to_string(),
        serde_json::json!(signature.return_type.is_some()),
    );
    data
}

fn map_proto_type(ty: &str) -> String {
    match TypeInferencer::map_to_idl_type(ty) {
        "int64" => "int64".to_string(),
        "double" => "double".to_string(),
        "bool" => "bool".to_string(),
        _ => "string".to_string(),
    }
}

fn map_thrift_type(ty: &str) -> String {
    match TypeInferencer::map_to_idl_type(ty) {
        "int64" => "i64".to_string(),
        "double" => "double".to_string(),
        "bool" => "bool".to_string(),
        _ => "string".to_string(),
    }
}

fn map_openapi_type(ty: &str) -> String {
    match TypeInferencer::map_to_idl_type(ty) {
        "int64" => "integer".to_string(),
        "double" => "number".to_string(),
        "bool" => "boolean".to_string(),
        _ => "string".to_string(),
    }
}

fn to_pascal_case(s: &str) -> String {
    s.split('_')
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                None => String::new(),
                Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
            }
        })
        .collect()
}

const PROTO_TEMPLATE: &str = r#"syntax = "proto3";

package {{service_name}};

message {{request_name}} {
{{#each params}}
  {{proto_type}} {{name}} = {{index}};
{{/each}}
}

message {{response_name}} {
  {{return_type}} result = 1;
}

service {{service_name}}Service {
  rpc {{name}}({{request_name}}) returns ({{response_name}});
}
"#;

const THRIFT_TEMPLATE: &str = r#"namespace rs {{service_name}}

struct {{request_name}} {
{{#each params}}
  {{index}}: {{thrift_type}} {{name}},
{{/each}}
}

struct {{response_name}} {
  1: {{return_thrift}} result
}

service {{service_name}}Service {
  {{response_name}} {{name}}(1: {{request_name}} req)
}
"#;

const OPENAPI_TEMPLATE: &str = r#"openapi: "3.0.0"
info:
  title: {{service_name}} API
  version: "1.0"
paths:
  /{{name}}:
    post:
      summary: {{name}}
      requestBody:
        content:
          application/json:
            schema:
              type: object
              properties:
{{#each params}}
                {{name}}:
                  type: {{openapi_type}}
{{/each}}
      responses:
        "200":
          description: Success
          content:
            application/json:
              schema:
                type: object
                properties:
                  result:
                    type: {{return_openapi}}
"#;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::signature::Param;

    #[test]
    fn test_proto_generation() {
        let signature = FunctionSignature {
            name: "calculate_discount".to_string(),
            module: None,
            params: vec![
                Param {
                    name: "price".to_string(),
                    type_: "f64".to_string(),
                },
                Param {
                    name: "tier".to_string(),
                    type_: "UserTier".to_string(),
                },
            ],
            return_type: Some("f64".to_string()),
            file_path: None,
        };

        let generator = IdlGenerator::new();
        let proto = generator.generate_proto(&signature).unwrap();

        assert!(proto.contains("message CalculateDiscountRequest"));
        assert!(proto.contains("double price = 1"));
    }
}
