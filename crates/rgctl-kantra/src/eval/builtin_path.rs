//! `builtin.xml` XPath (widened subset) and `builtin.json` path checks.

use crate::eval::filecontent::SourceCache;
use crate::eval::{MatchSite, violation};
use crate::findings::KantraViolation;
use std::path::Path;

/// Evaluate `builtin.xml` against discovered XML files.
pub fn eval_builtin_xml(
    rule_id: &str,
    xpath: &str,
    file_pattern: Option<&str>,
    repo_root: &Path,
    files: &[std::path::PathBuf],
    sources: &SourceCache,
) -> Vec<KantraViolation> {
    let mut out = Vec::new();
    for path in files {
        let rel = path
            .strip_prefix(repo_root)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/");
        if !rel.ends_with(".xml") {
            continue;
        }
        if let Some(pat) = file_pattern
            && !rel.contains(pat.trim_matches('*'))
            && !glob_match(pat, &rel)
        {
            continue;
        }
        let text = if let Some(s) = sources.get(&rel) {
            s.as_str().to_string()
        } else if let Ok(bytes) = std::fs::read(path) {
            String::from_utf8_lossy(&bytes).into_owned()
        } else {
            continue;
        };
        let Ok(doc) = roxmltree::Document::parse(&text) else {
            continue;
        };
        if xpath_matches(&doc, xpath, None) {
            out.push(violation(rule_id, "builtin.xml", &MatchSite::new(rel, 1)));
        }
    }
    out
}

/// JSONPath-ish: `$.a.b` exact object path presence.
pub fn eval_builtin_json(
    rule_id: &str,
    jsonpath: &str,
    file_pattern: Option<&str>,
    repo_root: &Path,
    files: &[std::path::PathBuf],
    sources: &SourceCache,
) -> Vec<KantraViolation> {
    let mut out = Vec::new();
    for path in files {
        let rel = path
            .strip_prefix(repo_root)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/");
        if !rel.ends_with(".json") {
            continue;
        }
        if let Some(pat) = file_pattern
            && !glob_match(pat, &rel)
            && !rel.contains(pat.trim_matches('*'))
        {
            continue;
        }
        let text = if let Some(s) = sources.get(&rel) {
            s.as_str().to_string()
        } else if let Ok(bytes) = std::fs::read(path) {
            String::from_utf8_lossy(&bytes).into_owned()
        } else {
            continue;
        };
        let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) else {
            continue;
        };
        if json_path_exists(&v, jsonpath) {
            out.push(violation(rule_id, "builtin.json", &MatchSite::new(rel, 1)));
        }
    }
    out
}

fn glob_match(pat: &str, path: &str) -> bool {
    if let Ok(g) = glob::Pattern::new(pat) {
        return g.matches(path);
    }
    path.contains(pat.trim_matches('*'))
}

/// Return a skip reason when the XPath uses constructs we cannot evaluate.
pub fn unsupported_xpath_reason(xpath: &str) -> Option<String> {
    let xpath = xpath.trim();
    if xpath.is_empty() {
        return Some("empty xpath".into());
    }
    // Functions / axes beyond our supported subset.
    let unsupported_tokens = [
        "following::",
        "preceding::",
        "ancestor::",
        "descendant-or-self::",
        "parent::",
        "child::",
        "position(",
        "count(",
        "contains(",
        "starts-with(",
        "text(",
        "normalize-space(",
        "substring(",
        "not(",
    ];
    for tok in unsupported_tokens {
        if xpath.contains(tok) {
            return Some(format!("unsupported xpath expression: {xpath}"));
        }
    }
    // `namespace-uri()` and `local-name()` are supported; other `()` functions are not.
    let without_ns = xpath
        .replace("local-name()", "")
        .replace("namespace-uri()", "")
        .replace("boolean(", "")
        .replace(')', "");
    if without_ns.contains('(') {
        return Some(format!("unsupported xpath expression: {xpath}"));
    }
    None
}

