# Language Configuration Guide (Phase 7)

rBuilder languages are defined in `languages.toml` and compiled into the binary via feature flags.

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
