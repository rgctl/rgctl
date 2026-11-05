//! `java.dependency` / `go.dependency` against graph Dependency nodes.

use crate::engine::EvalNode;
use crate::eval::{MatchSite, compile_kantra_pattern, violation};
use crate::findings::KantraViolation;

/// Match Kantra java.dependency / go.dependency conditions against Dependency nodes.
pub fn eval_dependency(
    rule_id: &str,
    ecosystem: &str,
    name: &str,
    nameregex: Option<&str>,
    lowerbound: Option<&str>,
    upperbound: Option<&str>,
    nodes: &[EvalNode],
) -> Vec<KantraViolation> {
    let mut out = Vec::new();
    let name_re = nameregex.and_then(|p| compile_kantra_pattern(p).ok());

    for node in nodes {
        if node.node_type != "Dependency" {
            continue;
        }
        let eco = node
            .labels
            .iter()
            .find(|l| l.starts_with("ecosystem:"))
            .map(|l| l.trim_start_matches("ecosystem:"))
            .or_else(|| {
                node.qualified_name
                    .as_deref()
                    .and_then(|q| q.split_once(':').map(|(e, _)| e))
            })
            .unwrap_or("");
        if !eco.is_empty() && eco != ecosystem && !(ecosystem == "maven" && eco == "gradle") {
            if ecosystem == "java" || ecosystem == "maven" {
                if eco != "maven" && eco != "gradle" {
                    continue;
                }
            } else if ecosystem == "go" || ecosystem == "golang" {
                if eco != "golang" {
                    continue;
                }
            } else if eco != ecosystem {
                continue;
            }
        }

        let matched = if let Some(re) = &name_re {
            coordinate_variants(&node.name)
                .into_iter()
                .chain(
                    node.qualified_name
                        .as_deref()
                        .into_iter()
                        .flat_map(coordinate_variants),
                )
                .any(|c| re.is_match(&c))
        } else if !name.is_empty() {
            coordinates_match(&node.name, name)
                || node
                    .qualified_name
                    .as_deref()
                    .is_some_and(|qn| coordinates_match(qn, name))
        } else {
            false
        };
        if !matched {
            continue;
        }

        if !version_matches(node.version.as_deref(), lowerbound, upperbound) {
            continue;
        }

        let site = MatchSite::new(
            node.file_path
                .clone()
                .unwrap_or_else(|| "<manifest>".into()),
            node.start_line.unwrap_or(1),
        )
        .with_symbol(node.name.clone());
        out.push(violation(rule_id, "dependency", &site));
    }
    out
}

fn coordinates_match(dep: &str, needle: &str) -> bool {
    let dep_variants = coordinate_variants(dep);
    let needle_variants = coordinate_variants(needle);
    for d in &dep_variants {
        for n in &needle_variants {
            if d == n || d.ends_with(&format!(":{n}")) || d.ends_with(&format!(".{n}")) {
                return true;
            }
        }
    }
    false
}

/// Produce colon and dot forms for Maven `groupId:artifactId` ↔ Kantra `groupId.artifactId`.
fn coordinate_variants(coord: &str) -> Vec<String> {
    let mut out = vec![coord.to_string()];
    // Strip ecosystem prefix `maven:group:artifact` → `group:artifact`
    let bare = coord
        .strip_prefix("maven:")
        .or_else(|| coord.strip_prefix("gradle:"))
        .or_else(|| coord.strip_prefix("golang:"))
        .unwrap_or(coord);
    if bare != coord {
        out.push(bare.to_string());
    }
    if bare.contains(':') {
        out.push(bare.replace(':', "."));
    } else if let Some((group, artifact)) = split_dot_coordinate(bare) {
        out.push(format!("{group}:{artifact}"));
    }
    out.sort();
    out.dedup();
    out
}

fn split_dot_coordinate(coord: &str) -> Option<(&str, &str)> {
    // Heuristic: last segment after the final `.` is artifactId when it contains `-`
    // or when there are ≥2 dots (group.artifact). Prefer last-dot split for Kantra
    // `org.jboss.bom.eap-runtime-artifacts` → `org.jboss.bom` + `eap-runtime-artifacts`.
    let (group, artifact) = coord.rsplit_once('.')?;
    if group.is_empty() || artifact.is_empty() {
        return None;
    }
    Some((group, artifact))
}

