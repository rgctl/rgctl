class OperandFlow { static int operand_sink(int value) { return value; } static int operand_relay(int value) { int alias = value; return operand_sink(alias); } }

class OperandMember { static int operand_member(Receiver value, int other) { return value.Perform(other); } }

class OperandSelf { int operand_instance(int other) { return this.Perform(other); } }


class OperandChoice { static object operand_choice(bool flag, object left, object right) { return flag ? left : right; } }
