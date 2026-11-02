//! Shared state for single-pass AST extraction (ScopeStack + KindDispatchTable).
//!
//! Language plugins migrate from multi-walk `extract_all` by driving one iterative
//! DFS that pushes/pops [`ScopeFrame`]s and dispatches via [`KindDispatchTable`].

use crate::{
    AstHandlerTable, ComplexityMetrics, HandlerMask, KindDispatchTable, ScopeFrame, ScopeKind,
    ScopeStack, Symbol,
};
use std::sync::atomic::{AtomicUsize, Ordering};
use tree_sitter::Language;

/// Metadata key for complexity harvested during `extract_all`.
pub const COMPLEXITY_META_KEY: &str = "rgctl_complexity";

/// Optional AST-kind hit callback for `RGCTL_AST_COVERAGE_TRACK=1` test wiring.
///
/// Language crates call [`track_ast_kind`] when a non-passive dispatch mask is
/// observed. Tests / `rgctl-languages` install a callback that forwards to
/// `AstCoverageTracker::hit`.
///
/// Stored as `AtomicUsize` (`0` = unset) so the production disabled path is a
/// single non-contended `Relaxed` load — not an `RwLock` reader-count bump on
/// every active AST node across Rayon workers.
type AstKindHitFn = fn(u16);

static AST_KIND_HIT: AtomicUsize = AtomicUsize::new(0);

/// Install or clear the process-wide AST kind hit callback.
pub fn set_ast_coverage_hit_fn(f: Option<AstKindHitFn>) {
    let bits = f.map(|func| func as usize).unwrap_or(0);
    AST_KIND_HIT.store(bits, Ordering::Release);
}

/// True when `RGCTL_AST_COVERAGE_TRACK` is set in the environment.
pub fn ast_coverage_track_enabled() -> bool {
    std::env::var_os("RGCTL_AST_COVERAGE_TRACK").is_some()
}

/// Record a kind hit when a callback is installed (no-op otherwise).
#[inline]
pub fn track_ast_kind(kind_id: u16) {
    if kind_id == 0 {
        return;
    }
    let bits = AST_KIND_HIT.load(Ordering::Relaxed);
    if bits == 0 {
        return;
    }
    // SAFETY: bits is either 0 or a value stored by `set_ast_coverage_hit_fn`
    // from a real `fn(u16)` pointer (never an arbitrary integer).
    let f: AstKindHitFn = unsafe { std::mem::transmute(bits) };
    f(kind_id);
}

/// Lookup dispatch mask and optionally track AST kind hits when a coverage tracker is installed.
#[inline]
pub fn dispatch_lookup_tracked(dispatch: &KindDispatchTable, kind_id: u16) -> HandlerMask {
    let mask = dispatch.lookup(kind_id);
    track_ast_kind(kind_id);
    mask
}

/// Read complexity attached during single-pass extraction.
pub fn complexity_from_metadata(symbol: &Symbol) -> Option<ComplexityMetrics> {
    let v = symbol.metadata.get(COMPLEXITY_META_KEY)?;
    serde_json::from_value(v.clone()).ok()
}

/// Persist complexity onto a symbol's JSON metadata.
pub fn attach_complexity(symbol: &mut Symbol, metrics: &ComplexityMetrics) {
    if let Ok(v) = serde_json::to_value(metrics)
        && let Some(obj) = symbol.metadata.as_object_mut()
    {
        obj.insert(COMPLEXITY_META_KEY.to_string(), v);
    }
}

/// Parse `{lang}-ast-coverage.json` `handlers` object into `(name, label)` pairs.
pub fn coverage_handler_entries(manifest_json: &str) -> Vec<(String, String)> {
    let v: serde_json::Value =
        serde_json::from_str(manifest_json).expect("ast-coverage.json parse");
    v["handlers"]
        .as_object()
        .expect("handlers object")
        .iter()
        .map(|(k, val)| (k.clone(), val.as_str().unwrap_or("Skip").to_string()))
        .collect()
}

