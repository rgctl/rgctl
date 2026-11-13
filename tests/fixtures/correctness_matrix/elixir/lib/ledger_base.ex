defmodule V2AuditPolicy do
  @callback audit_weight(integer()) :: integer()
end

defmodule V2BaseLedger do
  def audit_weight(x) do
    x + 1
  end
end

defmodule V2CloneCarrierB do
  def v2_clone_exact(a, b) do
    sum_val = a + b
    prod_val = a * b
    if sum_val > prod_val do
      sum_val - prod_val
    else
      prod_val - sum_val
    end
  end
end

defmodule CliqueBeta do
  def clique_beta_c(x) do
    x + 3
  end

  def clique_beta_b(x) do
    clique_beta_c(x + 2)
  end

  def clique_beta_a(x) do
    clique_beta_b(x + 1)
  end
end
