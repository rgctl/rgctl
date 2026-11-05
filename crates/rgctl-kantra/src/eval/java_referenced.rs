//! `java.referenced` evaluation against graph nodes, edges, and parked call sites.

use crate::engine::{EvalCallSite, EvalEdge, EvalNode};
use crate::eval::filecontent::SourceCache;
use crate::eval::{MatchSite, compile_kantra_pattern, violation_high, violation_low};
use crate::findings::KantraViolation;
use regex::Regex;
use std::collections::{HashMap, HashSet};

const CODE_EXTENSIONS: &[&str] = &[".java", ".kt", ".scala"];

/// Graph + source inputs for `java.referenced` evaluation.
pub struct JavaReferencedInput<'a> {
    pub nodes: &'a [EvalNode],
    pub edges: &'a [EvalEdge],
    pub call_sites: &'a [EvalCallSite],
    pub sources: &'a SourceCache,
}

/// Evaluate Java referenced rules against graph nodes, edges, and call sites.
pub fn eval_java_referenced(
    rule_id: &str,
    pattern: &str,
    location: Option<&str>,
    annotated_pattern: Option<&str>,
    input: &JavaReferencedInput<'_>,
) -> Result<Vec<KantraViolation>, regex::Error> {
    let re = compile_kantra_pattern(pattern)?;
    let annotated_re = match annotated_pattern {
        Some(p) if !p.is_empty() => Some(compile_kantra_pattern(p)?),
        _ => None,
    };
    let wildcards = wildcard_imports(input.nodes);
    let ctx = LocationCtx {
        rule_id,
        re: &re,
        annotated_re: annotated_re.as_ref(),
        input,
        wildcards: &wildcards,
    };

    let locs: Vec<&str> = match location {
        None => vec![
            "IMPORT",
            "TYPE",
            "PACKAGE",
            "INHERITANCE",
            "IMPLEMENTS_TYPE",
            "ANNOTATION",
            "CONSTRUCTOR_CALL",
            "METHOD_CALL",
            "METHOD",
            "FIELD_DECLARATION",
            "RETURN_TYPE",
            "THROWS_METHOD_DECLARATION",
            "VARIABLE_DECLARATION",
        ],
        Some(loc) => vec![loc],
    };

    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for loc in locs {
        let loc_up = loc.to_ascii_uppercase();
        let hits = eval_location(&ctx, &loc_up)?;
        for hit in hits {
            if seen.insert((hit.file.clone(), hit.line, hit.matched_location.clone())) {
                out.push(hit);
            }
        }
    }
    Ok(out)
}

struct LocationCtx<'a> {
    rule_id: &'a str,
    re: &'a Regex,
    annotated_re: Option<&'a Regex>,
    input: &'a JavaReferencedInput<'a>,
    wildcards: &'a HashMap<String, Vec<String>>,
}

