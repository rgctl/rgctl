//! E2E fixtures for `rgctl writeback migrate` (catalog + plan layers).

use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn rgctl_bin() -> PathBuf {
    std::env::var_os("CARGO_BIN_EXE_rgctl")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/debug/rgctl"))
}

fn run_in(repo: &Path, args: &[&str]) -> Output {
    Command::new(rgctl_bin())
        .args(args)
        .current_dir(repo)
        .output()
        .expect("spawn rgctl")
}

fn assert_ok(output: &Output, label: &str) {
    assert!(
        output.status.success(),
        "{label} failed status={:?}\nstdout: {}\nstderr: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn write_java(repo: &Path, rel: &str, body: &str) {
    let path = repo.join(rel);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, body).unwrap();
}

#[test]
fn fixture_a_javaee_to_quarkus() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path().join("repo");
    write_java(
        &repo,
        "src/OrderBean.java",
        r#"package com.acme;

import javax.ejb.Stateless;
import javax.ejb.EJB;
import javax.ejb.TransactionAttribute;
import javax.ejb.TransactionAttributeType;
import javax.persistence.PersistenceContext;
import javax.persistence.EntityManager;
import javax.ws.rs.Path;
import javax.ws.rs.GET;

/**
 * Javadoc must survive migration.
 */
@Stateless
@Path("/orders")
public class OrderBean {
    // keep field comment
    @PersistenceContext(unitName = "primary")
    private EntityManager em;

    @EJB
    private InventoryService inventory;

    @TransactionAttribute(TransactionAttributeType.REQUIRED)
    @GET
    @Path("/ping")
    public String ping() {
        return "ok";
    }
}
"#,
    );

    assert_ok(
        &run_in(&repo, &["discover", ".", "--lossless", "-l", "java"]),
        "discover",
    );
    let out = repo.join("migrated");
    let mig = run_in(
        &repo,
        &[
            "-f",
            "json",
            "writeback",
            "migrate",
            "--from",
            "javaee",
            "--to",
            "quarkus",
            "--output",
            out.to_str().unwrap(),
        ],
    );
    assert_ok(&mig, "migrate javaee→quarkus");
    let report: Value = serde_json::from_slice(&mig.stdout).expect("json");
    assert!(
        report["wave1_edits"].as_u64().unwrap_or(0) + report["wave2_edits"].as_u64().unwrap_or(0)
            > 0,
        "{report}"
    );

    let text = fs::read_to_string(out.join("src/OrderBean.java")).unwrap();
    assert!(text.contains("Javadoc must survive"), "{text}");
    assert!(text.contains("// keep field comment"), "{text}");
    assert!(text.contains("@ApplicationScoped"), "{text}");
    assert!(text.contains("@Inject"), "{text}");
    assert!(text.contains("jakarta."), "{text}");
    assert!(!text.contains("@Stateless"), "{text}");
    assert!(!text.contains("javax.ejb"), "{text}");
}

