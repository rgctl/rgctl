//! Integration tests for `rgctl dead-code` analysis.

use rgctl::analysis::dead_code::{
    DeadCodeAnalyzer, DeadCodeConfidence, DeadCodeKind, DeadCodeReport,
};
use rgctl::graph::SnapshotNodeStore;
use rgctl::graph::backend::{GraphBackend, MemoryBackend};
use rgctl::graph::schema::{Edge, EdgeType, Node, NodeType};

fn snapshot_from_backend(backend: &MemoryBackend) -> (SnapshotNodeStore, std::path::PathBuf) {
    let path = std::env::temp_dir().join(format!("dc_integ_{}.bin", uuid::Uuid::new_v4()));
    rgctl::graph::write_columnar_from_backend(backend, &path).unwrap();
    let store = SnapshotNodeStore::open(&path).unwrap();
    (store, path)
}

fn analyze(backend: &MemoryBackend) -> (DeadCodeReport, std::path::PathBuf) {
    let (store, path) = snapshot_from_backend(backend);
    let analyzer = DeadCodeAnalyzer::new(&store).unwrap();
    let report = analyzer.analyze().unwrap();
    (report, path)
}

#[test]
fn unused_import_with_internal_target_detected() {
    let mut backend = MemoryBackend::new();
    let main_fn = Node::new(NodeType::Function, "main").with_file_path("src/main.rs");
    let target_mod = Node::new(NodeType::Module, "used_module").with_file_path("src/used.rs");
    let used_imp = Node::new(NodeType::Import, "used_module").with_file_path("src/main.rs");
    let unused_imp = Node::new(NodeType::Import, "unused_internal").with_file_path("src/main.rs");
    let unused_target =
        Node::new(NodeType::Module, "unused_internal").with_file_path("src/unused.rs");

    let main_id = main_fn.id;
    let used_imp_id = used_imp.id;
    let target_mod_id = target_mod.id;
    let unused_imp_id = unused_imp.id;
    let unused_target_id = unused_target.id;

    backend.insert_node(main_fn).unwrap();
    backend.insert_node(target_mod).unwrap();
    backend.insert_node(used_imp).unwrap();
    backend.insert_node(unused_imp).unwrap();
    backend.insert_node(unused_target).unwrap();
    backend
        .insert_edge(Edge::new(main_id, used_imp_id, EdgeType::Uses))
        .unwrap();
    backend
        .insert_edge(Edge::new(used_imp_id, target_mod_id, EdgeType::Uses))
        .unwrap();
    backend
        .insert_edge(Edge::new(unused_imp_id, unused_target_id, EdgeType::Uses))
        .unwrap();

    let (report, path) = analyze(&backend);

    let unused: Vec<_> = report
        .findings
        .iter()
        .filter(|f| f.kind == DeadCodeKind::UnusedImport)
        .collect();
    assert!(
        unused.iter().any(|f| f.symbol_name == "unused_internal"),
        "import with internal target but no references should be flagged"
    );
    assert!(
        unused
            .iter()
            .all(|f| f.confidence == DeadCodeConfidence::High)
    );

    let _ = std::fs::remove_file(&path);
}

#[test]
fn external_import_not_flagged() {
    let mut backend = MemoryBackend::new();
    let main_fn = Node::new(NodeType::Function, "main").with_file_path("src/main.rs");
    let ext_import =
        Node::new(NodeType::Import, "std::collections::HashMap").with_file_path("src/main.rs");

    backend.insert_node(main_fn).unwrap();
    backend.insert_node(ext_import).unwrap();

    let (report, path) = analyze(&backend);

    assert!(
        !report
            .findings
            .iter()
            .any(|f| f.kind == DeadCodeKind::UnusedImport),
        "external imports without Imports edges should not be flagged"
    );

    let _ = std::fs::remove_file(&path);
}

