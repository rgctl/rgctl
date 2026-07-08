//! Louvain community summary exported beside the metagraph.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::metagraph::Metanode;

pub const COMMUNITIES_FILE: &str = "communities.json";
pub const COMMUNITIES_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommunitiesPayload {
    pub schema_version: u32,
    pub modularity: f64,
    pub communities: Vec<CommunitySummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommunitySummary {
    pub id: usize,
    pub label: String,
    pub color: String,
    pub member_count: u32,
    pub package_count: u32,
}

/// Build community rollups from metanodes that already carry `community_id`.
pub fn summarize_communities(modularity: f64, metanodes: &[Metanode]) -> CommunitiesPayload {
    let mut by_id: HashMap<usize, CommunitySummary> = HashMap::new();

    for node in metanodes {
        let Some(cid) = node.community_id else {
            continue;
        };
        let entry = by_id.entry(cid).or_insert_with(|| CommunitySummary {
            id: cid,
            label: format!("Community {cid}"),
            color: community_color_hsl(cid),
            member_count: 0,
            package_count: 0,
        });
        entry.member_count += node.size;
        entry.package_count += 1;
    }

    let mut communities: Vec<_> = by_id.into_values().collect();
    communities.sort_by_key(|c| std::cmp::Reverse(c.member_count));

    CommunitiesPayload {
        schema_version: COMMUNITIES_SCHEMA_VERSION,
        modularity,
        communities,
    }
}

/// Stable HSL palette aligned with the dashboard graph layout.
pub fn community_color_hsl(index: usize) -> String {
    let hue = (index * 47 + 210) % 360;
    format!("hsl({hue} 58% 52%)")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metagraph::Metanode;

    #[test]
    fn summarize_groups_packages() {
        let nodes = vec![
            Metanode {
                id: 0,
                label: "a".into(),
                size: 10,
                functions: 8,
                classes: 2,
                avg_complexity: 1.0,
                x: 0.0,
                y: 0.0,
                member_indices: vec![],
                community_id: Some(1),
            },
            Metanode {
                id: 1,
                label: "b".into(),
                size: 5,
                functions: 5,
                classes: 0,
                avg_complexity: 1.0,
                x: 0.0,
                y: 0.0,
                member_indices: vec![],
                community_id: Some(1),
            },
            Metanode {
                id: 2,
                label: "c".into(),
                size: 3,
                functions: 0,
                classes: 3,
                avg_complexity: 1.0,
                x: 0.0,
                y: 0.0,
                member_indices: vec![],
                community_id: Some(2),
            },
        ];
        let payload = summarize_communities(0.42, &nodes);
        assert_eq!(payload.communities.len(), 2);
        assert_eq!(payload.communities[0].member_count, 15);
        assert_eq!(payload.communities[0].package_count, 2);
    }
}
