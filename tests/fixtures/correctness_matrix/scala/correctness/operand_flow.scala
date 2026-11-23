object OperandFlow { def operand_sink(value: Int): Int = { return value }; def operand_relay(value: Int): Int = { val alias = value; return operand_sink(alias) } }

object OperandMember { def operand_member(value: Receiver, other: Int): Int = { return value.perform(other) } }

class OperandSelf { def operand_instance(other: Int): Int = { return this.perform(other) } }


object OperandChoice { def operand_choice(flag: Boolean, left: Any, right: Any): Any = { return if (flag) left else right } }