fn eval_location(ctx: &LocationCtx<'_>, loc: &str) -> Result<Vec<KantraViolation>, regex::Error> {
    let LocationCtx {
        rule_id,
        re,
        annotated_re,
        input,
        wildcards,
    } = ctx;
    let nodes = input.nodes;
    let edges = input.edges;
    let call_sites = input.call_sites;
    let sources = input.sources;
    let mut out = Vec::new();
    match loc {
        "IMPORT" => {
            for node in nodes {
                if node.node_type != "Import" || !is_code_file(node.file_path.as_deref()) {
                    continue;
                }
                if re.is_match(&node.name)
                    || node
                        .qualified_name
                        .as_deref()
                        .is_some_and(|qn| re.is_match(qn))
                {
                    push_node_hit(&mut out, rule_id, node, "IMPORT");
                }
            }
        }
        "TYPE" | "CLASS" => {
            for node in nodes {
                if !matches!(node.node_type.as_str(), "Class" | "Interface" | "Enum") {
                    continue;
                }
                if !is_code_file(node.file_path.as_deref()) {
                    continue;
                }
                let qn = node.qualified_name.as_deref().unwrap_or(&node.name);
                if re.is_match(qn) || re.is_match(&node.name) {
                    push_node_hit(&mut out, rule_id, node, loc);
                }
            }
        }
        "PACKAGE" => {
            for node in nodes {
                if !is_code_file(node.file_path.as_deref()) {
                    continue;
                }
                let candidates: Vec<&str> = match node.node_type.as_str() {
                    "Import" => vec![node.name.as_str()],
                    "Class" | "Interface" | "Enum" => {
                        let mut v = vec![node.name.as_str()];
                        if let Some(qn) = node.qualified_name.as_deref() {
                            v.push(qn);
                        }
                        v
                    }
                    _ => continue,
                };
                if candidates.iter().any(|c| re.is_match(c)) {
                    push_node_hit(&mut out, rule_id, node, "PACKAGE");
                }
            }
            for site in call_sites {
                if !is_code_file(Some(&site.file_path)) {
                    continue;
                }
                if site
                    .to_qualified_hint
                    .as_deref()
                    .is_some_and(|qn| re.is_match(qn))
                {
                    push_call_site_hit(&mut out, rule_id, site, "PACKAGE");
                }
            }
        }
        "INHERITANCE" | "IMPLEMENTS_TYPE" => {
            let want = if loc == "INHERITANCE" {
                "EXTENDS"
            } else {
                "IMPLEMENTS"
            };
            for edge in edges {
                if edge.edge_type != want {
                    continue;
                }
                if type_matches_with_wildcards(
                    re,
                    &edge.to_qualified,
                    Some(&edge.file_path),
                    wildcards,
                ) || type_matches_with_wildcards(
                    re,
                    &edge.to_name,
                    Some(&edge.file_path),
                    wildcards,
                ) {
                    out.push(violation_high(
                        rule_id,
                        "java.referenced",
                        &MatchSite::new(edge.file_path.clone(), edge.line)
                            .with_symbol(&edge.from_name),
                        loc,
                    ));
                }
            }
        }
        "ANNOTATION" => {
            for edge in edges {
                if edge.edge_type != "ANNOTATED_WITH" {
                    continue;
                }
                let ann_match = type_matches_with_wildcards(
                    re,
                    &edge.to_qualified,
                    Some(&edge.file_path),
                    wildcards,
                ) || type_matches_with_wildcards(
                    re,
                    &edge.to_name,
                    Some(&edge.file_path),
                    wildcards,
                );
                if !ann_match {
                    continue;
                }
                if let Some(are) = annotated_re
                    && !(type_matches_with_wildcards(
                        are,
                        &edge.to_qualified,
                        Some(&edge.file_path),
                        wildcards,
                    ) || type_matches_with_wildcards(
                        are,
                        &edge.to_name,
                        Some(&edge.file_path),
                        wildcards,
                    ) || are.is_match(&edge.from_qualified)
                        || are.is_match(&edge.from_name))
                {
                    continue;
                }
                out.push(violation_high(
                    rule_id,
                    "java.referenced",
                    &MatchSite::new(edge.file_path.clone(), edge.line).with_symbol(&edge.from_name),
                    "ANNOTATION",
                ));
            }
            for node in nodes {
                if node.node_type != "Annotation" || !is_code_file(node.file_path.as_deref()) {
                    continue;
                }
                let qn = node.qualified_name.as_deref().unwrap_or(&node.name);
                if !(re.is_match(qn) || re.is_match(&node.name)) {
                    continue;
                }
                if let Some(are) = annotated_re
                    && !(are.is_match(qn)
                        || are.is_match(&node.name)
                        || node.annotations.iter().any(|a| are.is_match(a)))
                {
                    continue;
                }
                push_node_hit(&mut out, rule_id, node, "ANNOTATION");
            }
        }
        "CONSTRUCTOR_CALL" => {
            let mut matched_in_file: HashSet<(&str, &str)> = HashSet::new();
            for site in call_sites {
                if site.relation_type != "Instantiates" || !is_code_file(Some(&site.file_path)) {
                    continue;
                }
                if call_site_candidates(site, wildcards)
                    .iter()
                    .any(|c| re.is_match(c))
                {
                    matched_in_file.insert((site.file_path.as_str(), site.to_name.as_str()));
                    if let Some(qn) = site.to_qualified_hint.as_deref() {
                        matched_in_file.insert((site.file_path.as_str(), qn));
                    }
                    push_call_site_hit(&mut out, rule_id, site, "CONSTRUCTOR_CALL");
                }
            }
            for edge in edges {
                if edge.edge_type != "INSTANTIATES" {
                    continue;
                }
                if matched_in_file.contains(&(edge.file_path.as_str(), edge.to_name.as_str()))
                    || matched_in_file
                        .contains(&(edge.file_path.as_str(), edge.to_qualified.as_str()))
                {
                    continue;
                }
                if type_matches_with_wildcards(
                    re,
                    &edge.to_qualified,
                    Some(&edge.file_path),
                    wildcards,
                ) || type_matches_with_wildcards(
                    re,
                    &edge.to_name,
                    Some(&edge.file_path),
                    wildcards,
                ) {
                    out.push(violation_high(
                        rule_id,
                        "java.referenced",
                        &MatchSite::new(edge.file_path.clone(), edge.line)
                            .with_symbol(&edge.to_qualified),
                        "CONSTRUCTOR_CALL",
                    ));
                }
            }
        }
        "METHOD_CALL" => {
            let mut matched_in_file: HashSet<(&str, &str)> = HashSet::new();
            for site in call_sites {
                if site.relation_type != "Calls" || !is_code_file(Some(&site.file_path)) {
                    continue;
                }
                if call_site_candidates(site, wildcards)
                    .iter()
                    .any(|c| re.is_match(c))
                {
                    matched_in_file.insert((site.file_path.as_str(), site.to_name.as_str()));
                    if let Some(qn) = site.to_qualified_hint.as_deref() {
                        matched_in_file.insert((site.file_path.as_str(), qn));
                    }
                    push_call_site_hit(&mut out, rule_id, site, "METHOD_CALL");
                }
            }
            for edge in edges {
                if edge.edge_type != "CALLS" {
                    continue;
                }
                if matched_in_file.contains(&(edge.file_path.as_str(), edge.to_name.as_str()))
                    || matched_in_file
                        .contains(&(edge.file_path.as_str(), edge.to_qualified.as_str()))
                {
                    continue;
                }
                if re.is_match(&edge.to_qualified) || re.is_match(&edge.to_name) {
                    out.push(violation_high(
                        rule_id,
                        "java.referenced",
                        &MatchSite::new(edge.file_path.clone(), edge.line)
                            .with_symbol(&edge.to_qualified),
                        "METHOD_CALL",
                    ));
                }
            }
        }
        "METHOD" => {
            for node in nodes {
                if node.node_type != "Function" || !is_code_file(node.file_path.as_deref()) {
                    continue;
                }
                let qn = node.qualified_name.as_deref().unwrap_or(&node.name);
                if re.is_match(qn) || re.is_match(&node.name) {
                    push_node_hit(&mut out, rule_id, node, "METHOD");
                }
            }
        }
        "FIELD_DECLARATION" | "FIELD" => {
            for node in nodes {
                if !is_code_file(node.file_path.as_deref()) {
                    continue;
                }
                let file = node.file_path.as_deref();
                if node.node_type == "Variable" {
                    let qn = node.qualified_name.as_deref().unwrap_or(&node.name);
                    if re.is_match(qn)
                        || re.is_match(&node.name)
                        || node
                            .field_type_fqns
                            .iter()
                            .any(|t| type_matches_with_wildcards(re, t, file, wildcards))
                    {
                        push_node_hit(&mut out, rule_id, node, loc);
                    }
                } else if matches!(node.node_type.as_str(), "Class" | "Interface" | "Enum")
                    && node
                        .field_type_fqns
                        .iter()
                        .any(|t| type_matches_with_wildcards(re, t, file, wildcards))
                {
                    push_node_hit(&mut out, rule_id, node, loc);
                }
            }
        }
        "RETURN_TYPE" => {
            for node in nodes {
                if node.node_type != "Function" || !is_code_file(node.file_path.as_deref()) {
                    continue;
                }
                if node.return_type_fqn.as_deref().is_some_and(|t| {
                    type_matches_with_wildcards(re, t, node.file_path.as_deref(), wildcards)
                }) {
                    push_node_hit(&mut out, rule_id, node, "RETURN_TYPE");
                }
            }
        }
        "THROWS_METHOD_DECLARATION" => {
            for node in nodes {
                if node.node_type != "Function" || !is_code_file(node.file_path.as_deref()) {
                    continue;
                }
                let file = node.file_path.as_deref();
                if node
                    .throws
                    .iter()
                    .any(|t| type_matches_with_wildcards(re, t, file, wildcards))
                {
                    push_node_hit(&mut out, rule_id, node, "THROWS_METHOD_DECLARATION");
                }
            }
        }
        "VARIABLE_DECLARATION" | "METHOD_PARAMETER" => {
            for node in nodes {
                if !is_code_file(node.file_path.as_deref()) {
                    continue;
                }
                let file = node.file_path.as_deref();
                if node.node_type == "Variable" {
                    let qn = node.qualified_name.as_deref().unwrap_or(&node.name);
                    if re.is_match(qn)
                        || re.is_match(&node.name)
                        || node
                            .param_type_fqns
                            .iter()
                            .any(|t| type_matches_with_wildcards(re, t, file, wildcards))
                        || node
                            .field_type_fqns
                            .iter()
                            .any(|t| type_matches_with_wildcards(re, t, file, wildcards))
                    {
                        push_node_hit(&mut out, rule_id, node, loc);
                    }
                } else if node.node_type == "Function"
                    && node
                        .param_type_fqns
                        .iter()
                        .any(|t| type_matches_with_wildcards(re, t, file, wildcards))
                {
                    push_node_hit(&mut out, rule_id, node, loc);
                }
            }
        }
        _ => {
            // Residual unknown location: low-confidence lexical fallback only.
            out.extend(filecontent_fallback(rule_id, re, sources, loc)?);
        }
    }
    Ok(out)
}

