# File qualification for JavaScript and TypeScript declarations

JavaScript and TypeScript now supply `qualified_name` for declarations that
previously left it empty. `src/app.ts` emits `src.app.exercise`; a missing name
inside that function includes its lexical owner. Existing explicit owner names
and callable identity discriminators are preserved. Python and Go qualification
remain separate follow-ups under issue #204.

Callables whose candidate `qualified_name` (`{owner}.{name}`) directly collides
with an explicit native-owned `qualified_name` in the same file keep their
existing unqualified representation, together with their lexical descendants.
For example, a free `create()` in a root-level `Foo.ts` (`Foo.create`) must not
merge with the native `Foo.create` method, whereas `src/Foo.ts` qualifies free
`create()` as `src.Foo.create` without colliding. Literal callable names
containing dots also retain their existing representation, so a property such as
`outer.inner` cannot merge with a nested `inner` function. Uncallable anonymous
callbacks (`anonymous`, `anonymous@L{line}:{col}`) leave `qualified_name` unset
while disambiguating any nested named declarations via `.L{line}`.

The qualifier runs on extraction workers after the existing tree walk. It does
not reparse source, read package-manager files, or load repository-wide state.
A sorted scope stack fills missing names; preindexed aliases rewrite local
relation sources and unambiguous target hints. Repeated source aliases use an
interval tree rather than scanning every symbol per call. Ambiguous owners or
identities are not guessed. Existing unambiguous local bindings are normalized
to their full names, including receiver calls; explicit external hints are
preserved. Combined and split APIs qualify names before the shared callable
identity finish pass, so binding and overload/generic checks see the final names.
This does not add receiver inference. The graph does not expand file
path components or generated lexical owners into package suffixes or
receiver-type indexes. Shared call binding also avoids inferring a receiver
owner from a generated name; native owner inference remains available.

Standalone absolute paths lack a repository context, so their initial namespace
uses the file stem. The extraction worker rebases generated names to the logical
repository-relative path after fresh extraction or a raw facts-cache hit.
`ExtractAllResult.file_namespace` stores the prefix once per file. A typed
`Symbol.qualified_name_origin` enum distinguishes file-generated, lexical and native
owner names across combined/split APIs and cache reuse. An explicit namespace is
never rebased, even when its name matches the file stem. No per-symbol namespace
JSON map or duplicate prefix string is introduced. Other Tier 1 language
policies leave both markers unset.

Identical source reused in another same-stem file or checkout cannot retain the
first file's logical namespace. Native explicitly qualified names are not
rebased. Facts-cache format 14 rejects older raw facts; extraction semantics 16
forces graph regeneration for the changed declaration identities.

Tests cover combined/split API agreement, nested declarations, explicit class
owners, same-stem modules in one graph, persistent cache reuse across checkout
roots, repeated alias intervals and ambiguous identities. The inventory guard
covers all 24 bundled Tier 1 languages and verifies that native names in the
other languages pass through unchanged. Qualification supplies missing names;
it does not implement module loading, import resolution, or interprocedural
receiver/value-flow analysis.
