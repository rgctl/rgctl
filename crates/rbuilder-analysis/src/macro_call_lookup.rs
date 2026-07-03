//! SQLite-backed instant lookup table for blast-radius queries.
//!
//! Stores symbol candidates with class/file context for FQN disambiguation and
//! sub-millisecond CLI lookups without loading the full graph.

use rbuilder_error::{Error, Result};
use rbuilder_graph::backend::MemoryBackend;
use rbuilder_graph::schema::{Node, NodeType};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use uuid::Uuid;

/// Parsed symbol query with optional namespace/file filters.
#[derive(Debug, Clone)]
pub struct ParsedSymbol {
    /// Method or function name (after FQN split).
    pub target_name: String,
    /// Optional class or namespace filter.
    pub class_filter: Option<String>,
    /// Optional source file path filter.
    pub file_filter: Option<String>,
}

/// A candidate symbol record used for disambiguation and cache lookup.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MacroIndexEntry {
    /// Graph node UUID.
    pub id: Uuid,
    /// Bare symbol name.
    pub symbol_name: String,
    /// Containing class or namespace (simple name when known).
    pub class_name: Option<String>,
    /// Source file path.
    pub file_path: String,
    /// Impact score (0–100).
    pub score: f64,
    /// Direct caller names.
    pub direct_callers: Vec<String>,
    /// Impact-zone function names.
    pub impact_zone: Vec<String>,
}

/// A row from the unique-symbol fast path (legacy compatibility).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MacroCallLookupRow {
    /// Function symbol name.
    pub symbol_name: String,
    /// Impact score (0–100).
    pub score: f64,
    /// Direct caller names (JSON array in storage).
    pub direct_callers: Vec<String>,
    /// Impact-zone function names (JSON array in storage).
    pub impact_zone: Vec<String>,
}

/// Tokenizes incoming input identifiers to separate names from namespace prefixes.
pub fn parse_fqn_symbol(
    input: &str,
    explicit_class: Option<String>,
    explicit_file: Option<String>,
) -> ParsedSymbol {
    if let Some(idx) = input.find("::") {
        let scope_hint = &input[..idx];
        let target_name = &input[idx + 2..];

        if scope_hint.contains('/')
            || scope_hint.contains('\\')
            || scope_hint.ends_with(".java")
            || scope_hint.ends_with(".rs")
        {
            ParsedSymbol {
                target_name: target_name.to_string(),
                class_filter: explicit_class,
                file_filter: Some(scope_hint.to_string()),
            }
        } else {
            ParsedSymbol {
                target_name: target_name.to_string(),
                class_filter: Some(scope_hint.to_string()),
                file_filter: explicit_file,
            }
        }
    } else {
        ParsedSymbol {
            target_name: input.to_string(),
            class_filter: explicit_class,
            file_filter: explicit_file,
        }
    }
}

/// Extract the simple class name from a graph node.
pub fn class_name_from_node(node: &Node) -> Option<String> {
    node.qualified_name.as_ref().and_then(|qn| {
        qn.rsplit_once('.').map(|(class, _)| {
            class.rsplit('.').next().unwrap_or(class).to_string()
        })
    })
}

fn class_matches(entry_class: Option<&str>, filter: &str) -> bool {
    let Some(class) = entry_class else {
        return false;
    };
    class == filter
        || class.ends_with(&format!(".{filter}"))
        || class.contains(filter)
}

fn file_matches(entry_path: &str, filter: &str) -> bool {
    let normalized = entry_path.replace('\\', "/");
    let filter = filter.replace('\\', "/");
    normalized.contains(&filter)
}

/// Disambiguates and resolves a single definitive node UUID from duplicate candidates.
pub fn resolve_symbol_uuid(candidates: &[MacroIndexEntry], parsed: &ParsedSymbol) -> Result<Uuid> {
    let filtered: Vec<&MacroIndexEntry> = candidates
        .iter()
        .filter(|entry| {
            if let Some(ref class_name) = parsed.class_filter {
                if !class_matches(entry.class_name.as_deref(), class_name) {
                    return false;
                }
            }
            if let Some(ref file_path) = parsed.file_filter {
                if !file_matches(&entry.file_path, file_path) {
                    return false;
                }
            }
            true
        })
        .collect();

    match filtered.len() {
        1 => Ok(filtered[0].id),
        0 => Err(Error::NotFound(format!(
            "No symbol matched criteria: '{}' (Filters: Class={:?}, File={:?})",
            parsed.target_name, parsed.class_filter, parsed.file_filter
        ))),
        count => {
            emit_ambiguous_manifest(&parsed.target_name, &filtered);
            Err(Error::AmbiguousSymbol {
                name: parsed.target_name.clone(),
                count,
            })
        }
    }
}

