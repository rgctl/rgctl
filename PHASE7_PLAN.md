# Phase 7: Tree-sitter Language System Refactor

**Status:** 🎯 Active  
**Duration:** 4 weeks  
**Current Sub-phase:** 7.1 - Infrastructure Setup  
**Progress:** 0/4 sub-phases complete

---

## Quick Links
- [Full Roadmap](./ROADMAP.md)
- [Deferred Tasks](./DEFERRED_TASKS.md)

---

## Current Sprint: Phase 7.1 (Week 1)

### Goals
- ✅ Create TOML-based language configuration
- ✅ Implement build-time code generation
- ✅ Add feature flags for optional languages
- ✅ Validate with existing 9 languages

### Task Breakdown

#### Task 7.1.1: Create `languages.toml` ⏳
**Owner:** TBD  
**Effort:** 2-3 hours  
**Status:** Not started

**Deliverables:**
- [ ] File created: `languages.toml`
- [ ] All 9 languages configured
- [ ] Bundle definitions added
- [ ] Schema documented

**Example Structure:**
```toml
# languages.toml
[metadata]
version = "1.0"
description = "rBuilder tree-sitter language configuration"

[languages.rust]
crate = "tree-sitter-rust"
version = "0.20"
repository = "https://github.com/tree-sitter/tree-sitter-rust"
extensions = ["rs"]
function_kinds = ["function_item", "function_signature_item"]
class_kinds = ["struct_item", "enum_item", "impl_item"]

[bundles.minimal]
description = "Core languages for backend development"
languages = ["rust", "python"]

[bundles.extended]
description = "Common web and systems languages"
languages = ["rust", "python", "typescript", "javascript", "go", "java"]

[bundles.full]
description = "All available languages"
languages = ["rust", "python", "typescript", "javascript", "go", "java", "kotlin", "csharp", "markdown"]
```

---

#### Task 7.1.2: Implement `build.rs` ⏳
**Owner:** TBD  
**Effort:** 4-5 hours  
**Status:** Not started

**Dependencies:** Task 7.1.1 complete

**Deliverables:**
- [ ] File created: `build.rs`
- [ ] TOML parser working
- [ ] Code generation tested
- [ ] Build validation added

**Code Generator Requirements:**
```rust
// Generated code should look like:
pub fn register_all_plugins(registry: &mut LanguageRegistry) {
    #[cfg(feature = "lang-rust")]
    registry.register_language_plugin(Arc::new(RustPlugin::new().unwrap()));
    
    #[cfg(feature = "lang-python")]
    registry.register_language_plugin(Arc::new(PythonPlugin::new().unwrap()));
    
    // ... etc for all languages
}
```

**Build Validation:**
- Validate TOML syntax
- Check for duplicate language IDs
- Verify crate names match available dependencies
- Ensure bundle references are valid

---

#### Task 7.1.3: Update `Cargo.toml` ⏳
**Owner:** TBD  
**Effort:** 1-2 hours  
**Status:** Not started

**Dependencies:** Task 7.1.2 complete

**Deliverables:**
- [ ] All tree-sitter deps made optional
- [ ] Feature flags created
- [ ] Bundles defined
- [ ] Build dependencies added

**Changes Required:**
```toml
[dependencies]
tree-sitter = "0.20"  # Always included

# Make all language grammars optional
tree-sitter-rust = { version = "0.20", optional = true }
tree-sitter-python = { version = "0.20", optional = true }
tree-sitter-typescript = { version = "0.20", optional = true }
tree-sitter-javascript = { version = "0.20", optional = true }
tree-sitter-go = { version = "0.20", optional = true }
tree-sitter-java = { version = "0.20", optional = true }

[build-dependencies]
toml = "0.8"
serde = { version = "1", features = ["derive"] }

[features]
default = ["bundle-extended"]

# Individual language features
lang-rust = ["tree-sitter-rust"]
lang-python = ["tree-sitter-python"]
lang-typescript = ["tree-sitter-typescript"]
lang-javascript = ["tree-sitter-javascript"]
lang-go = ["tree-sitter-go"]
lang-java = ["tree-sitter-java"]
lang-kotlin = []  # Uses tree-sitter-kotlin if available
lang-csharp = []  # Uses tree-sitter-c-sharp if available
lang-markdown = []

# Bundles
bundle-minimal = ["lang-rust", "lang-python"]
bundle-extended = ["bundle-minimal", "lang-typescript", "lang-javascript", "lang-go", "lang-java"]
bundle-full = ["bundle-extended", "lang-kotlin", "lang-csharp", "lang-markdown"]
```