#[test]
fn fixture_b_spring_to_quarkus_with_worklist() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path().join("repo");
    write_java(
        &repo,
        "src/OrderController.java",
        r#"package com.acme;

import org.springframework.web.bind.annotation.RestController;
import org.springframework.web.bind.annotation.RequestMapping;
import org.springframework.web.bind.annotation.GetMapping;
import org.springframework.web.bind.annotation.PathVariable;
import org.springframework.web.bind.annotation.RequestParam;
import org.springframework.beans.factory.annotation.Autowired;
import org.springframework.beans.factory.annotation.Value;

/** Controller docs. */
@RestController
@RequestMapping("/api/orders")
public class OrderController {
    @Autowired
    private OrderService orders;

    @Value("${orders.max:100}")
    private int max;

    @GetMapping("/{id}")
    public String get(
        @PathVariable("id") Long id,
        @RequestParam("filter") String filter
    ) {
        return filter;
    }
}
"#,
    );
    write_java(
        &repo,
        "src/OrderRepository.java",
        r#"package com.acme;

import org.springframework.data.jpa.repository.JpaRepository;

public interface OrderRepository extends JpaRepository<Order, Long> {}
"#,
    );

    assert_ok(
        &run_in(&repo, &["discover", ".", "--lossless", "-l", "java"]),
        "discover",
    );
    let out = repo.join("migrated");
    let mig = run_in(
        &repo,
        &[
            "-f",
            "json",
            "writeback",
            "migrate",
            "--from",
            "spring",
            "--to",
            "quarkus",
            "--output",
            out.to_str().unwrap(),
        ],
    );
    assert_ok(&mig, "migrate spring→quarkus");
    let report: Value = serde_json::from_slice(&mig.stdout).expect("json");

    let text = fs::read_to_string(out.join("src/OrderController.java")).unwrap();
    assert!(text.contains("Controller docs"), "{text}");
    assert!(text.contains("@ApplicationScoped"), "{text}");
    assert!(text.contains("@Inject"), "{text}");
    assert!(text.contains("@ConfigProperty"), "{text}");
    assert!(text.contains("@PathParam"), "{text}");
    assert!(text.contains("@QueryParam"), "{text}");
    assert!(!text.contains("@Autowired"), "{text}");
    assert!(!text.contains("@RestController"), "{text}");
    // Verify legacy Spring annotation imports were stripped and unused target imports were NOT injected.
    assert!(
        !text.contains("org.springframework.web.bind.annotation"),
        "expected Spring web imports stripped:\n{text}"
    );
    assert!(
        !text.contains("org.springframework.beans.factory.annotation"),
        "expected Spring DI imports stripped:\n{text}"
    );
    assert!(
        !text.contains("jakarta.ws.rs.DELETE"),
        "expected unused @DELETE import not injected:\n{text}"
    );
    assert!(
        !text.contains("jakarta.ws.rs.HeaderParam"),
        "expected unused @HeaderParam import not injected:\n{text}"
    );

    let repo_text = fs::read_to_string(out.join("src/OrderRepository.java")).unwrap();
    assert!(
        !repo_text.contains("jakarta.ws.rs."),
        "OrderRepository has no JAX-RS annotations and must not receive JAX-RS imports:\n{repo_text}"
    );

    let worklist = report["agent_worklist"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    assert!(
        worklist.iter().any(|w| w["symbol"]
            .as_str()
            .unwrap_or("")
            .contains("OrderRepository")
            && w["blast_radius_order"].as_u64().unwrap_or(0) >= 1),
        "expected graph-resolved OrderRepository symbol with blast_radius_order in worklist: {report}"
    );
}

#[test]
fn fixture_c_plan_junit4_to_junit5() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path().join("repo");
    write_java(
        &repo,
        "src/SampleTest.java",
        r#"package com.acme;

import org.junit.Test;
import org.junit.Before;
import org.junit.After;

public class SampleTest {
    @Before
    public void setUp() {}

    @After
    public void tearDown() {}

    @Test
    public void works() {}
}
"#,
    );
    let plan = repo.join("junit4-to-junit5.json");
    fs::write(
        &plan,
        r#"{
  "name": "junit4-to-junit5",
  "language": "java",
  "import_rewrites": {
    "org.junit.Test": "org.junit.jupiter.api.Test",
    "org.junit.Before": "org.junit.jupiter.api.BeforeEach",
    "org.junit.After": "org.junit.jupiter.api.AfterEach"
  },
  "annotation_rewrites": [
    { "from": "Before", "to": "@BeforeEach", "imports": ["org.junit.jupiter.api.BeforeEach"] },
    { "from": "After", "to": "@AfterEach", "imports": ["org.junit.jupiter.api.AfterEach"] },
    { "from": "Test", "to": "@Test", "imports": ["org.junit.jupiter.api.Test"] }
  ]
}"#,
    )
    .unwrap();

    assert_ok(
        &run_in(&repo, &["discover", ".", "--lossless", "-l", "java"]),
        "discover",
    );
    let out = repo.join("migrated");
    let mig = run_in(
        &repo,
        &[
            "-f",
            "json",
            "writeback",
            "migrate",
            "--plan",
            plan.to_str().unwrap(),
            "--output",
            out.to_str().unwrap(),
        ],
    );
    assert_ok(&mig, "migrate plan junit");
    let text = fs::read_to_string(out.join("src/SampleTest.java")).unwrap();
    assert!(text.contains("@BeforeEach"), "{text}");
    assert!(text.contains("@AfterEach"), "{text}");
    assert!(text.contains("org.junit.jupiter.api"), "{text}");
}

