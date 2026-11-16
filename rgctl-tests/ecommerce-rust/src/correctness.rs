//! Intentional call graph for rgctl expected-facts checks.
//! Prefer direct function calls so extraction does not depend on DI.

#![allow(dead_code, unused_variables)]

/// Leaf — no outbound application calls.
pub fn correctness_leaf() -> i32 {
    42
}

/// Mid — calls [`correctness_leaf`].
pub fn correctness_mid() -> i32 {
    correctness_leaf() + 1
}

/// Root — calls [`correctness_mid`] and branches for a non-trivial CFG.
pub fn correctness_root(flag: bool) -> i32 {
    let value = correctness_mid();
    if flag {
        value * 2
    } else {
        value
    }
}

/// Shared sink for diamond topology QE.
pub fn correctness_shared() -> i32 {
    1
}

pub fn correctness_left() -> i32 {
    correctness_shared()
}

pub fn correctness_right() -> i32 {
    correctness_shared()
}

/// Diamond root — dual callers into [`correctness_shared`].
pub fn correctness_diamond() -> i32 {
    correctness_left() + correctness_right()
}

pub fn v2_free_helper(offset: i32) -> i32 {
    offset + 5
}

pub trait V2AuditPolicy {
    fn audit_weight(&self) -> i32 {
        1
    }
}

pub struct V2OrderLedger {
    pub balance: i32,
}

impl V2OrderLedger {
    pub fn new(initial: i32) -> Self {
        Self { balance: initial }
    }

    pub fn balance(&self) -> i32 {
        v2_free_helper(42) + self.balance
    }

    pub fn apply_adjustment(&mut self, delta: i32) {
        self.balance = self.balance + delta;
    }
}

impl V2AuditPolicy for V2OrderLedger {
    fn audit_weight(&self) -> i32 {
        self.balance()
    }
}

pub fn v2_slice_flow_probe(seed_input: i32) -> i32 {
    let dead_unused = 999;
    let step_val = seed_input + 1;
    let alias_val = step_val;
    let final_out = alias_val * 2;
    final_out
}

pub struct V2CloneCarrierA;
pub struct V2CloneCarrierB;

impl V2CloneCarrierA {
    pub fn v2_clone_exact(x: i32) -> i32 {
        let step = x + 17;
        let scaled = step * 3;
        let adjusted = scaled - 5;
        adjusted + 1
    }
}

impl V2CloneCarrierB {
    pub fn v2_clone_exact(x: i32) -> i32 {
        let step = x + 17;
        let scaled = step * 3;
        let adjusted = scaled - 5;
        adjusted + 1
    }
}

pub fn archive_chargeback_audit() -> i32 {
    7
}

pub fn reconcile_chargeback_ledger() -> i32 {
    archive_chargeback_audit() + 1
}

pub fn clique_alpha_b() -> i32 {
    11
}

pub fn clique_alpha_a() -> i32 {
    clique_alpha_b() + 1
}

pub fn clique_beta_b() -> i32 {
    22
}

pub fn clique_beta_a() -> i32 {
    clique_beta_b() + 1
}