fn type_matches_with_wildcards(
    re: &Regex,
    raw_type: &str,
    file: Option<&str>,
    wildcards: &HashMap<String, Vec<String>>,
) -> bool {
    if re.is_match(raw_type) {
        return true;
    }
    let Some(file) = file else {
        return false;
    };
    let Some(pkgs) = wildcards.get(file) else {
        return false;
    };
    let simple = raw_type.rsplit('.').next().unwrap_or(raw_type);
    pkgs.iter()
        .any(|pkg| re.is_match(&format!("{pkg}.{simple}")))
}

fn wildcard_imports(nodes: &[EvalNode]) -> HashMap<String, Vec<String>> {
    let mut map: HashMap<String, Vec<String>> = HashMap::new();
    for node in nodes {
        if node.node_type != "Import" {
            continue;
        }
        let Some(file) = node.file_path.as_ref() else {
            continue;
        };
        let mut name = node.name.trim().trim_end_matches(';').trim();
        for prefix in ["import static ", "import "] {
            if let Some(rest) = name.strip_prefix(prefix) {
                name = rest.trim();
                break;
            }
        }
        if let Some(pkg) = name.strip_suffix(".*") {
            map.entry(file.clone()).or_default().push(pkg.to_string());
        }
    }
    map
}

fn call_site_candidates(
    site: &EvalCallSite,
    wildcards: &HashMap<String, Vec<String>>,
) -> Vec<String> {
    let mut out = Vec::new();
    let callee_name = site.to_name.rsplit('.').next().unwrap_or(&site.to_name);
    out.push(site.to_name.clone());
    if callee_name != site.to_name {
        out.push(callee_name.to_string());
    }
    if let Some(qn) = &site.to_qualified_hint {
        out.push(qn.clone());
    }
    if let Some(module) = &site.import_module_hint {
        out.push(format!("{module}.{}", site.to_name));
        out.push(format!("{module}.{callee_name}"));
        if let Some(hint) = &site.to_type_hint {
            let simple_hint = hint.rsplit('.').next().unwrap_or(hint);
            out.push(format!("{module}.{simple_hint}.{callee_name}"));
            out.push(format!("{module}.{simple_hint}"));
        }
    }
    if let Some(hint) = &site.to_type_hint {
        let simple_hint = hint.rsplit('.').next().unwrap_or(hint);
        out.push(format!("{hint}.{callee_name}"));
        out.push(format!("{simple_hint}.{callee_name}"));
        out.push(hint.clone());
        out.push(simple_hint.to_string());
        if let Some(pkgs) = wildcards.get(&site.file_path) {
            for pkg in pkgs {
                out.push(format!("{pkg}.{simple_hint}.{callee_name}"));
                out.push(format!("{pkg}.{simple_hint}"));
            }
        }
    }
    if let Some(pkgs) = wildcards.get(&site.file_path) {
        for pkg in pkgs {
            out.push(format!("{pkg}.{callee_name}"));
        }
    }
    out
}

