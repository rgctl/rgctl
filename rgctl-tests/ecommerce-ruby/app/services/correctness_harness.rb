class V2BaseLedger
  def audit_weight
    1
  end
end

class V2OrderLedger < V2BaseLedger
  attr_accessor :balance

  def initialize(initial)
    @balance = initial
  end

  def read_balance
    v2_free_helper(42) + @balance
  end

  def apply_adjustment(delta)
    @balance = @balance + delta
  end

  def audit_weight
    read_balance()
  end
end

class V2CloneCarrierA
  def v2_clone_exact(x)
    step = x + 17
    scaled = step * 3
    adjusted = scaled - 5
    adjusted + 1
  end
end

class V2CloneCarrierB
  def v2_clone_exact(x)
    step = x + 17
    scaled = step * 3
    adjusted = scaled - 5
    adjusted + 1
  end
end

def correctness_leaf
  42
end

def correctness_mid
  correctness_leaf() + 1
end

def correctness_root(flag)
  value = correctness_mid()
  if flag
    return value * 2
  end
  value
end

def correctness_shared
  1
end

def correctness_left
  correctness_shared()
end

def correctness_right
  correctness_shared()
end

def correctness_diamond
  correctness_left() + correctness_right()
end

def cfg_while_loop(limit)
  idx = 0
  while idx < limit
    idx = idx + 1
  end
  idx
end

def cfg_case_when(code)
  case code
  when 1
    10
  when 2
    20
  else
    0
  end
end

def cfg_begin_rescue(input)
  begin
    if input < 0
      raise 'neg'
    end
    input + 1
  rescue StandardError
    -1
  end
end

def v2_free_helper(offset)
  offset + 5
end

def v2_slice_flow_probe(seed_input)
  dead_unused = 999
  step_val = seed_input + 1
  alias_val = step_val
  final_out = alias_val * 2
  final_out
end

def taint_unsafe_shell(params)
  cmd = params[:cmd]
  system(cmd)
end

def archive_chargeback_audit
  7
end

def reconcile_chargeback_ledger
  archive_chargeback_audit() + 1
end

def clique_alpha_b
  11
end

def clique_alpha_a
  clique_alpha_b() + 1
end

def clique_beta_b
  22
end

def clique_beta_a
  clique_beta_b() + 1
end