fn emit_ambiguous_manifest(target_name: &str, filtered: &[&MacroIndexEntry]) {
    eprintln!(
        "\nError: Symbol '{target_name}' is ambiguous. Found {} matches.",
        filtered.len()
    );
    eprintln!(
        "{:<38} | {:<25} | {:<50}",
        "UUID", "Class Context", "Source File Path"
    );
    eprintln!("{}", "-".repeat(120));
    for entry in filtered.iter().take(10) {
        eprintln!(
            "{:<38} | {:<25} | {:<50}",
            entry.id,
            entry.class_name.as_deref().unwrap_or("<None>"),
            entry.file_path
        );
    }
    if filtered.len() > 10 {
        eprintln!("... and {} more matching records.", filtered.len() - 10);
    }
    eprintln!("\nRemediation: Refine your search query using a fully qualified namespace syntax:");
    eprintln!("  rbuilder blast-radius \"ClassName::{target_name}\"");
    eprintln!("  rbuilder blast-radius \"path/to/file.java::{target_name}\"");
}

/// Build candidate entries from graph nodes (slow path when cache is stale).
pub fn candidates_from_backend(
    backend: &MemoryBackend,
    target_name: &str,
) -> Result<Vec<MacroIndexEntry>> {
    let nodes = backend.find_nodes_by_name(target_name)?;
    Ok(nodes
        .into_iter()
        .filter(|n| n.node_type == NodeType::Function)
        .map(|n| node_to_candidate(&n, target_name, 0.0, vec![], vec![]))
        .collect())
}

/// Build candidate entries from a mmap snapshot without hydrating the backend.
pub fn candidates_from_snapshot(
    store: &rbuilder_graph::snapshot::SnapshotNodeStore,
    target_name: &str,
) -> Vec<MacroIndexEntry> {
    store
        .find_nodes_by_name(target_name)
        .into_iter()
        .filter(|n| n.node_type == NodeType::Function)
        .map(|n| node_to_candidate(n, target_name, 0.0, vec![], vec![]))
        .collect()
}

fn node_to_candidate(
    node: &Node,
    symbol_name: &str,
    score: f64,
    direct_callers: Vec<String>,
    impact_zone: Vec<String>,
) -> MacroIndexEntry {
    MacroIndexEntry {
        id: node.id,
        symbol_name: symbol_name.to_string(),
        class_name: class_name_from_node(node),
        file_path: node.file_path.clone().unwrap_or_default(),
        score,
        direct_callers,
        impact_zone,
    }
}

/// Try to parse a symbol string as a UUID for direct node targeting.
pub fn try_parse_symbol_uuid(input: &str) -> Option<Uuid> {
    Uuid::parse_str(input.trim()).ok()
}

/// SQLite cache for instant blast-radius lookups.
pub struct MacroCallLookupDb;

impl MacroCallLookupDb {
    /// Default SQLite path under a repository root.
    pub fn default_path(repo_root: &Path) -> PathBuf {
        repo_root.join(".rbuilder/macro_call_index.db")
    }

    fn open(path: &Path) -> Result<Connection> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(path).map_err(|e| {
            Error::QueryError(format!("open macro_call_index.db: {e}"))
        })?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS macro_call_index (
                symbol_name TEXT PRIMARY KEY,
                node_id TEXT NOT NULL,
                score REAL NOT NULL,
                direct_callers TEXT NOT NULL,
                impact_zone TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS macro_call_candidates (
                symbol_name TEXT NOT NULL,
                node_id TEXT NOT NULL,
                class_name TEXT,
                file_path TEXT NOT NULL,
                score REAL NOT NULL,
                direct_callers TEXT NOT NULL,
                impact_zone TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_macro_call_candidates_name
                ON macro_call_candidates(symbol_name);
            CREATE TABLE IF NOT EXISTS macro_call_meta (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );",
        )
        .map_err(|e| Error::QueryError(format!("init macro_call_index.db: {e}")))?;
        Ok(conn)
    }

