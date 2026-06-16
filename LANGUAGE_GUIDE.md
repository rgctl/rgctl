# Language Configuration Guide (Phase 7)

rBuilder languages are defined in `languages.toml` and compiled into the binary via feature flags.

---

## **Architectural Principle: Hybrid Tiering**

**Mission**: Arm AI coding agents with **deep, queryable codebase understanding**.

rBuilder uses a **three-tier hybrid approach** that balances quality and scalability:

### **Tier 1: Custom Plugins** (Rich AI Value)
**When to use**: High-value languages needing rich extraction (type inference, complex relationships)

**✅ REQUIREMENT: All Tier 1 custom plugins MUST use tree-sitter as the parsing foundation.**

Custom plugins are **"tree-sitter + enrichment"**, NOT replacements for tree-sitter.

**Structure**:
```rust
// CORRECT: tree-sitter foundation + custom enrichment
fn extract_symbols(&self, source: &[u8]) -> Result<Vec<Symbol>> {
    // 1. Parse with tree-sitter (REQUIRED)
    let mut parser = Parser::new();
    parser.set_language(tree_sitter_python::language())?;
    let tree = parser.parse(source, None)?;
    
    // 2. Basic extraction from tree-sitter AST
    let symbols = extract_from_tree(tree)?;
    
    // 3. Custom enrichment on top
    let inferencer = TypeInferencer::new();
    let inferred_types = inferencer.infer_python(function_source);
    for symbol in &mut symbols {
        // Add inferred types, relationships, etc.
    }
    
    Ok(symbols)
}
```

**Current Tier 1 Languages** (7):
- **Python** - tree-sitter-python + type inference
- **JavaScript** - tree-sitter-javascript + type inference
- **TypeScript** - tree-sitter-typescript + TSX handling
- **Rust** - tree-sitter-rust + trait/lifetime extraction
- **Go** - tree-sitter-go + package semantics
- **Java** - tree-sitter-java + annotation extraction
- **Markdown** - pulldown-cmark (EXCEPTION: specialized CommonMark parser)

**AI Agent Value**: HIGH - Rich type information, complex relationships, accurate complexity metrics

---

### **Tier 2: Generic Tree-Sitter** (Easy Coverage)
**When to use**: Languages where tree-sitter grammar exists, basic coverage sufficient

**Implementation**: TOML-only configuration, no Rust code needed

**Add time**: < 30 minutes

**Current Tier 2 Languages** (4):
- C, C++, Ruby, PHP

**AI Agent Value**: MEDIUM - Basic symbol extraction, complexity metrics

---

### **Tier 3: Regex Fallback** (Pragmatic Coverage)
**When to use**: No tree-sitter grammar available, or language is niche

**Implementation**: Regex patterns in TOML

**Add time**: < 15 minutes

**Current Tier 3 Languages** (2):
- Kotlin, C#

**AI Agent Value**: LOW-MEDIUM - Symbol names and locations only

---

### **Promotion Path**

As languages become more important or tree-sitter grammars mature:

```
Tier 3 (Regex) → Tier 2 (Tree-Sitter) → Tier 1 (Custom + Tree-Sitter)
```

**Examples**:
- Kotlin, C# can promote to Tier 2 (tree-sitter grammars exist)
- C, Ruby can promote to Tier 1 if type inference is needed

---

## Quick Reference

| Bundle | Languages | Use case |
|--------|-----------|----------|
| `bundle-minimal` | Rust, Python | Backend / scripting only |
| `bundle-extended` | + TS, JS, Go, Java | Web and systems development |
| `bundle-full` | + Kotlin, C#, Markdown | All built-in languages (default) |
| `bundle-extra` | + C, C++, Ruby, PHP | Additional TOML-only grammars |

## Build Examples

```bash
# Default: all 9 core languages
cargo build

# Minimal binary (~60% smaller language footprint)
cargo build --no-default-features --features bundle-minimal

# Custom selection
cargo build --no-default-features --features "lang-rust,lang-go,lang-python"

# All languages including C/Ruby/PHP/C++
cargo build --no-default-features --features "bundle-full,bundle-extra,mcp-server,nlp-patterns"
```

## Adding a New Language

### 1. Add entry to `languages.toml`

**Tree-sitter language (TOML-only, no Rust code):**

```toml
[languages.zig]
handler = "tree-sitter"
crate = "tree-sitter-zig"
extensions = ["zig"]
function_kinds = ["function_declaration"]
class_kinds = ["struct_declaration"]
enable_complexity = true
```

**Regex-based language:**

```toml
[languages.dockerfile]
handler = "regex"
extensions = ["dockerfile"]
enable_complexity = false

[[languages.dockerfile.regex_patterns]]
pattern = '(?m)^FROM\s+(\S+)'
symbol_type = "class"
```

### 2. Add Cargo feature and optional dependency

```toml
# Cargo.toml
tree-sitter-zig = { version = "0.20", optional = true }

[features]
lang-zig = ["tree-sitter-zig"]
```

### 3. Rebuild

`build.rs` automatically generates plugin registration from `languages.toml`. No manual registry edits needed.

## Handler Types

| Handler | Description |
|---------|-------------|
| `custom` | Hand-written plugin in `src/languages/builtin/` (Rust, Python, etc.) |
| `tree-sitter` | Generic plugin using node kind mappings from TOML |
| `regex` | Generic regex line scanner |
| `markdown` | Pulldown-cmark documentation parser |

## Custom Plugins with Proc Macros

Enable `proc-macros` feature and use `rbuilder-macros`:

```rust
use rbuilder_macros::LanguagePlugin;

#[derive(LanguagePlugin)]
#[lang(id = "rust", extensions = ["rs"], grammar = "tree_sitter_rust")]
pub struct RustPlugin {
    _parser: tree_sitter::Parser,
}

impl LanguagePlugin for RustPlugin {
    // delegate language_id/file_extensions/grammar to inherent methods
    fn language_id(&self) -> &str { Self::language_id(self) }
    fn file_extensions(&self) -> Vec<&str> { Self::file_extensions(self) }
    fn grammar(&self) -> Option<tree_sitter::Language> { Self::grammar(self) }
    // ... custom extract_symbols, etc.
}
```

## File Layout

```
languages.toml          # Language definitions (source of truth)
build.rs                # Generates registration + configs at compile time
src/languages/
  generic/              # Config-driven plugins (regex, tree-sitter)
  extraction/           # Shared extraction helpers
  builtin/              # Custom plugins (complex languages)
rbuilder-macros/        # #[derive(LanguagePlugin)] proc macro
```
