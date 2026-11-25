object OperandFlow { def operand_sink(value: Int): Int = { return value }; def operand_relay(value: Int): Int = { val alias = value; return operand_sink(alias) } }

object OperandMember { def operand_member(value: Receiver, other: Int): Int = { return value.perform(other) } }

class OperandSelf { def operand_instance(other: Int): Int = { return this.perform(other) } }


object OperandChoice { def operand_choice(flag: Boolean, left: Any, right: Any): Any = { return if (flag) left else right } }

object OperandStorage { def operand_field(box: OperandBox, value: Any): Any = { box.slot = value; return box.slot } }
object OperandElements { def operand_container(value: Any): (Any, Any) = { return (value, value) } }

object OperandCallback { def operand_callback(callback: Int => Int, value: Int): Int = { return callback(value) } }

object OperandBoundCaller { def operand_bound(callback: Int => Int, box: OperandBound, value: Int): Int = { return callback(value) } }
class OperandBound { def operand_bound_target(value: Int): Int = { return value } }

object OperandLookup { def operand_computed(box: OperandCallbacks, value: Int): Int = { return box.slot(value) } }
class OperandConstructed(val slot: Int)
object OperandConstructFactory { def operand_construct(value: Int): OperandConstructed = { return new OperandConstructed(value) } }
