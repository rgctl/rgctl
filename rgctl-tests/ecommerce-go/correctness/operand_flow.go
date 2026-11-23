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
