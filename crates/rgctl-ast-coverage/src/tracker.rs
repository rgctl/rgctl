//! Test harness: record which AST kinds were exercised during language tests.
//!
//! Language crates call [`AstCoverageTracker::hit`] / [`hit_name`] while walking
//! fixtures, then [`assert_all_exercised`] at the end of the suite so every
//! non-`Skip` / non-`Literal` entry in `{lang}-ast-coverage.json` has ≥1 hit.
//!
//! Enable with env `RGCTL_AST_COVERAGE_TRACK=1` or construct explicitly in tests.
//! Single-pass extractors call [`rgctl_plugin_api::dispatch_lookup_tracked`], which
//! forwards to a process-wide hit callback installed via [`install_hit_callback`].

#![allow(
    clippy::expect_used,
    clippy::missing_errors_doc,
    clippy::missing_panics_doc
)]

use crate::HandlerMask;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::{Mutex, OnceLock};

/// Records hit counts per grammar `kind_id` (and optionally by kind name).
#[derive(Debug, Default)]
pub struct AstCoverageTracker {
    language_id: String,
    /// Active kind_id → hit count
    hits: Mutex<HashMap<u16, u64>>,
    /// Passive / Unimplemented kind_id → hit count for empirical blind-spot ranking
    skip_hits: Mutex<BTreeMap<u16, u64>>,
    /// Expected active kinds: name → kind_id (from manifest + live grammar)
    expected: HashMap<String, u16>,
    /// Fast lookup set of expected active kind_ids
    expected_ids: HashSet<u16>,
    /// Passive / Unimplemented kind_id → grammar node kind name
    passive_by_id: HashMap<u16, String>,
}

impl AstCoverageTracker {
    /// Build a tracker for `language_id` from coverage-manifest handler map + live grammar.
    ///
    /// Only non-`Skip` / non-`Literal` handlers are required to be exercised.
    /// `Skip` and `Unimplemented` kinds are recorded in `skip_hits` for hotspot telemetry.
    pub fn from_manifest_handlers(
        language_id: impl Into<String>,
        handlers: &HashMap<String, String>,
        lang: &tree_sitter::Language,
    ) -> Self {
        let mut expected = HashMap::new();
        let mut expected_ids = HashSet::new();
        let mut passive_by_id = HashMap::new();
        for (name, label) in handlers {
            let Some(mask) = HandlerMask::from_coverage_label(label) else {
                continue;
            };
            let kind_id = lang.id_for_node_kind(name, true);
            if kind_id == 0 {
                continue;
            }
            if mask.is_passive() {
                if matches!(label.as_str(), "Skip" | "Unimplemented") {
                    passive_by_id.insert(kind_id, name.clone());
                }
                continue;
            }
            expected.insert(name.clone(), kind_id);
            expected_ids.insert(kind_id);
        }
        Self {
            language_id: language_id.into(),
            hits: Mutex::new(HashMap::new()),
            skip_hits: Mutex::new(BTreeMap::new()),
            expected,
            expected_ids,
            passive_by_id,
        }
    }

    /// Parse `{lang}-ast-coverage.json` text and build a tracker.
    pub fn from_manifest_json(
        language_id: impl Into<String>,
        manifest_json: &str,
        lang: &tree_sitter::Language,
    ) -> Result<Self, String> {
        let v: serde_json::Value =
            serde_json::from_str(manifest_json).map_err(|e| format!("manifest JSON: {e}"))?;
        let handlers_obj = v["handlers"]
            .as_object()
            .ok_or_else(|| "manifest missing `handlers` object".to_string())?;
        let handlers: HashMap<String, String> = handlers_obj
            .iter()
            .map(|(k, val)| (k.clone(), val.as_str().unwrap_or("Skip").to_string()))
            .collect();
        Ok(Self::from_manifest_handlers(language_id, &handlers, lang))
    }