---

#### Task 7.1.4: Test & Validate ⏳
**Owner:** TBD  
**Effort:** 2-3 hours  
**Status:** Not started

**Dependencies:** Tasks 7.1.1, 7.1.2, 7.1.3 complete

**Deliverables:**
- [ ] All 222 tests pass with default features
- [ ] Bundle builds tested
- [ ] Feature flag combinations validated
- [ ] Generated code reviewed

**Test Matrix:**
```bash
# Default build
cargo build

# Minimal build
cargo build --no-default-features --features bundle-minimal

# Extended build
cargo build --features bundle-extended

# Full build
cargo build --features bundle-full

# Custom build
cargo build --no-default-features --features "lang-rust,lang-go"

# Test all configurations
cargo test
cargo test --no-default-features --features bundle-minimal
cargo test --features bundle-full
```

**Acceptance Criteria:**
- ✅ `cargo build` succeeds with all feature combinations
- ✅ All 222 tests pass in all configurations
- ✅ Generated code is syntactically correct
- ✅ Binary sizes vary by feature selection
- ✅ No new clippy warnings

---

## Phase 7.1 Completion Checklist

- [ ] `languages.toml` created and validated
- [ ] `build.rs` generates correct registration code
- [ ] `Cargo.toml` updated with feature flags
- [ ] All tests pass in all feature configurations
- [ ] Documentation updated (README, CONTRIBUTING)
- [ ] Commit and tag: `v0.2.0-phase7.1`

---

## Next Steps (Phase 7.2)

After completing Phase 7.1, we'll move to:
- Create `rbuilder-macros` crate
- Implement `#[derive(LanguagePlugin)]` proc macro
- Develop generic extraction helpers
- Test macro-generated plugins

---

## Rollback Plan

If Phase 7.1 encounters blockers:
1. All changes are additive (no deletions yet)
2. Can revert Cargo.toml changes
3. Delete build.rs
4. Delete languages.toml
5. System returns to current state (all languages hard-coded)

No risk to existing functionality.

---

## Phase 7 Overall Timeline

| Sub-phase | Duration | Status |
|-----------|----------|--------|
| 7.1 Infrastructure | Week 20 (current) | 🎯 Active |
| 7.2 Proc Macros | Week 21 | ⏸️ Planned |
| 7.3 Migration | Week 22 | ⏸️ Planned |
| 7.4 Testing & Docs | Week 23 | ⏸️ Planned |

**Estimated Completion:** End of Week 23 (July 7, 2026)

---

## Questions & Decisions

### Q1: Should we support runtime plugin loading?
**Decision:** No, not in Phase 7. Keep compile-time only for now.  
**Reason:** Simpler, safer, faster. Can add later if needed.

### Q2: How granular should feature flags be?
**Decision:** One feature per language, plus bundles.  
**Reason:** Gives users maximum flexibility without overwhelming options.

### Q3: What if a tree-sitter grammar doesn't exist for a language?
**Decision:** Mark as "planned" in TOML, skip in build.rs if crate missing.  
**Reason:** Future-proofing for languages we want to support.

### Q4: Should we auto-generate plugin trait implementations?
**Decision:** Not in Phase 7.1. Phase 7.2 (proc macros) will handle this.  
**Reason:** Separation of concerns. Infrastructure first, then code generation.

---

**Last Updated:** June 16, 2026  
**Next Review:** June 23, 2026  
**Phase Owner:** TBD
