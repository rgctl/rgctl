def operand_sink(value)
 return value
end
def operand_relay(value)
 alias_value = value
 return operand_sink(alias_value)
end

def operand_member(value, other)
 return value.perform(other)
end