#[test]
fn fixture_d_comments_strings_and_method_request_mapping() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path().join("repo");
    write_java(
        &repo,
        "src/ItemController.java",
        r#"package com.acme;

import org.springframework.web.bind.annotation.*;
import javax.persistence.EntityManager;

/**
 * Legacy note: previously used @Autowired and javax.persistence.EntityManager.
 */
@RestController
@RequestMapping("/api/items")
public class ItemController {
    // Line comment mentioning @RestController and javax.persistence.Query
    private static final String LEGACY_PROP = "javax.persistence.query.timeout/@Autowired";

    private EntityManager em;

    @GetMapping(produces = "application/json", value = "/{id}")
    public String getById(@PathVariable("id") String id) {
        return LEGACY_PROP;
    }

    @RequestMapping(value = "/create", method = RequestMethod.POST)
    public String create() {
        return "created";
    }

    @GetMapping
    public String listAll() {
        return "all";
    }
}
"#,
    );

    assert_ok(
        &run_in(&repo, &["discover", ".", "--lossless", "-l", "java"]),
        "discover",
    );
    let mig = run_in(
        &repo,
        &[
            "-f",
            "json",
            "writeback",
            "migrate",
            "--from",
            "spring",
            "--to",
            "quarkus",
            "--in-place",
        ],
    );
    assert_ok(&mig, "migrate in-place");
    let stderr = String::from_utf8_lossy(&mig.stderr);
    assert!(
        !stderr.contains("exited with"),
        "expected clean in-place lossless refresh without subprocess failure: {stderr}"
    );

    let text = fs::read_to_string(repo.join("src/ItemController.java")).unwrap();
    // Comments and string literals must remain untouched!
    assert!(
        text.contains("previously used @Autowired and javax.persistence.EntityManager."),
        "Javadoc was corrupted:\n{text}"
    );
    assert!(
        text.contains("// Line comment mentioning @RestController and javax.persistence.Query"),
        "line comment was corrupted:\n{text}"
    );
    assert!(
        text.contains("\"javax.persistence.query.timeout/@Autowired\""),
        "string literal was corrupted:\n{text}"
    );
    // Multi-attribute @GetMapping(produces = "...", value = "/{id}") must extract "/{id}"
    assert!(
        text.contains("@Path(\"/{id}\")"),
        "expected @Path(\"/{{id}}\") from value= attribute:\n{text}"
    );
    assert!(
        !text.contains("@Path(\"application/json\")"),
        "must not mistake produces= for path:\n{text}"
    );
    // Method-level @RequestMapping(value = "/create", method = RequestMethod.POST) -> @POST + @Path("/create")
    assert!(text.contains("@POST"), "expected @POST:\n{text}");
    assert!(
        text.contains("@Path(\"/create\")"),
        "expected @Path(\"/create\"):\n{text}"
    );
    // Bare @GetMapping -> @GET without empty @Path("")
    assert!(
        !text.contains("@Path(\"\")"),
        "bare @GetMapping must not emit empty @Path(\"\"):\n{text}"
    );
    // Wildcard Spring import removed; jakarta.persistence.EntityManager rewritten
    assert!(!text.contains("org.springframework.web.bind.annotation.*"));
    assert!(text.contains("import jakarta.persistence.EntityManager;"));

    // Verify lossless sidecars are refreshed and pass `rgctl writeback verify --strict`
    let ver = run_in(&repo, &["writeback", "verify", "--strict"]);
    assert_ok(&ver, "writeback verify after --in-place migrate");
}