    /// Persist graph fingerprint metadata for cache validation.
    pub fn write_meta(path: &Path, file_size: u64, node_count: usize, edge_count: usize) -> Result<()> {
        Self::write_meta_with_digest(path, file_size, node_count, edge_count, None)
    }

    /// Persist metadata including optional binary snapshot digest.
    pub fn write_meta_with_digest(
        path: &Path,
        file_size: u64,
        node_count: usize,
        edge_count: usize,
        graph_digest: Option<&str>,
    ) -> Result<()> {
        let conn = Self::open(path)?;
        conn.execute(
            "INSERT OR REPLACE INTO macro_call_meta (key, value) VALUES (?1, ?2)",
            params!["graph_file_size", file_size.to_string()],
        )
        .map_err(sql_err)?;
        conn.execute(
            "INSERT OR REPLACE INTO macro_call_meta (key, value) VALUES (?1, ?2)",
            params!["node_count", node_count.to_string()],
        )
        .map_err(sql_err)?;
        conn.execute(
            "INSERT OR REPLACE INTO macro_call_meta (key, value) VALUES (?1, ?2)",
            params!["edge_count", edge_count.to_string()],
        )
        .map_err(sql_err)?;
        if let Some(digest) = graph_digest {
            conn.execute(
                "INSERT OR REPLACE INTO macro_call_meta (key, value) VALUES (?1, ?2)",
                params!["graph_digest", digest],
            )
            .map_err(sql_err)?;
        }
        Ok(())
    }

