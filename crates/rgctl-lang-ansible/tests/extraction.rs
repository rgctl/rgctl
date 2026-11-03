//! Ansible structure, embedded Jinja, and static-runtime boundaries.
#![allow(clippy::expect_used)]

use rgctl_lang_ansible::AnsiblePlugin;
use rgctl_plugin_api::{LanguagePlugin, RelationType, SourceReferenceKind, SymbolType};
use std::path::Path;

fn extract(path: &str, source: &str) -> rgctl_plugin_api::ExtractAllResult {
    AnsiblePlugin
        .extract_all(Path::new(path), source.as_bytes())
        .expect("extract Ansible")
}

#[test]
fn role_task_selection_and_template_lookups_keep_precise_references() {
    let result = extract(
        "roles/web/tasks/main.yml",
        r#"
- name: select tasks
  include_role: {name: common, tasks_from: setup}
- name: render lookup
  debug: {msg: "{{ lookup('template', 'config.j2') }}"}
- name: dynamic lookup
  debug: {msg: "{{ query('ansible.builtin.template', selected) }}"}
"#,
    );
    let selected = result
        .source_references
        .iter()
        .find(|r| r.kind == SourceReferenceKind::TaskInclude && r.target == "setup")
        .expect("tasks_from");
    assert_eq!(
        selected.properties.get("role_name").map(String::as_str),
        Some("common")
    );
    let lookup = result
        .source_references
        .iter()
        .find(|r| r.kind == SourceReferenceKind::TemplateRender && r.target == "config.j2")
        .expect("template lookup");
    assert!(!lookup.dynamic);
    assert_eq!(lookup.location.start_line, 5);
    assert!(
        result
            .source_references
            .iter()
            .any(|r| r.kind == SourceReferenceKind::TemplateRender
                && r.dynamic
                && r.target == "selected")
    );
}

#[test]
fn playbook_structure_and_nested_tasks_have_stable_distinct_keys() {
    let source = "- name: Deploy\n  hosts: web\n  roles: [web]\n  tasks:\n    - name: duplicate\n      block:\n        - name: duplicate\n          ansible.builtin.debug: {msg: hello}\n      rescue:\n        - ansible.builtin.debug: {msg: failed}\n      always:\n        - ansible.builtin.debug: {msg: done}\n    - name: duplicate\n      ansible.builtin.debug: {msg: end}\n";
    let result = extract("deploy.yml", source);
    assert_eq!(
        result
            .symbols
            .iter()
            .filter(|symbol| symbol.symbol_type == SymbolType::AnsiblePlaybook)
            .count(),
        1
    );
    assert_eq!(
        result
            .symbols
            .iter()
            .filter(|symbol| symbol.symbol_type == SymbolType::AnsiblePlay)
            .count(),
        1
    );
    let tasks: Vec<_> = result
        .symbols
        .iter()
        .filter(|symbol| symbol.symbol_type == SymbolType::AnsibleTask)
        .collect();
    assert_eq!(
        tasks.len(),
        5,
        "tasks: {:?}",
        tasks
            .iter()
            .map(|task| (&task.name, &task.metadata["section"], &task.qualified_name))
            .collect::<Vec<_>>()
    );
    let mut keys = tasks
        .iter()
        .map(|symbol| symbol.qualified_name.clone())
        .collect::<Vec<_>>();
    keys.sort();
    keys.dedup();
    assert_eq!(keys.len(), 5);
    let shifted = extract("deploy.yml", &format!("# comment\n\n{source}"));
    let all_keys = |result: &rgctl_plugin_api::ExtractAllResult| {
        let mut keys = result
            .symbols
            .iter()
            .map(|symbol| symbol.qualified_name.clone())
            .collect::<Vec<_>>();
        keys.sort();
        keys
    };
    assert_eq!(all_keys(&result), all_keys(&shifted));
    assert!(
        result
            .source_references
            .iter()
            .any(
                |reference| reference.kind == SourceReferenceKind::AnsibleRole
                    && reference.target == "web"
            )
    );
}

#[test]
fn literal_action_forms_and_unknown_collection_names_remain_distinct() {
    let source = "- name: short\n  template: {src: config.j2, dest: /tmp/config}\n- name: qualified\n  ansible.builtin.template: {src: config.j2, dest: /tmp/config}\n- name: action\n  action: {module: ansible.builtin.template, src: config.j2, dest: /tmp/config}\n- name: custom\n  vendor.collection.template: {src: custom.j2}\n";
    let result = extract("roles/web/tasks/main.yml", source);
    let templates: Vec<_> = result
        .source_references
        .iter()
        .filter(|reference| reference.kind == SourceReferenceKind::TemplateRender)
        .collect();
    assert_eq!(templates.len(), 3);
    assert!(
        templates
            .iter()
            .all(|reference| reference.target == "config.j2")
    );
    assert!(
        templates
            .iter()
            .all(|reference| reference.context.role_root.as_deref() == Some("roles/web"))
    );
    assert!(
        result
            .symbols
            .iter()
            .any(|symbol| symbol.metadata["module"] == "vendor.collection.template")
    );
    assert!(
        !result
            .relations
            .iter()
            .any(|relation| relation.relation_type == RelationType::Calls)
    );
}