    /// Record that `kind_id` was visited during AST extraction.
    ///
    /// Active kinds increment `hits`; tracked `Skip`/`Unimplemented` kinds increment `skip_hits`.
    pub fn hit(&self, kind_id: u16) {
        if kind_id == 0 {
            return;
        }
        if !self.expected_ids.contains(&kind_id) && self.passive_by_id.contains_key(&kind_id) {
            self.record_skip_hit(kind_id);
            return;
        }
        let mut guard = self.hits.lock().expect("AstCoverageTracker poisoned");
        *guard.entry(kind_id).or_insert(0) += 1;
    }

    /// Record a hit on a passive (`Skip` / `Unimplemented`) AST node kind for blind-spot ranking.
    pub fn record_skip_hit(&self, kind_id: u16) {
        if kind_id == 0 {
            return;
        }
        let mut guard = self.skip_hits.lock().expect("AstCoverageTracker poisoned");
        *guard.entry(kind_id).or_insert(0) += 1;
    }

    /// Hit count for a passive (`Skip` / `Unimplemented`) kind id.
    pub fn skip_hit_count(&self, kind_id: u16) -> u64 {
        self.skip_hits
            .lock()
            .expect("AstCoverageTracker poisoned")
            .get(&kind_id)
            .copied()
            .unwrap_or(0)
    }

    /// Return the top `top_n` highest-frequency unhandled (`Skip` / `Unimplemented`) AST node kinds.
    pub fn unhandled_hotspots(&self, top_n: usize) -> Vec<(String, u64)> {
        let guard = self.skip_hits.lock().expect("AstCoverageTracker poisoned");
        let mut items: Vec<(String, u64)> = guard
            .iter()
            .filter(|(_, count)| **count > 0)
            .map(|(id, count)| {
                let name = self
                    .passive_by_id
                    .get(id)
                    .cloned()
                    .unwrap_or_else(|| format!("kind_{id}"));
                (name, *count)
            })
            .collect();
        items.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        items.truncate(top_n);
        items
    }

    /// Alias for [`Self::hit`] (OpenSpec / depth-suite naming).
    pub fn record_hit(&self, kind_id: u16) {
        self.hit(kind_id);
    }

    /// Record a hit by grammar kind name (resolves id via `expected` or `lang`).
    pub fn hit_name(&self, kind_name: &str, lang: &tree_sitter::Language) {
        if let Some(&id) = self.expected.get(kind_name) {
            self.hit(id);
            return;
        }
        let id = lang.id_for_node_kind(kind_name, true);
        self.hit(id);
    }

    /// Language id this tracker was built for.
    pub fn language_id(&self) -> &str {
        &self.language_id
    }

    /// Number of expected active kinds.
    pub fn expected_len(&self) -> usize {
        self.expected.len()
    }

    /// Kind ids that the manifest requires to be exercised.
    pub fn expected_kind_ids(&self) -> Vec<u16> {
        let mut ids: Vec<u16> = self.expected.values().copied().collect();
        ids.sort_unstable();
        ids.dedup();
        ids
    }

    /// Hit count for a kind id (0 if never seen).
    pub fn hit_count(&self, kind_id: u16) -> u64 {
        self.hits
            .lock()
            .expect("AstCoverageTracker poisoned")
            .get(&kind_id)
            .copied()
            .unwrap_or(0)
    }

    /// Total recorded hits across all kind ids.
    pub fn total_hits(&self) -> u64 {
        self.hits
            .lock()
            .expect("AstCoverageTracker poisoned")
            .values()
            .sum()
    }

