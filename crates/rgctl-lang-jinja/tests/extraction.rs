//! Semantic extraction of scopes, declarations, and deferred source links.
#![allow(clippy::expect_used)]

use rgctl_lang_jinja::JinjaPlugin;
use rgctl_plugin_api::{LanguagePlugin, RelationType, SourceReferenceKind, SymbolType};
use std::path::Path;

fn extract(source: &str) -> rgctl_plugin_api::ExtractAllResult {
    JinjaPlugin::default()
        .extract_all(Path::new("templates/config.j2"), source.as_bytes())
        .expect("extract")
}

#[test]
fn slices_preserve_operands_and_later_template_inputs() {
    let result = extract(
        "{{ (computer | default('WIN'))[0:limit:step] }} {{ password }} {{ items[-4:] }} {{ items[:stop] }}",
    );
    assert_eq!(result.symbols[0].metadata["parse_status"], "complete");
    let inputs: Vec<_> = result
        .symbols
        .iter()
        .filter(|symbol| symbol.metadata["kind"] == "jinja_context_input")
        .map(|symbol| symbol.name.as_str())
        .collect();
    for name in ["computer", "limit", "step", "password", "items", "stop"] {
        assert!(inputs.contains(&name), "missing {name}: {inputs:?}");
    }
}

#[test]
fn macro_parameters_and_loop_bindings_are_not_context_inputs() {
    let result = extract(
        "{% macro field(name, value='') %}{{ name }} {{ value }}{% endmacro %}\n{% for server in servers %}{{ server.name }}{% endfor %}",
    );
    let inputs: Vec<_> = result
        .symbols
        .iter()
        .filter(|symbol| symbol.metadata["kind"] == "jinja_context_input")
        .map(|symbol| symbol.name.as_str())
        .collect();
    assert_eq!(inputs, ["servers"]);
    let field = result
        .symbols
        .iter()
        .find(|symbol| symbol.metadata["kind"] == "jinja_macro")
        .expect("macro");
    assert_eq!(field.symbol_type, SymbolType::Function);
    assert_eq!(
        field
            .parameters
            .iter()
            .map(|parameter| parameter.name.as_str())
            .collect::<Vec<_>>(),
        ["name", "value"]
    );
    assert_eq!(field.parameters[1].default_value.as_deref(), Some("''"));
    assert!(
        result.source_references.is_empty(),
        "declaration is not a macro call"
    );
}

#[test]
fn assignments_are_lexically_scoped_and_namespace_writes_are_references() {
    let result = extract(
        "{% set ns = namespace() %}{% for item in items %}{% set inner = item %}{% set ns.found = item %}{% endfor %}{{ inner }}{{ ns.found }}",
    );
    let inner: Vec<_> = result
        .symbols
        .iter()
        .filter(|symbol| symbol.name == "inner")
        .collect();
    assert_eq!(inner.len(), 2);
    assert_ne!(inner[0].qualified_name, inner[1].qualified_name);
    assert_eq!(inner[1].metadata["kind"], "jinja_context_input");
    assert!(!result.symbols.iter().any(|symbol| symbol.name == "found"));
    assert!(
        result
            .relations
            .iter()
            .any(|relation| relation.relation_type == RelationType::Modifies
                && relation.to.ends_with("binding:ns"))
    );
}

#[test]
fn imports_and_macro_calls_preserve_exact_alias_targets() {
    let result = extract(
        "{% import 'forms.jinja' as forms %}{% from 'widgets.jinja' import input as field, button as submit %}{{ forms.input('name') }}{{ field(value) }}",
    );
    let imports: Vec<_> = result
        .source_references
        .iter()
        .filter(|reference| reference.kind == SourceReferenceKind::JinjaImport)
        .collect();
    assert_eq!(imports.len(), 3);
    let calls: Vec<_> = result
        .source_references
        .iter()
        .filter(|reference| reference.kind == SourceReferenceKind::MacroCall)
        .collect();
    assert_eq!(calls.len(), 2);
    assert_eq!(
        (calls[0].target.as_str(), calls[0].member.as_deref()),
        ("forms.jinja", Some("input"))
    );
    assert_eq!(
        (calls[1].target.as_str(), calls[1].member.as_deref()),
        ("widgets.jinja", Some("input"))
    );
    assert!(
        !result.symbols.iter().any(
            |symbol| symbol.name == "forms" && symbol.metadata["kind"] == "jinja_context_input"
        )
    );
}

