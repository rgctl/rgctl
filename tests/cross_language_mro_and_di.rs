//! Cross-language MRO, Overrides, preprocessor pruning, and framework DI/route edges.

mod rgctl_harness;

use rgctl::analysis::{GitChurnConfig, compute_and_apply_git_churn};
use rgctl::graph::backend::{GraphBackend, MemoryBackend};
use rgctl::graph::schema::{EdgeType, Node, NodeType};
use rgctl::graph::{SNAPSHOT_FILE, SnapshotNodeStore};
use rgctl_harness::{assert_ok, run_in_repo};
use std::collections::{BTreeSet, HashMap};
use std::fs;
use std::path::Path;
use std::process::Command;
use tempfile::TempDir;

fn write(repo: &Path, rel: &str, body: &str) {
    let path = repo.join(rel);
    if let Some(p) = path.parent() {
        fs::create_dir_all(p).unwrap();
    }
    fs::write(path, body).unwrap();
}

fn discover(repo: &Path, langs: &str) {
    assert_ok(
        &run_in_repo(repo, &["discover", ".", "--languages", langs]),
        "discover",
    );
}

fn open_store(repo: &Path) -> SnapshotNodeStore {
    SnapshotNodeStore::open(&repo.join(".rgctl").join(SNAPSHOT_FILE)).expect("open snapshot")
}

struct GraphView {
    /// id → (name, qualified_name)
    nodes: HashMap<uuid::Uuid, (String, String)>,
    /// (from_qn, to_qn, edge_debug)
    edges: Vec<(String, String, String)>,
}

fn load_view(repo: &Path) -> GraphView {
    let store = open_store(repo);
    let col = store.mmap().columnar().expect("columnar");
    let mut nodes = HashMap::new();
    for idx in 0..col.node_count() {
        let n = col.materialize_node_at(idx).unwrap();
        let qn = n
            .qualified_name
            .as_ref()
            .map(|q| q.to_string())
            .unwrap_or_else(|| n.name.to_string());
        nodes.insert(n.id, (n.name.to_string(), qn));
    }
    let mut edges = Vec::new();
    for idx in 0..col.edge_count() {
        let (from, to, et) = col.edge_at(idx).unwrap();
        let Some((_, fq)) = nodes.get(&from) else {
            continue;
        };
        let Some((_, tq)) = nodes.get(&to) else {
            continue;
        };
        edges.push((fq.clone(), tq.clone(), format!("{et:?}")));
    }
    GraphView { nodes, edges }
}

fn has_edge(view: &GraphView, from_sub: &str, to_sub: &str, edge: &str) -> bool {
    view.edges
        .iter()
        .any(|(f, t, e)| e == edge && f.contains(from_sub) && t.contains(to_sub))
}

fn calls_targets(view: &GraphView, from_sub: &str) -> BTreeSet<String> {
    view.edges
        .iter()
        .filter(|(f, _, e)| e == "Calls" && f.contains(from_sub))
        .map(|(_, t, _)| t.clone())
        .collect()
}

fn node_names(view: &GraphView) -> BTreeSet<String> {
    view.nodes.values().map(|(n, _)| n.clone()).collect()
}

fn git(repo: &Path, args: &[&str]) {
    let status = Command::new("git")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .arg("-C")
        .arg(repo)
        .args(args)
        .status()
        .expect("spawn git");
    assert!(status.success(), "git {args:?} failed: {status}");
}

fn python_diamond_fixture(repo: &Path) {
    write(
        repo,
        "diamond.py",
        r#"
class Base:
    def f(self):
        return "base"

class Left(Base):
    pass

class Right(Base):
    def f(self):
        return "right"

class Child(Left, Right):
    def run(self):
        return self.f()
"#,
    );
}

