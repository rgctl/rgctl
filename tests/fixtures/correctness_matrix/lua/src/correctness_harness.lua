function correctness_leaf()
  return 42
end

function correctness_mid(flag)
  if flag then
    return correctness_leaf()
  end
  return 0
end

function correctness_root(n)
  if n > 0 then
    return correctness_mid(true)
  else
    return -1
  end
end

function correctness_shared()
  return 7
end

function correctness_left()
  return correctness_shared() + 1
end

function correctness_right()
  return correctness_shared() + 2
end

function correctness_diamond()
  return correctness_left() + correctness_right()
end

function cfg_while_loop(limit)
  local acc = 0
  while acc < limit do
    acc = acc + 1
  end
  return acc
end

function v2_slice_flow_probe(seed_input)
  local noise_val = 999
  local step_val = seed_input + 10
  local alias_val = step_val
  local final_out = alias_val * 2
  if noise_val < 0 then
    return 0
  end
  return final_out
end

function taint_unsafe_exec()
  local raw = io.read()
  os.execute(raw)
  return 1
end

function archive_chargeback_audit(amount)
  return amount
end

function reconcile_chargeback_ledger(amount)
  return archive_chargeback_audit(amount)
end

function v2_clone_exact_a(a, b)
  local sum_val = a + b
  local prod_val = a * b
  if sum_val > prod_val then
    return sum_val - prod_val
  end
  return prod_val - sum_val
end

function clique_alpha_c(x)
  return x + 30
end

function clique_alpha_b(x)
  return clique_alpha_c(x + 20)
end

function clique_alpha_a(x)
  return clique_alpha_b(x + 10)
end