#[test]
fn fixture_e_openliberty_to_quarkus_and_alias_and_unknown_framework_error() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path().join("repo");
    write_java(
        &repo,
        "src/LibertyOrderService.java",
        r#"package com.acme.liberty;

import com.ibm.websphere.security.auth.WSSubject;
import javax.ejb.Stateless;
import javax.ejb.EJB;
import javax.ejb.TransactionAttribute;
import javax.ejb.TransactionAttributeType;
import javax.ws.rs.GET;
import javax.ws.rs.Path;
import javax.ws.rs.PathParam;

@Stateless
@Path("/orders")
public class LibertyOrderService {
    @EJB
    private InventoryBean inventory;

    @GET
    @Path("/{id}")
    @TransactionAttribute(TransactionAttributeType.REQUIRED)
    public String getOrder(@PathParam("id") String id) {
        String caller = WSSubject.getCallerPrincipal();
        return caller + ":" + id;
    }
}
"#,
    );

    assert_ok(
        &run_in(&repo, &["discover", ".", "--lossless", "-l", "java"]),
        "discover",
    );

    // 1. Unknown framework must return a clear self-correcting error listing valid frameworks and --plan fallback.
    let bad = run_in(
        &repo,
        &[
            "-f",
            "json",
            "writeback",
            "migrate",
            "--from",
            "dropwizard",
            "--to",
            "quarkus",
            "--dry-run",
        ],
    );
    assert!(
        !bad.status.success(),
        "expected unknown framework 'dropwizard' to fail"
    );
    let bad_err = format!(
        "{}{}",
        String::from_utf8_lossy(&bad.stdout),
        String::from_utf8_lossy(&bad.stderr)
    );
    assert!(
        bad_err.contains("Valid catalog frameworks for java")
            && bad_err.contains("openliberty")
            && bad_err.contains("--plan"),
        "expected self-correcting error message with valid frameworks and --plan hint, got: {bad_err}"
    );

    // 2. Test alias resolution (`websphere` -> `openliberty`) and OpenLiberty -> Quarkus migration.
    let out = repo.join("migrated");
    let mig = run_in(
        &repo,
        &[
            "-f",
            "json",
            "writeback",
            "migrate",
            "--from",
            "websphere",
            "--to",
            "quarkus",
            "--output",
            out.to_str().unwrap(),
        ],
    );
    assert_ok(&mig, "migrate openliberty -> quarkus");
    let v: Value = serde_json::from_slice(&mig.stdout).unwrap();
    assert_eq!(v["from_framework"], "openliberty");
    assert_eq!(v["to_framework"], "quarkus");
    assert_eq!(v["recipe_or_plan"], "openliberty→quarkus");

    let text = fs::read_to_string(out.join("src/LibertyOrderService.java")).unwrap();
    assert!(text.contains("@ApplicationScoped"), "{text}");
    assert!(text.contains("@Inject"), "{text}");
    assert!(text.contains("@Transactional"), "{text}");
    assert!(text.contains("import jakarta.ws.rs.GET;"), "{text}");
    assert!(text.contains("import jakarta.ws.rs.Path;"), "{text}");
    assert!(!text.contains("import javax.ejb.Stateless;"), "{text}");

    // WSSubject / getCallerPrincipal must appear in agent_worklist[] and com.ibm.websphere.* counted as remaining legacy import.
    assert!(
        v["legacy_imports_remaining"].as_u64().unwrap_or(0) >= 1,
        "expected com.ibm.websphere import to be counted in legacy_imports_remaining: {v}"
    );
    let wl = v["agent_worklist"].as_array().unwrap();
    assert!(
        wl.iter().any(|w| {
            let r = w["reason"].as_str().unwrap_or("");
            r.contains("WSSubject") || r.contains("getCallerPrincipal")
        }),
        "expected WSSubject/getCallerPrincipal in agent_worklist: {wl:?}"
    );
}