#[test]
fn python_c3_diamond_extends_and_overrides() {
    let tmp = TempDir::new().unwrap();
    let repo = tmp.path();
    python_diamond_fixture(repo);
    discover(repo, "python");
    let view = load_view(repo);
    assert!(
        has_edge(&view, "Child", "Left", "Extends") && has_edge(&view, "Child", "Right", "Extends"),
        "Child must Extends Left and Right"
    );
    assert!(
        has_edge(&view, "Right.f", "Base.f", "Overrides")
            || has_edge(&view, "Right", "Base", "Overrides"),
        "Right.f Overrides Base.f: {:?}",
        view.edges
            .iter()
            .filter(|(_, _, e)| e == "Overrides")
            .collect::<Vec<_>>()
    );
}

#[test]
fn python_c3_diamond_calls_right_not_base() {
    let tmp = TempDir::new().unwrap();
    let repo = tmp.path();
    python_diamond_fixture(repo);
    discover(repo, "python");
    let view = load_view(repo);
    let targets = calls_targets(&view, "Child.run");
    assert!(
        targets
            .iter()
            .any(|t| t.contains("Right") && t.contains("f")),
        "Child.run should Call Right.f, got {targets:?}"
    );
    assert!(
        !targets
            .iter()
            .any(|t| (t.contains("Base") && t.contains(".f")) || t == "Base.f"),
        "Child.run must not Call Base.f, got {targets:?}"
    );
}

#[test]
fn java_class_over_interface_extends_implements() {
    let tmp = TempDir::new().unwrap();
    let repo = tmp.path();
    write(
        repo,
        "src/Base.java",
        r#"
public class Base {
    public void save() {}
}
"#,
    );
    write(
        repo,
        "src/Repo.java",
        r#"
public interface Repo {
    default void save() {}
}
"#,
    );
    write(
        repo,
        "src/Impl.java",
        r#"
public class Impl extends Base implements Repo {
    public void run() {
        this.save();
    }
}
"#,
    );
    discover(repo, "java");
    let view = load_view(repo);
    assert!(
        has_edge(&view, "Impl", "Base", "Extends"),
        "Impl Extends Base"
    );
    assert!(
        has_edge(&view, "Impl", "Repo", "Implements"),
        "Impl Implements Repo"
    );
}

#[test]
fn java_class_over_interface_default_calls_base_save() {
    let tmp = TempDir::new().unwrap();
    let repo = tmp.path();
    write(
        repo,
        "src/Base.java",
        r#"
public class Base {
    public void save() {}
}
"#,
    );
    write(
        repo,
        "src/Repo.java",
        r#"
public interface Repo {
    default void save() {}
}
"#,
    );
    write(
        repo,
        "src/Impl.java",
        r#"
public class Impl extends Base implements Repo {
    public void run() {
        this.save();
    }
}
"#,
    );
    discover(repo, "java");
    let view = load_view(repo);
    let targets = calls_targets(&view, "Impl.run");
    assert!(
        targets
            .iter()
            .any(|t| t.contains("Base") && t.contains("save")),
        "Impl.run should Call Base.save, got {targets:?}"
    );
    assert!(
        !targets
            .iter()
            .any(|t| t.contains("Repo") && t.contains("save")),
        "Impl.run must not prefer Repo.save, got {targets:?}"
    );
}

#[test]
fn ruby_include_order_last_wins() {
    let tmp = TempDir::new().unwrap();
    let repo = tmp.path();
    write(
        repo,
        "svc.rb",
        r#"
module M1
  def ping
  end
end

module M2
  def ping
  end
end

class Svc
  include M1
  include M2
  def go
    ping()
  end
end
"#,
    );
    discover(repo, "ruby");
    let view = load_view(repo);
    let targets = calls_targets(&view, "Svc#go");
    let targets_alt = if targets.is_empty() {
        calls_targets(&view, "go")
    } else {
        targets
    };
    assert!(
        targets_alt
            .iter()
            .any(|t| t.contains("M2") && t.contains("ping")),
        "Svc#go should Call M2#ping, got {targets_alt:?}; all calls={:?}",
        view.edges
            .iter()
            .filter(|(_, _, e)| e == "Calls")
            .collect::<Vec<_>>()
    );
}