    /// Fail if any expected kind has zero hits. Returns Ok with a short summary.
    pub fn assert_all_exercised(&self) -> Result<String, String> {
        let hits = self.hits.lock().expect("AstCoverageTracker poisoned");
        let mut missing: Vec<&str> = self
            .expected
            .iter()
            .filter(|(_, id)| hits.get(*id).copied().unwrap_or(0) == 0)
            .map(|(name, _)| name.as_str())
            .collect();
        missing.sort_unstable();
        if missing.is_empty() {
            return Ok(format!(
                "{}: all {} active AST kinds exercised",
                self.language_id,
                self.expected.len()
            ));
        }
        Err(format!(
            "{}: {} active AST kind(s) in *-ast-coverage.json have 0 test hits: {}",
            self.language_id,
            missing.len(),
            missing.join(", ")
        ))
    }

    /// Soft gate: every expected kind whose id appears in `present_kind_ids` must
    /// have ≥1 hit (kinds absent from the fixture trees are skipped).
    pub fn assert_present_kinds_exercised(
        &self,
        present_kind_ids: impl IntoIterator<Item = u16>,
    ) -> Result<String, String> {
        let present: HashSet<u16> = present_kind_ids.into_iter().filter(|&id| id != 0).collect();
        let hits = self.hits.lock().expect("AstCoverageTracker poisoned");
        let mut missing: Vec<&str> = self
            .expected
            .iter()
            .filter(|(_, id)| present.contains(id) && hits.get(*id).copied().unwrap_or(0) == 0)
            .map(|(name, _)| name.as_str())
            .collect();
        missing.sort_unstable();
        if missing.is_empty() {
            let n = self
                .expected
                .values()
                .filter(|id| present.contains(id))
                .count();
            return Ok(format!(
                "{}: all {} fixture-present active AST kinds exercised",
                self.language_id, n
            ));
        }
        Err(format!(
            "{}: {} fixture-present active AST kind(s) have 0 hits: {}",
            self.language_id,
            missing.len(),
            missing.join(", ")
        ))
    }

    /// Process-wide tracker installed via [`install_hit_callback`], if any.
    pub fn global() -> Option<&'static AstCoverageTracker> {
        PROCESS_TRACKER.get()
    }

    /// Assert using the process-wide tracker for `language_id`.
    ///
    /// `manifest_json` is accepted for call-site clarity; the installed tracker
    /// is the source of expected kinds (built at install time from that manifest).
    pub fn assert_all_exercised_for(
        language_id: &str,
        _manifest_json: &str,
    ) -> Result<String, String> {
        let tracker = Self::global().ok_or_else(|| {
            "no AstCoverageTracker installed; call install_hit_callback first".to_string()
        })?;
        if tracker.language_id != language_id {
            return Err(format!(
                "tracker language_id={} does not match requested {language_id}",
                tracker.language_id
            ));
        }
        tracker.assert_all_exercised()
    }
}

static PROCESS_TRACKER: OnceLock<AstCoverageTracker> = OnceLock::new();

fn forward_process_hit(kind_id: u16) {
    if let Some(t) = PROCESS_TRACKER.get() {
        t.record_hit(kind_id);
    }
}

/// Install `tracker` as the process-wide hit sink for
/// [`rgctl_plugin_api::dispatch_lookup_tracked`] (single-pass extractors).
///
/// Call once per test process when `RGCTL_AST_COVERAGE_TRACK=1`. Subsequent
/// installs are ignored (`OnceLock`).
pub fn install_hit_callback(tracker: AstCoverageTracker) {
    if PROCESS_TRACKER.set(tracker).is_ok() {
        rgctl_plugin_api::set_ast_coverage_hit_fn(Some(forward_process_hit));
    }
}

/// Clear the plugin-api hit callback (does not drop an already-installed tracker).
pub fn clear_hit_callback() {
    rgctl_plugin_api::set_ast_coverage_hit_fn(None);
}