fn push_node_hit(out: &mut Vec<KantraViolation>, rule_id: &str, node: &EvalNode, loc: &str) {
    let file = node.file_path.clone().unwrap_or_default();
    let line = node.start_line.unwrap_or(1);
    let sym = node.qualified_name.as_deref().unwrap_or(&node.name);
    out.push(violation_high(
        rule_id,
        "java.referenced",
        &MatchSite::new(file, line).with_symbol(sym),
        loc,
    ));
}

fn push_call_site_hit(
    out: &mut Vec<KantraViolation>,
    rule_id: &str,
    site: &EvalCallSite,
    loc: &str,
) {
    let sym = site
        .to_qualified_hint
        .clone()
        .unwrap_or_else(|| site.to_name.clone());
    out.push(violation_high(
        rule_id,
        "java.referenced",
        &MatchSite::new(site.file_path.clone(), site.line).with_symbol(sym),
        loc,
    ));
}

fn filecontent_fallback(
    rule_id: &str,
    re: &Regex,
    sources: &SourceCache,
    loc: &str,
) -> Result<Vec<KantraViolation>, regex::Error> {
    let mut out = Vec::new();
    for (path, content) in sources {
        if !is_code_file(Some(path)) {
            continue;
        }
        for (i, line) in content.lines().enumerate() {
            if re.is_match(line) {
                out.push(violation_low(
                    rule_id,
                    "java.referenced",
                    &MatchSite::new(path.clone(), i + 1),
                    loc,
                ));
            }
        }
    }
    Ok(out)
}

