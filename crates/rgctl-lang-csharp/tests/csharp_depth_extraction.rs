//! Golden correctness + unconditional AST coverage exercise for C#.

use rgctl_lang_csharp::CSharpPlugin;
use rgctl_plugin_api::{LanguagePlugin, SymbolType};
use std::path::Path;

#[test]
fn enum_members_events_and_record() {
    let source = br#"
public enum Status { Pending, Done }

public class Service {
  public event EventHandler Changed;
  public string Name { get; set; }
}

public record Order(string Id);
"#;
    let plugin = CSharpPlugin::new().expect("plugin");
    let out = plugin
        .extract_all(Path::new("depth.cs"), source)
        .expect("extract_all");

    let status = out
        .symbols
        .iter()
        .find(|s| s.name == "Status" && s.symbol_type == SymbolType::Enum)
        .expect("Status");
    assert!(status.fields.iter().any(|f| f.name == "Pending"));
    assert!(status.fields.iter().any(|f| f.name == "Done"));

    let service = out
        .symbols
        .iter()
        .find(|s| s.name == "Service" && s.symbol_type == SymbolType::Class)
        .expect("Service");
    assert!(service.fields.iter().any(|f| f.name == "Changed"));
    assert!(service.fields.iter().any(|f| f.name == "Name"));

    assert!(
        out.symbols.iter().any(|s| {
            s.name == "Order" && s.metadata.get("is_record").and_then(|v| v.as_bool()) == Some(true)
        }),
        "record Order missing"
    );
}

#[test]
fn ast_coverage_all_declared_handlers_exercised() {
    const MANIFEST: &str = include_str!("../csharp-ast-coverage.json");
    let lang: tree_sitter::Language = tree_sitter_c_sharp::LANGUAGE.into();
    let tracker =
        rgctl_ast_coverage::AstCoverageTracker::from_manifest_json("csharp", MANIFEST, &lang)
            .expect("tracker from manifest");
    assert!(
        tracker.expected_len() > 0,
        "manifest should declare active handlers"
    );
    rgctl_ast_coverage::install_hit_callback(tracker);

    let sources: &[(&str, &[u8])] = &[
        (
            "ast_coverage_corpus.cs",
            br#"
extern alias Core;
using System;
using System.Collections.Generic;
using System.Linq;
[assembly: System.Reflection.AssemblyTitle("x")]

#if !DEBUG
#endif

namespace Nested.Ns {
  public class Base {
    public Base() {}
    public Base(int x) {}
  }
  public class Derived : Base {
    public Derived() : base(1) {}
  }
  public class Gen<T, U> where T : class, new() where U : struct {
    public Gen() {}
    public void LocalFn() {
      void inner() { yield break; yield return 1; }
    }
  }
  public record Rec(int X);
  public record R2(int X) : Rec(X);
}

namespace Demo;
public enum Color { Red }
public interface IRepo {
  int this[int i] { get; set; }
}
public delegate void D(int x);
public struct S {
  public event EventHandler Ev;
  public event Action Ae { add {} remove {} }
  ~S() {}
  public static S operator +(S a, S b) => a;
  public static explicit operator int(S s) => 0;
  public int this[int i] { get => i; set {} }
  public int P { get; set; }
  public int F;
}

unsafe public class Widget : IRepo {
  public int this[int i] { get { return i; } set { } }
  public void M(ref int x, out int y, in int z, params int[] arr) {
    y = 0;
    int local = 1;
    var (a, b) = (1, 2);
    int[] xs = new int[1] { 1 };
    int[] ys = new[] { 1, 2 };
    var anon = new { Name = "n" };
    var obj = new Widget();
    Widget w = new();
    Span<int> sp = stackalloc int[2];
    Span<int> sp2 = stackalloc[] { 1, 2 };
    List<int> list = [1, 2, ..xs];
    object o = null;
    string s = $"hi {local,5:X}";
    string raw = """raw""";
    char ch = 'x';
    bool t = true;
    double d = 1.5;
    int n = sizeof(int);
    Type ty = typeof(Widget);
    int tern = local > 0 ? 1 : 0;
    int defv = default(int);
    (int te_a, int te_b) tup_ty = (1, 2);
    TypedReference tr = __makeref(local);
    Type rt = __reftype(tr);
    int rv = __refvalue(tr, int);
    checked { local = checked(local + 1); }
    unsafe { int* q = &local; }
    fixed (int* fp = xs) { }
    lock (obj) { }
    using (var u = (IDisposable)null) { }
    using var u2 = (IDisposable)null;
    try { throw new Exception(); }
    catch (Exception ex) when (ex != null) { }
    finally { }
    if (local is int ip) {}
    if (o is string) {}
    if (o as string != null) {}
    if (local is > 0 and < 10 or not 5) {}
    if (o is (var pa, var pb)) {}
    if (o is (not null)) {}
    if (xs is [1, ..]) {}
    if (o is { Length: 1 }) {}
    switch (o) {
      case int i when i > 0: break;
      case string { Length: 1 }: break;
      case var (ca, cb): break;
      case (int, int): break;
      default: break;
    }
    var sw = o switch {
      int i => i,
      _ => 0
    };
    empty:;
    goto empty;
    for (int i = 0; i < 1; i++) continue;
    foreach (var item in xs) break;
    do { } while (false);
    while (false) { }
    await System.Threading.Tasks.Task.CompletedTask;
    local++;
    --local;
    local = -local;
    int rng = xs[0..1];
    var q = from xx in xs
            join yy in ys on xx equals yy into g
            from zz in g.DefaultIfEmpty()
            let ll = zz
            where ll > 0
            orderby ll ascending
            group ll by ll into gg
            select gg.Key;
    Func<int,int> lam = xx => xx + 1;
    Func<int,int> lam2 = (int xx) => { return xx; };
    D anonm = delegate(int xx) { };
    ref int r = ref local;
    int.TryParse("1", out int parsed);
    var x = o?[0];
    var y = o?.ToString()?.Length;
    System.Action boom = () => throw new Exception();
    Widget w2 = obj with { P = 2 };
    delegate*<int, void> fp = null;
    #region r
    #endregion
    #nullable enable
    #pragma warning disable
    #line 1
    #error e
    #warning w
    #define FOO
    #undef FOO
    #if true
    #elif false
    #else
    #endif
    return;
  }
  async System.Threading.Tasks.Task T() => await System.Threading.Tasks.Task.CompletedTask;
}
"#,
        ),
        (
            "global_statements.cs",
            br#"
using System;
Console.WriteLine("hi");
int x = 1;
"#,
        ),
    ];

    let plugin = CSharpPlugin::new().expect("plugin");
    for (path, src) in sources {
        plugin
            .extract_all(std::path::Path::new(path), src)
            .expect("extract_all");
    }

    let tracker = rgctl_ast_coverage::AstCoverageTracker::global().expect("tracker installed");
    let summary = tracker
        .assert_all_exercised()
        .expect("every active C# AST kind in csharp-ast-coverage.json must be exercised");
    eprintln!("{summary}");
    let hotspots = tracker.unhandled_hotspots(10);
    eprintln!("csharp unhandled hotspots: {hotspots:?}");
}