/// Collect named grammar kind ids that appear in `source` under `lang`.
pub fn named_kind_ids_in_source(lang: &tree_sitter::Language, source: &[u8]) -> HashSet<u16> {
    let mut parser = tree_sitter::Parser::new();
    if parser.set_language(lang).is_err() {
        return HashSet::new();
    }
    let Some(tree) = parser.parse(source, None) else {
        return HashSet::new();
    };
    let mut ids = HashSet::new();
    let mut cursor = tree.walk();
    loop {
        let node = cursor.node();
        if node.is_named() {
            ids.insert(node.kind_id());
        }
        if cursor.goto_first_child() {
            continue;
        }
        loop {
            if cursor.goto_next_sibling() {
                break;
            }
            if !cursor.goto_parent() {
                return ids;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tracker_passes_when_all_hit() {
        let lang: tree_sitter::Language = tree_sitter_rust::LANGUAGE.into();
        let mut handlers = HashMap::new();
        handlers.insert("function_item".into(), "Symbol".into());
        handlers.insert("identifier".into(), "Skip".into());
        let tracker = AstCoverageTracker::from_manifest_handlers("rust", &handlers, &lang);
        assert_eq!(tracker.expected_len(), 1);
        let id = lang.id_for_node_kind("function_item", true);
        tracker.hit(id);
        assert!(tracker.assert_all_exercised().is_ok());
    }

    #[test]
    fn tracker_fails_when_unhit() {
        let lang: tree_sitter::Language = tree_sitter_rust::LANGUAGE.into();
        let mut handlers = HashMap::new();
        handlers.insert("function_item".into(), "Symbol".into());
        let tracker = AstCoverageTracker::from_manifest_handlers("rust", &handlers, &lang);
        let err = tracker.assert_all_exercised().unwrap_err();
        assert!(err.contains("function_item"), "{err}");
    }

    #[test]
    fn record_hit_aliases_hit() {
        let lang: tree_sitter::Language = tree_sitter_rust::LANGUAGE.into();
        let mut handlers = HashMap::new();
        handlers.insert("function_item".into(), "Symbol".into());
        let tracker = AstCoverageTracker::from_manifest_handlers("rust", &handlers, &lang);
        let id = lang.id_for_node_kind("function_item", true);
        tracker.record_hit(id);
        assert_eq!(tracker.hit_count(id), 1);
    }

    #[test]
    fn assert_present_kinds_skips_absent() {
        let lang: tree_sitter::Language = tree_sitter_rust::LANGUAGE.into();
        let mut handlers = HashMap::new();
        handlers.insert("function_item".into(), "Symbol".into());
        handlers.insert("struct_item".into(), "Symbol".into());
        let tracker = AstCoverageTracker::from_manifest_handlers("rust", &handlers, &lang);
        let fn_id = lang.id_for_node_kind("function_item", true);
        tracker.hit(fn_id);
        // Only require function_item (present); struct_item absent from present set.
        assert!(tracker.assert_present_kinds_exercised([fn_id]).is_ok());
        assert!(tracker.assert_all_exercised().is_err());
    }

    #[test]
    fn unhandled_hotspots_ranks_skip_and_unimplemented_hits() {
        let lang: tree_sitter::Language = tree_sitter_rust::LANGUAGE.into();
        let mut handlers = HashMap::new();
        handlers.insert("function_item".into(), "Symbol".into());
        handlers.insert("macro_definition".into(), "Unimplemented".into());
        handlers.insert("trait_item".into(), "Skip".into());
        handlers.insert("block".into(), "SkipWrapper".into());
        let tracker = AstCoverageTracker::from_manifest_handlers("rust", &handlers, &lang);
        let macro_id = lang.id_for_node_kind("macro_definition", true);
        let trait_id = lang.id_for_node_kind("trait_item", true);
        tracker.hit(macro_id);
        tracker.hit(macro_id);
        tracker.hit(macro_id);
        tracker.hit(trait_id);
        let hotspots = tracker.unhandled_hotspots(5);
        assert_eq!(
            hotspots,
            vec![
                ("macro_definition".to_string(), 3),
                ("trait_item".to_string(), 1),
            ]
        );
    }
}
