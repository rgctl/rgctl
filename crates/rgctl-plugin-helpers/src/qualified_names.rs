//! File namespaces for declarations without a language-owned qualified name.
//! Provenance is stored once per file; relation lookups never scan all symbols.
use rgctl_plugin_api::{
    ExtractAllResult, QualifiedNameOrigin, Relation, RelationType, SourceLocation, Symbol,
    SymbolType,
};
use rustc_hash::{FxHashMap, FxHashSet};
use std::fmt::Write as _;
use std::path::{Component, Path};

type Point = (usize, usize);

fn loc_start(p: &SourceLocation) -> Point {
    (p.start_line, p.start_column)
}

fn loc_end(p: &SourceLocation) -> Point {
    (p.end_line, p.end_column)
}

fn span_contains(a: &SourceLocation, b: &SourceLocation) -> bool {
    loc_start(a) <= loc_start(b) && loc_end(b) <= loc_end(a)
}

fn is_lexical_scope(s: &Symbol) -> bool {
    matches!(
        s.symbol_type,
        SymbolType::Function
            | SymbolType::Method
            | SymbolType::Class
            | SymbolType::Struct
            | SymbolType::Interface
            | SymbolType::Enum
            | SymbolType::Module
    )
}

fn is_qualifiable_symbol(s: &Symbol) -> bool {
    is_lexical_scope(s) || matches!(s.symbol_type, SymbolType::Variable | SymbolType::TypeAlias)
}

/// Whether `name` is a synthetic uncallable callback name (`anonymous`, `anonymous@L...`, or `<lambda@...>`).
#[inline]
pub fn is_anonymous_callable(name: &str) -> bool {
    name == "anonymous" || name.starts_with("anonymous@L") || name.starts_with("<lambda@")
}

fn strip_known_ext(file_name: &str) -> &str {
    for ext in [".d.ts", ".d.mts", ".d.cts"] {
        if let Some(stem) = file_name.strip_suffix(ext) {
            return stem;
        }
    }
    Path::new(file_name)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(file_name)
}

/// Standalone absolute paths use only the stem. Repository extraction supplies
/// the logical path directly or rebases cached stem facts without reparsing.
pub fn file_module(path: &Path) -> String {
    let path = if path.is_absolute() {
        Path::new(path.file_name().unwrap_or_default())
    } else {
        path
    };
    let mut parts: Vec<String> = Vec::new();
    for part in path.components() {
        match part {
            Component::Normal(s) => parts.push(s.to_string_lossy().into_owned()),
            Component::ParentDir => {
                parts.pop();
            }
            _ => {}
        }
    }
    if let Some(last) = parts.last_mut() {
        let stripped = strip_known_ext(last);
        if stripped.len() != last.len() {
            last.truncate(stripped.len());
        }
    }
    parts.retain(|p| !p.is_empty());
    parts.join(".")
}

/// Fill absent names using `file_path` for both `Symbol.location.file` matching
/// and module derivation.
pub fn qualify_file(
    path: &Path,
    symbols: &mut [Symbol],
    relations: &mut [Relation],
) -> Option<Box<str>> {
    qualify_file_in_module(path, path, symbols, relations)
}

/// Python package initializers use their package's logical namespace.
pub fn qualify_python_file(
    path: &Path,
    symbols: &mut [Symbol],
    relations: &mut [Relation],
) -> Option<Box<str>> {
    qualify_python_file_in_module(path, path, symbols, relations)
}

/// Fill absent Python names while deriving the module prefix from `module_path`
/// (`pkg/__init__.py` -> `pkg`).
pub fn qualify_python_file_in_module(
    file_path: &Path,
    module_path: &Path,
    symbols: &mut [Symbol],
    relations: &mut [Relation],
) -> Option<Box<str>> {
    let mut module = file_module(module_path);
    if module_path
        .file_stem()
        .is_some_and(|stem| stem == "__init__")
        && let Some((package, _)) = module.rsplit_once('.')
    {
        module.truncate(package.len());
    }
    qualify_with_module(file_path, module, symbols, relations)
}

