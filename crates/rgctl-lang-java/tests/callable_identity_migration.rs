//! Regression contracts for Java's migration onto the shared identity helper.
use rgctl_lang_java::JavaPlugin;
use rgctl_plugin_api::{LanguagePlugin, RelationType, SymbolType};
use rgctl_plugin_helpers::callable_identity::{self, Policy};
use std::path::Path;

#[test]
fn shared_call_policy_keeps_java_expression_evidence_and_unknowns() {
    let source = br#"class Cases {
        void accept(int value) {}
        void accept(long value) {}
        void accept(float value) {}
        void accept(double value) {}
        void accept(char value) {}
        void accept(boolean value) {}
        void accept(String value) {}
        void accept(Widget value) {}
        void caller(String text) {
            accept(0x10);
            accept(1L);
            accept(1.0f);
            accept(1.0);
            accept('x');
            accept(true);
            accept(text);
            String local = "example";
            accept(local);
            accept(new Widget());
            accept(missing());
        }
    }"#;
    let plugin = JavaPlugin::new().unwrap();
    let result = plugin.extract_all(Path::new("Cases.java"), source).unwrap();
    let mut calls: Vec<_> = result
        .relations
        .iter()
        .filter(|r| {
            r.relation_type == RelationType::Calls
                && r.from == "Cases.caller"
                && r.to.ends_with("accept")
        })
        .collect();
    calls.sort_by_key(|r| (r.location.start_line, r.location.start_column));
    assert_eq!(calls.len(), 10);
    for (call, expected) in calls.iter().zip([
        Some("(int)"),
        Some("(long)"),
        Some("(float)"),
        Some("(double)"),
        Some("(char)"),
        Some("(boolean)"),
        Some("(String)"),
        Some("(String)"),
        Some("(Widget)"),
        None,
    ]) {
        assert_eq!(
            call.metadata["argument_identity_discriminator"].as_str(),
            expected,
            "call on line {}",
            call.location.start_line
        );
        assert_eq!(call.metadata["from_identity_discriminator"], "(String)");
        if expected.is_none() {
            assert!(call.metadata["to_identity_discriminator"].is_null());
        }
    }
}

#[test]
fn shared_java_symbol_policy_preserves_spans_and_propagates_type_errors() {
    let source = b"class A { void quote(Widget value) {} }";
    let plugin = JavaPlugin::new().unwrap();
    let result = plugin.extract_all(Path::new("A.java"), source).unwrap();
    let original = result
        .symbols
        .iter()
        .find(|s| s.symbol_type == SymbolType::Function)
        .unwrap();
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&tree_sitter_java::LANGUAGE.into())
        .unwrap();
    let tree = parser.parse(source, None).unwrap();
    let class = tree.root_node().named_child(0).unwrap();
    let method = class
        .child_by_field_name("body")
        .unwrap()
        .named_child(0)
        .unwrap();
    let migrated =
        callable_identity::try_symbol(original.clone(), method, source, Policy::Java).unwrap();
    assert_eq!(migrated.metadata["identity_discriminator"], "(Widget)");
    assert_eq!(migrated.metadata["source_start_byte"], method.start_byte());
    assert_eq!(migrated.metadata["source_end_byte"], method.end_byte());
    assert_eq!(migrated.qualified_name, original.qualified_name);

    // Keep the CST but make the type token invalid UTF-8: the fallible helper
    // must propagate the decoding error instead of assigning a default identity.
    let mut malformed = source.to_vec();
    let start = source.windows(6).position(|w| w == b"Widget").unwrap();
    malformed[start] = 0xff;
    assert!(
        callable_identity::try_symbol(original.clone(), method, &malformed, Policy::Java).is_err()
    );
}

#[test]
fn legacy_and_single_pass_keep_the_same_java_declaration_identities() {
    let source = br#"class Outer {
        Outer(int value) {}
        Outer(String value) {}
        void quote(@A String[] values) { Runnable r = () -> {}; }
        void quote(int value) { Runnable r = () -> {}; }
        void receiver(Outer this, int value) {}
        class Inner { void quote(String... values) {} }
    }
    record Point(int x, String y) { Point {} }"#;
    let plugin = JavaPlugin::new().unwrap();
    let file = Path::new("Outer.java");
    let legacy = plugin.extract_symbols(file, source).unwrap();
    let single = plugin.extract_all(file, source).unwrap();
    let identities = |symbols: &[rgctl_plugin_api::Symbol]| {
        let mut identities: Vec<_> = symbols
            .iter()
            .filter(|s| s.symbol_type == SymbolType::Function)
            .map(|s| {
                (
                    s.qualified_name.clone(),
                    s.metadata["identity_discriminator"]
                        .as_str()
                        .map(str::to_owned),
                    s.metadata["source_start_byte"].as_u64(),
                    s.metadata["source_end_byte"].as_u64(),
                )
            })
            .collect();
        identities.sort();
        identities
    };
    assert_eq!(identities(&legacy), identities(&single.symbols));
    for (qualified, expected) in [
        ("Outer.<init>", "(int)"),
        ("Outer.<init>", "(String)"),
        ("Outer.quote", "(String[])"),
        ("Outer.quote", "(int)"),
        ("Outer.receiver", "(int)"),
        ("Outer.Inner.quote", "(String[])"),
        ("Point.<init>", "(int,String)"),
        ("Outer.quote.$lambda$0", "(String[])"),
        ("Outer.quote.$lambda$0", "(int)"),
    ] {
        assert!(
            single.symbols.iter().any(|s| {
                s.qualified_name.as_deref() == Some(qualified)
                    && s.metadata["identity_discriminator"] == expected
            }),
            "missing {qualified} {expected}"
        );
    }
}
