# Reuse Java lambda identities across extraction passes

Upstream issue: https://github.com/rgctl/rgctl/issues/197

The combined Java extraction API creates lambda symbols and subsequently visits
the same tree to emit synthetic Calls relations. Both walks shared an ExtractCtx
and incremented its lambda counter, so a symbol `$lambda$0` was targeted as
`$lambda$1`. Split extraction used fresh contexts and hid the mismatch.

ExtractCtx now memoizes the assigned index for each immutable tree-sitter node.
The first visit still uses the existing document-order counter scoped by owner
and enclosing callable discriminator. Later visits reuse the index. Tree node ids
are ephemeral per-file lookup keys; they are not persisted or included in graph
identity, so checkout moves, parser allocation and line shifts do not alter the
identity scheme. No new AST walk or parse is added.

Facts-cache format 9 and extraction semantics version 13 invalidate cached broken
relations and force graph regeneration. Existing symbol naming and overload
discriminators are retained. The regression compares exact lambda names and
discriminators in combined and split extraction for single/sibling/nested lambdas,
overloaded owners, fields, constructors and initializers, including line shifts.

This repairs target identity consistency. The existing synthetic Calls relation
still expresses conservative possible reachability from a lambda's owner; this
change does not establish actual callback invocation or scheduling.
