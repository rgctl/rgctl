defmodule CorrectnessHarness do
  def correctness_leaf() do
    42
  end

  def correctness_mid(flag) do
    if flag do
      correctness_leaf()
    else
      0
    end
  end

  def correctness_root(n) do
    if n > 0 do
      correctness_mid(true)
    else
      -1
    end
  end

  def correctness_shared() do
    7
  end

  def correctness_left() do
    correctness_shared() + 1
  end

  def correctness_right() do
    correctness_shared() + 2
  end

  def correctness_diamond() do
    correctness_left() + correctness_right()
  end

  def v2_slice_flow_probe(seed_input) do
    noise_val = 999
    step_val = seed_input + 10
    alias_val = step_val
    final_out = alias_val * 2
    if noise_val < 0 do
      0
    else
      final_out
    end
  end

  def taint_unsafe_exec() do
    raw = System.get_env("ORDER_FILTER")
    Ecto.Adapters.SQL.query(Repo, raw)
  end

  def archive_chargeback_audit(amount) do
    amount
  end

  def reconcile_chargeback_ledger(amount) do
    archive_chargeback_audit(amount)
  end
end

defmodule CfgFeatures do
  def cfg_if_branch(code) do
    if code == 1 do
      10
    else
      20
    end
  end
end

defmodule V2OrderLedger do
  @behaviour V2AuditPolicy
  use V2BaseLedger

  def read_balance(b) do
    b + 5
  end

  def audit_weight(b) do
    read_balance(b)
  end
end

defmodule V2CloneCarrierA do
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

defmodule CliqueAlpha do
  def clique_alpha_c(x) do
    x + 30
  end

  def clique_alpha_b(x) do
    clique_alpha_c(x + 20)
  end

  def clique_alpha_a(x) do
    clique_alpha_b(x + 10)
  end
end