#[test]
fn unreferenced_function_detected() {
    let mut backend = MemoryBackend::new();
    let main_fn = Node::new(NodeType::Function, "main").with_file_path("src/main.rs");
    let called = Node::new(NodeType::Function, "called_fn").with_file_path("src/lib.rs");
    let dead = Node::new(NodeType::Function, "dead_fn").with_file_path("src/lib.rs");

    let main_id = main_fn.id;
    let called_id = called.id;

    backend.insert_node(main_fn).unwrap();
    backend.insert_node(called).unwrap();
    backend.insert_node(dead).unwrap();
    backend
        .insert_edge(Edge::new(main_id, called_id, EdgeType::Calls))
        .unwrap();

    let (report, path) = analyze(&backend);

    assert!(
        report
            .findings
            .iter()
            .any(|f| f.symbol_name == "dead_fn" && f.kind == DeadCodeKind::UnreferencedFunction)
    );
    assert!(!report.findings.iter().any(|f| f.symbol_name == "called_fn"));
    assert!(!report.findings.iter().any(|f| f.symbol_name == "main"));

    let _ = std::fs::remove_file(&path);
}

#[test]
fn unreferenced_public_fn_reported_as_low_confidence() {
    let mut backend = MemoryBackend::new();
    let public_fn = Node::new(NodeType::Function, "exported_api")
        .with_file_path("src/api.rs")
        .with_visibility(rgctl::graph::schema::SymbolVisibility::Public);

    backend.insert_node(public_fn).unwrap();

    let (report, path) = analyze(&backend);

    let finding = report
        .findings
        .iter()
        .find(|f| f.symbol_name == "exported_api");
    assert!(
        finding.is_some(),
        "unreferenced public function should be reported"
    );
    assert_eq!(
        finding.unwrap().confidence,
        DeadCodeConfidence::Low,
        "unreferenced public function should have Low confidence"
    );

    let _ = std::fs::remove_file(&path);
}

#[test]
fn public_fn_callees_still_reachable() {
    let mut backend = MemoryBackend::new();
    let public_fn = Node::new(NodeType::Function, "exported_api")
        .with_file_path("src/api.rs")
        .with_visibility(rgctl::graph::schema::SymbolVisibility::Public);
    let callee_fn = Node::new(NodeType::Function, "called_from_pub").with_file_path("src/api.rs");

    let main_fn = Node::new(NodeType::Function, "main").with_file_path("src/main.rs");
    let main_id = main_fn.id;
    let pub_id = public_fn.id;
    let callee_id = callee_fn.id;

    backend.insert_node(main_fn).unwrap();
    backend.insert_node(public_fn).unwrap();
    backend.insert_node(callee_fn).unwrap();
    backend
        .insert_edge(Edge::new(main_id, pub_id, EdgeType::Calls))
        .unwrap();
    backend
        .insert_edge(Edge::new(pub_id, callee_id, EdgeType::Calls))
        .unwrap();

    let (report, path) = analyze(&backend);

    assert!(
        !report
            .findings
            .iter()
            .any(|f| f.symbol_name == "exported_api"),
        "public fn called from main should be reachable"
    );
    assert!(
        !report
            .findings
            .iter()
            .any(|f| f.symbol_name == "called_from_pub"),
        "callee of reachable public fn should be reachable"
    );

    let _ = std::fs::remove_file(&path);
}

#[test]
fn entry_point_class_methods_reachable_via_qn_prefix() {
    // Real `discover` graphs carry `Contains` only for File→Symbol (never
    // Type→Method), so methods of entry-point types are reached through
    // qualified-name prefix propagation.
    let mut backend = MemoryBackend::new();
    let class = Node::new(NodeType::Class, "MyController")
        .with_file_path("src/controller.rs")
        .with_qualified_name("MyController")
        .with_visibility(rgctl::graph::schema::SymbolVisibility::Public);
    let annotation = Node::new(NodeType::Annotation, "RestController");
    let method = Node::new(NodeType::Function, "handle_request")
        .with_file_path("src/controller.rs")
        .with_qualified_name("MyController.handle_request");

    let class_id = class.id;
    let ann_id = annotation.id;

    backend.insert_node(class).unwrap();
    backend.insert_node(annotation).unwrap();
    backend.insert_node(method).unwrap();
    backend
        .insert_edge(Edge::new(class_id, ann_id, EdgeType::AnnotatedWith))
        .unwrap();

    let (report, path) = analyze(&backend);

    assert!(
        !report
            .findings
            .iter()
            .any(|f| f.symbol_name == "handle_request"),
        "methods on entry-point classes should be reachable via qn prefix propagation"
    );

    let _ = std::fs::remove_file(&path);
}

