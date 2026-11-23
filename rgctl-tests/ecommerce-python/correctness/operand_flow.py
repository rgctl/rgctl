def operand_sink(value):
    return value
def operand_relay(value):
    alias = value
    return operand_sink(alias)

def operand_member(value, other):
    return value.perform(other)
