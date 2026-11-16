//! Maven `pom.xml` → DependencyDeclaration.

use super::{DependencyDeclaration, loc};
use std::collections::HashMap;
use std::path::Path;

pub fn extract(path: &Path, source: &[u8]) -> Vec<DependencyDeclaration> {
    let Ok(text) = std::str::from_utf8(source) else {
        return Vec::new();
    };
    let Ok(doc) = roxmltree::Document::parse(text) else {
        return Vec::new();
    };

    let root = doc.root_element();
    let mut props: HashMap<String, String> = HashMap::new();
    // Seed built-in Maven coordinates from <parent> then <project> (task 3.1).
    if let Some(parent_el) = root
        .children()
        .find(|n| n.is_element() && n.tag_name().name() == "parent")
    {
        if let Some(pv) = child_text(parent_el, "version") {
            props.insert("project.version".into(), pv.clone());
            props.insert("pom.version".into(), pv.clone());
            props.insert("project.parent.version".into(), pv);
        }
        if let Some(pg) = child_text(parent_el, "groupId") {
            props.insert("project.groupId".into(), pg.clone());
            props.insert("pom.groupId".into(), pg.clone());
            props.insert("project.parent.groupId".into(), pg);
        }
    }
    if let Some(v) = child_text(root, "version") {
        props.insert("project.version".into(), v.clone());
        props.insert("pom.version".into(), v);
    }
    if let Some(g) = child_text(root, "groupId") {
        props.insert("project.groupId".into(), g.clone());
        props.insert("pom.groupId".into(), g);
    }
    if let Some(props_el) = root
        .children()
        .find(|n| n.is_element() && n.tag_name().name() == "properties")
        .or_else(|| find_child_deep(root, "properties"))
    {
        for child in props_el.children().filter(|n| n.is_element()) {
            let name = child.tag_name().name().to_string();
            if let Some(val) = child.text().map(str::trim).filter(|s| !s.is_empty()) {
                props.insert(name, val.to_string());
            }
        }
    }

    let mut out = Vec::new();
    collect_deps(root, path, &props, &mut out, /*in_dep_mgmt*/ false);
    out
}

fn collect_deps(
    node: roxmltree::Node<'_, '_>,
    path: &Path,
    props: &HashMap<String, String>,
    out: &mut Vec<DependencyDeclaration>,
    in_dep_mgmt: bool,
) {
    let tag = node.tag_name().name();
    let next_mgmt = in_dep_mgmt || tag == "dependencyManagement";

    if tag == "dependency" {
        let group = child_text(node, "groupId");
        let artifact = child_text(node, "artifactId");
        let version_raw = child_text(node, "version");
        let scope = child_text(node, "scope");
        let optional = child_text(node, "optional").as_deref() == Some("true");
        let typ = child_text(node, "type");

        if let (Some(g), Some(a)) = (group, artifact) {
            let g = resolve_props(&g, props);
            let a = resolve_props(&a, props);
            let version = version_raw.map(|v| resolve_props(&v, props));
            let unresolved = version.as_ref().is_some_and(|v| v.contains("${"));
            let mut scope = scope.unwrap_or_else(|| {
                if next_mgmt && typ.as_deref() == Some("pom") {
                    "import".to_string()
                } else if next_mgmt {
                    "dependencyManagement".to_string()
                } else {
                    "compile".to_string()
                }
            });
            if typ.as_deref() == Some("pom") && scope != "import" {
                // keep
                let _ = &mut scope;
            }
            let line = node.document().text_pos_at(node.range().start).row as usize;
            out.push(DependencyDeclaration {
                name: format!("{g}:{a}"),
                version_requirement: version,
                scope: Some(scope),
                ecosystem: "maven".to_string(),
                location: loc(path, line, line),
                optional,
                unresolved,
            });
        }
        return;
    }

    for child in node.children().filter(|n| n.is_element()) {
        collect_deps(child, path, props, out, next_mgmt);
    }
}

