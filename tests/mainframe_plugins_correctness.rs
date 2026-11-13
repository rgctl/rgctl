//! Contract correctness suite for the 3 mainframe / batch plugins (`cobol`, `pli`, `jcl`).
//!
//! Verifies multi-file symbol/call/copybook/dataset extraction through the full
//! `ProcessingPipeline -> ColumnarGraphMmap` snapshot path and enforces `INV-1..INV-9`.

use rgctl_graph::schema::{EdgeType, Node, NodeType};
use rgctl_graph::{SNAPSHOT_FILE, SnapshotNodeStore, assert_graph_invariants};
use rgctl_languages::default_registry;
use rgctl_pipeline::{PipelineConfig, ProcessingPipeline};
use rgctl_registry::LanguageRegistry;
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::sync::{Arc, OnceLock};
use tempfile::TempDir;

fn shared_registry() -> Arc<LanguageRegistry> {
    static REGISTRY: OnceLock<Arc<LanguageRegistry>> = OnceLock::new();
    Arc::clone(REGISTRY.get_or_init(|| Arc::new(default_registry())))
}

fn write_file(repo: &Path, rel: &str, content: &str) {
    let path = repo.join(rel);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, content).unwrap();
}

fn ingest_and_verify(repo: &Path) -> (Vec<Node>, Vec<(String, String, EdgeType)>) {
    fs::create_dir_all(repo.join(".rgctl")).unwrap();
    let snap = repo.join(".rgctl").join(SNAPSHOT_FILE);
    let pipeline = ProcessingPipeline::with_config(
        shared_registry(),
        PipelineConfig {
            show_progress: false,
            thread_count: Some(1),
            facts_cache: false,
            ..PipelineConfig::default()
        },
    );
    pipeline
        .process_repository_to_snapshot(repo, &snap, None)
        .expect("mainframe snapshot ingest");
    let store = SnapshotNodeStore::open(&snap).expect("open snapshot");
    let col = store.mmap().columnar().expect("columnar");
    assert_graph_invariants(col).expect("INV-1..INV-9 must hold on mainframe snapshot");

    let mut nodes = Vec::new();
    let mut by_id: HashMap<uuid::Uuid, String> = HashMap::new();
    for idx in 0..col.node_count() {
        let n = col.materialize_node_at(idx).expect("materialize node");
        let label = n
            .qualified_name
            .as_ref()
            .map(|q| q.to_string())
            .unwrap_or_else(|| n.name.to_string());
        by_id.insert(n.id, label);
        nodes.push(n);
    }
    let mut edges = Vec::new();
    for idx in 0..col.edge_count() {
        let (from, to, et) = col.edge_at(idx).expect("edge_at");
        if let (Some(f), Some(t)) = (by_id.get(&from), by_id.get(&to)) {
            edges.push((f.clone(), t.clone(), et));
        }
    }
    (nodes, edges)
}