/// Evaluate a Konveyor-style XPath subset against an XML document.
///
/// Supports:
/// - Union `|` (top-level)
/// - Descendant `//tag` / `//prefix:tag` / `//*`
/// - Rooted `/a/b`
/// - Attribute equality `[@attr='val']`
/// - `[local-name()='...' and boolean(namespace-uri()='...')]`
pub fn xpath_matches(
    doc: &roxmltree::Document<'_>,
    xpath: &str,
    _namespaces: Option<&std::collections::HashMap<String, String>>,
) -> bool {
    let xpath = xpath.trim();
    if unsupported_xpath_reason(xpath).is_some() {
        return false;
    }
    split_xpath_union(xpath)
        .into_iter()
        .any(|branch| eval_xpath_branch(doc, branch.trim()))
}

fn split_xpath_union(xpath: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut start = 0;
    let mut depth = 0usize;
    let mut in_quote: Option<char> = None;
    let chars: Vec<(usize, char)> = xpath.char_indices().collect();
    for &(i, c) in &chars {
        match (c, in_quote) {
            ('\'' | '"', None) => in_quote = Some(c),
            (q, Some(open)) if q == open => in_quote = None,
            ('[', None) => depth += 1,
            (']', None) if depth > 0 => depth -= 1,
            ('|', None) if depth == 0 => {
                parts.push(&xpath[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    parts.push(&xpath[start..]);
    parts
}

fn eval_xpath_branch(doc: &roxmltree::Document<'_>, branch: &str) -> bool {
    let branch = branch.trim();
    if branch.is_empty() {
        return false;
    }
    if let Some(rest) = branch.strip_prefix("//") {
        return match_descendant(doc, rest);
    }
    if let Some(rest) = branch.strip_prefix('/') {
        return match_rooted(doc, rest);
    }
    // Bare `tag` treated as descendant.
    match_descendant(doc, branch)
}

fn match_descendant(doc: &roxmltree::Document<'_>, expr: &str) -> bool {
    let (step, pred) = split_step_predicate(expr);
    let local = local_name_of_step(step);
    doc.descendants().any(|n| {
        if !n.is_element() {
            return false;
        }
        let name_ok = local == "*" || n.tag_name().name() == local;
        name_ok && predicate_matches(n, pred.as_deref(), doc)
    })
}

fn match_rooted(doc: &roxmltree::Document<'_>, path: &str) -> bool {
    let steps: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    if steps.is_empty() {
        return false;
    }
    fn walk<'a, 'input: 'a>(
        node: roxmltree::Node<'a, 'input>,
        steps: &[&str],
        doc: &roxmltree::Document<'input>,
    ) -> bool {
        if steps.is_empty() {
            return true;
        }
        let (step, pred) = split_step_predicate(steps[0]);
        let local = local_name_of_step(step);
        node.children().any(|child| {
            if !child.is_element() {
                return false;
            }
            let name_ok = local == "*" || child.tag_name().name() == local;
            name_ok
                && predicate_matches(child, pred.as_deref(), doc)
                && walk(child, &steps[1..], doc)
        })
    }
    let root = doc.root_element();
    let (first, pred) = split_step_predicate(steps[0]);
    let local = local_name_of_step(first);
    let name_ok = local == "*" || root.tag_name().name() == local;
    name_ok && predicate_matches(root, pred.as_deref(), doc) && walk(root, &steps[1..], doc)
}

fn split_step_predicate(step: &str) -> (&str, Option<String>) {
    if let Some(open) = step.find('[')
        && step.ends_with(']')
    {
        return (
            &step[..open],
            Some(step[open + 1..step.len() - 1].to_string()),
        );
    }
    (step, None)
}

fn local_name_of_step(step: &str) -> &str {
    // `s:decorate` → `decorate`; `*` stays `*`
    step.rsplit_once(':')
        .map(|(_, local)| local)
        .unwrap_or(step)
}

fn predicate_matches(
    node: roxmltree::Node<'_, '_>,
    pred: Option<&str>,
    _doc: &roxmltree::Document<'_>,
) -> bool {
    let Some(pred) = pred.map(str::trim).filter(|p| !p.is_empty()) else {
        return true;
    };
    if let Some(rest) = pred.strip_prefix('@') {
        // [@attr='val'] or [@attr="val"]
        let (attr, want) = if let Some((a, v)) = rest.split_once('=') {
            (a.trim(), unquote(v.trim()))
        } else {
            return node.has_attribute(rest.trim());
        };
        return node.attribute(attr) == Some(want.as_str());
    }
    // [local-name()='application' and boolean(namespace-uri()='http://...')]
    // Also supports predicates on `//*` where local-name filters the element.
    if pred.contains("local-name()") || pred.contains("namespace-uri()") {
        let want_local = extract_quoted_after(pred, "local-name()");
        let want_ns = extract_quoted_after(pred, "namespace-uri()");
        let local_ok = want_local
            .as_deref()
            .is_none_or(|w| node.tag_name().name() == w);
        let ns_ok = want_ns.as_deref().is_none_or(|w| {
            node.tag_name().namespace() == Some(w)
                || node
                    .attribute(("http://www.w3.org/2000/xmlns/", "xmlns"))
                    .or_else(|| node.attribute("xmlns"))
                    == Some(w)
        });
        return local_ok && ns_ok;
    }
    if let Some((child_expr, want_raw)) = pred.split_once('=')
        && !child_expr.contains('(')
    {
        let child_local = local_name_of_step(child_expr.trim());
        let want = unquote(want_raw.trim());
        return node.children().any(|c| {
            c.is_element()
                && c.tag_name().name() == child_local
                && c.text().map(str::trim) == Some(want.as_str())
        });
    }
    false
}

fn unquote(s: &str) -> String {
    let s = s.trim();
    if (s.starts_with('\'') && s.ends_with('\'')) || (s.starts_with('"') && s.ends_with('"')) {
        s[1..s.len() - 1].to_string()
    } else {
        s.to_string()
    }
}

fn extract_quoted_after(pred: &str, func: &str) -> Option<String> {
    let idx = pred.find(func)?;
    let after = &pred[idx + func.len()..];
    let eq = after.find('=')?;
    let rest = after[eq + 1..].trim_start();
    let quote = rest.chars().next()?;
    if quote != '\'' && quote != '"' {
        return None;
    }
    let end = rest[1..].find(quote)? + 1;
    Some(rest[1..end].to_string())
}

fn json_path_exists(v: &serde_json::Value, path: &str) -> bool {
    let path = path.trim().trim_start_matches('$').trim_start_matches('.');
    if path.is_empty() {
        return true;
    }
    let mut cur = v;
    for part in path.split('.') {
        match cur {
            serde_json::Value::Object(map) => {
                if let Some(next) = map.get(part) {
                    cur = next;
                } else {
                    return false;
                }
            }
            _ => return false,
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn descendant_and_rooted_and_union() {
        let xml = r#"<?xml version="1.0"?><jboss-web><context-root>/app</context-root><class-loading/></jboss-web>"#;
        let d = roxmltree::Document::parse(xml).unwrap();
        assert!(xpath_matches(&d, "/jboss-web/context-root", None));
        assert!(xpath_matches(&d, "//class-loading", None));
        assert!(xpath_matches(
            &d,
            "/jboss-web/context-root | //class-loading",
            None
        ));
        assert!(!xpath_matches(&d, "/jboss-web/missing", None));
    }

    #[test]
    fn namespace_prefixed_local_name() {
        let xml = r#"<?xml version="1.0"?><root xmlns:s="http://jboss.org/schema/seam/pages"><s:decorate/></root>"#;
        let d = roxmltree::Document::parse(xml).unwrap();
        assert!(xpath_matches(&d, "//s:decorate", None));
    }

    #[test]
    fn attr_and_namespace_uri_predicate() {
        let xml = r#"<?xml version="1.0"?><application xmlns="http://java.sun.com/xml/ns/javaee" version="6"><module/></application>"#;
        let d = roxmltree::Document::parse(xml).unwrap();
        assert!(xpath_matches(&d, "//*[@version='6']", None));
        assert!(xpath_matches(
            &d,
            "//*[local-name()='application' and boolean(namespace-uri()='http://java.sun.com/xml/ns/javaee')]",
            None
        ));
    }

    #[test]
    fn unsupported_xpath_detected() {
        assert!(unsupported_xpath_reason("//foo[position()=1]").is_some());
        assert!(unsupported_xpath_reason("//foo").is_none());
        assert!(
            unsupported_xpath_reason(
                "//*[local-name()='application' and boolean(namespace-uri()='http://x')]"
            )
            .is_none()
        );
    }
}