#[test]
fn rust_inherent_over_trait_and_turbofish_helper() {
    let tmp = TempDir::new().unwrap();
    let repo = tmp.path();
    write(
        repo,
        "app.rs",
        r#"
trait Runner {
    fn exec(&self) {}
}

struct App;

impl App {
    fn exec(&self) {
        Self::helper::<u32>();
    }
    fn helper<T>() {}
}

impl Runner for App {}

fn drive(a: &App) {
    a.exec();
}
"#,
    );
    discover(repo, "rust");
    let view = load_view(repo);
    let exec_targets = calls_targets(&view, "App::exec");
    let exec_targets = if exec_targets.is_empty() {
        calls_targets(&view, ".exec")
            .into_iter()
            .chain(calls_targets(&view, "exec"))
            .collect()
    } else {
        exec_targets
    };
    // Narrow: Calls from inherent App::exec
    let from_exec: BTreeSet<_> = view
        .edges
        .iter()
        .filter(|(f, _, e)| {
            e == "Calls"
                && (f.contains("App::exec") || f.ends_with("App.exec") || f == "exec")
                && !f.contains("Runner")
        })
        .map(|(_, t, _)| t.clone())
        .collect();
    assert!(
        from_exec.iter().any(|t| t.contains("helper")),
        "App::exec should Call helper (turbofish), got {from_exec:?} / {exec_targets:?}"
    );
    let drive_targets = calls_targets(&view, "drive");
    assert!(
        drive_targets
            .iter()
            .any(|t| t.contains("App") && t.contains("exec") && !t.contains("Runner")),
        "drive should Call inherent App::exec, got {drive_targets:?}"
    );
}

#[test]
fn cpp_if_zero_prunes_ghost_symbols() {
    let tmp = TempDir::new().unwrap();
    let repo = tmp.path();
    write(
        repo,
        "live.cpp",
        r#"
void target() {}

#if 0
void ghost_call() { vanished(); }
#else
void live_call() { target(); }
#endif
"#,
    );
    discover(repo, "cpp");
    let view = load_view(repo);
    let names = node_names(&view);
    assert!(
        !names.contains("ghost_call") && !names.contains("vanished"),
        "ghost_call/vanished must be pruned, names={names:?}"
    );
    assert!(
        names.contains("live_call") && names.contains("target"),
        "live_call and target must exist, names={names:?}"
    );
    assert!(
        has_edge(&view, "live_call", "target", "Calls"),
        "live_call -> target Calls missing: {:?}",
        view.edges
            .iter()
            .filter(|(_, _, e)| e == "Calls")
            .collect::<Vec<_>>()
    );
}

#[test]
fn nearest_ancestor_overrides_only() {
    let tmp = TempDir::new().unwrap();
    let repo = tmp.path();
    write(
        repo,
        "chain.py",
        r#"
class GrandParent:
    def m(self):
        pass

class Parent(GrandParent):
    def m(self):
        pass

class Child(Parent):
    def m(self):
        pass
"#,
    );
    discover(repo, "python");
    let view = load_view(repo);
    let overrides: Vec<_> = view
        .edges
        .iter()
        .filter(|(_, _, e)| e == "Overrides")
        .cloned()
        .collect();
    assert!(
        !overrides.is_empty(),
        "expected Overrides edges on 3-level hierarchy"
    );
    assert!(
        overrides.iter().any(|(f, t, _)| f.contains("Child")
            && f.contains("m")
            && t.contains("Parent")
            && t.contains("m")),
        "Child.m Overrides Parent.m: {overrides:?}"
    );
    assert!(
        overrides.iter().any(|(f, t, _)| f.contains("Parent")
            && f.contains("m")
            && t.contains("GrandParent")
            && t.contains("m")),
        "Parent.m Overrides GrandParent.m: {overrides:?}"
    );
    assert!(
        !overrides.iter().any(|(f, t, _)| f.contains("Child")
            && f.contains("m")
            && t.contains("GrandParent")
            && t.contains("m")),
        "no transitive Child.m -> GrandParent.m Overrides: {overrides:?}"
    );
}