#[test]
fn cobol_program_sections_paragraphs_copybooks_records_and_calls() {
    let tmp = TempDir::new().unwrap();
    let repo = tmp.path();

    write_file(
        repo,
        "copy/CUSTREC.cpy",
        r#"       01  CUST-COPY-REC.
           05 CUST-COPY-ID PIC X(10).
"#,
    );
    write_file(
        repo,
        "src/TAXUTIL.cbl",
        r#"       IDENTIFICATION DIVISION.
       PROGRAM-ID. TAXUTIL.
       PROCEDURE DIVISION.
       ENTRY-PARA.
           GOBACK.
"#,
    );
    write_file(
        repo,
        "src/CICSAUDT.cbl",
        r#"       IDENTIFICATION DIVISION.
       PROGRAM-ID. CICSAUDT.
       PROCEDURE DIVISION.
       AUDIT-ENTRY.
           GOBACK.
"#,
    );
    write_file(
        repo,
        "src/CUSTBILL.cbl",
        r#"       IDENTIFICATION DIVISION.
       PROGRAM-ID. CUSTBILL.
       DATA DIVISION.
       WORKING-STORAGE SECTION.
       01  BILLING-RECORD.
           05 BILL-AMOUNT PIC 9(7)V99.
       PROCEDURE DIVISION.
       BILLING-DRIVER SECTION.
       MAIN-PARA.
           COPY CUSTREC.
           EXEC SQL INCLUDE SQLCA END-EXEC.
           PERFORM CALC-TAX
           CALL 'TAXUTIL'
           EXEC CICS LINK PROGRAM('CICSAUDT') END-EXEC.
           STOP RUN.
       CALC-TAX.
           EXIT.
"#,
    );

    let (nodes, edges) = ingest_and_verify(repo);

    // Program module
    assert!(
        nodes
            .iter()
            .any(|n| n.node_type == NodeType::Module && n.name.as_str() == "CUSTBILL"),
        "expected CUSTBILL Module node, got {nodes:?}"
    );
    // 01 record Struct/Class
    assert!(
        nodes.iter().any(|n| {
            matches!(n.node_type, NodeType::Struct | NodeType::Class)
                && n.name.as_str() == "BILLING-RECORD"
        }),
        "expected 01 BILLING-RECORD Struct/Class node, got {nodes:?}"
    );
    // Section & paragraph Functions with non-overlapping multi-line spans
    for fn_qn in [
        "CUSTBILL::BILLING-DRIVER",
        "CUSTBILL::MAIN-PARA",
        "CUSTBILL::CALC-TAX",
    ] {
        assert!(
            nodes.iter().any(|n| {
                n.node_type == NodeType::Function && n.qualified_name.as_deref() == Some(fn_qn)
            }),
            "expected COBOL Function {fn_qn}, got {nodes:?}"
        );
    }
    // COPY + EXEC SQL INCLUDE Imports
    for imp in ["CUSTREC", "SQLCA"] {
        assert!(
            nodes
                .iter()
                .any(|n| n.node_type == NodeType::Import && n.name.contains(imp)),
            "expected COBOL Import {imp}, got {nodes:?}"
        );
    }
    // PERFORM + CALL + EXEC CICS LINK Calls edges
    assert!(
        edges.iter().any(|(f, t, et)| {
            *et == EdgeType::Calls && f.contains("MAIN-PARA") && t.contains("CALC-TAX")
        }),
        "expected PERFORM edge MAIN-PARA -> CALC-TAX, edges={edges:?}"
    );
    assert!(
        edges.iter().any(|(f, t, et)| {
            *et == EdgeType::Calls && f.contains("MAIN-PARA") && t.contains("TAXUTIL")
        }),
        "expected CALL edge MAIN-PARA -> TAXUTIL, edges={edges:?}"
    );
    assert!(
        edges.iter().any(|(f, t, et)| {
            *et == EdgeType::Calls && f.contains("MAIN-PARA") && t.contains("CICSAUDT")
        }),
        "expected EXEC CICS LINK edge MAIN-PARA -> CICSAUDT, edges={edges:?}"
    );
}

#[test]
fn pli_package_nested_procedures_include_dcl1_call_fetch_and_cics() {
    let tmp = TempDir::new().unwrap();
    let repo = tmp.path();

    write_file(
        repo,
        "inc/CUSTINC.inc",
        r#"DCL 1 SHARED_HDR,
      2 HDR_VER FIXED BIN(31);
"#,
    );
    write_file(
        repo,
        "src/dyntax.pli",
        r#"DYNTAX: PROC OPTIONS(MAIN);
END DYNTAX;
"#,
    );
    write_file(
        repo,
        "src/cicspost.pli",
        r#"CICSPOST: PROC OPTIONS(MAIN);
END CICSPOST;
"#,
    );
    write_file(
        repo,
        "src/billing.pli",
        r#"ACCTS_PKG: PACKAGE;
  %INCLUDE CUSTINC;
  DCL 1 CUST_LEDGER,
        2 ACCT_NO CHAR(12),
        2 BALANCE DEC FIXED(11,2);

  LOG_AUDIT: PROC(MSG);
    DCL MSG CHAR(32);
  END LOG_AUDIT;

  BILL_MAIN: PROC OPTIONS(MAIN);
    COMPUTE_STEP: PROC;
      CALL LOG_AUDIT('OK');
      FETCH DYNTAX;
      EXEC CICS LINK PROGRAM('CICSPOST');
    END COMPUTE_STEP;
    CALL COMPUTE_STEP;
  END BILL_MAIN;
END ACCTS_PKG;
"#,
    );

    let (nodes, edges) = ingest_and_verify(repo);

    // PACKAGE Module
    assert!(
        nodes
            .iter()
            .any(|n| n.node_type == NodeType::Module && n.name.as_str() == "ACCTS_PKG"),
        "expected PL/I PACKAGE Module ACCTS_PKG, got {nodes:?}"
    );
    // %INCLUDE Import
    assert!(
        nodes
            .iter()
            .any(|n| n.node_type == NodeType::Import && n.name.contains("CUSTINC")),
        "expected PL/I %INCLUDE CUSTINC Import, got {nodes:?}"
    );
    // DCL 1 structure
    assert!(
        nodes.iter().any(|n| {
            matches!(n.node_type, NodeType::Struct | NodeType::Class)
                && n.name.as_str() == "CUST_LEDGER"
        }),
        "expected PL/I DCL 1 CUST_LEDGER structure, got {nodes:?}"
    );
    // Nested PROC hierarchy
    for proc_name in ["LOG_AUDIT", "BILL_MAIN", "COMPUTE_STEP"] {
        assert!(
            nodes
                .iter()
                .any(|n| n.node_type == NodeType::Function && n.name.as_str() == proc_name),
            "expected PL/I PROC {proc_name}, got {nodes:?}"
        );
    }
    // CALL, FETCH, and EXEC CICS LINK Calls edges from innermost COMPUTE_STEP
    for callee in ["LOG_AUDIT", "DYNTAX", "CICSPOST"] {
        assert!(
            edges.iter().any(|(f, t, et)| {
                *et == EdgeType::Calls && f.contains("COMPUTE_STEP") && t.contains(callee)
            }),
            "expected PL/I Calls edge COMPUTE_STEP -> {callee}, edges={edges:?}"
        );
    }
}