#[test]
fn dependency_lists_and_runtime_expressions_remain_distinct() {
    let result = extract(
        "{% extends 'base.jinja' %}{% include ['one.jinja', 'two.jinja'] ignore missing %}{% include selected_template %}",
    );
    assert_eq!(result.source_references.len(), 3);
    assert_eq!(
        result.source_references[0].kind,
        SourceReferenceKind::JinjaExtends
    );
    assert_eq!(
        result.source_references[1].candidates,
        ["one.jinja", "two.jinja"]
    );
    assert!(!result.source_references[1].dynamic);
    assert!(result.source_references[2].dynamic);
    assert!(
        result
            .symbols
            .iter()
            .any(|symbol| symbol.name == "selected_template")
    );
}

#[test]
fn raw_comments_and_invalid_profiles_have_honest_output() {
    let result = extract("{# {{ ignored }} #}{% raw %}{{ ignored_too }}{% endraw %}{{ actual }}");
    let inputs: Vec<_> = result
        .symbols
        .iter()
        .filter(|symbol| symbol.metadata["kind"] == "jinja_context_input")
        .map(|symbol| symbol.name.as_str())
        .collect();
    assert_eq!(inputs, ["actual"]);
    let invalid = extract("#jinja2:variable_start_string:''\n{{ should_not_parse }}");
    assert_eq!(invalid.symbols.len(), 1);
    assert_eq!(invalid.symbols[0].metadata["parse_status"], "error");
    assert!(invalid.symbols[0].metadata["source_diagnostics_json"].is_string());
}

#[test]
fn named_entity_keys_do_not_depend_on_comments_or_lines() {
    let source =
        "{% macro f(x) %}{{ x }}{% endmacro %}{% for item in items %}{{ item }}{% endfor %}";
    let before = extract(source);
    let after = extract(&format!("\n{{# comment #}}\n{source}"));
    let keys = |result: &rgctl_plugin_api::ExtractAllResult| {
        let mut keys = result
            .symbols
            .iter()
            .map(|symbol| {
                (
                    symbol.name.clone(),
                    symbol.symbol_type as usize,
                    symbol.qualified_name.clone(),
                )
            })
            .collect::<Vec<_>>();
        keys.sort();
        keys
    };
    assert_eq!(keys(&before), keys(&after));
}

#[test]
fn call_blocks_bind_caller_parameters_without_exposing_them_as_inputs() {
    let result =
        extract("{% call(user, index) render(users) %}{{ user.name }}{{ index }}{% endcall %}");
    let inputs: Vec<_> = result
        .symbols
        .iter()
        .filter(|symbol| symbol.metadata["kind"] == "jinja_context_input")
        .map(|symbol| symbol.name.as_str())
        .collect();
    assert_eq!(inputs, ["users"]);
    assert_eq!(
        result.source_references[0].member.as_deref(),
        Some("render")
    );
}

#[test]
fn with_scopes_do_not_leak_and_all_initializers_use_the_outer_context() {
    let result = extract("{% with x = outer, y = x %}{{ x }}{{ y }}{% endwith %}{{ y }}");
    let inputs: Vec<_> = result
        .symbols
        .iter()
        .filter(|symbol| symbol.metadata["kind"] == "jinja_context_input")
        .map(|symbol| symbol.name.as_str())
        .collect();
    assert_eq!(inputs, ["outer", "x", "y"]);
}

