//! Named GQL query macros (Phase 12.4 / 12.5).

use rbuilder_error::{Error, Result};
use std::collections::HashMap;

/// A saved/named GQL query macro.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueryMacro {
    /// Macro name used for lookup
    pub name: String,
    /// Short description
    pub description: String,
    /// GQL query text
    pub query: String,
}

/// Registry of named query macros.
#[derive(Debug, Clone, Default)]
pub struct QueryMacroRegistry {
    macros: HashMap<String, QueryMacro>,
}

impl QueryMacroRegistry {
    /// Create an empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a registry preloaded with built-in macros.
    pub fn with_defaults() -> Self {
        let mut registry = Self::new();
        registry.register(QueryMacro {
            name: "all_functions".into(),
            description: "All function nodes".into(),
            query: "MATCH (f:Function) RETURN f".into(),
        });
        registry.register(QueryMacro {
            name: "direct_calls".into(),
            description: "One-hop call relationships between functions".into(),
            query: "MATCH (a:Function)-[:CALLS*1..1]->(b:Function) RETURN a,b".into(),
        });
        registry.register(QueryMacro {
            name: "call_chain".into(),
            description: "Multi-hop call chain up to 3 hops".into(),
            query: "MATCH (a:Function)-[:CALLS*1..3]->(b:Function) RETURN a,b".into(),
        });
        registry
    }

    /// Register or replace a macro by name.
    pub fn register(&mut self, mac: QueryMacro) {
        self.macros.insert(mac.name.clone(), mac);
    }

    /// Lookup a macro by name.
    pub fn get(&self, name: &str) -> Option<&QueryMacro> {
        self.macros.get(name)
    }

    /// Resolve macro name to query text.
    pub fn resolve(&self, name: &str) -> Result<&str> {
        self.get(name)
            .map(|m| m.query.as_str())
            .ok_or_else(|| Error::NotFound(format!("query macro not found: {name}")))
    }

    /// List registered macro names.
    pub fn names(&self) -> Vec<String> {
        let mut names: Vec<String> = self.macros.keys().cloned().collect();
        names.sort();
        names
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_macros() {
        let registry = QueryMacroRegistry::with_defaults();
        assert!(registry.get("all_functions").is_some());
        assert_eq!(
            registry.resolve("call_chain").unwrap(),
            "MATCH (a:Function)-[:CALLS*1..3]->(b:Function) RETURN a,b"
        );
    }

    #[test]
    fn test_missing_macro() {
        let registry = QueryMacroRegistry::new();
        assert!(registry.resolve("missing").is_err());
    }
}
