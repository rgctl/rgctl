class OperandFlow { static int operand_sink(int value) { return value; } static int operand_relay(int value) { int alias = value; return operand_sink(alias); } }

class OperandMember { static int operand_member(Receiver value, int other) { return value.perform(other); } }

class OperandChoice { static Object operand_choice(boolean flag, Object left, Object right) { return flag ? left : right; } }

class OperandStorage { static Object operand_field(OperandBox box, Object value) { box.slot = value; return box.slot; } }
class OperandElements { static Object operand_element(Object[] box, Object value) { box[0] = value; return box[1]; } }

class OperandRest { static Object operand_rest(Object... values) { return values[0]; } }

class OperandRestRelay { static Object operand_rest_relay(Object value) { return OperandRest.operand_rest(value); } }