/// Fill absent names using a sorted lexical scope stack, matching symbol
/// locations against `file_path` while deriving the module prefix from
/// `module_path`. Preserves explicit owner names and callable identities.
pub fn qualify_file_in_module(
    file_path: &Path,
    module_path: &Path,
    symbols: &mut [Symbol],
    relations: &mut [Relation],
) -> Option<Box<str>> {
    qualify_with_module(file_path, file_module(module_path), symbols, relations)
}

fn qualify_with_module(
    file_path: &Path,
    module: String,
    symbols: &mut [Symbol],
    relations: &mut [Relation],
) -> Option<Box<str>> {
    if module.is_empty() {
        return None;
    }
    let own_file = file_path.to_str();
    let mut order: Vec<usize> = (0..symbols.len())
        .filter(|&i| {
            is_qualifiable_symbol(&symbols[i])
                && own_file.map_or_else(
                    || symbols[i].location.file == file_path.to_string_lossy(),
                    |f| symbols[i].location.file == f,
                )
        })
        .collect();
    let span_key = |&i: &usize| {
        (
            loc_start(&symbols[i].location),
            std::cmp::Reverse(loc_end(&symbols[i].location)),
        )
    };
    if !order.is_sorted_by_key(span_key) {
        order.sort_unstable_by_key(span_key);
    }

    let mut explicit_fqns: FxHashSet<String> = symbols
        .iter()
        .filter(|s| s.qualified_name_origin != QualifiedNameOrigin::File)
        .filter_map(|s| s.qualified_name.clone())
        .collect();

    let mut stack: Vec<usize> = Vec::new();
    let mut deferred: Vec<bool> = Vec::new();
    let mut generated = false;
    for i in order {
        while stack.last().is_some_and(|&p| {
            !span_contains(&symbols[p].location, &symbols[i].location)
                || (
                    loc_start(&symbols[p].location),
                    loc_end(&symbols[p].location),
                ) == (
                    loc_start(&symbols[i].location),
                    loc_end(&symbols[i].location),
                )
        }) {
            stack.pop();
            deferred.pop();
        }
        let is_anon = is_anonymous_callable(&symbols[i].name);
        let parent_deferred = deferred.last().copied().unwrap_or(false);
        let mut defer = parent_deferred;
        if !defer && symbols[i].qualified_name.is_none() && !is_anon {
            let is_callable = matches!(
                symbols[i].symbol_type,
                SymbolType::Function | SymbolType::Method
            );
            if is_callable && symbols[i].name.contains('.') {
                defer = true;
            } else {
                let (candidate, origin) = build_qualified_candidate(&module, &stack, symbols, i);
                if is_callable && explicit_fqns.contains(&candidate) {
                    defer = true;
                } else {
                    if origin != QualifiedNameOrigin::File {
                        explicit_fqns.insert(candidate.clone());
                    }
                    symbols[i].qualified_name = Some(candidate);
                    symbols[i].qualified_name_origin = origin;
                    generated |= origin == QualifiedNameOrigin::File;
                }
            }
        }
        if is_lexical_scope(&symbols[i]) {
            stack.push(i);
            deferred.push(defer);
        }
    }
    rewrite_endpoints(file_path, symbols, relations, &module);
    if !generated {
        None
    } else {
        Some(module.into_boxed_str())
    }
}

