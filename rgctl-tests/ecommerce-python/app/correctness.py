"""Intentional call graph for rgctl expected-facts checks."""


def correctness_leaf() -> int:
    """Leaf — no outbound application calls."""
    return 42


def correctness_mid() -> int:
    """Mid — calls correctness_leaf."""
    return correctness_leaf() + 1


def correctness_root(flag: bool) -> int:
    """Root — calls correctness_mid and branches for a non-trivial CFG."""
    value = correctness_mid()
    if flag:
        return value * 2
    return value


def correctness_shared() -> int:
    """Shared sink for diamond topology QE."""
    return 1


def correctness_left() -> int:
    return correctness_shared()


def correctness_right() -> int:
    return correctness_shared()


def correctness_diamond() -> int:
    """Diamond root — dual callers into correctness_shared."""
    return correctness_left() + correctness_right()


def v2_free_helper(offset: int) -> int:
    return offset + 5


class V2AuditPolicy:
    def audit_weight(self) -> int:
        return 1


class V2OrderLedger(V2AuditPolicy):
    def __init__(self, initial: int) -> None:
        self.balance_amount = initial

    def read_balance(self) -> int:
        return v2_free_helper(42) + self.balance_amount

    def apply_adjustment(self, delta: int) -> None:
        self.balance_amount = self.balance_amount + delta

    def audit_weight(self) -> int:
        return self.read_balance()


def v2_slice_flow_probe(seed_input: int) -> int:
    dead_unused = 999
    step_val = seed_input + 1
    alias_val = step_val
    final_out = alias_val * 2
    return final_out


class V2CloneCarrierA:
    @staticmethod
    def v2_clone_exact(x: int) -> int:
        step = x + 17
        scaled = step * 3
        adjusted = scaled - 5
        return adjusted + 1


class V2CloneCarrierB:
    @staticmethod
    def v2_clone_exact(x: int) -> int:
        step = x + 17
        scaled = step * 3
        adjusted = scaled - 5
        return adjusted + 1


def archive_chargeback_audit() -> int:
    return 7


def reconcile_chargeback_ledger() -> int:
    return archive_chargeback_audit() + 1


def clique_alpha_b() -> int:
    return 11


def clique_alpha_a() -> int:
    return clique_alpha_b() + 1


def clique_beta_b() -> int:
    return 22


def clique_beta_a() -> int:
    return clique_beta_b() + 1
