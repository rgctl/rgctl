package operandflow
func operand_sink(value int) int { return value }
func operand_relay(value int) int { alias := value; return operand_sink(alias) }

func operand_pair(left any, right any) (any, any) { return left, right }
func operand_projection(value any, other any) any {
    first, second := operand_pair(value, other)
    _ = first
    return second
}
func operand_switch(value any) any {
    switch alias := value.(type) {
    case int:
        return alias
    default:
        return value
    }
}

func operand_member(value Receiver, other int) int { return value.perform(other) }

func operand_choice(flag bool, left any, right any) any { if flag { return left }; return right }

func operand_field(box OperandBox, value any) any { box.slot = value; return box.slot }
func operand_element(box []any, value any) any { box[0] = value; return box[1] }

func operand_rest(values ...any) any { return values[0] }

func operand_rest_relay(value any) any { return operand_rest(value) }

func operand_callback(callback func(int) int, value int) int { return callback(value) }

func operand_bound(callback func(int) int, box OperandBound, value int) int { return callback(value) }
func(box OperandBound) operand_bound_target(value int) int { return value }

func operand_callable_field(box OperandCallable, callback func(int) int, value int) int { box.Callback = callback; return box.Callback(value) }