fn build_qualified_candidate(
    module: &str,
    stack: &[usize],
    symbols: &[Symbol],
    sym_idx: usize,
) -> (String, QualifiedNameOrigin) {
    let named_pos = stack
        .iter()
        .rposition(|&idx| symbols[idx].qualified_name.is_some());
    let (root_prefix, origin, anon_slice) = match named_pos {
        Some(pos) => {
            let base = &symbols[stack[pos]];
            let qn = base.qualified_name.as_deref().unwrap_or(module);
            let orig = if base.qualified_name_origin == QualifiedNameOrigin::File {
                QualifiedNameOrigin::File
            } else {
                QualifiedNameOrigin::Lexical
            };
            (qn, orig, &stack[pos + 1..])
        }
        None => (module, QualifiedNameOrigin::File, stack),
    };
    let name = &symbols[sym_idx].name;
    let mut out = String::with_capacity(root_prefix.len() + anon_slice.len() * 6 + name.len() + 1);
    out.push_str(root_prefix);
    for &anon_idx in anon_slice {
        let _ = write!(out, ".L{}", symbols[anon_idx].location.start_line);
    }
    out.push('.');
    out.push_str(name);
    (out, origin)
}

/// Rebase generated names after a raw facts-cache hit (or stem-qualified extract).
/// Other languages and their native qualified names pass through unchanged.
pub fn qualify_in_repo(language: &str, logical_path: &Path, result: &mut ExtractAllResult) {
    if !matches!(language, "javascript" | "typescript" | "python") {
        return;
    }
    let Some(old_context) = result.file_namespace.as_deref().map(str::to_owned) else {
        return;
    };
    let mut module = file_module(logical_path);
    if language == "python"
        && logical_path
            .file_stem()
            .is_some_and(|stem| stem == "__init__")
        && let Some((package, _)) = module.rsplit_once('.')
    {
        module.truncate(package.len());
    }
    if module.is_empty() || module == old_context {
        return;
    }

    let explicit_fqns: FxHashSet<String> = result
        .symbols
        .iter()
        .filter(|s| s.qualified_name_origin != QualifiedNameOrigin::File)
        .filter_map(|s| s.qualified_name.clone())
        .collect();

    let mut cleared_prefixes: Vec<String> = Vec::new();
    let mut old_names: Vec<Option<String>> = vec![None; result.symbols.len()];
    let mut any_file_origin_kept = false;

    for (i, symbol) in result.symbols.iter_mut().enumerate() {
        if symbol.qualified_name_origin != QualifiedNameOrigin::File {
            continue;
        }
        let Some(current_qn) = symbol.qualified_name.as_deref() else {
            continue;
        };
        if cleared_prefixes.iter().any(|p| {
            current_qn == p
                || current_qn
                    .strip_prefix(p.as_str())
                    .is_some_and(|rest| rest.starts_with('.'))
        }) {
            old_names[i] = symbol.qualified_name.take();
            symbol.qualified_name_origin = QualifiedNameOrigin::Native;
            continue;
        }
        let Some(suffix) = current_qn
            .strip_prefix(old_context.as_str())
            .and_then(|rest| rest.strip_prefix('.'))
        else {
            any_file_origin_kept = true;
            continue;
        };
        let replacement = format!("{module}.{suffix}");
        let is_callable = matches!(
            symbol.symbol_type,
            SymbolType::Function | SymbolType::Method
        );
        if is_callable && explicit_fqns.contains(&replacement) {
            let cleared = symbol.qualified_name.take();
            if let Some(ref c) = cleared {
                cleared_prefixes.push(c.clone());
            }
            old_names[i] = cleared;
            symbol.qualified_name_origin = QualifiedNameOrigin::Native;
        } else {
            old_names[i] = symbol.qualified_name.replace(replacement);
            any_file_origin_kept = true;
        }
    }

    if !result.relations.is_empty() {
        let mut old_to_new: FxHashMap<&str, (Option<&str>, &str)> =
            FxHashMap::with_capacity_and_hasher(old_names.len(), Default::default());
        for (i, old) in old_names.iter().enumerate() {
            if let Some(old_qn) = old.as_deref() {
                old_to_new.insert(
                    old_qn,
                    (
                        result.symbols[i].qualified_name.as_deref(),
                        result.symbols[i].name.as_str(),
                    ),
                );
            }
        }
        if !old_to_new.is_empty() {
            for rel in &mut result.relations {
                if let Some(&(new_from, bare)) = old_to_new.get(rel.from.as_str()) {
                    let next = new_from.unwrap_or(bare);
                    rel.from.clear();
                    rel.from.push_str(next);
                }
                if let Some(&(new_to, bare)) = old_to_new.get(rel.to.as_str()) {
                    let next = new_to.unwrap_or(bare);
                    rel.to.clear();
                    rel.to.push_str(next);
                }
                if let Some(hint_val) = rel.to_qualified_hint.as_deref()
                    && let Some(&(new_hint, _)) = old_to_new.get(hint_val)
                {
                    match new_hint {
                        Some(rebased) => {
                            if let Some(slot) = rel.to_qualified_hint.as_mut() {
                                slot.clear();
                                slot.push_str(rebased);
                            }
                        }
                        None => rel.to_qualified_hint = None,
                    }
                }
            }
        }
    }

    result.file_namespace = any_file_origin_kept.then(|| module.into_boxed_str());
    if !cleared_prefixes.is_empty() {
        let policy = match language {
            "typescript" => crate::callable_identity::Policy::Typescript,
            "python" => crate::callable_identity::Policy::Python,
            _ => crate::callable_identity::Policy::Javascript,
        };
        crate::callable_identity::finish(policy, &mut result.symbols, &mut result.relations);
    }
}

