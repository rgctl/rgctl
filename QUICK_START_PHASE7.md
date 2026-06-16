# Quick Start: Phase 7 Implementation

This guide helps you get started with Phase 7.1 immediately.

---

## What We're Building

**Goal:** Replace manual language plugins with TOML configuration + build-time code generation

**Before (Current):**
- 9 languages hard-coded
- ~5,000 LOC of repetitive plugin code
- Adding new language = 4-8 hours of Rust coding

**After (Phase 7 Complete):**
- 15-20 languages initially, 110+ eventually
- ~1,500 LOC total (70% reduction)
- Adding new language = 30 minutes of TOML config

---

## Phase 7.1: First Week Tasks

### Task 1: Create `languages.toml` (2-3 hours)

**File:** `/Users/sshaaf/git/rust/rBuilder/languages.toml`

**Start with this template:**

```toml
# languages.toml - Tree-sitter language configuration for rBuilder
[metadata]
version = "1.0"
description = "Official tree-sitter grammar configuration"

# ============================================================================
# RUST
# ============================================================================
[languages.rust]
crate = "tree-sitter-rust"
version = "0.20"
repository = "https://github.com/tree-sitter/tree-sitter-rust"
extensions = ["rs"]
aliases = ["rs"]

# Tree-sitter node types for extraction
function_kinds = ["function_item", "function_signature_item"]
class_kinds = ["struct_item", "enum_item", "impl_item"]
import_kinds = ["use_declaration"]

# Analysis settings
enable_complexity = true
enable_type_inference = false  # Rust has explicit types

# ============================================================================
# PYTHON
# ============================================================================
[languages.python]
crate = "tree-sitter-python"
version = "0.20"
repository = "https://github.com/tree-sitter/tree-sitter-python"
extensions = ["py", "pyw"]
aliases = ["python", "python3", "py"]
shebangs = ["python", "python3"]

# Tree-sitter node types
function_kinds = ["function_definition"]
class_kinds = ["class_definition"]
import_kinds = ["import_statement", "import_from_statement"]

# Analysis settings
enable_complexity = true
enable_type_inference = true  # Python benefits from type inference

# ============================================================================
# TYPESCRIPT
# ============================================================================
[languages.typescript]
crate = "tree-sitter-typescript"
version = "0.20"
repository = "https://github.com/tree-sitter/tree-sitter-typescript"
extensions = ["ts", "tsx"]
aliases = ["ts"]

# Tree-sitter node types
function_kinds = ["function_declaration", "method_definition", "arrow_function"]
class_kinds = ["class_declaration", "interface_declaration"]
import_kinds = ["import_statement"]

# Analysis settings
enable_complexity = true
enable_type_inference = false

# ============================================================================
# JAVASCRIPT
# ============================================================================
[languages.javascript]
crate = "tree-sitter-javascript"
version = "0.20"
repository = "https://github.com/tree-sitter/tree-sitter-javascript"
extensions = ["js", "jsx", "mjs"]
aliases = ["js"]

# Tree-sitter node types
function_kinds = ["function_declaration", "method_definition", "arrow_function"]
class_kinds = ["class_declaration"]
import_kinds = ["import_statement"]

# Analysis settings
enable_complexity = true
enable_type_inference = true

# ============================================================================
# GO
# ============================================================================
[languages.go]
crate = "tree-sitter-go"
version = "0.20"
repository = "https://github.com/tree-sitter/tree-sitter-go"
extensions = ["go"]
aliases = []

# Tree-sitter node types
function_kinds = ["function_declaration", "method_declaration"]
class_kinds = ["type_declaration"]  # Go uses type declarations
import_kinds = ["import_declaration"]

# Analysis settings
enable_complexity = true
enable_type_inference = false

# ============================================================================
# JAVA
# ============================================================================
[languages.java]
crate = "tree-sitter-java"
version = "0.20"
repository = "https://github.com/tree-sitter/tree-sitter-java"
extensions = ["java"]
aliases = []

# Tree-sitter node types
function_kinds = ["method_declaration"]
class_kinds = ["class_declaration", "interface_declaration", "enum_declaration"]
import_kinds = ["import_declaration"]

# Analysis settings
enable_complexity = true
enable_type_inference = false

# ============================================================================
# BUNDLES
# ============================================================================
[bundles.minimal]
description = "Core languages for backend development"
languages = ["rust", "python"]

[bundles.extended]
description = "Common web and systems languages"
languages = ["rust", "python", "typescript", "javascript", "go", "java"]

[bundles.full]
description = "All available languages"
languages = ["rust", "python", "typescript", "javascript", "go", "java"]
```

**TODO:** Add Kotlin, C#, and Markdown configurations (follow same pattern)

---

### Task 2: Create `build.rs` (4-5 hours)

**File:** `/Users/sshaaf/git/rust/rBuilder/build.rs`

**Implementation:**

