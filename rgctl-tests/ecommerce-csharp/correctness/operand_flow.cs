class OperandFlow { static int operand_sink(int value) { return value; } static int operand_relay(int value) { int alias = value; return operand_sink(alias); } }

class OperandMember { static int operand_member(Receiver value, int other) { return value.Perform(other); } }