#[derive(Clone, Copy)]
struct RangeCandidate {
    start: Point,
    end: Point,
    symbol: Option<usize>,
}

/// Only repeated aliases need an interval tree. It locates the innermost owner
/// in O(log S), including repeated local names and same-line nested functions.
struct IntervalIndex {
    spans: Vec<RangeCandidate>,
    maxima: Vec<Point>,
    leaves: usize,
}

impl IntervalIndex {
    fn from_symbol_indices(mut ids: Vec<usize>, symbols: &[Symbol]) -> Self {
        let span_key = |&i: &usize| {
            (
                loc_start(&symbols[i].location),
                std::cmp::Reverse(loc_end(&symbols[i].location)),
            )
        };
        if !ids.is_sorted_by_key(span_key) {
            ids.sort_unstable_by_key(span_key);
        }
        let mut spans: Vec<RangeCandidate> = Vec::with_capacity(ids.len());
        for i in ids {
            let p = &symbols[i].location;
            if let Some(last) = spans.last_mut()
                && (last.start, last.end) == (loc_start(p), loc_end(p))
            {
                if last
                    .symbol
                    .is_some_and(|j| symbols[j].qualified_name != symbols[i].qualified_name)
                {
                    last.symbol = None;
                }
            } else {
                spans.push(RangeCandidate {
                    start: loc_start(p),
                    end: loc_end(p),
                    symbol: Some(i),
                });
            }
        }
        let leaves = spans.len().next_power_of_two();
        let mut maxima = vec![(0, 0); leaves * 2];
        for (i, span) in spans.iter().enumerate() {
            maxima[leaves + i] = span.end;
        }
        for i in (1..leaves).rev() {
            maxima[i] = maxima[2 * i].max(maxima[2 * i + 1]);
        }
        Self {
            spans,
            maxima,
            leaves,
        }
    }

    fn find_innermost(&self, position: &SourceLocation) -> Option<usize> {
        let limit = self
            .spans
            .partition_point(|s| s.start <= loc_start(position));
        self.rightmost_leaf(1, 0, self.leaves, limit, loc_end(position))
            .and_then(|i| self.spans[i].symbol)
    }

    // Segment-tree depth is bounded by usize::BITS, independently of AST depth.
    fn rightmost_leaf(
        &self,
        node: usize,
        lo: usize,
        hi: usize,
        limit: usize,
        required_end: Point,
    ) -> Option<usize> {
        if lo >= limit || self.maxima[node] < required_end {
            return None;
        }
        if hi - lo == 1 {
            return Some(lo);
        }
        let mid = lo + (hi - lo) / 2;
        self.rightmost_leaf(node * 2 + 1, mid, hi, limit, required_end)
            .or_else(|| self.rightmost_leaf(node * 2, lo, mid, limit, required_end))
    }
}

