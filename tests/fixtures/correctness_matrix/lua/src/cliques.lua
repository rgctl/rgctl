function v2_clone_exact_b(a, b)
  local sum_val = a + b
  local prod_val = a * b
  if sum_val > prod_val then
    return sum_val - prod_val
  end
  return prod_val - sum_val
end

function clique_beta_c(x)
  return x + 3
end

function clique_beta_b(x)
  return clique_beta_c(x + 2)
end

function clique_beta_a(x)
  return clique_beta_b(x + 1)
end
