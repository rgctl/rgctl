//! Explicit lexical scope stack for single-pass AST extraction.

use smallvec::SmallVec;

/// Kind of an active scope frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ScopeKind {
    Module,
    Class,
    Struct,
    Enum,
    Impl,
    Trait,
    Interface,
    Function,
    Method,
    Lambda,
    GuardBlock,
}

impl ScopeKind {
    /// True for function-like frames (enclosing call/complexity context).
    pub const fn is_function_like(self) -> bool {
        matches!(self, Self::Function | Self::Method | Self::Lambda)
    }

    /// True for type-like frames (enclosing `self.x` / inherent methods).
    pub const fn is_type_like(self) -> bool {
        matches!(
            self,
            Self::Class | Self::Struct | Self::Enum | Self::Impl | Self::Trait | Self::Interface
        )
    }
}

/// Complexity / metrics accumulated on a function-like frame during one AST pass.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ScopeComplexity {
    pub cyclomatic: u32,
    pub cognitive: u32,
    pub max_nesting: u32,
    pub return_count: u32,
    pub loc: u32,
    pub param_count: u32,
    /// Cognitive nesting depth local to this function/closure (starts at 0;
    /// nested lambdas do not inherit the outer frame's branch depth).
    pub cognitive_nesting: u32,
}

/// One frame on the [`ScopeStack`].
#[derive(Debug, Clone)]
pub struct ScopeFrame {
    pub kind: ScopeKind,
    /// Index into the in-progress `symbols` vec (if already pushed).
    pub symbol_idx: Option<usize>,
    /// Unqualified name when known.
    pub name: Option<String>,
    /// Qualified prefix for nested symbols (e.g. `pkg.Class`).
    pub qualified_prefix: String,
    pub start_byte: usize,
    pub end_byte: usize,
    /// Nesting depth of this frame (0 = file/module root).
    pub depth: u32,
    /// Only meaningful for function-like frames.
    pub complexity: ScopeComplexity,
}

impl ScopeFrame {
    pub fn new(
        kind: ScopeKind,
        name: Option<String>,
        qualified_prefix: String,
        start_byte: usize,
        end_byte: usize,
        depth: u32,
    ) -> Self {
        Self {
            kind,
            symbol_idx: None,
            name,
            qualified_prefix,
            start_byte,
            end_byte,
            depth,
            complexity: ScopeComplexity::default(),
        }
    }
}

/// Explicit scope stack (`SmallVec` inline capacity 8).
#[derive(Debug, Clone, Default)]
pub struct ScopeStack {
    frames: SmallVec<[ScopeFrame; 8]>,
}

impl ScopeStack {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, frame: ScopeFrame) {
        self.frames.push(frame);
    }

    pub fn pop(&mut self) -> Option<ScopeFrame> {
        self.frames.pop()
    }

    pub fn len(&self) -> usize {
        self.frames.len()
    }

    pub fn is_empty(&self) -> bool {
        self.frames.is_empty()
    }

    pub fn current(&self) -> Option<&ScopeFrame> {
        self.frames.last()
    }

    pub fn current_mut(&mut self) -> Option<&mut ScopeFrame> {
        self.frames.last_mut()
    }

    /// Innermost function / method / lambda frame.
    pub fn current_function(&self) -> Option<&ScopeFrame> {
        self.frames.iter().rev().find(|f| f.kind.is_function_like())
    }

    pub fn current_function_mut(&mut self) -> Option<&mut ScopeFrame> {
        self.frames
            .iter_mut()
            .rev()
            .find(|f| f.kind.is_function_like())
    }

    /// Innermost class / struct / enum / impl / trait / interface.
    pub fn current_type(&self) -> Option<&ScopeFrame> {
        self.frames.iter().rev().find(|f| f.kind.is_type_like())
    }

    /// Innermost module frame (`defmodule`, `module`, `namespace`, etc.).
    pub fn current_module(&self) -> Option<&ScopeFrame> {
        self.frames
            .iter()
            .rev()
            .find(|f| f.kind == ScopeKind::Module)
    }

    pub fn depth(&self) -> u32 {
        self.frames.len() as u32
    }

    /// Join active module/type frame names into a qualified prefix for `local` (`.` sep).
    pub fn qualified_name(&self, local: &str) -> String {
        self.qualified_name_sep(local, ".")
    }

    /// Join active module/type frame names with `sep` (e.g. `::` for C++).
    pub fn qualified_name_sep(&self, local: &str, sep: &str) -> String {
        let mut parts: SmallVec<[&str; 4]> = SmallVec::new();
        for frame in &self.frames {
            if matches!(
                frame.kind,
                ScopeKind::Module
                    | ScopeKind::Class
                    | ScopeKind::Struct
                    | ScopeKind::Enum
                    | ScopeKind::Impl
                    | ScopeKind::Trait
                    | ScopeKind::Interface
            ) && let Some(name) = frame.name.as_deref()
                && !name.is_empty()
            {
                parts.push(name);
            }
        }
        if parts.is_empty() {
            return local.to_string();
        }
        let mut out = parts.join(sep);
        out.push_str(sep);
        out.push_str(local);
        out
    }

    /// Bump cyclomatic (+1) and optional cognitive on the active function frame.
    ///
    /// `max_nesting` is derived from the function frame's own `cognitive_nesting`
    /// (not `frames.len()`, which falsely includes Module/Class/Impl frames).
    pub fn note_branch(&mut self, cognitive_delta: u32) {
        if let Some(f) = self.current_function_mut() {
            f.complexity.cyclomatic = f.complexity.cyclomatic.saturating_add(1);
            f.complexity.cognitive = f.complexity.cognitive.saturating_add(cognitive_delta);
            let nest = f.complexity.cognitive_nesting.saturating_add(1);
            f.complexity.max_nesting = f.complexity.max_nesting.max(nest);
        }
    }

    /// Enter a control-flow / nesting construct inside the active function.
    pub fn enter_branch(&mut self) {
        if let Some(f) = self.current_function_mut() {
            f.complexity.cognitive_nesting = f.complexity.cognitive_nesting.saturating_add(1);
        }
    }

    /// Leave a control-flow / nesting construct inside the active function.
    pub fn leave_branch(&mut self) {
        if let Some(f) = self.current_function_mut() {
            f.complexity.cognitive_nesting = f.complexity.cognitive_nesting.saturating_sub(1);
        }
    }

    pub fn note_return(&mut self) {
        if let Some(f) = self.current_function_mut() {
            f.complexity.return_count = f.complexity.return_count.saturating_add(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enclosing_function_and_type() {
        let mut stack = ScopeStack::new();
        stack.push(ScopeFrame::new(
            ScopeKind::Module,
            Some("mod".into()),
            String::new(),
            0,
            100,
            0,
        ));
        stack.push(ScopeFrame::new(
            ScopeKind::Class,
            Some("Foo".into()),
            "Foo".into(),
            10,
            90,
            1,
        ));
        stack.push(ScopeFrame::new(
            ScopeKind::Method,
            Some("bar".into()),
            "Foo.bar".into(),
            20,
            80,
            2,
        ));
        assert_eq!(stack.current_type().unwrap().name.as_deref(), Some("Foo"));
        assert_eq!(
            stack.current_function().unwrap().name.as_deref(),
            Some("bar")
        );
        stack.note_branch(1);
        stack.note_return();
        let f = stack.current_function().unwrap();
        assert_eq!(f.complexity.cyclomatic, 1);
        assert_eq!(f.complexity.return_count, 1);
    }
}