#[test]
fn jcl_job_instream_proc_exec_pgm_proc_include_and_dd_dataset_references() {
    let tmp = TempDir::new().unwrap();
    let repo = tmp.path();

    write_file(
        repo,
        "cobol/CUSTBILL.cbl",
        r#"       IDENTIFICATION DIVISION.
       PROGRAM-ID. CUSTBILL.
       PROCEDURE DIVISION.
       MAIN-PARA.
           STOP RUN.
"#,
    );
    write_file(
        repo,
        "cobol/ACCTS_RPT.cbl",
        r#"       IDENTIFICATION DIVISION.
       PROGRAM-ID. ACCTS_RPT.
       PROCEDURE DIVISION.
       RPT-MAIN.
           STOP RUN.
"#,
    );
    write_file(
        repo,
        "cntl/NIGHTLY.jcl",
        r#"//NIGHTLY  JOB (ACCT),'BATCH LEDGER',CLASS=A,MSGCLASS=X
//         INCLUDE MEMBER=ENVSET
//BILLPROC PROC
//PSTEP1   EXEC PGM=CUSTBILL
//         PEND
//STEP1    EXEC PROC=BILLPROC
//OUTDD    DD DSN=PROD.LEDGER.WORK,DISP=(NEW,CATLG,DELETE)
//STEP2    EXEC PGM=ACCTS_RPT
//INDD     DD DSN=PROD.LEDGER.WORK,DISP=SHR
"#,
    );

    let (nodes, edges) = ingest_and_verify(repo);

    // Every mainframe symbol node carries concrete byte spans so INV-8/INV-9 actively check it
    for sym in nodes.iter().filter(|n| n.node_type != NodeType::File) {
        assert!(
            sym.start_byte.is_some() && sym.end_byte.is_some(),
            "expected concrete byte spans on mainframe node {sym:?}"
        );
    }

    // JOB Module
    assert!(
        nodes
            .iter()
            .any(|n| n.node_type == NodeType::Module && n.name.as_str() == "NIGHTLY"),
        "expected JCL JOB Module NIGHTLY, got {nodes:?}"
    );
    // INCLUDE MEMBER Import
    assert!(
        nodes
            .iter()
            .any(|n| n.node_type == NodeType::Import && n.name.contains("ENVSET")),
        "expected JCL INCLUDE MEMBER=ENVSET Import, got {nodes:?}"
    );
    // PROC and EXEC steps, including PEND restoring NIGHTLY scope for STEP1/STEP2
    for (step, qn) in [
        ("BILLPROC", "BILLPROC"),
        ("PSTEP1", "BILLPROC::PSTEP1"),
        ("STEP1", "NIGHTLY::STEP1"),
        ("STEP2", "NIGHTLY::STEP2"),
    ] {
        assert!(
            nodes.iter().any(|n| {
                n.node_type == NodeType::Function
                    && n.name.as_str() == step
                    && n.qualified_name.as_deref() == Some(qn)
            }),
            "expected JCL Function {step} with qualified_name {qn}, got {nodes:?}"
        );
    }
    // EXEC PGM= and EXEC PROC= Calls
    assert!(
        edges.iter().any(|(f, t, et)| {
            *et == EdgeType::Calls && f.contains("PSTEP1") && t.contains("CUSTBILL")
        }),
        "expected JCL EXEC PGM=CUSTBILL Calls edge, edges={edges:?}"
    );
    assert!(
        edges.iter().any(|(f, t, et)| {
            *et == EdgeType::Calls && f.contains("STEP1") && t.contains("BILLPROC")
        }),
        "expected JCL EXEC PROC=BILLPROC Calls edge, edges={edges:?}"
    );
    // DD DSN= dataset node + producer/consumer References edges
    assert!(
        nodes.iter().any(|n| n.name.contains("PROD.LEDGER.WORK")),
        "expected dataset node PROD.LEDGER.WORK, got {nodes:?}"
    );
    assert!(
        edges.iter().any(|(f, t, et)| {
            *et == EdgeType::References && f.contains("STEP1") && t.contains("PROD.LEDGER.WORK")
        }),
        "expected STEP1 -> PROD.LEDGER.WORK References edge, edges={edges:?}"
    );
    assert!(
        edges.iter().any(|(f, t, et)| {
            *et == EdgeType::References && f.contains("STEP2") && t.contains("PROD.LEDGER.WORK")
        }),
        "expected STEP2 -> PROD.LEDGER.WORK References edge, edges={edges:?}"
    );
}