#[test]
fn fastapi_and_spring_expose_route_and_injects() {
    let tmp = TempDir::new().unwrap();
    let repo = tmp.path();
    write(
        repo,
        "users.py",
        r#"
from fastapi import Depends

@app.get("/users/{id}")
def get_user(id: str, db = Depends(get_db)):
    return db

def get_db():
    return None
"#,
    );
    write(
        repo,
        "OrderController.java",
        r#"
@RestController
@RequestMapping("/api")
public class OrderController {
    @Autowired
    private OrderService orderService;

    @GetMapping("/orders")
    public String list() {
        return "ok";
    }
}
"#,
    );
    discover(repo, "python,java");
    let view = load_view(repo);
    assert!(
        view.edges.iter().any(|(_, t, e)| {
            e == "ExposesRoute" && (t.contains("GET /users") || t.contains("/users"))
        }),
        "expected ExposesRoute GET /users: {:?}",
        view.edges
            .iter()
            .filter(|(_, _, e)| e == "ExposesRoute")
            .collect::<Vec<_>>()
    );
    assert!(
        view.edges.iter().any(|(_, t, e)| {
            e == "ExposesRoute" && (t.contains("GET /api/orders") || t.contains("/orders"))
        }),
        "expected ExposesRoute GET /api/orders"
    );
    assert!(
        view.edges
            .iter()
            .any(|(_, t, e)| e == "Injects" && (t.contains("get_db") || t.contains("OrderService"))),
        "expected Injects edges: {:?}",
        view.edges
            .iter()
            .filter(|(_, _, e)| e == "Injects")
            .collect::<Vec<_>>()
    );

    // CoChangesWith via MemoryBackend + git churn (not full discover --with-git).
    assert!(!EdgeType::CoChangesWith.is_call_traversal());
    let churn_dir = TempDir::new().unwrap();
    let git_repo = churn_dir.path();
    git(git_repo, &["init"]);
    git(git_repo, &["config", "user.email", "test@example.com"]);
    git(git_repo, &["config", "user.name", "Test"]);
    git(git_repo, &["config", "commit.gpgsign", "false"]);
    for rel in ["a.py", "b.py"] {
        write(git_repo, rel, "# v0\n");
        git(git_repo, &["add", rel]);
    }
    git(git_repo, &["commit", "-m", "seed"]);
    for i in 1..=3 {
        write(git_repo, "a.py", &format!("# a{i}\n"));
        write(git_repo, "b.py", &format!("# b{i}\n"));
        git(git_repo, &["add", "a.py", "b.py"]);
        git(git_repo, &["commit", "-m", &format!("ab{i}")]);
    }
    let mut backend = MemoryBackend::new();
    for p in ["a.py", "b.py"] {
        backend
            .insert_node(Node::new(NodeType::File, p.to_string()).with_file_path(p.to_string()))
            .unwrap();
    }
    let config = GitChurnConfig {
        max_commits: 50,
        since: "10.years".into(),
        min_cochange_count: 3,
        min_jaccard: 0.25,
        max_files_per_commit: 10,
        max_edges_per_file: 15,
    };
    let report = compute_and_apply_git_churn(git_repo, &mut backend, &config).unwrap();
    assert!(report.edges_emitted >= 2, "report={report:?}");
    let co = backend.find_edges_by_type(EdgeType::CoChangesWith).unwrap();
    assert!(!co.is_empty(), "expected CoChangesWith edges");

    assert_ok(
        &run_in_repo(
            git_repo,
            &[
                "discover",
                ".",
                "--languages",
                "python",
                "--with-git",
                "--git-since",
                "10.years",
            ],
        ),
        "discover --with-git",
    );
    let git_view = load_view(git_repo);
    assert!(
        git_view
            .edges
            .iter()
            .any(|(f, t, e)| e == "CoChangesWith" && f.contains("a.py") && t.contains("b.py")),
        "expected CoChangesWith in snapshot after discover --with-git, got {:?}",
        git_view
            .edges
            .iter()
            .filter(|(_, _, e)| e == "CoChangesWith")
            .collect::<Vec<_>>()
    );
}