/// Build [`KindDispatchTable`] only from a coverage manifest (jump table).
pub fn dispatch_table_from_manifest(grammar: &Language, manifest_json: &str) -> KindDispatchTable {
    let entries = coverage_handler_entries(manifest_json);
    KindDispatchTable::from_coverage(
        grammar,
        entries.iter().map(|(n, l)| (n.as_str(), l.as_str())),
    )
}

/// Build [`AstHandlerTable`] from the plugin's actual handled kinds (not the JSON).
///
/// Use this for [`crate::LanguagePlugin::declared_ast_handlers`] so
/// `check_plugin_manifest_sync` is non-tautological.
pub fn handler_table_from_kinds(
    grammar: &Language,
    kinds: &[(&str, HandlerMask)],
) -> AstHandlerTable {
    let mut table = AstHandlerTable::new();
    for &(name, mask) in kinds {
        let kind_id = grammar.id_for_node_kind(name, true);
        if kind_id == 0 {
            continue;
        }
        table.insert(kind_id, name.to_string(), mask);
    }
    table
}

/// Build [`AstHandlerTable`] + [`KindDispatchTable`] from a coverage manifest.
///
/// Prefer [`handler_table_from_kinds`] + [`dispatch_table_from_manifest`] so the
/// handler table reflects plugin code rather than re-parsing the same JSON.
pub fn coverage_tables_from_manifest(
    grammar: &Language,
    manifest_json: &str,
) -> (AstHandlerTable, KindDispatchTable) {
    let entries = coverage_handler_entries(manifest_json);
    let handlers = AstHandlerTable::from_coverage_entries(
        grammar,
        entries.iter().map(|(n, l)| (n.as_str(), l.as_str())),
    );
    let dispatch = KindDispatchTable::from_coverage(
        grammar,
        entries.iter().map(|(n, l)| (n.as_str(), l.as_str())),
    );
    (handlers, dispatch)
}

/// Mutable context held for the duration of one file's single AST pass.
#[derive(Debug)]
pub struct SinglePassCtx {
    pub scopes: ScopeStack,
    /// Process-wide jump table (no per-file `Box<[HandlerMask]>` clone).
    pub dispatch: &'static KindDispatchTable,
    pub symbols: Vec<Symbol>,
}

impl SinglePassCtx {
    pub fn new(dispatch: &'static KindDispatchTable) -> Self {
        Self {
            scopes: ScopeStack::new(),
            dispatch,
            symbols: Vec::new(),
        }
    }

    /// Push a scope and optionally bind it to a just-pushed symbol.
    pub fn enter_scope(
        &mut self,
        kind: ScopeKind,
        name: Option<String>,
        qualified_prefix: String,
        start_byte: usize,
        end_byte: usize,
        bind_last_symbol: bool,
    ) {
        let depth = self.scopes.depth();
        let mut frame = ScopeFrame::new(kind, name, qualified_prefix, start_byte, end_byte, depth);
        if bind_last_symbol && !self.symbols.is_empty() {
            frame.symbol_idx = Some(self.symbols.len() - 1);
        }
        self.scopes.push(frame);
    }

    pub fn leave_scope(&mut self) -> Option<ScopeFrame> {
        self.scopes.pop()
    }

    /// After popping a function-like frame, build [`ComplexityMetrics`] and attach to symbol.
    pub fn harvest_function_complexity(&mut self, frame: &ScopeFrame) {
        if !frame.kind.is_function_like() {
            return;
        }
        let Some(idx) = frame.symbol_idx else {
            return;
        };
        let mut metrics = Self::complexity_metrics(frame);
        if metrics.cyclomatic == 0 {
            metrics.cyclomatic = 1;
        }
        if let Some(sym) = self.symbols.get_mut(idx) {
            attach_complexity(sym, &metrics);
        }
    }

    /// After popping a function-like frame, build [`ComplexityMetrics`].
    pub fn complexity_metrics(frame: &ScopeFrame) -> ComplexityMetrics {
        let c = &frame.complexity;
        ComplexityMetrics {
            cyclomatic: c.cyclomatic as usize,
            cognitive: c.cognitive as usize,
            loc: c.loc as usize,
            parameters: c.param_count as usize,
            nesting_depth: c.max_nesting as usize,
            returns: c.return_count as usize,
        }
    }
}
