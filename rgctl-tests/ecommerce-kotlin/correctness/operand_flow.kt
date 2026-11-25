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

fun operand_callback(callback: (Int) -> Int, value: Int): Int { return callback(value) }

fun operand_bound(callback: (Int) -> Int, box: OperandBound, value: Int): Int { return callback(value) }
class OperandBound { fun operand_bound_target(value: Int): Int { return value } }

fun operand_callable_field(box: OperandCallable, callback: (Int) -> Int, value: Int): Int { box.callback = callback; return box.callback(value) }

suspend fun operand_deferred_source(value: Any): Any { return value }
suspend fun operand_await(value: Any): Any { return operand_deferred_source(value) }
