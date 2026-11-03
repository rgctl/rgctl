#![allow(clippy::expect_used)]
//! Syntax compatibility gates for the pinned Jinja grammar.

use rgctl_plugin_api::parse_with_pool;

fn parse(source: &str) -> tree_sitter::Tree {
    let tree = parse_with_pool("jinja", rgctl_lang_jinja::language(), source.as_bytes())
        .expect("parse Jinja fixture");
    assert!(
        !tree.root_node().has_error(),
        "fixture: {source}\nAST: {}",
        tree.root_node().to_sexp()
    );
    tree
}

#[test]
fn ansible_syntax_compatibility_matrix() {
    for source in [
        "{{ value | ansible.builtin.default(fallback) }}",
        "{% if result is ansible.builtin.succeeded %}yes{% endif %}",
        "{% macro input(name, value='') %}{{ name }}{% endmacro %}",
        "{% block content scoped required %}{{ value }}{% endblock content %}",
        "{% call(user, index) render(users) %}{{ user }}{% endcall %}",
        "{{ hostvars[target]['ansible_host'] | default('127.0.0.1') }}",
        "{{ (computer | default('WIN'))[0:limit:step] }} {{ items[-4:] }} {{ items[:stop] }}",
        "{{ (a + b) * -factor if enabled else +fallback }}",
        "{%- for item in items -%}{{- item -}}{%- endfor -%}",
        "{% raw %}{{ untouched }}{% endraw %}",
        "{% include ['preferred.jinja', 'fallback.jinja'] ignore missing %}",
        "{% from 'macros.jinja' import input as field with context %}",
        "{% from 'macros.jinja' import input as field, button as submit with context %}",
    ] {
        parse(source);
    }
}

#[test]
fn default_syntax_keeps_hash_prefixed_configuration_as_content() {
    let tree = parse("# for item in items\n## server {{ hostname }}\n# endfor\n");
    let mut pending = vec![tree.root_node()];
    while let Some(node) = pending.pop() {
        assert!(!matches!(node.kind(), "inline" | "comment"));
        let mut cursor = node.walk();
        pending.extend(node.named_children(&mut cursor));
    }
}

#[test]
fn raw_and_comments_do_not_contain_expression_nodes() {
    for source in [
        "{# {{ untouched }} #}",
        "{% raw %}{{ untouched }}{% endraw %}",
    ] {
        let tree = parse(source);
        assert!(!tree.root_node().to_sexp().contains("render_expression"));
    }
}