fn find_child_deep<'a, 'input>(
    node: roxmltree::Node<'a, 'input>,
    name: &str,
) -> Option<roxmltree::Node<'a, 'input>> {
    if node.is_element() && node.tag_name().name() == name {
        return Some(node);
    }
    for child in node.children() {
        if let Some(found) = find_child_deep(child, name) {
            return Some(found);
        }
    }
    None
}

fn child_text(node: roxmltree::Node<'_, '_>, name: &str) -> Option<String> {
    node.children()
        .find(|c| c.is_element() && c.tag_name().name() == name)
        .and_then(|c| c.text().map(|t| t.trim().to_string()))
        .filter(|s| !s.is_empty())
}

fn resolve_props(s: &str, props: &HashMap<String, String>) -> String {
    let mut out = s.to_string();
    // Bounded multi-pass ${key} substitution so chained properties (`<a>1.0</a><b>${a}</b>`) resolve.
    for _ in 0..4 {
        if !out.contains("${") {
            break;
        }
        let prev = out.clone();
        for (k, v) in props {
            let needle = format!("${{{k}}}");
            if out.contains(&needle) {
                out = out.replace(&needle, v);
            }
        }
        if out == prev {
            break;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quarkus_style_pom() {
        let src = r#"
<project>
  <properties>
    <quarkus.platform.version>2.16.12.Final</quarkus.platform.version>
  </properties>
  <dependencyManagement>
    <dependencies>
      <dependency>
        <groupId>io.quarkus</groupId>
        <artifactId>quarkus-bom</artifactId>
        <version>${quarkus.platform.version}</version>
        <type>pom</type>
        <scope>import</scope>
      </dependency>
    </dependencies>
  </dependencyManagement>
  <dependencies>
    <dependency>
      <groupId>io.quarkus</groupId>
      <artifactId>quarkus-hibernate-orm</artifactId>
    </dependency>
  </dependencies>
</project>
"#;
        let decls = extract(Path::new("pom.xml"), src.as_bytes());
        assert!(
            decls
                .iter()
                .any(|d| d.name == "io.quarkus:quarkus-hibernate-orm")
        );
        let bom = decls
            .iter()
            .find(|d| d.name == "io.quarkus:quarkus-bom")
            .unwrap();
        assert_eq!(bom.version_requirement.as_deref(), Some("2.16.12.Final"));
        assert_eq!(bom.scope.as_deref(), Some("import"));
    }

    #[test]
    fn interpolates_project_parent_and_chained_properties() {
        let src = r#"
<project>
  <parent>
    <groupId>com.example</groupId>
    <artifactId>parent-pom</artifactId>
    <version>3.1.4</version>
  </parent>
  <artifactId>child</artifactId>
  <properties>
    <base.jackson>2.13.5</base.jackson>
    <jackson.version>${base.jackson}</jackson.version>
  </properties>
  <dependencies>
    <dependency>
      <groupId>${project.groupId}</groupId>
      <artifactId>sibling-lib</artifactId>
      <version>${project.version}</version>
    </dependency>
    <dependency>
      <groupId>com.fasterxml.jackson.core</groupId>
      <artifactId>jackson-databind</artifactId>
      <version>${jackson.version}</version>
    </dependency>
  </dependencies>
</project>
"#;
        let decls = extract(Path::new("pom.xml"), src.as_bytes());
        let sibling = decls
            .iter()
            .find(|d| d.name == "com.example:sibling-lib")
            .unwrap();
        assert_eq!(sibling.version_requirement.as_deref(), Some("3.1.4"));
        assert!(!sibling.unresolved);
        let jackson = decls
            .iter()
            .find(|d| d.name == "com.fasterxml.jackson.core:jackson-databind")
            .unwrap();
        assert_eq!(jackson.version_requirement.as_deref(), Some("2.13.5"));
        assert!(!jackson.unresolved);
    }
}
