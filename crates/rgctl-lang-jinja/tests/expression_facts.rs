//! Expression facts distinguish inputs from syntax and plugin names.
#![allow(clippy::expect_used)]

use rgctl_lang_jinja::{InvocationKind, expression_facts, parse_expression};
use std::path::Path;

fn facts(source: &str) -> rgctl_lang_jinja::ExpressionFacts {
    let parsed = parse_expression(Path::new("task.yml"), source.as_bytes()).expect("expression");
    assert!(
        !parsed.tree.root_node().has_error(),
        "{}",
        parsed.tree.root_node().to_sexp()
    );
    expression_facts(&parsed, parsed.tree.root_node())
}

#[test]
fn qualified_filter_names_are_not_context_inputs() {
    let facts = facts("value | ansible.builtin.default(fallback)");
    let names: Vec<_> = facts
        .variables
        .iter()
        .map(|access| access.name.as_str())
        .collect();
    assert_eq!(names, ["value", "fallback"]);
    assert_eq!(facts.invocations.len(), 1);
    assert_eq!(facts.invocations[0].kind, InvocationKind::Filter);
    assert_eq!(facts.invocations[0].name, "ansible.builtin.default");
}

#[test]
fn qualified_tests_are_not_context_inputs() {
    let facts = facts("result is ansible.builtin.succeeded");
    assert_eq!(facts.variables.len(), 1);
    assert_eq!(facts.variables[0].name, "result");
    assert_eq!(facts.invocations[0].kind, InvocationKind::Test);
    assert_eq!(facts.invocations[0].name, "ansible.builtin.succeeded");
}

#[test]
fn dynamic_indexes_keep_both_inputs_and_literal_path_components() {
    let facts = facts("hostvars[target]['ansible_host']");
    let names: Vec<_> = facts
        .variables
        .iter()
        .map(|access| access.name.as_str())
        .collect();
    assert_eq!(names, ["hostvars", "target"]);
    assert_eq!(
        facts.variables[0].access,
        "hostvars[target]['ansible_host']"
    );
}

#[test]
fn lookup_arguments_are_recorded_without_becoming_fake_inputs() {
    let facts = facts("lookup('env', 'HOME') | default(fallback)");
    let names: Vec<_> = facts
        .variables
        .iter()
        .map(|access| access.name.as_str())
        .collect();
    assert_eq!(names, ["fallback"]);
    let lookup = facts
        .invocations
        .iter()
        .find(|call| call.kind == InvocationKind::Lookup)
        .expect("lookup");
    assert_eq!(lookup.plugin.as_deref(), Some("env"));
    assert_eq!(lookup.arguments, ["'env'", "'HOME'"]);
}

#[test]
fn argument_names_and_attribute_components_are_not_inputs() {
    let facts = facts("forms.input('name', value=port)");
    let names: Vec<_> = facts
        .variables
        .iter()
        .map(|access| access.name.as_str())
        .collect();
    assert_eq!(names, ["forms", "port"]);
    assert_eq!(facts.invocations[0].name, "forms.input");
}

#[test]
fn template_and_expression_entrypoints_produce_the_same_logical_facts() {
    let bare = facts("value | default(fallback)");
    let parsed = rgctl_lang_jinja::parse_template(
        Path::new("config.j2"),
        b"prefix {{ value | default(fallback) }} suffix",
        &rgctl_lang_jinja::SyntaxProfile::default(),
    )
    .expect("template");
    let template = expression_facts(&parsed, parsed.tree.root_node());
    let variables = |facts: &rgctl_lang_jinja::ExpressionFacts| {
        facts
            .variables
            .iter()
            .map(|item| (item.name.clone(), item.access.clone()))
            .collect::<Vec<_>>()
    };
    assert_eq!(variables(&template), variables(&bare));
    assert_eq!(template.invocations[0].name, bare.invocations[0].name);
    assert_eq!(
        template.invocations[0].arguments,
        bare.invocations[0].arguments
    );
}

#[test]
fn negated_and_parameterized_tests_keep_argument_dependencies() {
    let negated = facts("result is not defined");
    assert_eq!(negated.invocations[0].name, "defined");
    assert!(negated.invocations[0].negated);
    assert_eq!(
        negated
            .variables
            .iter()
            .map(|v| v.name.as_str())
            .collect::<Vec<_>>(),
        ["result"]
    );
    let parameterized = facts("value is divisibleby divisor");
    assert_eq!(parameterized.invocations[0].name, "divisibleby");
    assert_eq!(
        parameterized
            .variables
            .iter()
            .map(|v| v.name.as_str())
            .collect::<Vec<_>>(),
        ["value", "divisor"]
    );
}

#[test]
fn jinja_literal_aliases_do_not_become_context_inputs() {
    let literal = facts("True and not False and None is none");
    assert!(literal.variables.is_empty());
    let null = facts("null");
    assert_eq!(
        null.variables[0].name, "null",
        "null is a context name, not a Jinja literal"
    );
}
