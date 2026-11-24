fun operand_sink(value: Int): Int { return value }
fun operand_relay(value: Int): Int { val alias = value; return operand_sink(alias) }

fun operand_member(value: Receiver, other: Int): Int { return value.perform(other) }

class OperandSelf { fun operand_instance(other: Int): Int { return this.perform(other) }
}



fun operand_choice(flag: Boolean, left: Any, right: Any): Any { return if (flag) left else right }

fun operand_field(box: OperandBox, value: Any): Any { box.slot = value; return box.slot }
fun operand_element(box: Array<Any>, value: Any): Any { box[0] = value; return box[1] }

fun operand_rest(vararg values: Any): Any { return values[0] }

fun operand_rest_relay(value: Any): Any { return operand_rest(value) }
