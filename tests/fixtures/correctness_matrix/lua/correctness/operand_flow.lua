function operand_sink(value)
 return value
end
function operand_relay(value)
 local alias_value = value
 return operand_sink(alias_value)
end

function operand_member(value, other)
 return value:perform(other)
end
