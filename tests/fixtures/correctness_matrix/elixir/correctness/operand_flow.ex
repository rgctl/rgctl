defmodule OperandFlow do
 def operand_sink(value) do
  value
 end
 def operand_relay(value) do
  alias_value = value
  operand_sink(alias_value)
 end
end


defmodule OperandElements do
 def operand_element(value) do
  items = %{slot: value}
  items[:slot]
 end
end

defmodule OperandCallback do
 def operand_callback(callback, value) do
  callback.(value)
 end
end

defmodule OperandLookup do
 def operand_computed(box, key, value) do
  box[key].(value)
 end
end
defmodule OperandConstructed do
 defstruct [:slot]
end
defmodule OperandConstructFactory do
 def operand_construct(value) do
  %OperandConstructed{slot: value}
 end
end