#[test]
fn includes_dependencies_and_notifications_keep_static_and_dynamic_evidence() {
    let source = "- include_tasks: install.yml\n- ansible.builtin.import_role: {name: common}\n- include_role: {name: '{{ selected_role }}'}\n- template: {src: config.j2, dest: /etc/service.conf, variable_start_string: '[%', variable_end_string: '%]'}\n  notify: [restart service, reload config]\n";
    let result = extract("roles/web/tasks/main.yml", source);
    assert!(
        result
            .source_references
            .iter()
            .any(
                |reference| reference.kind == SourceReferenceKind::TaskInclude
                    && reference.target == "install.yml"
            )
    );
    assert!(
        result
            .source_references
            .iter()
            .any(
                |reference| reference.kind == SourceReferenceKind::AnsibleRole && reference.dynamic
            )
    );
    assert_eq!(
        result
            .source_references
            .iter()
            .filter(|reference| reference.kind == SourceReferenceKind::HandlerNotify)
            .count(),
        2
    );
    let template = result
        .source_references
        .iter()
        .find(|reference| reference.kind == SourceReferenceKind::TemplateRender)
        .expect("render");
    assert_eq!(
        template
            .properties
            .get("variable_start_string")
            .map(String::as_str),
        Some("[%")
    );
    assert!(
        !result
            .source_references
            .iter()
            .any(|reference| reference.target == "/etc/service.conf")
    );
    let meta = extract(
        "roles/web/meta/main.yml",
        "dependencies:\n  - common\n  - role: vendor.collection.role\n",
    );
    assert_eq!(
        meta.source_references
            .iter()
            .filter(|reference| reference.kind == SourceReferenceKind::RoleDependency)
            .count(),
        2
    );
}

#[test]
fn vars_register_set_fact_and_loop_bindings_are_source_declarations() {
    let source = "- hosts: all\n  vars:\n    port: 8080\n    message: '{{ later }}'\n    later: value\n  tasks:\n    - command: echo hi\n      register: result\n    - set_fact: {derived: '{{ port }}', cacheable: true}\n    - debug: {msg: '{{ server.name }} {{ derived }}'}\n      loop: '{{ servers }}'\n      loop_control: {loop_var: server}\n      when: [enabled, 'result is succeeded']\n";
    let result = extract("deploy.yml", source);
    assert!(
        result
            .symbols
            .iter()
            .any(|symbol| symbol.name == "result" && symbol.metadata["kind"] == "ansible_register")
    );
    assert!(result.symbols.iter().any(|symbol| symbol.name == "derived" && symbol.metadata["kind"] == "ansible_set_fact"));
    assert!(
        !result
            .symbols
            .iter()
            .any(|symbol| symbol.name == "cacheable")
    );
    assert!(!result.symbols.iter().any(
        |symbol| symbol.name == "server" && symbol.metadata["kind"] == "ansible_context_input"
    ));
    assert!(
        !result
            .symbols
            .iter()
            .any(|symbol| symbol.name == "later"
                && symbol.metadata["kind"] == "ansible_context_input")
    );
    assert!(result.symbols.iter().any(
        |symbol| symbol.name == "enabled" && symbol.metadata["kind"] == "ansible_context_input"
    ));
}

#[test]
fn folded_and_escaped_scalar_references_use_original_yaml_positions() {
    let source = "- name: mapped\r\n  debug:\r\n    msg: >-\r\n      prefix {{ hostvars[target]['ansible_host'] }}\r\n      suffix\r\n  when: \"enabled and\\n    ready\"\r\n";
    let result = extract("roles/web/tasks/main.yml", source);
    let target = result
        .symbols
        .iter()
        .find(|symbol| symbol.name == "target")
        .expect("target");
    assert_eq!(
        (target.location.start_line, target.location.start_column),
        (4, 25)
    );
    let ready = result
        .symbols
        .iter()
        .find(|symbol| symbol.name == "ready")
        .expect("ready");
    assert_eq!(
        ready.location.start_line, 6,
        "escaped newline stays on the original physical line"
    );
    assert!(
        source
            .lines()
            .nth(ready.location.start_line - 1)
            .expect("line")
            .as_bytes()[ready.location.start_column..]
            .starts_with(b"ready")
    );
}

