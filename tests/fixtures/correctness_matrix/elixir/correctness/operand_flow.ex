defmodule OperandFlow do
 def operand_sink(value) do
  value
 end
 def operand_relay(value) do
  alias_value = value
  operand_sink(alias_value)
 end
end
