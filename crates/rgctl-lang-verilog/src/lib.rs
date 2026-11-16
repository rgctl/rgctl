//! Verilog Tier-2 language plugin (`handler = "queries"`).

mod config;

use rgctl_lang_runtime::QueryLanguagePlugin;
use rgctl_registry::LanguageRegistry;
use std::sync::Arc;

pub use config::{CONFIG, TAGS_SCM};

/// Register this Tier-2 query plugin.
#[allow(clippy::expect_used, clippy::missing_panics_doc)]
pub fn register(registry: &mut LanguageRegistry) {
    let plugin = QueryLanguagePlugin::from_config(&CONFIG, || tree_sitter_verilog::LANGUAGE.into())
        .expect("compile verilog tags.scm");
    registry.register_language_plugin(Arc::new(plugin));
}

#[cfg(test)]
mod tests {
    use super::*;
    use rgctl_ast_coverage::check_tags_scm;
    use rgctl_plugin_api::LanguagePlugin;
    use std::path::Path;

    #[test]
    fn tags_scm_passes_ast_coverage_vocabulary_gate() {
        let issues = check_tags_scm("verilog", TAGS_SCM);
        assert!(issues.is_empty(), "{issues:?}");
    }

    #[test]
    fn extract_smoke() {
        let plugin =
            QueryLanguagePlugin::from_config(&CONFIG, || tree_sitter_verilog::LANGUAGE.into())
                .expect("compile");
        let source = br###"module adder;
  task automatic helper();
  endtask
  function int compute(input int x);
    helper();
    return x + 1;
  endfunction
endmodule
"###;
        let result = plugin
            .extract_all(Path::new("adder.sv"), source)
            .expect("extract_all");
        assert!(
            result.symbols.iter().any(|s| s.name == "adder"),
            "expected symbol 'adder', got {:?}",
            result.symbols
        );
        assert!(
            result.symbols.iter().any(|s| s.name == "helper"),
            "expected task 'helper', got {:?}",
            result.symbols
        );
        assert!(
            result.symbols.iter().any(|s| s.name == "compute"),
            "expected function 'compute', got {:?}",
            result.symbols
        );
    }
}