/// Inclusive version bounds matching Konveyor semantics (numeric segment compare).
pub fn version_matches(
    dep_version: Option<&str>,
    lowerbound: Option<&str>,
    upperbound: Option<&str>,
) -> bool {
    if lowerbound.is_none() && upperbound.is_none() {
        return true;
    }
    let Some(ver) = dep_version.filter(|v| !v.is_empty()) else {
        // Bounds specified but dependency has no version → do not match.
        return false;
    };
    let parsed = parse_version_segments(ver);
    if let Some(lo) = lowerbound
        && cmp_versions(&parsed, &parse_version_segments(lo)) == std::cmp::Ordering::Less
    {
        return false;
    }
    if let Some(hi) = upperbound
        && cmp_versions(&parsed, &parse_version_segments(hi)) == std::cmp::Ordering::Greater
    {
        return false;
    }
    true
}

fn parse_version_segments(raw: &str) -> Vec<u64> {
    let cleaned = raw
        .trim()
        .trim_end_matches(".Final")
        .trim_end_matches(".RELEASE")
        .trim_end_matches("-SNAPSHOT")
        .trim_end_matches(".SNAPSHOT");
    cleaned
        .split(|c: char| !c.is_ascii_digit())
        .filter(|s| !s.is_empty())
        .filter_map(|s| s.parse::<u64>().ok())
        .collect()
}

fn cmp_versions(a: &[u64], b: &[u64]) -> std::cmp::Ordering {
    let len = a.len().max(b.len());
    for i in 0..len {
        let av = a.get(i).copied().unwrap_or(0);
        let bv = b.get(i).copied().unwrap_or(0);
        match av.cmp(&bv) {
            std::cmp::Ordering::Equal => {}
            other => return other,
        }
    }
    std::cmp::Ordering::Equal
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::EvalNode;

    fn dep(name: &str, version: Option<&str>) -> EvalNode {
        EvalNode {
            node_type: "Dependency".into(),
            name: name.into(),
            qualified_name: Some(format!("maven:{name}")),
            file_path: Some("pom.xml".into()),
            start_line: Some(10),
            version: version.map(str::to_string),
            ..Default::default()
        }
    }

    #[test]
    fn matches_maven_coordinate() {
        let nodes = vec![dep("io.quarkus:quarkus-core", None)];
        let v = eval_dependency(
            "r1",
            "maven",
            "io.quarkus:quarkus-core",
            None,
            None,
            None,
            &nodes,
        );
        assert_eq!(v.len(), 1);
    }

    #[test]
    fn matches_dot_kantra_against_colon_maven() {
        let nodes = vec![dep("org.jboss.bom:eap-runtime-artifacts", Some("7.4.0"))];
        let v = eval_dependency(
            "r1",
            "maven",
            "org.jboss.bom.eap-runtime-artifacts",
            None,
            None,
            None,
            &nodes,
        );
        assert_eq!(v.len(), 1);
    }

    #[test]
    fn version_bounds_enforced() {
        let nodes = vec![dep("org.foo:bar", Some("1.2.3"))];
        assert_eq!(
            eval_dependency(
                "r",
                "maven",
                "org.foo:bar",
                None,
                Some("1.0"),
                Some("2.0"),
                &nodes
            )
            .len(),
            1
        );
        assert!(
            eval_dependency("r", "maven", "org.foo:bar", None, Some("2.0"), None, &nodes)
                .is_empty()
        );
        assert!(
            eval_dependency("r", "maven", "org.foo:bar", None, None, Some("1.0"), &nodes)
                .is_empty()
        );
    }

    #[test]
    fn nameregex_accepts_dot_or_colon() {
        let nodes = vec![dep("org.jboss.bom:eap-runtime-artifacts", None)];
        let v = eval_dependency(
            "r1",
            "maven",
            "",
            Some("org.jboss.bom.eap-runtime-artifacts"),
            None,
            None,
            &nodes,
        );
        assert_eq!(v.len(), 1);
    }
}
