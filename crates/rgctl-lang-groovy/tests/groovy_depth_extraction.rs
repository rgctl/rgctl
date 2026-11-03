//! Golden correctness + unconditional AST coverage exercise for Groovy.

use rgctl_lang_groovy::GroovyPlugin;
use rgctl_plugin_api::{LanguagePlugin, RelationType, SymbolType};
use std::path::Path;

#[test]
fn class_extends_implements_and_calls() {
    let source = br#"
package demo
import java.util.List

interface Named {
    String label()
}

class Base {}
class Widget extends Base implements Named {
    String label() {
        return new String("w")
    }

    void run(Named n) {
        n.label()
        for (String s : ["a"]) {
            println s
        }
    }
}
"#;
    let plugin = GroovyPlugin::new().expect("plugin");
    let out = plugin
        .extract_all(Path::new("Widget.groovy"), source)
        .expect("extract_all");

    assert!(
        out.symbols
            .iter()
            .any(|s| s.name == "Named" && s.symbol_type == SymbolType::Interface),
        "Named interface missing: {:?}",
        out.symbols
            .iter()
            .map(|s| (&s.name, &s.symbol_type))
            .collect::<Vec<_>>()
    );
    assert!(
        out.relations
            .iter()
            .any(|r| r.relation_type == RelationType::Extends && r.to.contains("Base")),
        "extends Base missing: {:?}",
        out.relations
    );
    assert!(
        out.relations
            .iter()
            .any(|r| r.relation_type == RelationType::Implements && r.to.contains("Named")),
        "implements Named missing: {:?}",
        out.relations
    );
    assert!(
        out.relations
            .iter()
            .any(|r| r.relation_type == RelationType::Calls && r.to.contains("label")),
        "method_invocation Calls missing: {:?}",
        out.relations
    );
}

#[test]
fn ast_coverage_all_declared_handlers_exercised() {
    const MANIFEST: &str = include_str!("../groovy-ast-coverage.json");
    let lang: tree_sitter::Language = tree_sitter_groovy::LANGUAGE.into();
    let tracker =
        rgctl_ast_coverage::AstCoverageTracker::from_manifest_json("groovy", MANIFEST, &lang)
            .expect("tracker from manifest");
    assert!(
        tracker.expected_len() > 0,
        "manifest should declare active handlers"
    );
    rgctl_ast_coverage::install_hit_callback(tracker);

    let sources: &[(&str, &[u8])] = &[
        (
            "ast_coverage_corpus.groovy",
            br#"
package demo
import java.util.List

@Target([ElementType.TYPE_USE, ElementType.TYPE, ElementType.PARAMETER])
@interface Tag {
    String value() default "x"
}

interface Repo<T extends CharSequence> {
    int MAX = 10
    void save(T item) throws Exception
}

interface SubRepo extends Repo<String> {}

enum Kind {
    ALPHA
}

record Pair(int x, int y) {
    public Pair {
        assert x >= 0
    }
}

sealed class Base permits Widget {
    Base(int v) {}
    void save(String item) throws Exception {}
}

@Tag("w")
class Widget extends Base implements Repo<String> {
    private int count = 0

    static {
        int boot = 1
    }

    Widget() {
        super(1)
    }

    @Override
    public void save(java.lang.@Tag("p") String item, String... extra) throws Exception {
        List<@Tag("u") String> taggedList = List.of("a")
        Class<?> cls = Widget.class
        int[] arr = new int[2]
        int[] arr2 = new int[]{1, 2}
        Runnable r = this::helper
        Runnable r2 = Widget::helper
        def fn = { a, b -> a + b }
        java.util.function.BiFunction<Integer, Integer, Integer> lam = (a, b) -> a + b
        int local = -count
        local++
        local = (local > 0 && arr.length > 0) ? 1 : 2
        def nums = 1..3
        def map = [a: 1, b: 2]
        println "hi"
        outer:
        while (local < 5) {
            do {
                for (int i = 0; i < 2; i++) {
                    if (i == 1) {
                        continue outer
                    }
                    break
                }
            } while (false)
            break
        }
        for (String s : ["a"]) {
            synchronized (this) {
                switch (s) {
                    case "a":
                        break
                    default:
                        break
                }
            }
        }
        Object obj = new Pair(1, 2)
        if (obj instanceof String str) {
            this.save(str)
        }
        int res = switch (obj) {
            case Pair(int px, int py) when px > 0 -> {
                yield px + py
            }
            case String tp -> tp.length()
            default -> 0
        }
        try {
            helper()
        } catch (RuntimeException re) {
            helper()
        }
        try (AutoCloseable ac = () -> {}) {
            new Widget()
        } catch (Exception e) {
            throw e
        } finally {
            helper()
        }
        this.count
        return
    }

    void helper() {}
}

def scriptHelper(String x) {
    x
}
"#,
        ),
        (
            "module-info.groovy",
            br#"
module demo.mod {
    requires java.base;
    exports demo;
    opens demo to java.base;
    uses demo.Repo;
    provides demo.Repo with demo.Widget;
}
"#,
        ),
    ];

    let plugin = GroovyPlugin::new().expect("plugin");
    for (path, src) in sources {
        plugin
            .extract_all(std::path::Path::new(path), src)
            .expect("extract_all");
    }

    let tracker = rgctl_ast_coverage::AstCoverageTracker::global().expect("tracker installed");
    let summary = tracker
        .assert_all_exercised()
        .expect("every active Groovy AST kind in groovy-ast-coverage.json must be exercised");
    eprintln!("{summary}");
    let hotspots = tracker.unhandled_hotspots(10);
    eprintln!("groovy unhandled hotspots: {hotspots:?}");
}
