//! JSON policy file loader for CLI commands.

use anyhow::{Context, Result};
use crate::analysis::PolicyRegistry;
use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;
use uuid::Uuid;

#[derive(Debug, Deserialize)]
pub struct PolicyFile {
    #[serde(default)]
    pub forbidden_crossings: Vec<[String; 2]>,
    #[serde(default = "default_max_impact")]
    pub max_impact_nodes: usize,
    #[serde(default = "default_centrality_threshold")]
    pub centrality_alert_threshold: f64,
    #[serde(default)]
    pub node_domains: HashMap<String, String>,
}

fn default_max_impact() -> usize {
    usize::MAX
}

fn default_centrality_threshold() -> f64 {
    f64::MAX
}

impl PolicyFile {
    pub fn load(path: &Path) -> Result<Self> {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("read policy file {}", path.display()))?;
        serde_json::from_str(&text).context("parse policy JSON")
    }

    pub fn into_registry(self) -> PolicyRegistry {
        let mut registry = PolicyRegistry {
            forbidden_crossings: self
                .forbidden_crossings
                .into_iter()
                .map(|pair| (pair[0].clone(), pair[1].clone()))
                .collect(),
            max_impact_nodes: self.max_impact_nodes,
            centrality_alert_threshold: self.centrality_alert_threshold,
            node_domains: HashMap::new(),
        };
        for (id, domain) in self.node_domains {
            if let Ok(uuid) = Uuid::parse_str(&id) {
                registry.assign_domain(uuid, domain);
            }
        }
        registry
    }
}