#[test]
fn test_symbols_excluded_by_default() {
    let mut backend = MemoryBackend::new();
    let test_fn = Node::new(NodeType::Function, "test_login").with_file_path("tests/test_auth.rs");
    backend.insert_node(test_fn).unwrap();

    let (report, path) = analyze(&backend);

    assert!(
        !report
            .findings
            .iter()
            .any(|f| f.symbol_name == "test_login"),
        "test symbols should not appear in findings by default"
    );

    let _ = std::fs::remove_file(&path);
}

#[test]
fn report_includes_source_locations() {
    let mut backend = MemoryBackend::new();
    let dead = Node::new(NodeType::Function, "orphan")
        .with_file_path("src/orphan.rs")
        .with_location(10, 25);
    backend.insert_node(dead).unwrap();

    let (report, path) = analyze(&backend);

    let finding = report
        .findings
        .iter()
        .find(|f| f.symbol_name == "orphan")
        .expect("orphan should be reported");
    assert_eq!(finding.file_path.as_deref(), Some("src/orphan.rs"));
    assert_eq!(finding.start_line, Some(10));
    assert_eq!(finding.end_line, Some(25));

    let _ = std::fs::remove_file(&path);
}

#[test]
fn entry_point_config_overrides_default() {
    let mut backend = MemoryBackend::new();
    let custom_entry = Node::new(NodeType::Function, "plugin_init").with_file_path("src/plugin.rs");
    let helper = Node::new(NodeType::Function, "plugin_helper").with_file_path("src/plugin.rs");

    let entry_id = custom_entry.id;
    let helper_id = helper.id;

    backend.insert_node(custom_entry).unwrap();
    backend.insert_node(helper).unwrap();
    backend
        .insert_edge(Edge::new(entry_id, helper_id, EdgeType::Calls))
        .unwrap();

    let (store, path) = snapshot_from_backend(&backend);
    let analyzer = DeadCodeAnalyzer::new(&store)
        .unwrap()
        .with_entry_points(vec!["plugin_init".to_string()]);
    let report = analyzer.analyze().unwrap();

    assert!(
        !report
            .findings
            .iter()
            .any(|f| f.symbol_name == "plugin_init"),
        "configured entry point should not be dead"
    );
    assert!(
        !report
            .findings
            .iter()
            .any(|f| f.symbol_name == "plugin_helper"),
        "symbol called from entry point should be reachable"
    );

    let _ = std::fs::remove_file(&path);
}

#[test]
fn machine_readable_json_output() {
    let mut backend = MemoryBackend::new();
    let dead = Node::new(NodeType::Function, "json_dead")
        .with_file_path("src/dead.rs")
        .with_location(5, 15);
    backend.insert_node(dead).unwrap();

    let (store, path) = snapshot_from_backend(&backend);
    let analyzer = DeadCodeAnalyzer::new(&store).unwrap();
    let report = analyzer.analyze().unwrap();

    let json = serde_json::to_value(&report).unwrap();
    assert_eq!(json["schema_version"], 1);
    assert!(json["summary"]["total_symbols"].is_number());
    assert!(json["summary"]["dead_symbols"].is_number());
    assert!(json["findings"].is_array());

    let findings = json["findings"].as_array().unwrap();
    if let Some(f) = findings.iter().find(|f| f["symbol_name"] == "json_dead") {
        assert!(f["kind"].is_string());
        assert!(f["confidence"].is_string());
        assert!(f["reason"].is_string());
        assert!(f["file_path"].is_string());
    }

    let _ = std::fs::remove_file(&path);
}

// ---------------------------------------------------------------------------
// End-to-end `discover` + `dead-code` tests (real pipeline graphs — no
// hand-crafted `Contains` edges; the graph builder's actual output shape).
// ---------------------------------------------------------------------------

use std::process::Command;

fn rgctl_bin() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_BIN_EXE_rgctl"))
}

