# Phase 7: Tree-sitter Language System Refactor

> **⚠️ IMPORTANT: This file is deprecated.**
> 
> **Single Source of Truth:** [TASK_PLAN.md](./TASK_PLAN.md)
> 
> All Phase 7 planning has been integrated into `TASK_PLAN.md` to maintain a single authoritative source for project planning.

---

## Quick Navigation

For Phase 7 details, see **[TASK_PLAN.md - Phase 7](./TASK_PLAN.md#phase-7-tree-sitter-language-system-refactor-weeks-20-23-)** which includes:

- **Phase 7.1** - Infrastructure Setup (Week 20) 🎯 CURRENT
  - Task 7.1.1: Create `languages.toml` configuration
  - Task 7.1.2: Implement `build.rs` code generator
  - Task 7.1.3: Update `Cargo.toml` with feature flags
  - Task 7.1.4: Test & validate infrastructure

- **Phase 7.2** - Procedural Macro Development (Week 21)
  - Create `rbuilder-macros` crate
  - Implement `#[derive(LanguagePlugin)]` macro
  - Generic extraction helpers

- **Phase 7.3** - Migration of Existing Languages (Week 22)
  - Migrate all 9 languages to macro-based approach
  - Remove legacy plugin code

- **Phase 7.4** - Testing & Documentation (Week 23)
  - Comprehensive testing
  - Add 5-10 new languages
  - Update documentation

---

## Why This File Exists

This file was created as a detailed Phase 7 execution plan but caused confusion because it was separate from the main TASK_PLAN.md. Cursor and other tools were looking at TASK_PLAN.md while this file contained the actual Phase 7 details, leading to misalignment.

**Solution:** All content has been merged into TASK_PLAN.md.

---

**Note:** This file is kept for historical reference only. All current planning is in TASK_PLAN.md.

**Last Updated:** June 17, 2026  
**Status:** Deprecated - See TASK_PLAN.md
