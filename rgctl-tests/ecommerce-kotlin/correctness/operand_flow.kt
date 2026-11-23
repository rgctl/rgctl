fun operand_sink(value: Int): Int { return value }
fun operand_relay(value: Int): Int { val alias = value; return operand_sink(alias) }

fun operand_member(value: Receiver, other: Int): Int { return value.perform(other) }