enum Candidates {
    One(usize),
    Pending(Vec<usize>),
    Indexed(IntervalIndex),
}

impl Candidates {
    fn push_candidate(&mut self, i: usize) {
        match self {
            Self::One(previous) => {
                if *previous != i {
                    *self = Self::Pending(vec![*previous, i]);
                }
            }
            Self::Pending(ids) => ids.push(i),
            Self::Indexed(_) => {}
        }
    }

    fn find_enclosing(&self, location: &SourceLocation, symbols: &[Symbol]) -> Option<usize> {
        match self {
            Self::One(i) => span_contains(&symbols[*i].location, location).then_some(*i),
            Self::Indexed(index) => index.find_innermost(location),
            Self::Pending(_) => None,
        }
    }
}

fn rewrite_endpoints(
    file_path: &Path,
    symbols: &[Symbol],
    relations: &mut [Relation],
    file_prefix: &str,
) {
    if relations.is_empty() || symbols.is_empty() {
        return;
    }
    let own_file_cow;
    let target_file: &str = match file_path.to_str() {
        Some(s) => s,
        None => {
            own_file_cow = file_path.to_string_lossy();
            own_file_cow.as_ref()
        }
    };
    let mut sources: FxHashMap<(&str, Option<&str>), Candidates> =
        FxHashMap::with_capacity_and_hasher(symbols.len() * 2, Default::default());
    let mut targets: FxHashMap<&str, Option<usize>> =
        FxHashMap::with_capacity_and_hasher(symbols.len() * 2, Default::default());

    for (i, symbol) in symbols.iter().enumerate() {
        if is_anonymous_callable(&symbol.name) || symbol.location.file != target_file {
            continue;
        }
        let aliases = [
            Some(symbol.name.as_str()),
            symbol.qualified_name.as_deref(),
            // Existing receiver hints may carry a lexical owner without the
            // generated file prefix (e.g. Foo.methodA in prototype code).
            (symbol.qualified_name_origin == QualifiedNameOrigin::File)
                .then(|| {
                    symbol
                        .qualified_name
                        .as_deref()?
                        .strip_prefix(file_prefix)?
                        .strip_prefix('.')
                })
                .flatten(),
        ];
        for (slot, alias) in aliases.iter().enumerate() {
            let Some(alias) = *alias else {
                continue;
            };
            if aliases[..slot].contains(&Some(alias)) {
                continue;
            }
            let bare_alias = slot == 0 && symbol.qualified_name.as_deref() != Some(alias);
            if !bare_alias
                || symbol.qualified_name_origin != QualifiedNameOrigin::Native
                || symbol.qualified_name.is_none()
                || symbol.symbol_type == SymbolType::Import
            {
                let value = if bare_alias && symbol.symbol_type == SymbolType::Import {
                    None
                } else {
                    symbol.qualified_name.as_ref().map(|_| i)
                };
                targets
                    .entry(alias)
                    .and_modify(|entry| {
                        if value.is_none()
                            || entry
                                .is_some_and(|j| symbols[j].qualified_name != symbol.qualified_name)
                        {
                            *entry = None;
                        }
                    })
                    .or_insert(value);
            }
            if bare_alias && symbol.qualified_name_origin == QualifiedNameOrigin::Native {
                continue;
            }
            if symbol.qualified_name.is_none() {
                continue;
            }
            sources
                .entry((alias, None))
                .and_modify(|v| v.push_candidate(i))
                .or_insert(Candidates::One(i));
            if let Some(disc) = symbol.identity_discriminator.as_deref() {
                sources
                    .entry((alias, Some(disc)))
                    .and_modify(|v| v.push_candidate(i))
                    .or_insert(Candidates::One(i));
            }
        }
    }

    for value in sources.values_mut() {
        if let Candidates::Pending(ids) = value {
            *value = Candidates::Indexed(IntervalIndex::from_symbol_indices(
                std::mem::take(ids),
                symbols,
            ));
        }
    }

    for relation in relations {
        if relation.location.file != target_file {
            continue;
        }
        if let Some(i) = sources
            .get(&(
                relation.from.as_str(),
                relation.from_identity_discriminator_str(),
            ))
            .and_then(|c| c.find_enclosing(&relation.location, symbols))
        {
            if let Some(name) = &symbols[i].qualified_name {
                relation.from.clone_from(name);
            }
        }
        if matches!(relation.relation_type, RelationType::Calls) {
            let is_unqualified =
                relation.is_inferred_callable_owner() || relation.to_qualified_hint.is_none();
            let hint = if relation.is_inferred_callable_owner() {
                // The generic callable pass may have guessed Box.inner for an
                // unqualified call to a lexical Box.method.inner declaration.
                // Preserve the emitter's local endpoint instead of that guess.
                relation.to.as_str()
            } else {
                relation
                    .to_qualified_hint
                    .as_deref()
                    .unwrap_or(&relation.to)
            };
            let resolved = match targets.get(hint).copied() {
                Some(Some(target)) => Some(target),
                Some(None)
                    if is_unqualified
                        && relation.to_type_hint.is_none()
                        && !hint.contains('.')
                        && relation
                            .call_site_byte_len
                            .is_none_or(|len| usize::from(len) == hint.len()) =>
                {
                    resolve_lexical_target(
                        &relation.from,
                        &relation.location,
                        hint,
                        &sources,
                        &targets,
                        symbols,
                    )
                }
                _ => None,
            };
            if let Some(target) = resolved {
                let symbol = &symbols[target];
                relation
                    .to_qualified_hint
                    .clone_from(&symbol.qualified_name);
            }
        }
    }
}

