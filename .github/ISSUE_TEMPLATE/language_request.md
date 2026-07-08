---
name: Language Support Request
about: Request support for a new programming language
title: '[LANGUAGE] Add support for <language name>'
labels: language-support
assignees: ''
---

## Language Information

**Language Name:** (e.g., Elixir, Zig, OCaml)

**File Extensions:** (e.g., .ex, .exs)

**Official Website:** (link to language homepage)

## Tree-sitter Grammar

**Does a tree-sitter grammar exist?**

- [ ] Yes - https://github.com/tree-sitter/tree-sitter-LANGUAGE
- [ ] No - would need regex-based support

**Grammar Quality:**

- [ ] Mature and stable
- [ ] Experimental or incomplete
- [ ] Unknown

## Language Tier

Based on the [Tier 1 language support guide](../../docs/tier-1-language-support.md), which tier would be most appropriate?

- [ ] **Tier 1 (Custom)** - Need type inference or complex relationships
- [ ] **Tier 2 (Tree-sitter)** - Basic tree-sitter extraction sufficient
- [ ] **Tier 3 (Regex)** - No tree-sitter available or niche language

## Use Case

Why do you need support for this language?

**Project context:**
- Project size: (LOC, number of files)
- Language features used: (e.g., OOP, functional, metaprogramming)
- Integration needs: (e.g., type inference, cross-language calls)

## Sample Code

Provide a small code sample that demonstrates the language features you'd like extracted:

```your-language
// Paste sample code here
class Example {
    func method() {
        // ...
    }
}
```

**Expected symbols to extract:**
- Classes: Example
- Functions: method
- Other: (imports, types, etc.)

## Priority

How important is this to your workflow?

- [ ] Critical - Blocking major project
- [ ] High - Needed soon
- [ ] Medium - Would be nice to have
- [ ] Low - Exploratory

## I Can Help

- [ ] I can provide sample code and test cases
- [ ] I can test the implementation
- [ ] I can write the plugin myself (with guidance)
- [ ] I can contribute tree-sitter node kind mappings

## Additional Context

Add any other information about the language or your needs here.
