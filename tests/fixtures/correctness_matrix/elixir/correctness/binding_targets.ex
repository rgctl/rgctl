defmodule BindingTargets do
 def binding_helper(), do: 111
 def binding_parameter(binding_helper), do: binding_helper.()
 def binding_direct(binding_helper), do: binding_helper()
def binding_local(callback) do
 binding_helper=callback
 binding_helper.()
 end
end
