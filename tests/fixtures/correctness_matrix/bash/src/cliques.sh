#!/usr/bin/env bash
# Community separation cliques for Bash correctness matrix (UTF-8: α β)

clique_alpha_c() {
    echo "10"
}

clique_alpha_b() {
    clique_alpha_c
}

clique_alpha_a() {
    clique_alpha_b
    clique_alpha_c
}

clique_beta_c() {
    echo "20"
}

clique_beta_b() {
    clique_beta_c
}

clique_beta_a() {
    clique_beta_b
    clique_beta_c
}
