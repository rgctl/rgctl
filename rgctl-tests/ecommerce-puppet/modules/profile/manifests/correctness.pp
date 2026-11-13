function correctness_leaf() {
  42
}

function correctness_mid() {
  correctness_leaf() + 1
}

function correctness_root(Boolean $flag) {
  $value = correctness_mid()
  if $flag {
    $value * 2
  } else {
    $value
  }
}

function correctness_shared() {
  1
}

function correctness_left() {
  correctness_shared()
}

function correctness_right() {
  correctness_shared()
}

function correctness_diamond() {
  correctness_left() + correctness_right()
}

function cfg_unless_probe(Boolean $skip) {
  unless $skip {
    $res = 10
  } else {
    $res = 20
  }
  $res
}

function cfg_case_probe(String $env) {
  case $env {
    'prod': { $port = 443 }
    'dev':  { $port = 8080 }
    default: { $port = 80 }
  }
  $port
}

function v2_slice_flow_probe(Integer $seed_input) {
  $dead_unused = 999
  $step_val = $seed_input + 1
  $alias_val = $step_val
  $final_out = $alias_val * 2
  $final_out
}

function v2_clone_bloom_a(Integer $x) {
  $alpha = $x + 17
  $beta = $alpha * 3
  $gamma = $beta - 5
  $delta = $gamma + 11
  $delta
}

function v2_clone_bloom_b(Integer $x) {
  $alpha = $x + 17
  $beta = $alpha * 3
  $gamma = $beta - 5
  $delta = $gamma + 11
  $delta
}

function archive_chargeback_audit() {
  7
}

function reconcile_chargeback_ledger() {
  archive_chargeback_audit() + 1
}

function taint_unsafe_exec() {
  $cmd = lookup('web.unsafe_cmd')
  exec { 'unsafe_exec':
    command => $cmd,
  }
}

function clique_alpha_b() {
  11
}

function clique_alpha_a() {
  clique_alpha_b() + 1
}

function clique_beta_b() {
  22
}

function clique_beta_a() {
  clique_beta_b() + 1
}
