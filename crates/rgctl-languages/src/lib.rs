//! Built-in Tier 1 language plugin registration.

use rgctl_registry::LanguageRegistry;

/// Register all Tier 1 language plugins.
pub fn register_languages(registry: &mut LanguageRegistry) {
    rgctl_lang_rust::register(registry);
    rgctl_lang_python::register(registry);
    rgctl_lang_javascript::register(registry);
    rgctl_lang_typescript::register(registry);
    rgctl_lang_go::register(registry);
    rgctl_lang_java::register(registry);
    rgctl_lang_csharp::register(registry);
    rgctl_lang_c::register(registry);
    rgctl_lang_cpp::register(registry);
    rgctl_lang_markdown::register(registry);
    rgctl_lang_php::register(registry);
    rgctl_lang_ruby::register(registry);
    rgctl_lang_puppet::register(registry);
    rgctl_lang_erb::register(registry);
    rgctl_lang_jinja::register(registry);
    rgctl_lang_ansible::register(registry);
    rgctl_lang_kotlin::register(registry);
    rgctl_lang_groovy::register(registry);
    rgctl_lang_swift::register(registry);
    rgctl_lang_scala::register(registry);
    rgctl_lang_dart::register(registry);
    rgctl_lang_elixir::register(registry);
    rgctl_lang_lua::register(registry);
    rgctl_lang_bash::register(registry);
    rgctl_lang_sql::register(registry);
    rgctl_lang_hcl::register(registry);
    // Tier-2 (`handler = "queries"`)
    rgctl_lang_zig::register(registry);
    rgctl_lang_nix::register(registry);
    rgctl_lang_proto::register(registry);
    rgctl_lang_solidity::register(registry);
    // JSON/YAML stay on config-format plugins (`rgctl-config-formats`); Tier-2
    // query crates exist for standalone use but are not dual-registered here.
    rgctl_lang_julia::register(registry);
    rgctl_lang_r::register(registry);
    rgctl_lang_haskell::register(registry);
    rgctl_lang_gdscript::register(registry);
    rgctl_lang_fortran::register(registry);
    rgctl_lang_verilog::register(registry);
    rgctl_lang_objc::register(registry);
    rgctl_lang_clojure::register(registry);
    rgctl_lang_ocaml::register(registry);
    rgctl_lang_elisp::register(registry);
    rgctl_lang_html::register(registry);
    rgctl_lang_css::register(registry);
    rgctl_lang_svelte::register(registry);
    rgctl_lang_graphql::register(registry);
    rgctl_lang_prisma::register(registry);
    rgctl_lang_elm::register(registry);
    rgctl_lang_erlang::register(registry);
    rgctl_lang_gleam::register(registry);
    rgctl_lang_fsharp::register(registry);
    rgctl_lang_racket::register(registry);
    rgctl_lang_scheme::register(registry);
    rgctl_lang_d::register(registry);
    rgctl_lang_odin::register(registry);
    rgctl_lang_cuda::register(registry);
    rgctl_lang_glsl::register(registry);
    rgctl_lang_vhdl::register(registry);
    rgctl_lang_cmake::register(registry);
    rgctl_lang_make::register(registry);
    rgctl_lang_starlark::register(registry);
    rgctl_lang_powershell::register(registry);
    // Legacy / mainframe / enterprise migration
    rgctl_lang_cobol::register(registry);
    rgctl_lang_pli::register(registry);
    rgctl_lang_jcl::register(registry);
    rgctl_lang_apex::register(registry);
    rgctl_lang_pascal::register(registry);
    rgctl_lang_perl::register(registry);
    rgctl_lang_ada::register(registry);
    // Register matlab before objc so path-only `.m` defaults stay objc when
    // content sniff is unavailable; extract paths use sniff_objc_vs_matlab.
    rgctl_lang_matlab::register(registry);
    // Re-register objc last so extension_map["m"] defaults to objc.
    rgctl_lang_objc::register(registry);
}