    fn read_meta(conn: &Connection, key: &str) -> Result<Option<String>> {
        match conn.query_row(
            "SELECT value FROM macro_call_meta WHERE key = ?1",
            params![key],
            |row| row.get(0),
        ) {
            Ok(value) => Ok(Some(value)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(sql_err(e)),
        }
    }

    /// Returns true when the SQLite cache matches the repository graph state.
    pub fn is_valid_for_repo(path: &Path, repo_root: &Path) -> Result<bool> {
        if !path.exists() {
            return Ok(false);
        }
        let conn = Self::open(path)?;

        let snapshot_path = repo_root
            .join(rbuilder_graph::code_graph::GRAPH_DIR)
            .join(rbuilder_graph::snapshot::SNAPSHOT_FILE);
        if snapshot_path.exists() {
            if let Ok(mmap) = rbuilder_graph::snapshot::MmappedGraphSnapshot::open(&snapshot_path) {
                if let Some(stored) = Self::read_meta(&conn, "graph_digest")? {
                    return Ok(stored == mmap.content_digest());
                }
                let node_count: Option<String> = Self::read_meta(&conn, "node_count")?;
                let edge_count: Option<String> = Self::read_meta(&conn, "edge_count")?;
                if let (Some(nodes), Some(edges)) = (node_count, edge_count) {
                    return Ok(
                        nodes == mmap.node_count().to_string()
                            && edges == mmap.edge_count().to_string(),
                    );
                }
            }
        }

        let graph_db = repo_root
            .join(rbuilder_graph::code_graph::GRAPH_DIR)
            .join(rbuilder_graph::code_graph::GRAPH_FILE);
        Self::is_valid_for_graph(path, &graph_db)
    }

    /// Returns true when the SQLite cache matches the on-disk graph file.
    pub fn is_valid_for_graph(path: &Path, graph_db_path: &Path) -> Result<bool> {
        if !path.exists() || !graph_db_path.exists() {
            return Ok(false);
        }
        let conn = Self::open(path)?;
        let stored_size: Option<String> = conn
            .query_row(
                "SELECT value FROM macro_call_meta WHERE key = 'graph_file_size'",
                [],
                |row| row.get(0),
            )
            .ok();
        let Some(stored_size) = stored_size else {
            return Ok(false);
        };
        let meta = std::fs::metadata(graph_db_path)?;
        Ok(meta.len().to_string() == stored_size)
    }

    /// Replace all candidate rows (full disambiguation index).
    pub fn replace_candidates(path: &Path, entries: &[MacroIndexEntry]) -> Result<()> {
        let conn = Self::open(path)?;
        let tx = conn.unchecked_transaction().map_err(sql_err)?;
        tx.execute("DELETE FROM macro_call_candidates", [])
            .map_err(sql_err)?;
        {
            let mut stmt = tx
                .prepare(
                    "INSERT INTO macro_call_candidates
                     (symbol_name, node_id, class_name, file_path, score, direct_callers, impact_zone)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                )
                .map_err(sql_err)?;
            for entry in entries {
                let direct = serde_json::to_string(&entry.direct_callers).map_err(json_err)?;
                let impact = serde_json::to_string(&entry.impact_zone).map_err(json_err)?;
                stmt.execute(params![
                    entry.symbol_name,
                    entry.id.to_string(),
                    entry.class_name,
                    entry.file_path,
                    entry.score,
                    direct,
                    impact,
                ])
                .map_err(sql_err)?;
            }
        }
        tx.commit().map_err(sql_err)?;
        Ok(())
    }

    /// Replace uniquely-named rows in the legacy fast-path table.
    pub fn replace_all(path: &Path, rows: &[MacroCallLookupRow]) -> Result<()> {
        let conn = Self::open(path)?;
        let tx = conn.unchecked_transaction().map_err(sql_err)?;
        tx.execute("DELETE FROM macro_call_index", []).map_err(sql_err)?;
        {
            let mut stmt = tx
                .prepare(
                    "INSERT INTO macro_call_index
                     (symbol_name, node_id, score, direct_callers, impact_zone)
                     VALUES (?1, ?2, ?3, ?4, ?5)",
                )
                .map_err(sql_err)?;
            for row in rows {
                let direct = serde_json::to_string(&row.direct_callers).map_err(json_err)?;
                let impact = serde_json::to_string(&row.impact_zone).map_err(json_err)?;
                stmt.execute(params![row.symbol_name, "", row.score, direct, impact])
                    .map_err(sql_err)?;
            }
        }
        tx.commit().map_err(sql_err)?;
        Ok(())
    }

    fn row_to_entry(row: &rusqlite::Row<'_>) -> Result<MacroIndexEntry> {
        let direct_json: String = row.get(5).map_err(sql_err)?;
        let impact_json: String = row.get(6).map_err(sql_err)?;
        let id_str: String = row.get(1).map_err(sql_err)?;
        Ok(MacroIndexEntry {
            symbol_name: row.get(0).map_err(sql_err)?,
            id: Uuid::parse_str(&id_str).map_err(|e| {
                Error::SerdeError(format!("invalid node_id in macro_call_index.db: {e}"))
            })?,
            class_name: row.get(2).map_err(sql_err)?,
            file_path: row.get(3).map_err(sql_err)?,
            score: row.get(4).map_err(sql_err)?,
            direct_callers: serde_json::from_str(&direct_json).map_err(json_err)?,
            impact_zone: serde_json::from_str(&impact_json).map_err(json_err)?,
        })
    }

    /// Fetch all cached candidates matching a bare symbol name.
    pub fn get_candidates(path: &Path, symbol_name: &str) -> Result<Vec<MacroIndexEntry>> {
        if !path.exists() {
            return Ok(vec![]);
        }
        let conn = Self::open(path)?;
        let mut stmt = conn
            .prepare(
                "SELECT symbol_name, node_id, class_name, file_path, score, direct_callers, impact_zone
                 FROM macro_call_candidates WHERE symbol_name = ?1",
            )
            .map_err(sql_err)?;
        let mut matches = stmt.query(params![symbol_name]).map_err(sql_err)?;
        let mut found = Vec::new();
        while let Some(row) = matches.next().map_err(sql_err)? {
            found.push(Self::row_to_entry(&row)?);
        }
        Ok(found)
    }

    /// Resolve a parsed symbol against the SQLite candidate index.
    pub fn lookup_resolved(path: &Path, parsed: &ParsedSymbol) -> Result<Option<MacroIndexEntry>> {
        let candidates = Self::get_candidates(path, &parsed.target_name)?;
        if candidates.is_empty() {
            return Ok(None);
        }
        let id = resolve_symbol_uuid(&candidates, parsed)?;
        Ok(candidates.into_iter().find(|c| c.id == id))
    }

    /// Lookup a uniquely-named symbol in the legacy fast-path table.
    pub fn lookup(path: &Path, symbol: &str) -> Result<Option<MacroCallLookupRow>> {
        if !path.exists() {
            return Ok(None);
        }
        let conn = Self::open(path)?;
        let mut stmt = conn
            .prepare(
                "SELECT symbol_name, score, direct_callers, impact_zone
                 FROM macro_call_index WHERE symbol_name = ?1",
            )
            .map_err(sql_err)?;
        let mut matches = stmt.query(params![symbol]).map_err(sql_err)?;
        let mut found: Vec<MacroCallLookupRow> = Vec::new();
        while let Some(row) = matches.next().map_err(sql_err)? {
            let direct_json: String = row.get(2).map_err(sql_err)?;
            let impact_json: String = row.get(3).map_err(sql_err)?;
            found.push(MacroCallLookupRow {
                symbol_name: row.get(0).map_err(sql_err)?,
                score: row.get(1).map_err(sql_err)?,
                direct_callers: serde_json::from_str(&direct_json).map_err(json_err)?,
                impact_zone: serde_json::from_str(&impact_json).map_err(json_err)?,
            });
        }
        match found.len() {
            0 => Ok(None),
            1 => Ok(Some(found.remove(0))),
            count => Err(Error::AmbiguousSymbol {
                name: symbol.to_string(),
                count,
            }),
        }
    }
}

fn sql_err(e: rusqlite::Error) -> Error {
    Error::QueryError(format!("macro_call_index.db: {e}"))
}

fn json_err(e: serde_json::Error) -> Error {
    Error::SerdeError(format!("macro_call_index.db json: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_fqn_class_scope() {
        let parsed = parse_fqn_symbol("MRequest::checkChange", None, None);
        assert_eq!(parsed.target_name, "checkChange");
        assert_eq!(parsed.class_filter.as_deref(), Some("MRequest"));
        assert!(parsed.file_filter.is_none());
    }

    #[test]
    fn parse_fqn_file_scope() {
        let parsed = parse_fqn_symbol("src/Foo.java::bar", None, None);
        assert_eq!(parsed.target_name, "bar");
        assert_eq!(parsed.file_filter.as_deref(), Some("src/Foo.java"));
    }

    #[test]
    fn resolve_symbol_uuid_filters_by_class() {
        let candidates = vec![
            MacroIndexEntry {
                id: Uuid::new_v4(),
                symbol_name: "getChangeType".into(),
                class_name: Some("OrderLine".into()),
                file_path: "a/OrderLine.java".into(),
                score: 1.0,
                direct_callers: vec![],
                impact_zone: vec![],
            },
            MacroIndexEntry {
                id: Uuid::new_v4(),
                symbol_name: "getChangeType".into(),
                class_name: Some("Invoice".into()),
                file_path: "b/Invoice.java".into(),
                score: 1.0,
                direct_callers: vec![],
                impact_zone: vec![],
            },
        ];
        let parsed = parse_fqn_symbol("OrderLine::getChangeType", None, None);
        let id = resolve_symbol_uuid(&candidates, &parsed).unwrap();
        assert_eq!(id, candidates[0].id);
    }

    #[test]
    fn sqlite_candidates_round_trip() {
        let tmp = tempfile::TempDir::new().unwrap();
        let db = tmp.path().join("macro_call_index.db");
        MacroCallLookupDb::write_meta(&db, 123, 10, 20).unwrap();
        let id = Uuid::new_v4();
        MacroCallLookupDb::replace_candidates(
            &db,
            &[MacroIndexEntry {
                id,
                symbol_name: "saveError".into(),
                class_name: Some("Logger".into()),
                file_path: "src/Logger.java".into(),
                score: 64.0,
                direct_callers: vec!["main".into()],
                impact_zone: vec!["a".into()],
            }],
        )
        .unwrap();
        let parsed = parse_fqn_symbol("saveError", None, None);
        let row = MacroCallLookupDb::lookup_resolved(&db, &parsed)
            .unwrap()
            .unwrap();
        assert_eq!(row.score, 64.0);
        assert_eq!(row.id, id);
    }
}
