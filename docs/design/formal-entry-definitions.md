# Formal entry definitions

Native CFG lowering captures the supported function's operand roles before
building its body, using the existing parsed tree. Each supported formal becomes
one synthetic `StatementKind::Parameter(FormalId)` definition at CFG entry.
The ID indexes the same `FunctionOperands::formals` pool used by invocation
bindings. Receivers, positional parameters and rest parameters keep their existing
formal IDs and conventions.

The statement's text is the parameter name, its line is the enclosing callable's
start line, and it has no uses. It represents an incoming value, not a source
assignment or evaluation of a default expression. The formal's operand record
retains its native source span when one exists. Implicit receivers have no token.

Formal definitions seed lexical scope before body lowering. Existing local
reaching definitions and PDG construction then create ordinary data edges from
incoming parameters to local aliases, reads and returns. Java/ECMA `this` and
Rust `self` tokens are read values. JavaScript/TypeScript lexical declarations
participate in scoped declaration lowering, so inner `let`/`const` definitions
can shadow a formal without replacing its outer binding.

## Joining the APIs

```rust
// transfer.to is a ValueRef from a selected invocation's binding.
let cfg = icfg.get_cfg(transfer.to.function).expect("loaded callee");
if let ExpressionKind::Formal(id) =
    cfg.operands.expressions[transfer.to.expression.0 as usize].kind
{
    let entry = pdg.formal_definitions().find(|(formal, _)| *formal == id);
    // Follow the existing local PDG from this entry node.
}
```

Import `ExpressionKind` from `rgctl_analysis::operands`.
`ProgramDependenceGraph::formal_definitions()` returns a borrowed iterator;
build a query-local index once when joining many transfers. Qualify local formal
IDs with the canonical enclosing callable and procedural revision. Neither name
spelling nor line numbers establish the identity.

A regression binds `consume(second, first)` to `(left, right)`, joins the second
formal by ID, and follows PDG edges through `const alias = right` to
`return alias.execute()`. It supplies the canonical callee explicitly. This is
not an end-to-end dispatch or dependency-impact qualification.

## Compatibility and limits

CFG semantics revision is **4**, procedural archive format **RBCP v7**. Existing
revision guards invalidate older lowering even if source is unchanged. Formal
statement IDs and CFG block shapes can change; use canonical callable IDs and
typed local formal IDs rather than retaining statement UUIDs across revisions.
Defaults are still handled by the invocation binder, not executed at entry.

The statement-level PDG remains conservative: earlier same-block definitions can
remain in its dependency set after reassignment. This change does not qualify
precise dead-definition exclusions. Hoisting, Python function-wide local-name
rules, complex parameter patterns, capture/state
flow and other unsupported language forms still need dedicated qualification;
these tests do not establish complete lexical semantics.

[Local value transfers](local-value-flow.md) now map supported body expression
occurrences to exact scoped reads and initializer/assignment values. Matched
invocation contexts, receiver-origin propagation, target discovery and shared
interprocedural taint consumption remain separate work. A partial or
empty result cannot certify absence of dependency impact.
