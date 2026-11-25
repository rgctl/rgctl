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

class OperandSelf
 def operand_instance(other)
  return self.perform(other)
 end
end


def operand_choice(flag, left, right)
 return flag ? left : right
end

class OperandStorage
 def operand_field(value)
  @slot = value
  return @slot
 end
end
def operand_element(box, value)
 box[0] = value
 return box[1]
end

def operand_rest(*values)
 return values[0]
end

def operand_rest_relay(value)
 return operand_rest(value)
end