/// Default registry with config formats and all built-in languages.
pub fn default_registry() -> LanguageRegistry {
    let mut registry = LanguageRegistry::with_config_formats();
    register_languages(&mut registry);
    registry
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn default_registry_registers_markdown() {
        let registry = default_registry();
        assert!(registry.has_plugin("markdown"));
        let plugin = registry
            .get_plugin_for_file(Path::new("docs/guide.md"))
            .expect("md plugin");
        assert_eq!(plugin.language_id(), "markdown");
        let mdx = registry
            .get_plugin_for_file(Path::new("docs/overview.mdx"))
            .expect("mdx plugin");
        assert_eq!(mdx.language_id(), "markdown");
    }

    #[test]
    fn default_registry_can_process_markdown_files() {
        let registry = default_registry();
        assert!(registry.can_process_file(Path::new("README.md")));
        assert!(registry.can_process_file(Path::new("notes/page.mdx")));
        let langs = registry.supported_languages();
        assert!(langs.iter().any(|l| l == "markdown"));
    }

    #[test]
    fn default_registry_can_process_php_files() {
        let registry = default_registry();
        assert!(registry.can_process_file(Path::new("app/Service.php")));
        let plugin = registry
            .get_plugin_for_file(Path::new("src/User.php"))
            .expect("php plugin");
        assert_eq!(plugin.language_id(), "php");
    }

    #[test]
    fn default_registry_can_process_ruby_files() {
        let registry = default_registry();
        assert!(registry.can_process_file(Path::new("app/models/user.rb")));
        let plugin = registry
            .get_plugin_for_file(Path::new("lib/order.rb"))
            .expect("ruby plugin");
        assert_eq!(plugin.language_id(), "ruby");
    }

    #[test]
    fn default_registry_can_process_puppet_files() {
        let registry = default_registry();
        assert!(registry.can_process_file(Path::new("modules/nginx/manifests/init.pp")));
        let plugin = registry
            .get_plugin_for_file(Path::new("manifests/site.pp"))
            .expect("puppet plugin");
        assert_eq!(plugin.language_id(), "puppet");
    }

    #[test]
    fn default_registry_can_process_erb_files() {
        let registry = default_registry();
        assert!(registry.can_process_file(Path::new("modules/nginx/templates/nginx.conf.erb")));
        let plugin = registry
            .get_plugin_for_file(Path::new("templates/index.html.erb"))
            .expect("erb plugin");
        assert_eq!(plugin.language_id(), "erb");
    }

    #[test]
    fn default_registry_can_process_kotlin_files() {
        let registry = default_registry();
        assert!(registry.can_process_file(Path::new("src/main/kotlin/App.kt")));
        let plugin = registry
            .get_plugin_for_file(Path::new("UserService.kt"))
            .expect("kotlin plugin");
        assert_eq!(plugin.language_id(), "kotlin");
        // Manifest basename wins over `.kts` language extension
        assert!(registry.is_manifest_file(Path::new("build.gradle.kts")));
        assert!(
            registry
                .get_plugin_for_file(Path::new("build.gradle.kts"))
                .is_err()
        );
    }

    #[test]
    fn default_registry_can_process_groovy_files() {
        let registry = default_registry();
        assert!(registry.can_process_file(Path::new("src/Deploy.groovy")));
        let plugin = registry
            .get_plugin_for_file(Path::new("scripts/Job.groovy"))
            .expect("groovy plugin");
        assert_eq!(plugin.language_id(), "groovy");
        assert!(registry.is_manifest_file(Path::new("build.gradle")));
        assert!(
            registry
                .get_plugin_for_file(Path::new("build.gradle"))
                .is_err()
        );
    }

    #[test]
    fn default_registry_can_process_tier2_query_languages() {
        let registry = default_registry();
        for (path, id) in [
            ("src/main.zig", "zig"),
            ("flake.nix", "nix"),
            ("api/v1.proto", "proto"),
            ("contracts/Token.sol", "solidity"),
            ("math.jl", "julia"),
            ("math.R", "r"),
            ("Math.hs", "haskell"),
            ("math.gd", "gdscript"),
            ("math.f90", "fortran"),
            ("adder.v", "verilog"),
            ("Foo.m", "objc"),
            ("solver.matlab", "matlab"),
            ("App.cls", "apex"),
            ("unit1.pas", "pascal"),
            ("script.pl", "perl"),
            ("main.adb", "ada"),
            ("CUSTBILL.cbl", "cobol"),
            ("billing.pli", "pli"),
            ("nightly.jcl", "jcl"),
            ("math.clj", "clojure"),
            ("math.ml", "ocaml"),
            ("math.el", "elisp"),
            ("index.html", "html"),
            ("app.css", "css"),
            ("App.svelte", "svelte"),
            ("schema.graphql", "graphql"),
            ("schema.prisma", "prisma"),
            ("Math.elm", "elm"),
            ("math.erl", "erlang"),
            ("math.gleam", "gleam"),
            ("Math.fs", "fsharp"),
            ("math.rkt", "racket"),
            ("math.scm", "scheme"),
            ("math.d", "d"),
            ("math.odin", "odin"),
            ("kernel.cu", "cuda"),
            ("main.vert", "glsl"),
            ("adder.vhd", "vhdl"),
            ("CMakeLists.cmake", "cmake"),
            ("build.mk", "make"),
            ("defs.bzl", "starlark"),
            ("Math.ps1", "powershell"),
        ] {
            assert!(
                registry.can_process_file(Path::new(path)),
                "expected can_process {path}"
            );
            let plugin = registry
                .get_plugin_for_file(Path::new(path))
                .unwrap_or_else(|_| panic!("{id} plugin for {path}"));
            assert_eq!(plugin.language_id(), id);
        }
    }

    #[test]
    fn m_extension_disambiguates_matlab_vs_objc_by_content() {
        let registry = default_registry();
        let matlab = registry
            .get_plugin_for_file_with_source(
                Path::new("filt.m"),
                b"function [y] = filt(x)\ny = x;\nend\n",
            )
            .expect("matlab");
        assert_eq!(matlab.language_id(), "matlab");
        let objc = registry
            .get_plugin_for_file_with_source(
                Path::new("Foo.m"),
                b"#import <Foundation/Foundation.h>\n@interface Foo\n@end\n",
            )
            .expect("objc");
        assert_eq!(objc.language_id(), "objc");
    }

    #[test]
    fn default_registry_can_process_new_tier1_languages() {
        let registry = default_registry();
        for (path, id) in [
            ("Sources/App/Main.swift", "swift"),
            ("src/main/scala/App.scala", "scala"),
            ("scripts/Job.sc", "scala"),
            ("lib/main.dart", "dart"),
            ("lib/app.ex", "elixir"),
            ("test/app_test.exs", "elixir"),
            ("src/util.lua", "lua"),
            ("scripts/setup.sh", "bash"),
            ("scripts/run.bash", "bash"),
            ("db/schema.sql", "sql"),
            ("infra/main.tf", "hcl"),
            ("infra/vars.tfvars", "hcl"),
            ("config/app.hcl", "hcl"),
        ] {
            assert!(
                registry.can_process_file(Path::new(path)),
                "expected can_process {path}"
            );
            let plugin = registry
                .get_plugin_for_file(Path::new(path))
                .unwrap_or_else(|_| panic!("{id} plugin for {path}"));
            assert_eq!(plugin.language_id(), id, "path {path}");
        }
    }

    #[test]
    fn markdown_not_treated_as_yaml_config() {
        let registry = default_registry();
        assert!(
            registry
                .get_config_plugin_for_file(Path::new("readme.md"))
                .is_err()
        );
    }

    /// Bi-directional sync: each Tier-1 plugin's `declared_ast_handlers()` vs
    /// `{lang}-ast-coverage.json` (non-tautological — handlers come from plugin code).
    #[test]
    fn tier1_declared_ast_handlers_sync_with_coverage_manifests() {
        let registry = default_registry();
        let crates_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
        let mut failures = Vec::new();

        for spec in rgctl_ast_coverage::bundled_specs() {
            if spec.id == "markdown" {
                // Markdown stays on the legacy multi-pass path (not CST jump-table).
                continue;
            }
            let Some(plugin) = registry.get_language_plugin(spec.id) else {
                failures.push(format!("{}: not in default_registry()", spec.id));
                continue;
            };
            let Some(table) = plugin.declared_ast_handlers() else {
                failures.push(format!(
                    "{}: declared_ast_handlers() returned None",
                    spec.id
                ));
                continue;
            };
            let manifest_path = crates_dir.join(spec.crate_dir).join(spec.manifest_file);
            let manifest_json = match std::fs::read_to_string(&manifest_path) {
                Ok(s) => s,
                Err(e) => {
                    failures.push(format!(
                        "{}: read {}: {e}",
                        spec.id,
                        manifest_path.display()
                    ));
                    continue;
                }
            };
            let lang = (spec.language)();
            let issues = rgctl_ast_coverage::check_plugin_manifest_sync(
                spec.id,
                &manifest_json,
                table,
                &lang,
            );
            for issue in issues {
                failures.push(format!("{}: {}", spec.id, issue.message));
            }
        }

        assert!(
            failures.is_empty(),
            "plugin ↔ ast-coverage sync failures:\n{}",
            failures.join("\n")
        );
    }
}