fn is_code_file(path: Option<&str>) -> bool {
    path.map(|p| CODE_EXTENSIONS.iter().any(|ext| p.ends_with(ext)))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::{EvalCallSite, EvalEdge, EvalNode};
    use std::sync::Arc;
    use uuid::Uuid;

    fn import_node(file: &str, line: usize, name: &str) -> EvalNode {
        EvalNode {
            node_type: "Import".into(),
            name: name.into(),
            qualified_name: Some(name.into()),
            file_path: Some(file.into()),
            start_line: Some(line),
            ..Default::default()
        }
    }

    #[test]
    fn formatter_scanner_pattern_jr6() {
        let nodes = vec![
            import_node("A.java", 1, "java.util.Scanner"),
            import_node("B.java", 1, "java.util.*"),
            import_node("C.java", 1, "java.util.List"),
            import_node("D.java", 1, "java.util.Formatter"),
        ];
        let hits = eval_java_referenced(
            "r1",
            "java.util.(Formatter|Scanner)*",
            Some("IMPORT"),
            None,
            &JavaReferencedInput {
                nodes: &nodes,
                edges: &[],
                call_sites: &[],
                sources: &SourceCache::new(),
            },
        )
        .unwrap();
        let files: HashSet<_> = hits.iter().map(|v| v.file.as_str()).collect();
        assert!(files.contains("A.java"));
        assert!(files.contains("D.java"));
        assert!(!files.contains("B.java"));
        assert!(!files.contains("C.java"));
    }

    #[test]
    fn leading_star_and_persistence_patterns() {
        let nodes = vec![
            import_node("E.java", 1, "javax.persistence.EntityManager"),
            import_node("Q.java", 1, "javax.persistence.Query"),
        ];
        let em = eval_java_referenced(
            "r1",
            "*EntityManager",
            Some("IMPORT"),
            None,
            &JavaReferencedInput {
                nodes: &nodes,
                edges: &[],
                call_sites: &[],
                sources: &SourceCache::new(),
            },
        )
        .unwrap();
        assert_eq!(em.len(), 1);
        let pers = eval_java_referenced(
            "r2",
            "*.persistence.*",
            Some("IMPORT"),
            None,
            &JavaReferencedInput {
                nodes: &nodes,
                edges: &[],
                call_sites: &[],
                sources: &SourceCache::new(),
            },
        )
        .unwrap();
        assert_eq!(pers.len(), 2);
    }

    #[test]
    fn classloader_boundary_jr5() {
        let nodes = vec![
            import_node("Ok.java", 1, "java.lang.ClassLoader"),
            import_node("Bad.java", 1, "java.net.URLClassLoader"),
        ];
        let hits = eval_java_referenced(
            "r1",
            "ClassLoader",
            Some("IMPORT"),
            None,
            &JavaReferencedInput {
                nodes: &nodes,
                edges: &[],
                call_sites: &[],
                sources: &SourceCache::new(),
            },
        )
        .unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].file, "Ok.java");
    }

    #[test]
    fn constructor_call_uses_call_site_line() {
        let site = EvalCallSite {
            caller_id: Uuid::new_v4(),
            file_path: "Net.java".into(),
            line: 42,
            relation_type: "Instantiates".into(),
            to_name: "URL".into(),
            to_qualified_hint: Some("java.net.URL".into()),
            to_type_hint: None,
            import_module_hint: Some("java.net".into()),
        };
        let hits = eval_java_referenced(
            "r1",
            "java.net.URL*",
            Some("CONSTRUCTOR_CALL"),
            None,
            &JavaReferencedInput {
                nodes: &[],
                edges: &[],
                call_sites: &[site],
                sources: &SourceCache::new(),
            },
        )
        .unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].line, 42);
        assert_eq!(hits[0].confidence.as_deref(), Some("high"));
        assert_eq!(
            hits[0].matched_location.as_deref(),
            Some("CONSTRUCTOR_CALL")
        );
    }

    #[test]
    fn method_call_expands_wildcard_import() {
        let nodes = vec![import_node("S.java", 1, "javax.servlet.http.*")];
        let site = EvalCallSite {
            caller_id: Uuid::new_v4(),
            file_path: "S.java".into(),
            line: 17,
            relation_type: "Calls".into(),
            to_name: "setAttribute".into(),
            to_qualified_hint: None,
            to_type_hint: Some("HttpSession".into()),
            import_module_hint: None,
        };
        let hits = eval_java_referenced(
            "r1",
            "javax.servlet.http.HttpSession.setAttribute*",
            Some("METHOD_CALL"),
            None,
            &JavaReferencedInput {
                nodes: &nodes,
                edges: &[],
                call_sites: &[site],
                sources: &SourceCache::new(),
            },
        )
        .unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].line, 17);
        assert_eq!(hits[0].confidence.as_deref(), Some("high"));
    }

    #[test]
    fn package_matches_import_not_comment_grep() {
        let mut sources = SourceCache::new();
        sources.insert(
            "X.java".into(),
            Arc::new("// org.jboss.ejb3 is mentioned in a comment\n".into()),
        );
        let nodes = vec![import_node(
            "Y.java",
            2,
            "org.jboss.ejb3.annotation.SecurityDomain",
        )];
        let hits = eval_java_referenced(
            "r1",
            "org.jboss.ejb3*",
            Some("PACKAGE"),
            None,
            &JavaReferencedInput {
                nodes: &nodes,
                edges: &[],
                call_sites: &[],
                sources: &sources,
            },
        )
        .unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].file, "Y.java");
        assert_eq!(hits[0].confidence.as_deref(), Some("high"));
    }

    #[test]
    fn symbol_metadata_locations() {
        let field = EvalNode {
            node_type: "Variable".into(),
            name: "em".into(),
            file_path: Some("F.java".into()),
            start_line: Some(5),
            field_type_fqns: vec!["javax.persistence.EntityManager".into()],
            ..Default::default()
        };
        let func = EvalNode {
            node_type: "Function".into(),
            name: "run".into(),
            file_path: Some("F.java".into()),
            start_line: Some(10),
            return_type_fqn: Some("javax.persistence.Query".into()),
            throws: vec!["javax.persistence.PersistenceException".into()],
            param_type_fqns: vec!["javax.persistence.EntityManager".into()],
            ..Default::default()
        };
        let nodes = vec![field, func];
        for (loc, pattern) in [
            ("FIELD_DECLARATION", "javax.persistence.EntityManager*"),
            ("RETURN_TYPE", "javax.persistence.Query*"),
            (
                "THROWS_METHOD_DECLARATION",
                "javax.persistence.PersistenceException*",
            ),
            ("VARIABLE_DECLARATION", "javax.persistence.EntityManager*"),
        ] {
            let hits = eval_java_referenced(
                "r",
                pattern,
                Some(loc),
                None,
                &JavaReferencedInput {
                    nodes: &nodes,
                    edges: &[],
                    call_sites: &[],
                    sources: &SourceCache::new(),
                },
            )
            .unwrap();
            assert!(!hits.is_empty(), "expected hits for {loc}");
            assert_eq!(hits[0].confidence.as_deref(), Some("high"));
        }
    }

    #[test]
    fn annotated_pattern_filters_annotation_hits() {
        let edge = EvalEdge {
            edge_type: "ANNOTATED_WITH".into(),
            from_name: "Bean".into(),
            from_qualified: "com.ex.Bean".into(),
            to_name: "Stateless".into(),
            to_qualified: "javax.ejb.Stateless".into(),
            file_path: "Bean.java".into(),
            line: 3,
        };
        let hits = eval_java_referenced(
            "r1",
            "javax.ejb.Stateless*",
            Some("ANNOTATION"),
            Some("javax.ejb.Stateless*"),
            &JavaReferencedInput {
                nodes: &[],
                edges: &[edge.clone()],
                call_sites: &[],
                sources: &SourceCache::new(),
            },
        )
        .unwrap();
        assert_eq!(hits.len(), 1);

        let missed = eval_java_referenced(
            "r1",
            "javax.ejb.Stateless*",
            Some("ANNOTATION"),
            Some("org.springframework.*"),
            &JavaReferencedInput {
                nodes: &[],
                edges: &[edge],
                call_sites: &[],
                sources: &SourceCache::new(),
            },
        )
        .unwrap();
        assert!(missed.is_empty());
    }

    #[test]
    fn constructor_call_dedupes_edge_when_call_site_present_jr4() {
        let site = EvalCallSite {
            caller_id: Uuid::new_v4(),
            file_path: "Net.java".into(),
            line: 42,
            relation_type: "Instantiates".into(),
            to_name: "URL".into(),
            to_qualified_hint: Some("java.net.URL".into()),
            to_type_hint: Some("URL".into()),
            import_module_hint: Some("java.net.URL".into()),
        };
        let edge = EvalEdge {
            edge_type: "INSTANTIATES".into(),
            from_name: "connect".into(),
            from_qualified: "com.acme.Net.connect".into(),
            to_name: "URL".into(),
            to_qualified: "java.net.URL".into(),
            file_path: "Net.java".into(),
            line: 10, // enclosing method start line
        };
        let hits = eval_java_referenced(
            "r1",
            "java.net.URL*",
            Some("CONSTRUCTOR_CALL"),
            None,
            &JavaReferencedInput {
                nodes: &[],
                edges: &[edge],
                call_sites: &[site],
                sources: &SourceCache::new(),
            },
        )
        .unwrap();
        assert_eq!(
            hits.len(),
            1,
            "must not emit duplicate hit at method start line"
        );
        assert_eq!(hits[0].line, 42);
    }

    #[test]
    fn wildcard_import_with_import_keyword_and_package_qualified_hint_jr3() {
        let nodes = vec![import_node("S.java", 1, "import javax.servlet.http.*;")];
        let site = EvalCallSite {
            caller_id: Uuid::new_v4(),
            file_path: "S.java".into(),
            line: 17,
            relation_type: "Calls".into(),
            to_name: "setAttribute".into(),
            to_qualified_hint: Some("com.acme.HttpSession.setAttribute".into()),
            to_type_hint: Some("com.acme.HttpSession".into()),
            import_module_hint: Some("javax.servlet.http".into()),
        };
        let hits = eval_java_referenced(
            "r1",
            "javax.servlet.http.HttpSession.setAttribute*",
            Some("METHOD_CALL"),
            None,
            &JavaReferencedInput {
                nodes: &nodes,
                edges: &[],
                call_sites: &[site],
                sources: &SourceCache::new(),
            },
        )
        .unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].line, 17);
    }

    #[test]
    fn inheritance_does_not_match_subclass_own_name_jr5() {
        let edge = EvalEdge {
            edge_type: "IMPLEMENTS".into(),
            from_name: "StatefulBean".into(),
            from_qualified: "com.acme.StatefulBean".into(),
            to_name: "Serializable".into(),
            to_qualified: "java.io.Serializable".into(),
            file_path: "StatefulBean.java".into(),
            line: 5,
        };
        let hits = eval_java_referenced(
            "r1",
            "*Stateful*",
            Some("IMPLEMENTS_TYPE"),
            None,
            &JavaReferencedInput {
                nodes: &[],
                edges: &[edge],
                call_sites: &[],
                sources: &SourceCache::new(),
            },
        )
        .unwrap();
        assert!(
            hits.is_empty(),
            "subclass name must not match IMPLEMENTS_TYPE pattern"
        );
    }
}