#[test]
fn imported_call_block_and_partial_parse_keep_safe_evidence() {
    let result = extract(
        "{% import 'macros.jinja' as forms %}{% call(user) forms.render(users) %}{{ user.name }}{% endcall %}",
    );
    let call = result
        .source_references
        .iter()
        .find(|reference| reference.kind == SourceReferenceKind::MacroCall)
        .expect("call");
    assert_eq!(
        (call.target.as_str(), call.member.as_deref()),
        ("macros.jinja", Some("render"))
    );
    let partial = extract("{{ valid }}{% unsupported_extension value %}");
    assert_eq!(partial.symbols[0].metadata["parse_status"], "partial");
    assert!(partial.symbols.iter().any(|symbol| symbol.name == "valid"));
    assert!(partial.symbols[0].metadata["source_diagnostics_json"].is_string());
}

#[test]
fn block_filter_and_conditional_inheritance_keep_dependencies_honest() {
    let result = extract(
        "{% filter indent(width) %}{{ content }}{% endfilter %}{% extends 'base.jinja' if enabled else 'other.jinja' %}",
    );
    let inputs: Vec<_> = result
        .symbols
        .iter()
        .filter(|symbol| symbol.metadata["kind"] == "jinja_context_input")
        .map(|symbol| symbol.name.as_str())
        .collect();
    assert_eq!(inputs, ["width", "content", "enabled"]);
    let reference = result
        .source_references
        .iter()
        .find(|reference| reference.kind == SourceReferenceKind::JinjaExtends)
        .expect("extends");
    assert!(reference.dynamic);
    let evidence = result
        .symbols
        .iter()
        .find_map(|symbol| symbol.metadata["expression_facts_json"].as_str())
        .expect("evidence");
    assert!(evidence.contains("indent"));
}

#[test]
fn nested_macros_and_function_values_keep_lexical_ownership() {
    let result = extract(
        "{% macro outer() %}{% macro inner() %}ok{% endmacro %}{{ inner }}{% endmacro %}{{ outer }}",
    );
    let macros: Vec<_> = result
        .symbols
        .iter()
        .filter(|symbol| symbol.metadata["kind"] == "jinja_macro")
        .collect();
    assert_eq!(macros.len(), 2);
    let outer = macros
        .iter()
        .find(|symbol| symbol.name == "outer")
        .expect("outer");
    let inner = macros
        .iter()
        .find(|symbol| symbol.name == "inner")
        .expect("inner");
    assert!(
        inner
            .qualified_name
            .as_deref()
            .expect("inner key")
            .starts_with(outer.qualified_name.as_deref().expect("outer key"))
    );
    assert_eq!(inner.metadata["exported"], false);
    assert!(
        !result
            .symbols
            .iter()
            .any(|symbol| symbol.metadata["kind"] == "jinja_context_input")
    );
}

#[test]
fn vaulted_templates_are_diagnostic_roots_without_invented_content() {
    let result = extract("$ANSIBLE_VAULT;1.1;AES256\n6162636465\n");
    assert_eq!(result.symbols.len(), 1);
    assert_eq!(result.symbols[0].metadata["parse_status"], "error");
    assert!(
        result.symbols[0].metadata["source_diagnostics_json"]
            .as_str()
            .unwrap_or("")
            .contains("encrypted_vault_content")
    );
    assert!(result.relations.is_empty());
    assert!(result.source_references.is_empty());
}

#[test]
fn duplicate_macro_declarations_keep_distinct_stable_identities() {
    let source =
        "{% macro same() %}one{% endmacro %}{% macro same() %}two{% endmacro %}{{ same() }}";
    let result = extract(source);
    let definitions: Vec<_> = result
        .symbols
        .iter()
        .filter(|symbol| symbol.metadata["kind"] == "jinja_macro")
        .collect();
    assert_eq!(definitions.len(), 2);
    assert_ne!(definitions[0].qualified_name, definitions[1].qualified_name);
    let shifted = extract(&format!("\n{{# comment #}}\n{source}"));
    let shifted: Vec<_> = shifted
        .symbols
        .iter()
        .filter(|symbol| symbol.metadata["kind"] == "jinja_macro")
        .map(|symbol| &symbol.qualified_name)
        .collect();
    assert_eq!(
        definitions
            .iter()
            .map(|symbol| &symbol.qualified_name)
            .collect::<Vec<_>>(),
        shifted
    );
}