/// Optional CLI mirror: relations JSON can see ExposesRoute after discover.
#[test]
fn relations_cli_lists_exposes_route() {
    let tmp = TempDir::new().unwrap();
    let repo = tmp.path();
    write(
        repo,
        "users.py",
        r#"
from fastapi import Depends

@app.get("/api/orders")
def list_orders(db = Depends(get_db)):
    return []

def get_db():
    pass
"#,
    );
    discover(repo, "python");
    let out = run_in_repo(
        repo,
        &[
            "-f",
            "json",
            "relations",
            "--edge",
            "exposesroute",
            "--direction",
            "out",
            "--limit",
            "50",
        ],
    );
    assert_ok(&out, "relations exposesroute");
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let edges = v
        .get("edges")
        .and_then(|e| e.as_array())
        .cloned()
        .unwrap_or_default();
    assert!(
        !edges.is_empty(),
        "expected seedless ExposesRoute edges, got {v}"
    );
}

#[test]
fn all_16_oop_trait_languages_mro_overrides_inherited_dispatch_and_unqualified_calls() {
    use rgctl_graph::assert_graph_invariants;
    use rgctl_languages::default_registry;
    use rgctl_pipeline::{PipelineConfig, ProcessingPipeline};
    use rgctl_registry::LanguageRegistry;
    use std::sync::{Arc, OnceLock};

    static REGISTRY: OnceLock<Arc<LanguageRegistry>> = OnceLock::new();
    let registry = Arc::clone(REGISTRY.get_or_init(|| Arc::new(default_registry())));

    struct OopCase {
        lang: &'static str,
        file: &'static str,
        code: &'static str,
        override_method: &'static str,
        inherited_owner: &'static str,
        inherited_method: &'static str,
        helper_method: &'static str,
    }

    let cases = [
        OopCase {
            lang: "java",
            file: "src/App.java",
            code: r#"
interface Repo {
    default int inheritedSave(int x) { return x; }
    int overrideMe(int x);
}
class Base {
    public int inheritedSave(int x) { return x + 1; }
    public int overrideMe(int x) { return x; }
}
public class Child extends Base implements Repo {
    @Override
    public int overrideMe(int x) { return x + 2; }
    public int localHelper(int a, int b) { return a + b; }
    public int run() {
        int s = this.inheritedSave(42);
        return localHelper(s, 7);
    }
}
"#,
            override_method: "overrideMe",
            inherited_owner: "Base",
            inherited_method: "inheritedSave",
            helper_method: "localHelper",
        },
        OopCase {
            lang: "kotlin",
            file: "src/App.kt",
            code: r#"
interface Repo {
    fun inheritedSave(x: Int): Int = x
    fun overrideMe(x: Int): Int
}
open class Base {
    open fun inheritedSave(x: Int): Int = x + 1
    open fun overrideMe(x: Int): Int = x
}
class Child : Base(), Repo {
    override fun overrideMe(x: Int): Int = x + 2
    fun localHelper(a: Int, b: Int): Int = a + b
    fun run(): Int {
        val s = this.inheritedSave(42)
        return localHelper(s, 7)
    }
}
"#,
            override_method: "overrideMe",
            inherited_owner: "Base",
            inherited_method: "inheritedSave",
            helper_method: "localHelper",
        },
        OopCase {
            lang: "groovy",
            file: "src/App.groovy",
            code: r#"
interface Repo {
    int overrideMe(int x)
}
class Base {
    int inheritedSave(int x) { return x + 1 }
    int overrideMe(int x) { return x }
}
class Child extends Base implements Repo {
    int overrideMe(int x) { return x + 2 }
    int localHelper(int a, int b) { return a + b }
    int run() {
        int s = this.inheritedSave(42)
        return localHelper(s, 7)
    }
}
"#,
            override_method: "overrideMe",
            inherited_owner: "Base",
            inherited_method: "inheritedSave",
            helper_method: "localHelper",
        },
        OopCase {
            lang: "scala",
            file: "src/App.scala",
            code: r#"
trait Repo {
  def overrideMe(x: Int): Int
}
class Base {
  def inheritedSave(x: Int): Int = x + 1
  def overrideMe(x: Int): Int = x
}
class Child extends Base with Repo {
  override def overrideMe(x: Int): Int = x + 2
  def localHelper(a: Int, b: Int): Int = a + b
  def run(): Int = {
    val s = this.inheritedSave(42)
    localHelper(s, 7)
  }
}
"#,
            override_method: "overrideMe",
            inherited_owner: "Base",
            inherited_method: "inheritedSave",
            helper_method: "localHelper",
        },
        OopCase {
            lang: "csharp",
            file: "src/App.cs",
            code: r#"
public interface IRepo {
    int OverrideMe(int x);
}
public class Base {
    public virtual int InheritedSave(int x) { return x + 1; }
    public virtual int OverrideMe(int x) { return x; }
}
public class Child : Base, IRepo {
    public override int OverrideMe(int x) { return x + 2; }
    public int LocalHelper(int a, int b) { return a + b; }
    public int Run() {
        int s = this.InheritedSave(42);
        return LocalHelper(s, 7);
    }
}
"#,
            override_method: "OverrideMe",
            inherited_owner: "Base",
            inherited_method: "InheritedSave",
            helper_method: "LocalHelper",
        },
        OopCase {
            lang: "cpp",
            file: "src/app.cpp",
            code: r#"
class Base {
public:
    virtual int inheritedSave(int x) { return x + 1; }
    virtual int overrideMe(int x) { return x; }
};
class Child : public Base {
public:
    int overrideMe(int x) override { return x + 2; }
    int localHelper(int a, int b) { return a + b; }
    int run() {
        int s = this->inheritedSave(42);
        return localHelper(s, 7);
    }
};
"#,
            override_method: "overrideMe",
            inherited_owner: "Base",
            inherited_method: "inheritedSave",
            helper_method: "localHelper",
        },
        OopCase {
            lang: "python",
            file: "app.py",
            code: r#"
class Base:
    def inherited_save(self, x):
        return x + 1
    def override_me(self, x):
        return x

class Child(Base):
    def override_me(self, x):
        return x + 2
    def local_helper(self, a, b):
        return a + b
    def run(self):
        s = self.inherited_save(42)
        return self.local_helper(s, 7)
"#,
            override_method: "override_me",
            inherited_owner: "Base",
            inherited_method: "inherited_save",
            helper_method: "local_helper",
        },
        OopCase {
            lang: "ruby",
            file: "app.rb",
            code: r#"
class Base
  def inherited_save(x)
    x + 1
  end
  def override_me(x)
    x
  end
end

class Child < Base
  def override_me(x)
    x + 2
  end
  def local_helper(a, b)
    a + b
  end
  def run
    s = self.inherited_save(42)
    local_helper(s, 7)
  end
end
"#,
            override_method: "override_me",
            inherited_owner: "Base",
            inherited_method: "inherited_save",
            helper_method: "local_helper",
        },
        OopCase {
            lang: "php",
            file: "app.php",
            code: r#"<?php
interface Repo {
    public function overrideMe(int $x): int;
}
class Base {
    public function inheritedSave(int $x): int { return $x + 1; }
    public function overrideMe(int $x): int { return $x; }
}
class Child extends Base implements Repo {
    public function overrideMe(int $x): int { return $x + 2; }
    public function localHelper(int $a, int $b): int { return $a + $b; }
    public function run(): int {
        $s = $this->inheritedSave(42);
        return $this->localHelper($s, 7);
    }
}
"#,
            override_method: "overrideMe",
            inherited_owner: "Base",
            inherited_method: "inheritedSave",
            helper_method: "localHelper",
        },
        OopCase {
            lang: "typescript",
            file: "app.ts",
            code: r#"
interface Repo {
    overrideMe(x: number): number;
}
class Base {
    inheritedSave(x: number): number { return x + 1; }
    overrideMe(x: number): number { return x; }
}
class Child extends Base implements Repo {
    overrideMe(x: number): number { return x + 2; }
    localHelper(a: number, b: number): number { return a + b; }
    run(): number {
        const s = this.inheritedSave(42);
        return this.localHelper(s, 7);
    }
}
"#,
            override_method: "overrideMe",
            inherited_owner: "Base",
            inherited_method: "inheritedSave",
            helper_method: "localHelper",
        },
        OopCase {
            lang: "javascript",
            file: "app.js",
            code: r#"
class Base {
    inheritedSave(x) { return x + 1; }
    overrideMe(x) { return x; }
}
class Child extends Base {
    overrideMe(x) { return x + 2; }
    localHelper(a, b) { return a + b; }
    run() {
        const s = this.inheritedSave(42);
        return this.localHelper(s, 7);
    }
}
"#,
            override_method: "overrideMe",
            inherited_owner: "Base",
            inherited_method: "inheritedSave",
            helper_method: "localHelper",
        },
        OopCase {
            lang: "dart",
            file: "app.dart",
            code: r#"
abstract class Repo {
  int overrideMe(int x);
}
class Base {
  int inheritedSave(int x) => x + 1;
  int overrideMe(int x) => x;
}
class Child extends Base implements Repo {
  @override
  int overrideMe(int x) => x + 2;
  int localHelper(int a, int b) => a + b;
  int run() {
    var s = this.inheritedSave(42);
    return localHelper(s, 7);
  }
}
"#,
            override_method: "overrideMe",
            inherited_owner: "Base",
            inherited_method: "inheritedSave",
            helper_method: "localHelper",
        },
        OopCase {
            lang: "swift",
            file: "App.swift",
            code: r#"
protocol Repo {
    func overrideMe(_ x: Int) -> Int
}
class Base {
    func inheritedSave(_ x: Int) -> Int { return x + 1 }
    func overrideMe(_ x: Int) -> Int { return x }
}
class Child: Base, Repo {
    override func overrideMe(_ x: Int) -> Int { return x + 2 }
    func localHelper(_ a: Int, _ b: Int) -> Int { return a + b }
    func run() -> Int {
        let s = self.inheritedSave(42)
        return localHelper(s, 7)
    }
}
"#,
            override_method: "overrideMe",
            inherited_owner: "Base",
            inherited_method: "inheritedSave",
            helper_method: "localHelper",
        },
        OopCase {
            lang: "rust",
            file: "src/lib.rs",
            code: r#"
pub trait Base {
    fn inherited_save(&self, x: i32) -> i32 { x + 1 }
    fn override_me(&self, x: i32) -> i32 { x }
}
pub struct Child;
impl Base for Child {
    fn override_me(&self, x: i32) -> i32 { x + 2 }
}
pub fn local_helper(a: i32, b: i32) -> i32 { a + b }
impl Child {
    pub fn run(&self) -> i32 {
        let s = self.inherited_save(42);
        local_helper(s, 7)
    }
}
"#,
            override_method: "override_me",
            inherited_owner: "Base",
            inherited_method: "inherited_save",
            helper_method: "local_helper",
        },
        OopCase {
            lang: "go",
            file: "app.go",
            code: r#"package app
type Base struct{}
func (b Base) InheritedSave(x int) int { return x + 1 }
func (b Base) OverrideMe(x int) int { return x }

type Child struct {
    Base
}
func (c Child) OverrideMe(x int) int { return x + 2 }
func localHelper(a int, b int) int { return a + b }
func (c Child) Run() int {
    s := c.InheritedSave(42)
    return localHelper(s, 7)
}
"#,
            override_method: "OverrideMe",
            inherited_owner: "Base",
            inherited_method: "InheritedSave",
            helper_method: "localHelper",
        },
        OopCase {
            lang: "elixir",
            file: "lib/app.ex",
            code: r#"
defmodule Repo do
  @callback override_me(integer()) :: integer()
end

defmodule Base do
  def inherited_save(x), do: x + 1
  defmacro __using__(_opts) do
    quote do
      import Base
    end
  end
end

defmodule Child do
  @behaviour Repo
  use Base
  def override_me(x), do: x + 2
  def local_helper(a, b), do: a + b
  def run do
    s = Base.inherited_save(42)
    local_helper(s, 7)
  end
end
"#,
            override_method: "override_me",
            inherited_owner: "Base",
            inherited_method: "inherited_save",
            helper_method: "local_helper",
        },
    ];

    assert_eq!(
        cases.len(),
        16,
        "must cover all 16 OOP/trait Tier-1 languages"
    );

    for case in &cases {
        let tmp = TempDir::new().unwrap();
        let repo = tmp.path();
        write(repo, case.file, case.code);
        fs::create_dir_all(repo.join(".rgctl")).unwrap();
        let snap = repo.join(".rgctl").join(SNAPSHOT_FILE);
        let pipeline = ProcessingPipeline::with_config(
            Arc::clone(&registry),
            PipelineConfig {
                show_progress: false,
                thread_count: Some(1),
                facts_cache: false,
                ..PipelineConfig::default()
            },
        );
        pipeline
            .process_repository_to_snapshot(repo, &snap, None)
            .unwrap_or_else(|e| panic!("[{}] discover failed: {e}", case.lang));

        let store = open_store(repo);
        let col = store.mmap().columnar().expect("columnar");
        assert_graph_invariants(col)
            .unwrap_or_else(|e| panic!("[{}] invariant failure: {e}", case.lang));

        let view = load_view(repo);

        // 1. Type hierarchy (Extends or Implements from Child -> Base/Repo)
        let has_hierarchy = has_edge(&view, "Child", "Base", "Extends")
            || has_edge(&view, "Child", "Base", "Implements");
        assert!(
            has_hierarchy,
            "[{}] expected Child -> Base Extends/Implements edge, edges={:?}",
            case.lang,
            view.edges
                .iter()
                .filter(|(_, _, e)| e == "Extends" || e == "Implements")
                .collect::<Vec<_>>()
        );

        // 2. MRO Overrides edge
        let has_override = view.edges.iter().any(|(f, t, e)| {
            e == "Overrides"
                && f.contains(case.override_method)
                && t.contains(case.override_method)
                && f != t
        });
        assert!(
            has_override,
            "[{}] expected Overrides edge on {}, overrides={:?}",
            case.lang,
            case.override_method,
            view.edges
                .iter()
                .filter(|(_, _, e)| e == "Overrides")
                .collect::<Vec<_>>()
        );

        // 3. Inherited dispatch from Child.run / Child.Run -> Base.inheritedSave
        let run_targets: BTreeSet<String> = view
            .edges
            .iter()
            .filter(|(f, _, e)| {
                e == "Calls"
                    && (f.ends_with(".run")
                        || f.ends_with("#run")
                        || f.ends_with("::run")
                        || f.ends_with(".Run")
                        || f == "run"
                        || f == "Run")
            })
            .map(|(_, t, _)| t.clone())
            .collect();

        assert!(
            run_targets
                .iter()
                .any(|t| t.contains(case.inherited_owner) && t.contains(case.inherited_method)),
            "[{}] expected Child.run -> {}.{} inherited dispatch, got run_targets={run_targets:?}, all_calls={:?}",
            case.lang,
            case.inherited_owner,
            case.inherited_method,
            view.edges
                .iter()
                .filter(|(_, _, e)| e == "Calls")
                .collect::<Vec<_>>()
        );

        // 4. Unqualified call with arguments inside module/class -> localHelper
        assert!(
            run_targets.iter().any(|t| t.contains(case.helper_method)),
            "[{}] expected unqualified call with args to {}, got run_targets={run_targets:?}",
            case.lang,
            case.helper_method
        );
    }
}