```rust
use serde::Deserialize;
use std::env;
use std::fs;
use std::path::Path;

#[derive(Debug, Deserialize)]
struct LanguagesConfig {
    languages: std::collections::HashMap<String, LanguageConfig>,
}

#[derive(Debug, Deserialize)]
struct LanguageConfig {
    #[serde(rename = "crate")]
    crate_name: String,
}

fn main() {
    println!("cargo:rerun-if-changed=languages.toml");
    
    let out_dir = env::var("OUT_DIR").unwrap();
    let dest_path = Path::new(&out_dir).join("generated_plugins.rs");
    
    // Read and parse languages.toml
    let config_content = fs::read_to_string("languages.toml")
        .expect("Failed to read languages.toml");
    
    let config: LanguagesConfig = toml::from_str(&config_content)
        .expect("Failed to parse languages.toml");
    
    // Generate registration code
    let mut code = String::from("// Auto-generated by build.rs from languages.toml\n");
    code.push_str("// DO NOT EDIT MANUALLY\n\n");
    code.push_str("use std::sync::Arc;\n");
    code.push_str("use crate::languages::registry::LanguageRegistry;\n");
    code.push_str("use crate::languages::builtin::*;\n\n");
    code.push_str("pub fn register_all_plugins(registry: &mut LanguageRegistry) {\n");
    
    // Sort languages for deterministic output
    let mut lang_ids: Vec<_> = config.languages.keys().collect();
    lang_ids.sort();
    
    for lang_id in lang_ids {
        let plugin_name = capitalize(lang_id);
        code.push_str(&format!(
            "    #[cfg(feature = \"lang-{}\")]\n",
            lang_id
        ));
        code.push_str(&format!(
            "    registry.register_language_plugin(Arc::new({}Plugin::new().unwrap()));\n\n",
            plugin_name
        ));
    }
    
    code.push_str("}\n");
    
    // Write generated code
    fs::write(&dest_path, code).expect("Failed to write generated code");
    
    println!("Generated plugin registration code at {}", dest_path.display());
}

fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(first) => {
            let mut result = first.to_uppercase().collect::<String>();
            result.push_str(chars.as_str());
            result
        }
    }
}
```

---

### Task 3: Update `Cargo.toml` (1-2 hours)

**Changes to make:**

```toml
# Add to [build-dependencies] section
[build-dependencies]
toml = "0.8"
serde = { version = "1", features = ["derive"] }

# Make tree-sitter dependencies optional
[dependencies]
tree-sitter = "0.20"  # Keep required

tree-sitter-rust = { version = "0.20", optional = true }
tree-sitter-python = { version = "0.20", optional = true }
tree-sitter-typescript = { version = "0.20", optional = true }
tree-sitter-javascript = { version = "0.20", optional = true }
tree-sitter-go = { version = "0.20", optional = true }
tree-sitter-java = { version = "0.20", optional = true }

# Add features section
[features]
default = ["bundle-extended"]

# Individual languages
lang-rust = ["tree-sitter-rust"]
lang-python = ["tree-sitter-python"]
lang-typescript = ["tree-sitter-typescript"]
lang-javascript = ["tree-sitter-javascript"]
lang-go = ["tree-sitter-go"]
lang-java = ["tree-sitter-java"]

# Bundles
bundle-minimal = ["lang-rust", "lang-python"]
bundle-extended = ["bundle-minimal", "lang-typescript", "lang-javascript", "lang-go", "lang-java"]
bundle-full = ["bundle-extended"]
```

---

### Task 4: Update Registry (30 minutes)

**File:** `src/languages/registry.rs`

**Change this:**
```rust
impl LanguageRegistry {
    pub fn new() -> Self {
        let mut registry = Self {
            language_plugins: HashMap::new(),
            config_plugins: HashMap::new(),
            extension_map: HashMap::new(),
            config_extension_map: HashMap::new(),
        };

        // OLD: Manual registration
        registry.register_language_plugin(Arc::new(RustPlugin::new().unwrap()));
        registry.register_language_plugin(Arc::new(PythonPlugin::new().unwrap()));
        // ... etc
        
        registry
    }
}
```

**To this:**
```rust
// Include generated code
include!(concat!(env!("OUT_DIR"), "/generated_plugins.rs"));

impl LanguageRegistry {
    pub fn new() -> Self {
        let mut registry = Self {
            language_plugins: HashMap::new(),
            config_plugins: HashMap::new(),
            extension_map: HashMap::new(),
            config_extension_map: HashMap::new(),
        };

        // NEW: Auto-generated registration
        register_all_plugins(&mut registry);
        
        registry
    }
}
```

---

### Task 5: Test Everything (2-3 hours)

**Commands to run:**

```bash
# Clean build
cargo clean

# Default build (should include extended bundle)
cargo build

# Run all tests
cargo test

# Try minimal bundle
cargo build --no-default-features --features bundle-minimal

# Try custom configuration
cargo build --no-default-features --features "lang-rust,lang-go"

# Verify binary sizes
ls -lh target/debug/rbuilder
ls -lh target/release/rbuilder

# Check generated code
cat target/debug/build/rbuilder-*/out/generated_plugins.rs
```

---

## Expected Outcomes

After completing Phase 7.1:

✅ **Build System:**
- `cargo build` works with feature flags
- Generated code in `target/.../out/generated_plugins.rs`
- All 222 tests still pass

✅ **Configuration:**
- `languages.toml` defines all languages
- Bundles available: minimal, extended, full
- Feature flags working

✅ **No Breaking Changes:**
- All existing code still works
- Default build includes same languages as before
- Tests unchanged

✅ **Ready for Phase 7.2:**
- Infrastructure in place for proc macros
- TOML schema validated
- Feature flags tested

---

## Troubleshooting

**Problem:** `cargo build` fails with "cannot find feature"
**Solution:** Make sure feature names in `Cargo.toml` match those in `build.rs`

**Problem:** Generated code has syntax errors
**Solution:** Check `capitalize()` function in `build.rs` and plugin naming

**Problem:** Tests fail after changes
**Solution:** Verify all languages are included in default features

**Problem:** `languages.toml` not found
**Solution:** Make sure it's in the project root, next to `Cargo.toml`

---

## Next Steps After Phase 7.1

1. ✅ Phase 7.2: Create proc macros
2. ✅ Phase 7.3: Migrate plugins to use macros
3. ✅ Phase 7.4: Add 5-10 more languages
4. ✅ Documentation and community guide

---

**Estimated Time for Phase 7.1:** 8-12 hours total  
**Parallelizable:** Some tasks can be done simultaneously  
**Risk Level:** Low (all changes are additive, easy to rollback)