fn resolve_lexical_target(
    caller_qn: &str,
    call_loc: &SourceLocation,
    bare_callee: &str,
    sources: &FxHashMap<(&str, Option<&str>), Candidates>,
    targets: &FxHashMap<&str, Option<usize>>,
    symbols: &[Symbol],
) -> Option<usize> {
    let mut scope = caller_qn;
    let mut probe = String::with_capacity(caller_qn.len() + 1 + bare_callee.len());
    loop {
        let encloses_call = sources
            .get(&(scope, None))
            .and_then(|c| c.find_enclosing(call_loc, symbols))
            .is_some();
        if encloses_call {
            probe.clear();
            probe.push_str(scope);
            probe.push('.');
            probe.push_str(bare_callee);
            if let Some(&slot) = targets.get(probe.as_str()) {
                return slot.filter(|&idx| {
                    symbols[idx].qualified_name_origin != QualifiedNameOrigin::Native
                });
            }
        }
        match scope.rsplit_once('.') {
            Some((parent, _)) => scope = parent,
            None => break,
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_test_symbol(name: &str, lo: usize, hi: usize, qualified: &str) -> Symbol {
        Symbol {
            name: name.into(),
            qualified_name: Some(qualified.into()),
            qualified_name_origin: QualifiedNameOrigin::File,
            symbol_type: SymbolType::Function,
            location: SourceLocation {
                file: "test.js".into(),
                start_line: 1,
                end_line: 1,
                start_column: lo,
                end_column: hi,
            },
            ..Default::default()
        }
    }

    fn make_test_relation(lo: usize, hi: usize) -> Relation {
        Relation {
            from: "same".into(),
            to: "external".into(),
            location: SourceLocation {
                file: "test.js".into(),
                start_line: 1,
                end_line: 1,
                start_column: lo,
                end_column: hi,
            },
            ..Default::default()
        }
    }

    #[test]
    fn repeated_alias_intervals_match_innermost_owner_oracle() {
        let mut symbols = vec![make_test_symbol("same", 0, 10_000, "outer")];
        for i in 1..500 {
            symbols.push(make_test_symbol(
                "same",
                i * 10,
                i * 10 + 4,
                &format!("owner{i}"),
            ));
        }
        let mut relations: Vec<_> = (1..500)
            .flat_map(|i| {
                [
                    make_test_relation(i * 10 + 1, i * 10 + 2),
                    make_test_relation(i * 10 + 5, i * 10 + 6),
                ]
            })
            .collect();
        let expected: Vec<_> = relations
            .iter()
            .map(|r| {
                symbols
                    .iter()
                    .filter(|s| span_contains(&s.location, &r.location))
                    .max_by_key(|s| loc_start(&s.location))
                    .unwrap()
                    .qualified_name
                    .clone()
                    .unwrap()
            })
            .collect();
        qualify_file(Path::new("test.js"), &mut symbols, &mut relations);
        assert_eq!(
            relations.iter().map(|r| r.from.clone()).collect::<Vec<_>>(),
            expected
        );
    }

    #[test]
    fn overlapping_aliases_require_matching_identity() {
        let mut symbols = vec![
            make_test_symbol("same", 0, 20, "first"),
            make_test_symbol("same", 0, 20, "second"),
        ];
        symbols[0].identity_discriminator = Some("(int)".into());
        symbols[1].identity_discriminator = Some("(string)".into());
        let mut relations = vec![
            make_test_relation(1, 2),
            make_test_relation(1, 2),
            make_test_relation(1, 2),
        ];
        relations[1].from_identity_discriminator = Some("(int)".into());
        relations[2].from_identity_discriminator = Some("(unknown)".into());
        qualify_file(Path::new("test.js"), &mut symbols, &mut relations);
        assert_eq!(relations[0].from, "same");
        assert_eq!(relations[1].from, "first");
        assert_eq!(relations[2].from, "same");
        assert_eq!(
            relations[1].from_identity_discriminator.as_deref(),
            Some("(int)")
        );
        for symbol in &mut symbols {
            symbol.qualified_name = Some("test.same".into());
        }
        let mut call = make_test_relation(1, 2);
        call.from = "caller".into();
        call.to = "same".into();
        call.relation_type = RelationType::Calls;
        qualify_file(
            Path::new("test.js"),
            &mut symbols,
            std::slice::from_mut(&mut call),
        );
        assert_eq!(call.to_qualified_hint.as_deref(), Some("test.same"));
        assert!(
            call.to_identity_discriminator.is_none(),
            "must not select an overload"
        );
        symbols.truncate(1);
        qualify_file(
            Path::new("test.js"),
            &mut symbols,
            std::slice::from_mut(&mut call),
        );
        assert!(call.to_identity_discriminator.is_none());
        call.to_identity_discriminator = Some("supplied".into());
        qualify_file(
            Path::new("test.js"),
            &mut symbols,
            std::slice::from_mut(&mut call),
        );
        assert_eq!(call.to_identity_discriminator.as_deref(), Some("supplied"));
    }

    #[test]
    fn rebasing_preserves_explicit_owners_and_only_updates_generated_names() {
        let mut generated = make_test_symbol("same", 21, 40, "unused");
        generated.qualified_name = None;
        let mut explicit = make_test_symbol("native", 0, 20, "Owner.native");
        explicit.qualified_name_origin = QualifiedNameOrigin::Native;
        let mut symbols = vec![generated, explicit];
        let mut relations = vec![make_test_relation(22, 23)];
        for symbol in &mut symbols {
            symbol.location.file = "app.js".into();
        }
        relations[0].location.file = "app.js".into();
        let context = qualify_file(Path::new("app.js"), &mut symbols, &mut relations);
        let mut result = ExtractAllResult::from_parts(symbols, relations);
        result.file_namespace = context;
        qualify_in_repo("javascript", Path::new("src/app.js"), &mut result);
        assert_eq!(
            result.symbols[0].qualified_name.as_deref(),
            Some("src.app.same")
        );
        assert_eq!(
            result.symbols[1].qualified_name.as_deref(),
            Some("Owner.native")
        );
        assert_eq!(result.relations[0].from, "src.app.same");
        let previous = result.clone();
        qualify_in_repo("javascript", Path::new("src/app.js"), &mut result);
        assert_eq!(previous, result);
        assert!(result.symbols.iter().all(|s| s.metadata.is_null()));
    }

    #[test]
    fn normalization_preserves_local_bindings_and_explicit_receiver_hints() {
        let mut helper = make_test_symbol("helper", 100, 120, "unused");
        helper.qualified_name = None;
        let mut symbols = vec![make_test_symbol("same", 0, 99, "Caller"), helper];
        let mut relations = vec![
            make_test_relation(1, 2),
            make_test_relation(1, 2),
            make_test_relation(1, 2),
        ];
        for r in &mut relations {
            r.to = "helper".into();
            r.call_site_start_byte = Some(0);
        }
        relations[0].call_site_byte_len = Some(10); // obj.helper
        relations[1].call_site_byte_len = Some(6);
        relations[1].to_type_hint = Some("Other".into());
        relations[1].to_qualified_hint = Some("Other.helper".into());
        relations[2].to_qualified_hint = Some("External.helper".into());
        qualify_file(Path::new("test.js"), &mut symbols, &mut relations);
        assert_eq!(
            relations[0].to_qualified_hint.as_deref(),
            Some("test.helper")
        );
        assert_eq!(
            relations[1].to_qualified_hint.as_deref(),
            Some("Other.helper")
        );
        assert_eq!(
            relations[2].to_qualified_hint.as_deref(),
            Some("External.helper")
        );
        let mut plain = make_test_relation(1, 2);
        plain.to = "helper".into();
        plain.call_site_start_byte = Some(0);
        plain.call_site_byte_len = Some(6);
        qualify_file(
            Path::new("test.js"),
            &mut symbols,
            std::slice::from_mut(&mut plain),
        );
        assert_eq!(plain.to_qualified_hint.as_deref(), Some("test.helper"));
    }

    #[test]
    fn declaration_extensions_strip_compound_d_suffix() {
        assert_eq!(file_module(Path::new("src/types.d.ts")), "src.types");
        assert_eq!(file_module(Path::new("src/esm.d.mts")), "src.esm");
        assert_eq!(file_module(Path::new("src/cjs.d.cts")), "src.cjs");
        assert_eq!(file_module(Path::new("/abs/repo/types.d.ts")), "types");
    }

    #[test]
    fn anonymous_callbacks_stay_unqualified_while_disambiguating_named_children() {
        let mut anon = Symbol {
            name: "anonymous@L10:8".into(),
            symbol_type: SymbolType::Function,
            location: SourceLocation {
                file: "suite.js".into(),
                start_line: 10,
                end_line: 20,
                start_column: 8,
                end_column: 2,
            },
            ..Default::default()
        };
        anon.qualified_name = None;
        let inner = Symbol {
            name: "helper".into(),
            symbol_type: SymbolType::Function,
            location: SourceLocation {
                file: "suite.js".into(),
                start_line: 12,
                end_line: 14,
                start_column: 2,
                end_column: 3,
            },
            ..Default::default()
        };
        let mut import_rel = Relation {
            from: "suite.js".into(),
            to: "helper".into(),
            relation_type: RelationType::Uses,
            location: SourceLocation {
                file: "suite.js".into(),
                start_line: 1,
                end_line: 1,
                start_column: 0,
                end_column: 10,
            },
            ..Default::default()
        };
        let mut symbols = vec![anon, inner];
        qualify_file(
            Path::new("suite.js"),
            &mut symbols,
            std::slice::from_mut(&mut import_rel),
        );
        assert!(
            symbols[0].qualified_name.is_none(),
            "anonymous callback must not get a module-qualified FQN"
        );
        assert_eq!(
            symbols[1].qualified_name.as_deref(),
            Some("suite.L10.helper")
        );
        assert_eq!(symbols[1].qualified_name_origin, QualifiedNameOrigin::File);
        assert!(
            import_rel.to_qualified_hint.is_none(),
            "non-Calls relations must not allocate to_qualified_hint"
        );
    }
}
