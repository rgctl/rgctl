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

def operand_callback(callback, value)
 return callback.call(value)
end

def operand_bound(callback, box, value)
 return callback.call(value)
end
class OperandBound
 def operand_bound_target(value)
  return value
 end
end

def operand_callback_other(callback, value)
 return callback.object_id
end

def operand_computed(box, key, value)
 return box[key].call(value)
end
class OperandConstructed
 def initialize(value)
  @slot = value
 end
end
def operand_construct(value)
 return OperandConstructed.new(value)
end
def operand_constructor_alias(factory, value)
 return factory.new(value)
end
def operand_not_constructor(factory)
 return factory.to_s
end
