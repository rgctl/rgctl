class OperandFlow { static int operand_sink(int value) { return value; } static int operand_relay(int value) { int alias = value; return operand_sink(alias); } }

class OperandMember { static int operand_member(Receiver value, int other) { return value.Perform(other); } }

class OperandSelf { int operand_instance(int other) { return this.Perform(other); } }


class OperandChoice { static object operand_choice(bool flag, object left, object right) { return flag ? left : right; } }

class OperandStorage { static object operand_field(OperandBox box, object value) { box.slot = value; return box.slot; } }
class OperandElements { static object operand_element(object[] box, object value) { box[0] = value; return box[1]; } }

class OperandRest { static object operand_rest(params object[] values) { return values[0]; } }

class OperandRestRelay { static object operand_rest_relay(object value) { return OperandRest.operand_rest(value); } }