#[cfg(test)]
mod template_tests {
    #![allow(clippy::expect_used)]
    use super::*;
    use std::path::Path;
    #[test]
    fn associations_are_local_and_protected_routes_win() {
        let base = default_registry();
        let mut options = rgctl_registry::TemplateOptions::default();
        options.jinja.include = vec!["web/*.html".into(), "templates/*".into()];
        let a = base
            .with_template_options(Path::new("/a"), options.clone())
            .expect("options");
        let b = base
            .with_template_options(Path::new("/b"), Default::default())
            .expect("options");
        assert_eq!(
            a.get_plugin_for_file(Path::new("/a/web/page.html"))
                .expect("jinja")
                .language_id(),
            "jinja"
        );
        assert_eq!(
            b.get_plugin_for_file(Path::new("/b/web/page.html"))
                .expect("html")
                .language_id(),
            "html"
        );
        for path in [
            "main.j2",
            "main.jinja",
            "main.jinja2",
            "roles/web/templates/nginx.conf",
            "ansible_collections/acme/app/roles/web/templates/index.html",
        ] {
            assert_eq!(
                a.get_plugin_for_file(Path::new(path))
                    .expect(path)
                    .language_id(),
                "jinja"
            );
        }
        for path in [
            "data/config.yaml",
            "inventory.yml",
            "roles/web/defaults/main.yml",
            ".github/workflows/build.yml",
            "Cargo.toml",
        ] {
            assert!(a.get_plugin_for_file(Path::new(path)).is_err(), "{path}");
        }
        assert_eq!(
            a.get_plugin_for_file(Path::new("roles/web/templates/legacy.erb"))
                .expect("erb")
                .language_id(),
            "erb"
        );
        let mut protected = rgctl_registry::TemplateOptions::default();
        protected.jinja.include = vec!["Cargo.toml".into()];
        let protected = base
            .with_template_options(Path::new("/a"), protected)
            .expect("globs");
        assert!(protected.association(Path::new("/a/Cargo.toml")).is_err());
        options.ansible.include = vec!["web/*".into()];
        let conflict = base
            .with_template_options(Path::new("/a"), options)
            .expect("valid globs");
        assert!(conflict.association(Path::new("/a/web/page.html")).is_err());
        let mut invalid = rgctl_registry::TemplateOptions::default();
        invalid.jinja.template_roots = vec!["../outside".into()];
        assert!(
            base.with_template_options(Path::new("/a"), invalid)
                .is_err()
        );
    }
}