#[test]
fn embedded_statement_scopes_and_unsafe_tags_do_not_invent_context_inputs() {
    let source = "- debug:\n    msg: >-\n      {% for item in items %}{{ item.name }}{% endfor %}\n      {{ actual }}\n    literal: !unsafe '{{ untouched }}'\n";
    let result = extract("roles/web/tasks/main.yml", source);
    assert!(
        result
            .symbols
            .iter()
            .any(|symbol| symbol.name == "items"
                && symbol.metadata["kind"] == "ansible_context_input")
    );
    assert!(
        !result
            .symbols
            .iter()
            .any(|symbol| symbol.name == "item"
                && symbol.metadata["kind"] == "ansible_context_input")
    );
    assert!(
        !result
            .symbols
            .iter()
            .any(|symbol| symbol.name == "untouched")
    );
    assert!(result.symbols.iter().any(|symbol| symbol.name == "actual"));
}

#[test]
fn handler_topics_and_generic_inventory_boundaries_are_preserved() {
    let result = extract(
        "roles/web/handlers/main.yml",
        "- name: restart service\n  service: {name: web, state: restarted}\n  listen: [refresh web, reload web]\n",
    );
    let handler = result
        .symbols
        .iter()
        .find(|symbol| symbol.symbol_type == SymbolType::AnsibleHandler)
        .expect("handler");
    let topics: Vec<String> = serde_json::from_str(
        handler.metadata["listen_topics_json"]
            .as_str()
            .expect("topics"),
    )
    .expect("JSON");
    assert_eq!(topics, ["refresh web", "reload web"]);
    for path in [
        "inventory.yml",
        "group_vars/all.yml",
        "host_vars/web.yml",
        "roles/web/defaults/main.yml",
        "roles/web/vars/main.yml",
    ] {
        assert!(
            AnsiblePlugin
                .extract_candidate(
                    Path::new(path),
                    b"all:\n  hosts:\n    web:\n      port: 8080\n"
                )
                .expect("candidate")
                .is_none()
        );
    }
}

#[test]
fn malformed_and_encrypted_sources_produce_diagnostics_without_fake_tasks() {
    let malformed = extract("roles/web/tasks/main.yml", "- debug: [unterminated\n");
    assert_eq!(malformed.symbols.len(), 1);
    assert_eq!(malformed.symbols[0].metadata["parse_status"], "partial");
    assert!(malformed.source_references.is_empty());
    let vault = extract(
        "roles/web/tasks/main.yml",
        "$ANSIBLE_VAULT;1.1;AES256\n00000000\n",
    );
    assert_eq!(vault.symbols.len(), 1);
    assert!(
        vault.symbols[0].metadata["source_diagnostics_json"]
            .as_str()
            .expect("diagnostics")
            .contains("encrypted_vault_content")
    );
}

#[test]
fn conservative_candidates_keep_config_companions_even_with_play_shaped_values() {
    let source = b"- hosts: example\n  vars: {key: value}\n";
    for path in [
        "group_vars/all.yml",
        "host_vars/web.yaml",
        "roles/web/defaults/main.yml",
        "roles/web/vars/main.yaml",
        "inventories/prod/hosts.yml",
    ] {
        assert!(
            AnsiblePlugin
                .extract_candidate(Path::new(path), source)
                .expect("candidate")
                .is_none(),
            "{path}"
        );
    }
    for path in [
        "roles/web/tasks/main",
        "roles/inventory/tasks/main",
        "roles/web/handlers/main",
        "ansible_collections/acme/app/roles/web/tasks/main",
    ] {
        let result = AnsiblePlugin
            .extract_candidate(Path::new(path), b"- name: Check\n  debug: {msg: ok}\n")
            .expect("candidate")
            .expect("recognized main");
        assert!(result.symbols.iter().any(|symbol| matches!(
            symbol.symbol_type,
            SymbolType::AnsibleTask | SymbolType::AnsibleHandler
        )));
    }
}

#[test]
fn explicit_associations_accept_empty_task_lists_and_diagnose_invalid_roots() {
    let empty = extract("custom-entrypoint", "[]\n");
    assert!(
        empty
            .symbols
            .iter()
            .any(|symbol| symbol.metadata["parse_status"] == "complete")
    );
    let invalid = extract("custom-entrypoint", "not_tasks: mapping\n");
    assert!(invalid.symbols.iter().any(|symbol| {
        symbol.metadata["source_diagnostics_json"]
            .as_str()
            .unwrap_or("")
            .contains("invalid_task_list")
    }));
    assert!(
        !invalid
            .symbols
            .iter()
            .any(|symbol| symbol.symbol_type == SymbolType::AnsibleTask)
    );
}

#[test]
fn explicit_template_conditions_and_bare_conditions_share_inputs() {
    let result = extract(
        "roles/web/tasks/main.yml",
        "- debug: {msg: ok}\n  when: ['{{ enabled }}', \"result is succeeded\"]\n  changed_when: '{% if changed %}true{% else %}false{% endif %}'\n",
    );
    for name in ["enabled", "result", "changed"] {
        assert!(
            result
                .symbols
                .iter()
                .any(|symbol| symbol.name == name
                    && symbol.symbol_type == SymbolType::AnsibleVariable),
            "{name}"
        );
    }
}
