defmodule Example do
  def calculate(0), do: 11
  def calculate(value) when is_integer(value), do: 21
  def calculate(value, other), do: value + other
end
