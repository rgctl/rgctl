# Contributing to rBuilder

Thank you for your interest in contributing to rBuilder! This guide will help you get started.

---

## Table of Contents

- [Code of Conduct](#code-of-conduct)
- [Getting Started](#getting-started)
- [Development Setup](#development-setup)
- [Project Structure](#project-structure)
- [How to Contribute](#how-to-contribute)
- [Pull Request Process](#pull-request-process)
- [Coding Guidelines](#coding-guidelines)
- [Testing Guidelines](#testing-guidelines)
- [Adding New Languages](#adding-new-languages)
- [Documentation](#documentation)
- [Getting Help](#getting-help)

---

## Code of Conduct

This project follows the [Contributor Covenant Code of Conduct](CODE_OF_CONDUCT.md). By participating, you agree to uphold this code.

---

## Getting Started

### Prerequisites

- **Rust**: 1.70 or later (install via [rustup](https://rustup.rs/))
- **Git**: For version control

### Fork and Clone

```bash
# Fork the repository on GitHub, then clone your fork
git clone https://github.com/YOUR_USERNAME/rBuilder.git
cd rBuilder

# Add upstream remote
git remote add upstream https://github.com/sshaaf/rBuilder.git
```

---

## Development Setup

### Build the Project

```bash
# Build with all features (default)
cargo build

# Build with minimal features (faster compile)
cargo build --no-default-features --features bundle-minimal

# Run tests
cargo test

# Run linter
cargo clippy --all-targets -- -D warnings

# Format code
cargo fmt
```

See [LANGUAGE_GUIDE.md](LANGUAGE_GUIDE.md) for language feature flags and bundles.

---

## Project Structure

```
rBuilder/
├── src/
│   ├── graph/               # Graph database (memory + IndraDB backends)
│   ├── languages/           # Language plugins (3-tier hybrid architecture)
│   │   ├── builtin/         # Tier 1: Custom plugins (Rust, Python, TS, Go, Java)
│   │   ├── generic/         # Tier 2/3: Tree-sitter + Regex plugins
│   │   └── extraction/      # Shared extraction helpers
│   ├── nlp/                 # Natural language query engine
│   ├── mcp/                 # Model Context Protocol server
│   ├── pipeline/            # Extraction pipeline with parallel processing
│   ├── analysis/            # Complexity, communities, centrality
│   └── incremental/         # Git-aware incremental updates
├── languages.toml           # Language config (single source of truth)
├── build.rs                 # Build-time code generation
└── rbuilder-macros/         # Proc macros for plugin boilerplate
```

---

## How to Contribute

### Types of Contributions

1. **Bug Reports**: Use the Bug Report template
2. **Feature Requests**: Use the Feature Request template
3. **Language Support**: Add new programming languages
4. **Performance**: Optimize hot paths, reduce allocations
5. **Documentation**: Improve guides, add examples

### Good First Issues

Look for issues labeled `good first issue`.

---

## Pull Request Process

### 1. Create a Branch

```bash
git checkout -b feature/your-feature-name
```

### 2. Make Changes

- Follow coding guidelines below
- Add tests for new functionality
- Update documentation

### 3. Commit

Use conventional commit format:

```bash
git commit -m "feat: add support for Elixir language"
```

Types: `feat`, `fix`, `docs`, `style`, `refactor`, `perf`, `test`, `chore`

### 4. Push and Create PR

```bash
git push origin feature/your-feature-name
```

Then create a Pull Request on GitHub.

---

## Coding Guidelines

### Rust Style

- Use `cargo fmt` (enforced in CI)
- Use `cargo clippy` and fix all warnings (enforced in CI)
- Follow [Rust API Guidelines](https://rust-lang.github.io/api-guidelines/)
- Document public items with `///` doc comments

### Error Handling

```rust
// Use Result<T> with descriptive errors
pub fn parse(&self, path: &Path) -> Result<Vec<Node>> {
    // ...
}

// Don't use unwrap() in library code (tests OK)
```

### Performance

- Avoid allocations in hot paths
- Use `&str` over `String` when possible
- Profile before optimizing (`cargo bench`)

---

## Testing Guidelines

### Unit Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extraction() {
        // ...
    }
}
```

### Feature-Gated Tests

```rust
#[cfg(feature = "lang-python")]
#[test]
fn test_python_extraction() {
    // Only runs when Python language enabled
}
```

### Requirements

- All features MUST have tests
- Bug fixes MUST include regression tests
- Run: `cargo test -- --test-threads=1`
- Test minimal bundle: `cargo test --no-default-features --features bundle-minimal`

---

## Adding New Languages

See [LANGUAGE_GUIDE.md](LANGUAGE_GUIDE.md) for the complete three-tier architecture guide.

### Quick Summary

**Tier 3 (Regex)**: < 15 min - Edit `languages.toml`, add regex patterns
**Tier 2 (Tree-sitter)**: < 30 min - Add dependency + TOML config
**Tier 1 (Custom)**: 2-4 hours - Write custom plugin using tree-sitter + enrichment

All Tier 1 plugins MUST use tree-sitter as the parsing foundation.

---

## Documentation

### Code Docs

```rust
/// Parse source and extract symbols.
///
/// # Arguments
/// * `path` - File path
/// * `source` - Source bytes
///
/// # Returns
/// Vector of symbols
pub fn extract_symbols(path: &Path, source: &[u8]) -> Result<Vec<Symbol>> {
    // ...
}
```

### User Docs

- README.md - User-facing features
- LANGUAGE_GUIDE.md - Language additions
- TASK_PLAN.md - Implementation status

---

## Getting Help

- **Questions**: Open a Discussion
- **Bugs**: Create a Bug Report issue
- **Features**: Create a Feature Request issue

---

Thank you for contributing to rBuilder!
