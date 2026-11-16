/* Intentional call graph for rgctl expected-facts checks.
 * Avoid intermediate locals that some extractors mis-label as callers.
 */

int correctness_leaf(void) {
    return 42;
}

int correctness_mid(void) {
    return correctness_leaf() + 1;
}

int correctness_root(int flag) {
    if (flag) {
        return correctness_mid() * 2;
    }
    return correctness_mid();
}

int v2_free_helper(int offset) {
    return offset + 5;
}

int v2_slice_flow_probe(int seed_input) {
    int dead_unused = 999;
    int step_val = seed_input + 1;
    int alias_val = step_val;
    int final_out = alias_val * 2;
    (void)dead_unused;
    return final_out;
}

int v2_clone_bloom_a(int amount_cents, int tax_rate_bps, int discount_bps) {
    int gross_total = amount_cents + (amount_cents * tax_rate_bps) / 10000;
    int discount_total = (gross_total * discount_bps) / 10000;
    int net_payable = gross_total - discount_total;
    if (net_payable < 0) {
        return 0;
    }
    return net_payable + v2_free_helper(1);
}

int v2_clone_bloom_b(int amount_cents, int tax_rate_bps, int discount_bps) {
    int gross_total = amount_cents + (amount_cents * tax_rate_bps) / 10000;
    int discount_total = (gross_total * discount_bps) / 10000;
    int net_payable = gross_total - discount_total;
    if (net_payable < 0) {
        return 0;
    }
    return net_payable + v2_free_helper(1);
}

int archive_chargeback_audit(void) {
    return 7;
}

int reconcile_chargeback_ledger(void) {
    return archive_chargeback_audit() + 1;
}

int clique_alpha_b(void) {
    return 11;
}

int clique_alpha_a(void) {
    return clique_alpha_b() + 1;
}

int clique_beta_b(void) {
    return 22;
}

int clique_beta_a(void) {
    return clique_beta_b() + 1;
}
