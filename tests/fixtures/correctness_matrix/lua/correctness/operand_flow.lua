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

function OperandSelf:operand_instance(other)
 return self:perform(other)
end

function operand_field(box, value)
 box.slot = value
 return box.slot
end
function operand_element(box, value)
 box[0] = value
 return box[1]
end

function operand_callback(callback, value)
 return callback(value)
end

function operand_bound(callback, box, value)
 return callback(value)
end
function OperandBound:operand_bound_target(value)
 return value
end

function operand_callable_field(box, callback, value)
 box.callback = callback
 return box.callback(value)
end
