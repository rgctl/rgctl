#![allow(clippy::expect_used)]
//! Configured syntax must preserve literal content and original positions.

use rgctl_lang_jinja::{ParsedTemplate, SyntaxProfile, parse_expression, parse_template};
use std::path::Path;

fn identifiers(parsed: &ParsedTemplate) -> Vec<tree_sitter::Node<'_>> {
    let mut nodes = vec![parsed.tree.root_node()];
    let mut found = Vec::new();
    while let Some(node) = nodes.pop() {
        if node.kind() == "identifier" {
            found.push(node);
        }
        let mut cursor = node.walk();
        nodes.extend(node.named_children(&mut cursor));
    }
    found
}

#[test]
fn templates_and_bare_expressions_share_the_grammar() {
    let path = Path::new("task.yml");
    let template = parse_template(
        path,
        b"{{ result is succeeded }}",
        &SyntaxProfile::default(),
    )
    .expect("template");
    let expression = parse_expression(path, b"result is succeeded").expect("expression");
    assert!(!expression.tree.root_node().has_error());
    assert_eq!(
        template.tree.root_node().to_sexp(),
        expression.tree.root_node().to_sexp()
    );
    for node in identifiers(&expression) {
        let range = expression.original_range(node);
        assert!(range.end <= b"result is succeeded".len());
    }
}

#[test]
fn custom_delimiters_keep_default_markers_literal() {
    let path = Path::new("config.j2");
    let source = b"#jinja2:variable_start_string:'[%', variable_end_string:'%]'\r\nport=[% port %]\r\n{{ untouched }}\r\n";
    let parsed = parse_template(path, source, &SyntaxProfile::default()).expect("profile");
    assert!(
        !parsed.tree.root_node().has_error(),
        "{}",
        parsed.tree.root_node().to_sexp()
    );
    let names = identifiers(&parsed);
    assert_eq!(names.len(), 1);
    assert_eq!(&source[parsed.original_range(names[0])], b"port");
    let location = parsed.location(names[0], path);
    assert_eq!((location.start_line, location.start_column), (2, 8));
}

#[test]
fn unicode_and_unequal_delimiter_lengths_map_back_to_original_bytes() {
    let path = Path::new("config.j2");
    let syntax = SyntaxProfile {
        variable_start_string: "<<=".into(),
        variable_end_string: "=>>".into(),
        ..SyntaxProfile::default()
    };
    let source = "é <<= port =>>\r\n".as_bytes();
    let parsed = parse_template(path, source, &syntax).expect("profile");
    assert!(!parsed.tree.root_node().has_error());
    let names = identifiers(&parsed);
    assert_eq!(&source[parsed.original_range(names[0])], b"port");
    let location = parsed.location(names[0], path);
    assert_eq!((location.start_line, location.start_column), (1, 7));
}

#[test]
fn raw_regions_and_quoted_delimiters_do_not_become_code() {
    let source = b"{% raw %}{{ untouched }}{% endraw %}{{ '}}' }}{{ wanted }}";
    let parsed =
        parse_template(Path::new("config.j2"), source, &SyntaxProfile::default()).expect("parse");
    assert!(!parsed.tree.root_node().has_error());
    let names: Vec<_> = identifiers(&parsed)
        .iter()
        .map(|node| &source[parsed.original_range(*node)])
        .collect();
    assert_eq!(names, vec![b"wanted".as_slice()]);
}

#[test]
fn explicitly_configured_line_syntax_and_continuations_parse() {
    let syntax = SyntaxProfile {
        line_statement_prefix: Some("#".into()),
        line_comment_prefix: Some("##".into()),
        ..SyntaxProfile::default()
    };
    let source = b"## {{ ignored }}\n# for item in [\n  'a', 'b'\n]: \r\n{{ item }}\n# endfor\n";
    let parsed = parse_template(Path::new("config.j2"), source, &syntax).expect("profile");
    assert!(
        !parsed.tree.root_node().has_error(),
        "{}",
        parsed.tree.root_node().to_sexp()
    );
    let names: Vec<_> = identifiers(&parsed)
        .iter()
        .map(|node| &source[parsed.original_range(*node)])
        .collect();
    assert!(!names.contains(&b"ignored".as_slice()));
    assert_eq!(names.iter().filter(|name| **name == b"item").count(), 2);
}

#[test]
fn invalid_profiles_are_diagnosed() {
    let path = Path::new("config.j2");
    for source in [
        b"#jinja2:variable_start_string:'{%'\n{{ x }}".as_slice(),
        b"#jinja2:variable_start_string:''\n{{ x }}",
        b"#jinja2:variable_start_string:'unterminated\n{{ x }}",
        b"#jinja2:unknown_setting:'value'\n{{ x }}",
    ] {
        assert!(parse_template(path, source, &SyntaxProfile::default()).is_err());
    }
}

#[test]
fn inactive_canonical_comment_end_is_literal_inside_custom_comment() {
    let syntax = SyntaxProfile {
        comment_start_string: "<#".into(),
        comment_end_string: "#>".into(),
        ..SyntaxProfile::default()
    };
    let parsed = parse_template(Path::new("config.j2"), b"<# #} {{ ignored }} #>", &syntax)
        .expect("profile");
    assert!(!parsed.tree.root_node().has_error());
    assert!(identifiers(&parsed).is_empty());
}
