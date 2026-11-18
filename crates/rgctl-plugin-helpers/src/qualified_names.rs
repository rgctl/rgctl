//! File namespaces for declarations without a language-owned qualified name.
//! Provenance is stored once per file; relation lookups never scan all symbols.
use rgctl_plugin_api::{
    ExtractAllResult, QualifiedNameOrigin, Relation, SourceLocation, Symbol, SymbolType,
};
use std::{
    collections::HashMap,
    path::{Component, Path},
};

type Point = (usize, usize);
fn start(p: &SourceLocation) -> Point {
    (p.start_line, p.start_column)
}
fn end(p: &SourceLocation) -> Point {
    (p.end_line, p.end_column)
}
fn contains(a: &SourceLocation, b: &SourceLocation) -> bool {
    a.file == b.file && start(a) <= start(b) && end(b) <= end(a)
}
fn scope(s: &Symbol) -> bool {
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
fn eligible(s: &Symbol) -> bool {
    scope(s) || matches!(s.symbol_type, SymbolType::Variable | SymbolType::TypeAlias)
}
/// Standalone absolute paths use only the stem. Repository extraction rebases
/// this prefix using the logical path, without parsing the source again.
pub fn file_module(path: &Path) -> String {
    let path = if path.is_absolute() {
        Path::new(path.file_name().unwrap_or_default())
    } else {
        path
    };
    let stem = path.with_extension("");
    let mut parts = Vec::new();
    for part in stem.components() {
        match part {
            Component::Normal(s) => parts.push(s.to_string_lossy()),
            Component::ParentDir => {
                parts.pop();
            }
            _ => {}
        }
    }
    parts.join(".")
}

/// Fill absent names using a sorted lexical scope stack, preserving explicit
/// owner names and callable identities. Return compact, per-file provenance.
pub fn qualify_file(
    path: &Path,
    symbols: &mut [Symbol],
    relations: &mut [Relation],
) -> Option<Box<str>> {
    let module = file_module(path);
    if module.is_empty() {
        return None;
    }
    let own_file = path.to_string_lossy();
    let mut order: Vec<_> = (0..symbols.len())
        .filter(|&i| eligible(&symbols[i]) && symbols[i].location.file == own_file)
        .collect();
    order.sort_unstable_by_key(|&i| {
        (
            start(&symbols[i].location),
            std::cmp::Reverse(end(&symbols[i].location)),
        )
    });
    let mut stack: Vec<usize> = Vec::new();
    let mut generated = false;
    for i in order {
        while stack.last().is_some_and(|&p| {
            !contains(&symbols[p].location, &symbols[i].location)
                || (start(&symbols[p].location), end(&symbols[p].location))
                    == (start(&symbols[i].location), end(&symbols[i].location))
        }) {
            stack.pop();
        }
        if symbols[i].qualified_name.is_none() {
            let origin = match stack.last() {
                Some(&p) if symbols[p].qualified_name_origin != QualifiedNameOrigin::File => {
                    QualifiedNameOrigin::Lexical
                }
                _ => QualifiedNameOrigin::File,
            };
            let owner = stack
                .last()
                .and_then(|&p| symbols[p].qualified_name.as_deref())
                .unwrap_or(&module);
            symbols[i].qualified_name = Some(format!("{owner}.{}", symbols[i].name));
            symbols[i].qualified_name_origin = origin;
            generated |= origin == QualifiedNameOrigin::File;
        }
        if scope(&symbols[i]) {
            stack.push(i);
        }
    }
    rewrite_endpoints(symbols, relations, &[], &module);
    if !generated {
        None
    } else {
        Some(module.into_boxed_str())
    }
}

/// Rebase generated names after fresh extraction OR a raw facts-cache hit.
/// Other languages and their native qualified names pass through unchanged.
pub fn qualify_in_repo(language: &str, logical_path: &Path, result: &mut ExtractAllResult) {
    if !matches!(language, "javascript" | "typescript") {
        return;
    }
    let Some(context) = result.file_namespace.as_mut() else {
        return;
    };
    let module = file_module(logical_path);
    if module.is_empty() || module == context.as_ref() {
        return;
    }
    let mut old_names = vec![None; result.symbols.len()];
    for (i, symbol) in result.symbols.iter_mut().enumerate() {
        if symbol.qualified_name_origin != QualifiedNameOrigin::File {
            continue;
        }
        let Some(suffix) = symbol
            .qualified_name
            .as_deref()
            .and_then(|name| name.strip_prefix(context.as_ref()))
            .and_then(|name| name.strip_prefix('.'))
        else {
            continue;
        };
        let replacement = format!("{module}.{suffix}");
        old_names[i] = symbol.qualified_name.replace(replacement);
    }
    rewrite_endpoints(&result.symbols, &mut result.relations, &old_names, &module);
    *context = module.into_boxed_str();
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
    fn new(mut ids: Vec<usize>, symbols: &[Symbol]) -> Self {
        ids.sort_unstable_by_key(|&i| {
            (
                start(&symbols[i].location),
                std::cmp::Reverse(end(&symbols[i].location)),
            )
        });
        let mut spans: Vec<RangeCandidate> = Vec::with_capacity(ids.len());
        for i in ids {
            let p = &symbols[i].location;
            if let Some(last) = spans.last_mut()
                && (last.start, last.end) == (start(p), end(p))
            {
                if last
                    .symbol
                    .is_some_and(|j| symbols[j].qualified_name != symbols[i].qualified_name)
                {
                    last.symbol = None;
                }
            } else {
                spans.push(RangeCandidate {
                    start: start(p),
                    end: end(p),
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
    fn find(&self, position: &SourceLocation) -> Option<usize> {
        let limit = self.spans.partition_point(|s| s.start <= start(position));
        self.rightmost(1, 0, self.leaves, limit, end(position))
            .and_then(|i| self.spans[i].symbol)
    }
    // Segment-tree depth is bounded by usize::BITS, independently of AST depth.
    fn rightmost(
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
        self.rightmost(node * 2 + 1, mid, hi, limit, required_end)
            .or_else(|| self.rightmost(node * 2, lo, mid, limit, required_end))
    }
}
enum Candidates {
    One(usize),
    Pending(Vec<usize>),
    Indexed(IntervalIndex),
}
impl Candidates {
    fn push(&mut self, i: usize) {
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
    fn find(&self, location: &SourceLocation, symbols: &[Symbol]) -> Option<usize> {
        match self {
            Self::One(i) => contains(&symbols[*i].location, location).then_some(*i),
            Self::Indexed(index) => index
                .find(location)
                .filter(|&i| symbols[i].location.file == location.file),
            Self::Pending(_) => None,
        }
    }
}
fn rewrite_endpoints(
    symbols: &[Symbol],
    relations: &mut [Relation],
    old_names: &[Option<String>],
    file_prefix: &str,
) {
    let mut sources: HashMap<(&str, &str, Option<&str>), Candidates> = HashMap::new();
    let mut targets: HashMap<(&str, &str), Option<usize>> = HashMap::new();
    for (i, symbol) in symbols.iter().enumerate() {
        let aliases = [
            Some(symbol.name.as_str()),
            symbol.qualified_name.as_deref(),
            old_names.get(i).and_then(|n| n.as_deref()),
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
                    .entry((symbol.location.file.as_str(), alias))
                    .and_modify(|slot| {
                        if value.is_none()
                            || slot
                                .is_some_and(|j| symbols[j].qualified_name != symbol.qualified_name)
                        {
                            *slot = None;
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
            for identity in [
                None,
                Some(symbol.identity_discriminator.as_deref().unwrap_or("")),
            ] {
                sources
                    .entry((symbol.location.file.as_str(), alias, identity))
                    .and_modify(|v| v.push(i))
                    .or_insert(Candidates::One(i));
            }
        }
    }
    for value in sources.values_mut() {
        if let Candidates::Pending(ids) = value {
            *value = Candidates::Indexed(IntervalIndex::new(std::mem::take(ids), symbols));
        }
    }
    for relation in relations {
        if let Some(i) = sources
            .get(&(
                relation.location.file.as_str(),
                relation.from.as_str(),
                relation.from_identity_discriminator_str(),
            ))
            .and_then(|c| c.find(&relation.location, symbols))
        {
            if let Some(name) = &symbols[i].qualified_name {
                relation.from.clone_from(name);
            }
        }
        if let Some(Some(i)) = targets.get(&(relation.location.file.as_str(), relation.to.as_str()))
            && old_names.get(*i).and_then(|name| name.as_deref()) == Some(relation.to.as_str())
        {
            if let Some(name) = &symbols[*i].qualified_name {
                relation.to.clone_from(name);
            }
        }
        let hint = relation
            .to_qualified_hint
            .as_deref()
            .unwrap_or(&relation.to);
        if let Some(Some(i)) = targets.get(&(relation.location.file.as_str(), hint)) {
            relation
                .to_qualified_hint
                .clone_from(&symbols[*i].qualified_name);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn symbol(name: &str, lo: usize, hi: usize, qualified: &str) -> Symbol {
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
    fn relation(lo: usize, hi: usize) -> Relation {
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
        let mut symbols = vec![symbol("same", 0, 10_000, "outer")];
        for i in 1..500 {
            symbols.push(symbol("same", i * 10, i * 10 + 4, &format!("owner{i}")));
        }
        let mut relations: Vec<_> = (1..500)
            .flat_map(|i| {
                [
                    relation(i * 10 + 1, i * 10 + 2),
                    relation(i * 10 + 5, i * 10 + 6),
                ]
            })
            .collect();
        let expected: Vec<_> = relations
            .iter()
            .map(|r| {
                symbols
                    .iter()
                    .filter(|s| contains(&s.location, &r.location))
                    .max_by_key(|s| start(&s.location))
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
            symbol("same", 0, 20, "first"),
            symbol("same", 0, 20, "second"),
        ];
        symbols[0].identity_discriminator = Some("(int)".into());
        symbols[1].identity_discriminator = Some("(string)".into());
        let mut relations = vec![relation(1, 2), relation(1, 2), relation(1, 2)];
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
    }
    #[test]
    fn rebasing_preserves_explicit_owners_and_only_updates_generated_names() {
        let mut generated = symbol("same", 21, 40, "unused");
        generated.qualified_name = None;
        let mut explicit = symbol("native", 0, 20, "Owner.native");
        explicit.qualified_name_origin = QualifiedNameOrigin::Native;
        let mut symbols = vec![generated, explicit];
        let mut relations = vec![relation(22, 23)];
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
        let mut helper = symbol("helper", 100, 120, "unused");
        helper.qualified_name = None;
        let mut symbols = vec![symbol("same", 0, 99, "Caller"), helper];
        let mut relations = vec![relation(1, 2), relation(1, 2), relation(1, 2)];
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
        let mut plain = relation(1, 2);
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
}