/// Discover a scratch repo and return the parsed `dead-code` report.
fn discover_and_analyze(repo: &std::path::Path, extra_args: &[&str]) -> DeadCodeReport {
    let out = Command::new(rgctl_bin())
        .current_dir(repo)
        .args(["discover", "."])
        .output()
        .expect("spawn discover");
    assert!(
        out.status.success(),
        "discover failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let mut args = vec!["-f", "json", "dead-code"];
    args.extend_from_slice(extra_args);
    let out = Command::new(rgctl_bin())
        .current_dir(repo)
        .args(&args)
        .output()
        .expect("spawn dead-code");
    assert!(
        out.status.success(),
        "dead-code failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).expect("parse dead-code report")
}

#[test]
fn e2e_swift_main_struct_methods_reachable() {
    // `@main struct` with **no** `static func main()` — the attribute itself
    // is the program entry; its members must not be flagged.
    let dir = tempfile::TempDir::new().unwrap();
    let repo = dir.path().join("swiftapp");
    std::fs::create_dir_all(&repo).unwrap();
    std::fs::write(
        repo.join("App.swift"),
        r#"@main
struct MySwiftUIApp {
    func setupAppearance() {
        print("setup")
    }
}
"#,
    )
    .unwrap();

    let report = discover_and_analyze(&repo, &[]);
    assert!(
        !report
            .findings
            .iter()
            .any(|f| f.symbol_name == "setupAppearance"),
        "@main struct members must be reachable; got {:?}",
        report
            .findings
            .iter()
            .map(|f| f.symbol_name.as_str())
            .collect::<Vec<_>>()
    );
}

#[test]
fn e2e_scala_extends_app_members_reachable() {
    // `extends App` and `extends Base with App` (the latter emits
    // `Implements`, not `Extends`) — object bodies run at startup.
    let dir = tempfile::TempDir::new().unwrap();
    let repo = dir.path().join("scalaapp");
    std::fs::create_dir_all(&repo).unwrap();
    std::fs::write(
        repo.join("Main.scala"),
        r#"trait Base

object Main extends App {
  def run(): Unit = ()
}

object WithApp extends Base with App {
  def boot(): Unit = ()
}
"#,
    )
    .unwrap();

    let report = discover_and_analyze(&repo, &[]);
    let flagged: Vec<&str> = report
        .findings
        .iter()
        .map(|f| f.symbol_name.as_str())
        .collect();
    assert!(
        !flagged.contains(&"run") && !flagged.contains(&"boot"),
        "`extends App` object members must be reachable; got {flagged:?}"
    );
}

#[test]
fn e2e_scala3_main_annotation_no_collision_with_def_main() {
    // Scala 3 `@main def runTool()` alongside a classic `def main(...)` in
    // another file — the `AnnotatedWith` edge must resolve to an Annotation
    // node (not the `main` function), so `runTool` is an entry point.
    let dir = tempfile::TempDir::new().unwrap();
    let repo = dir.path().join("scalatool");
    std::fs::create_dir_all(&repo).unwrap();
    std::fs::write(repo.join("Tool.scala"), "@main def runTool(): Unit = ()\n").unwrap();
    std::fs::write(
        repo.join("Classic.scala"),
        "object Classic {\n  def main(args: Array[String]): Unit = ()\n}\n",
    )
    .unwrap();

    let report = discover_and_analyze(&repo, &[]);
    assert!(
        !report.findings.iter().any(|f| f.symbol_name == "runTool"),
        "@main def runTool must not collide with def main; got {:?}",
        report
            .findings
            .iter()
            .map(|f| f.symbol_name.as_str())
            .collect::<Vec<_>>()
    );
}

#[test]
fn e2e_rust_pub_fn_classifies_low() {
    let dir = tempfile::TempDir::new().unwrap();
    let repo = dir.path().join("rustapi");
    std::fs::create_dir_all(repo.join("src")).unwrap();
    std::fs::write(
        repo.join("src/lib.rs"),
        "pub fn exported_fn() -> u32 { 1 }\nfn private_fn() -> u32 { 2 }\n",
    )
    .unwrap();

    let report = discover_and_analyze(&repo, &[]);
    let exported = report
        .findings
        .iter()
        .find(|f| f.symbol_name == "exported_fn")
        .expect("exported_fn reported");
    assert_eq!(exported.confidence, DeadCodeConfidence::Low);
    let private = report
        .findings
        .iter()
        .find(|f| f.symbol_name == "private_fn")
        .expect("private_fn reported");
    assert_eq!(private.confidence, DeadCodeConfidence::High);
}

#[test]
fn e2e_c_linkage_and_live_header_prototypes() {
    let dir = tempfile::TempDir::new().unwrap();
    let repo = dir.path().join("caproject");
    std::fs::create_dir_all(&repo).unwrap();
    std::fs::write(
        repo.join("main.c"),
        "#include \"api.h\"\nint main(void) { return live_api(1); }\n",
    )
    .unwrap();
    std::fs::write(repo.join("api.h"), "int live_api(int x);\n").unwrap();
    std::fs::write(
        repo.join("impl.c"),
        "#include \"api.h\"\nint live_api(int x) { return x; }\nstatic int dead_static_fn(void) { return 0; }\nint dead_external_fn(void) { return 1; }\n",
    )
    .unwrap();

    let report = discover_and_analyze(&repo, &[]);
    let names: Vec<&str> = report
        .findings
        .iter()
        .map(|f| f.symbol_name.as_str())
        .collect();
    // Live header prototype: callers bind to the .c implementation, so the
    // bodiless declaration must not be flagged.
    assert!(
        !names.contains(&"live_api")
            || report.findings.iter().any(|f| f.symbol_name == "live_api"
                && f.file_path
                    .as_deref()
                    .is_some_and(|p| p.ends_with("impl.c"))),
        "live .h prototypes must not be flagged as dead code; got {names:?}"
    );
    let static_fn = report
        .findings
        .iter()
        .find(|f| f.symbol_name == "dead_static_fn")
        .expect("dead_static_fn reported");
    assert_eq!(static_fn.confidence, DeadCodeConfidence::High);
    let external_fn = report
        .findings
        .iter()
        .find(|f| f.symbol_name == "dead_external_fn")
        .expect("dead_external_fn reported");
    assert_eq!(external_fn.confidence, DeadCodeConfidence::Low);
}

#[test]
fn e2e_elixir_full_otp_callback_set_seeded() {
    let dir = tempfile::TempDir::new().unwrap();
    let repo = dir.path().join("exotp");
    std::fs::create_dir_all(repo.join("lib")).unwrap();
    std::fs::write(
        repo.join("lib/worker.ex"),
        r#"defmodule Worker do
  use GenServer

  def init(args), do: {:ok, args}
  def handle_call(:ping, _from, state), do: {:reply, :pong, state}
  def handle_info(_msg, state), do: {:noreply, state}
  def handle_continue(_cont, state), do: {:noreply, state}
  def terminate(_reason, _state), do: :ok
  def code_change(_old, state, _extra), do: {:ok, state}
end

defmodule Plain do
  def handle_call(x), do: x
end
"#,
    )
    .unwrap();

    let report = discover_and_analyze(&repo, &[]);
    let flagged: Vec<&str> = report
        .findings
        .iter()
        .map(|f| f.qualified_name.as_deref().unwrap_or(""))
        .collect();
    for cb in [
        "Worker.init",
        "Worker.handle_call",
        "Worker.handle_info",
        "Worker.handle_continue",
        "Worker.terminate",
        "Worker.code_change",
    ] {
        assert!(
            !flagged.contains(&cb),
            "GenServer callback {cb} must not be flagged; got {flagged:?}"
        );
    }
    assert!(
        flagged.contains(&"Plain.handle_call"),
        "Plain.handle_call (no behaviour) stays a dead-code candidate; got {flagged:?}"
    );
}

#[test]
fn e2e_entry_point_flag_wins_on_otp_callback_names() {
    // `--entry-point` must not be bypassed by the OTP-callback deferral.
    let dir = tempfile::TempDir::new().unwrap();
    let repo = dir.path().join("exentry");
    std::fs::create_dir_all(repo.join("lib")).unwrap();
    std::fs::write(
        repo.join("lib/plain.ex"),
        "defmodule Plain do\n  def handle_call(msg), do: inner(msg)\n  defp inner(x), do: x\nend\n",
    )
    .unwrap();

    let report = discover_and_analyze(&repo, &["--entry-point", "Plain.handle_call"]);
    assert!(
        !report
            .findings
            .iter()
            .any(|f| f.qualified_name.as_deref() == Some("Plain.handle_call")),
        "--entry-point Plain.handle_call must seed reachability"
    );
}

#[test]
fn e2e_di_annotated_class_private_helper_stays_dead() {
    // Round-2 review reproducer: a `private` helper inside a DI-annotated
    // class must stay reportable — framework containers never invoke
    // unannotated private methods directly.
    let dir = tempfile::TempDir::new().unwrap();
    let repo = dir.path().join("javadi");
    std::fs::create_dir_all(repo.join("src/main/java/com/acme")).unwrap();
    std::fs::write(
        repo.join("src/main/java/com/acme/OrderService.java"),
        r#"package com.acme;

@Service
public class OrderService {
    public String placeOrder() { return "ok"; }
    private String deadPrivateHelperInService() { return "dead"; }
}
"#,
    )
    .unwrap();

    let report = discover_and_analyze(&repo, &[]);
    let priv_helper = report
        .findings
        .iter()
        .find(|f| f.symbol_name == "deadPrivateHelperInService")
        .expect("private helper inside @Service class must be reported");
    assert_eq!(priv_helper.confidence, DeadCodeConfidence::High);
    assert!(
        !report
            .findings
            .iter()
            .any(|f| f.symbol_name == "placeOrder"),
        "public members of annotated classes stay reachable via propagation"
    );
    // Propagated members seed the BFS but are not counted as entry points.
    assert_eq!(
        report.summary.entry_points, 1,
        "only the @Service class itself is an entry point"
    );
}

#[test]
fn e2e_plugin_export_visibility_tiers() {
    // Round-2 review reproducers: TS/JS `export`, Swift `public`, Kotlin
    // default-`public`, and C++ external linkage must classify as `low`.
    let dir = tempfile::TempDir::new().unwrap();
    let repo = dir.path().join("visapi");
    std::fs::create_dir_all(&repo).unwrap();
    std::fs::write(
        repo.join("api.ts"),
        "export function exportedApi(): number { return 1; }\nfunction tsPrivateFn(): number { return 2; }\n",
    )
    .unwrap();
    std::fs::write(
        repo.join("App.swift"),
        "public func publicSwiftFn() -> Int { return 1 }\nfunc internalSwiftFn() -> Int { return 2 }\nprivate func privateSwiftFn() -> Int { return 3 }\n",
    )
    .unwrap();
    std::fs::write(
        repo.join("Api.kt"),
        "fun defaultApi(): Int = 1\npublic fun pubApi(): Int = 2\nprivate fun privApi(): Int = 3\n",
    )
    .unwrap();
    std::fs::write(
        repo.join("impl.cpp"),
        "int cpp_extern_fn() { return 1; }\nstatic int cpp_static_fn() { return 2; }\n",
    )
    .unwrap();

    let report = discover_and_analyze(&repo, &[]);
    let tier = |name: &str| {
        report
            .findings
            .iter()
            .find(|f| f.symbol_name == name)
            .unwrap_or_else(|| panic!("{name} missing from findings"))
            .confidence
    };
    // Exported / public-by-default / external linkage → Low.
    assert_eq!(tier("exportedApi"), DeadCodeConfidence::Low, "TS export");
    assert_eq!(
        tier("publicSwiftFn"),
        DeadCodeConfidence::Low,
        "Swift public"
    );
    assert_eq!(
        tier("defaultApi"),
        DeadCodeConfidence::Low,
        "Kotlin default"
    );
    assert_eq!(tier("pubApi"), DeadCodeConfidence::Low, "Kotlin public");
    assert_eq!(
        tier("cpp_extern_fn"),
        DeadCodeConfidence::Low,
        "C++ external linkage"
    );
    // Private / internal / non-exported → High.
    assert_eq!(tier("tsPrivateFn"), DeadCodeConfidence::High);
    assert_eq!(
        tier("internalSwiftFn"),
        DeadCodeConfidence::High,
        "Swift internal default"
    );
    assert_eq!(tier("privateSwiftFn"), DeadCodeConfidence::High);
    assert_eq!(tier("privApi"), DeadCodeConfidence::High);
    assert_eq!(
        tier("cpp_static_fn"),
        DeadCodeConfidence::High,
        "C++ static linkage"
    );
}

#[test]
fn e2e_round3_cpp_ts_js_kotlin_visibility_fixes() {
    let dir = tempfile::TempDir::new().unwrap();
    let repo = dir.path().join("round3vis");
    std::fs::create_dir_all(&repo).unwrap();

    std::fs::write(
        repo.join("widget.cpp"),
        r#"class Widget {
private:
    int deadPrivateMethod() { return 1; }
public:
    static int publicStaticFactory() { return 2; }
};
struct Pod {
    int implicitStructPub() { return 3; }
};
namespace {
    int anonHelper() { return 4; }
}
"#,
    )
    .unwrap();

    std::fs::write(
        repo.join("arrows.ts"),
        "export const exportedTsArrow = () => 1;\nexport const exportedTsFnExpr = function () { return 2; };\nconst localTsArrow = () => 3;\n",
    )
    .unwrap();

    std::fs::write(
        repo.join("arrows.js"),
        "export const exportedJsArrow = () => 1;\nexport const exportedJsFnExpr = function () { return 2; };\nconst localJsArrow = () => 3;\n",
    )
    .unwrap();

    std::fs::write(
        repo.join("Singleton.kt"),
        "class Singleton private constructor(val id: Int)\n",
    )
    .unwrap();

    let report = discover_and_analyze(&repo, &[]);
    let tier = |name: &str| {
        report
            .findings
            .iter()
            .find(|f| f.symbol_name == name)
            .unwrap_or_else(|| panic!("{name} missing from findings: {:?}", report.findings))
            .confidence
    };

    // C++ class/struct/anonymous-namespace visibility.
    assert_eq!(tier("deadPrivateMethod"), DeadCodeConfidence::High);
    assert_eq!(tier("publicStaticFactory"), DeadCodeConfidence::Low);
    assert_eq!(tier("implicitStructPub"), DeadCodeConfidence::Low);
    assert_eq!(tier("anonHelper"), DeadCodeConfidence::High);

    // TS & JS exported arrow / function expressions vs unexported locals.
    assert_eq!(tier("exportedTsArrow"), DeadCodeConfidence::Low);
    assert_eq!(tier("exportedTsFnExpr"), DeadCodeConfidence::Low);
    assert_eq!(tier("localTsArrow"), DeadCodeConfidence::High);
    assert_eq!(tier("exportedJsArrow"), DeadCodeConfidence::Low);
    assert_eq!(tier("exportedJsFnExpr"), DeadCodeConfidence::Low);
    assert_eq!(tier("localJsArrow"), DeadCodeConfidence::High);

    // Kotlin `class Singleton private constructor(...)` — class is Low, and
    // the primary constructor row in `graph.snapshot.bin` preserves `Private`
    // visibility (not overwritten by the outer class's `Public`).
    let singleton_class = report
        .findings
        .iter()
        .find(|f| f.qualified_name.as_deref() == Some("Singleton") && f.node_type == "Class")
        .expect("Singleton class finding");
    assert_eq!(singleton_class.confidence, DeadCodeConfidence::Low);

    let store = rgctl_graph::SnapshotNodeStore::open(&repo.join(".rgctl/graph.snapshot.bin"))
        .expect("open snapshot");
    let col = store.columnar().expect("columnar");
    let mut found_ctor = false;
    for idx in 0..col.node_count() {
        if col
            .node_qualified_name_str(idx)
            .unwrap()
            .is_some_and(|qn| qn.starts_with("Singleton.<init>"))
        {
            assert_eq!(
                col.node_visibility_at(idx).unwrap(),
                rgctl_plugin_api::SymbolVisibility::Private,
                "Singleton.<init> packed hot-column visibility must stay Private"
            );
            found_ctor = true;
        }
    }
    assert!(found_ctor, "Singleton.<init> node must exist in snapshot");
}

#[test]
fn e2e_v3_snapshot_incremental_update_upgrades_without_poisoning_rows() {
    // Simulate upgrading from v0.4.30 (`format_version = 3`, zero packed
    // visibility/declaration bits in `NodeRow.node_type`) and running
    // incremental `rgctl update` / `rgctl update --files`. Untouched `.h`
    // prototypes must not lose `NODE_DECLARATION_BIT` and untouched `pub fn`s
    // must not lose their `Public` visibility bits.
    let downgrade_snapshot_to_v3 = |snap_path: &std::path::Path| {
        let mut bytes = std::fs::read(snap_path).expect("read snapshot");
        bytes[4..8].copy_from_slice(&3u32.to_le_bytes());
        let node_count = u64::from_le_bytes(bytes[12..20].try_into().unwrap()) as usize;
        let offset_nodes = u64::from_le_bytes(bytes[92..100].try_into().unwrap()) as usize;
        for i in 0..node_count {
            let nt_off = offset_nodes + i * 64 + 16;
            let raw = u16::from_le_bytes(bytes[nt_off..nt_off + 2].try_into().unwrap());
            bytes[nt_off..nt_off + 2].copy_from_slice(&(raw & 0x0FFF).to_le_bytes());
        }
        std::fs::write(snap_path, bytes).expect("write downgraded v3 snapshot");
    };

    for use_files_flag in [false, true] {
        let dir = tempfile::TempDir::new().unwrap();
        let repo = dir.path().join("v3upgrade");
        std::fs::create_dir_all(&repo).unwrap();
        std::fs::write(repo.join("api.h"), "int untouched_header_proto(int x);\n").unwrap();
        std::fs::write(
            repo.join("keep.rs"),
            "pub fn untouched_pub_fn() -> i32 { 1 }\n",
        )
        .unwrap();
        std::fs::write(repo.join("edit.rs"), "fn edited_local_fn() -> i32 { 2 }\n").unwrap();

        let out = Command::new(rgctl_bin())
            .current_dir(&repo)
            .args(["discover", "."])
            .output()
            .expect("discover");
        assert!(out.status.success());

        let snap_path = repo.join(".rgctl/graph.snapshot.bin");
        downgrade_snapshot_to_v3(&snap_path);

        // Edit only `edit.rs`, leaving `api.h` and `keep.rs` untouched.
        std::fs::write(repo.join("edit.rs"), "fn edited_local_fn() -> i32 { 3 }\n").unwrap();

        let update_args: &[&str] = if use_files_flag {
            &["update", ".", "--files", "edit.rs"]
        } else {
            &["update", "."]
        };
        let out = Command::new(rgctl_bin())
            .current_dir(&repo)
            .args(update_args)
            .output()
            .expect("update");
        assert!(
            out.status.success(),
            "update failed ({update_args:?}): {}",
            String::from_utf8_lossy(&out.stderr)
        );

        let store = rgctl_graph::SnapshotNodeStore::open(&snap_path).unwrap();
        assert_eq!(
            store.columnar().unwrap().format_version(),
            rgctl_graph::COLUMNAR_SNAPSHOT_VERSION,
            "snapshot must upgrade to v4 after update ({update_args:?})"
        );
        drop(store);

        let out = Command::new(rgctl_bin())
            .current_dir(&repo)
            .args(["-f", "json", "dead-code"])
            .output()
            .expect("dead-code");
        assert!(out.status.success());
        let report: DeadCodeReport = serde_json::from_slice(&out.stdout).unwrap();

        assert!(
            !report
                .findings
                .iter()
                .any(|f| f.symbol_name == "untouched_header_proto"),
            "untouched .h prototype must not be poisoned into a dead function ({update_args:?}): {:?}",
            report.findings
        );
        let pub_fn = report
            .findings
            .iter()
            .find(|f| f.symbol_name == "untouched_pub_fn")
            .expect("untouched_pub_fn reported");
        assert_eq!(
            pub_fn.confidence,
            DeadCodeConfidence::Low,
            "untouched pub fn must retain Low confidence after v3->v4 update ({update_args:?})"
        );
    }
}
