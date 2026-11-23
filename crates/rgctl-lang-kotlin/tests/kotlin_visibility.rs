//! Typed visibility classification for Kotlin declarations.
//!
//! Regression gate for compound modifiers: `kotlin_declaration_visibility`
//! must classify by the individual `visibility_modifier` token, not the
//! whole `modifiers` node text — `private suspend fun` is Private, not
//! Public.

use rgctl_lang_kotlin::KotlinPlugin;
use rgctl_plugin_api::{LanguagePlugin, SymbolVisibility};
use std::path::Path;

#[test]
fn compound_modifier_visibility() {
    let source = br#"
class Svc {
    private suspend fun hidden() {}
    private fun plainPrivate() {}
    internal inline fun internalFn() {}
    internal fun plainInternal() {}
    public fun explicitPublic() {}
    suspend fun publicSuspend() {}
    protected open fun protectedOpen() {}
    override fun overridden() {}
}
"#;
    let plugin = KotlinPlugin::new().expect("plugin");
    let out = plugin
        .extract_all(Path::new("visibility.kt"), source)
        .expect("extract_all");

    let vis = |name: &str| {
        out.symbols
            .iter()
            .find(|s| s.name == name)
            .map(|s| s.visibility)
            .unwrap_or(SymbolVisibility::Unknown)
    };

    // Single visibility keyword — unchanged behavior.
    assert_eq!(vis("plainPrivate"), SymbolVisibility::Private);
    assert_eq!(vis("plainInternal"), SymbolVisibility::Internal);
    assert_eq!(vis("explicitPublic"), SymbolVisibility::Public);
    // Compound modifiers — the visibility token wins regardless of what
    // else shares the `modifiers` node.
    assert_eq!(
        vis("hidden"),
        SymbolVisibility::Private,
        "`private suspend fun` must classify Private (compound modifiers)"
    );
    assert_eq!(
        vis("internalFn"),
        SymbolVisibility::Internal,
        "`internal inline fun` must classify Internal (compound modifiers)"
    );
    assert_eq!(
        vis("protectedOpen"),
        SymbolVisibility::Private,
        "`protected open fun` must classify Private (compound modifiers)"
    );
    // No visibility token — Kotlin public-by-default.
    assert_eq!(vis("publicSuspend"), SymbolVisibility::Public);
    assert_eq!(vis("overridden"), SymbolVisibility::Public);
}

#[test]
fn constructor_visibility_not_overwritten_by_enclosing_class() {
    let source = br#"
class Singleton private constructor(val id: Int)
class InternalCtor internal constructor()
class DefaultCtor constructor(val x: Int)
"#;
    let plugin = KotlinPlugin::new().expect("plugin");
    let out = plugin
        .extract_all(Path::new("ctors.kt"), source)
        .expect("extract_all");

    let by_qn = |qn: &str| {
        out.symbols
            .iter()
            .find(|s| s.qualified_name.as_deref() == Some(qn))
            .unwrap_or_else(|| panic!("missing symbol {qn}; got {:?}", out.symbols))
            .visibility
    };

    assert_eq!(by_qn("Singleton"), SymbolVisibility::Public);
    assert_eq!(
        by_qn("Singleton.<init>"),
        SymbolVisibility::Private,
        "private primary constructor must not be overwritten by public class visibility"
    );
    assert_eq!(by_qn("InternalCtor"), SymbolVisibility::Public);
    assert_eq!(by_qn("InternalCtor.<init>"), SymbolVisibility::Internal);
    assert_eq!(by_qn("DefaultCtor"), SymbolVisibility::Public);
    assert_eq!(by_qn("DefaultCtor.<init>"), SymbolVisibility::Public);
}
